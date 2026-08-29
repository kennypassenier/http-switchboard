//! The inbound side: webhooks arriving over HTTP (L5 — K1, W1, W8;
//! AR3, AR9).
//!
//! Three decisions here came out of the Phase 4 critic pass and are worth
//! stating where the code is:
//!
//! * **One route per path, dispatching to the profiles behind it.** The
//!   draft said one route per profile, which makes the router panic at
//!   startup when two profiles share a path — and K9 requires exactly
//!   that. A panic inside a distroless container is a restart loop whose
//!   only documentation is a backtrace.
//! * **Concurrent, but bounded.** Serialising an HTTP source buys no
//!   ordering (the caller is the serialiser) and would make the twentieth
//!   caller in a burst wait minutes while their bodies pile up in memory.
//!   Past the bound the answer is an immediate 503 with `Retry-After`.
//! * **Answer only after delivering** (W1). This service stores nothing,
//!   so "accepted" up front and a failure afterwards means the message is
//!   gone while the sender believes it arrived.

use std::collections::BTreeMap;
use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, State};
use axum::http::{HeaderMap, StatusCode};
use axum::response::IntoResponse;
use axum::routing::post;
use axum::Router;

use crate::adapters::{deliver_with_retry, Clock, Sink};
use crate::config::{Config, Profile, Source};
use crate::pump::Step;
use crate::translate;

/// A body cap exists from day one even though the configurable limits of
/// W9 are rated Later: the cap is three lines, and its absence is
/// unbounded memory.
pub const MAX_BODY_BYTES: usize = 1024 * 1024;

/// How many inbound requests may be in flight at once. Past this the
/// answer is a refusal, not a queue — a queue with no bound is the same
/// memory problem wearing a hat.
pub const MAX_IN_FLIGHT: usize = 32;

pub struct Inbound {
    profiles: Vec<Arc<Profile>>,
    sink: Arc<dyn Sink>,
    clock: Arc<dyn Clock>,
    permits: Arc<tokio::sync::Semaphore>,
}

/// One request's worth of outcome, for the log line and the metrics that
/// L6 attaches.
#[derive(Debug)]
pub struct InboundReport {
    pub steps: Vec<(String, Step)>,
}

pub fn router(config: &Config, sink: Arc<dyn Sink>, clock: Arc<dyn Clock>) -> Router {
    let mut by_path: BTreeMap<String, Vec<Arc<Profile>>> = BTreeMap::new();
    for profile in &config.profiles {
        if let Source::Http { path } = &profile.source {
            by_path
                .entry(path.clone())
                .or_default()
                .push(Arc::new(profile.clone()));
        }
    }

    let permits = Arc::new(tokio::sync::Semaphore::new(MAX_IN_FLIGHT));
    let mut router = Router::new();
    for (path, profiles) in by_path {
        let state = Arc::new(Inbound {
            profiles,
            sink: Arc::clone(&sink),
            clock: Arc::clone(&clock),
            permits: Arc::clone(&permits),
        });
        router = router.route(&path, post(handle).with_state(state));
    }
    router.layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
}

async fn handle(
    State(state): State<Arc<Inbound>>,
    headers: HeaderMap,
    body: Bytes,
) -> impl IntoResponse {
    // W8: the door is checked before anything else is done with the body.
    if let Some(expected) = state.profiles[0].inbound_token.as_ref() {
        let presented = headers
            .get("authorization")
            .and_then(|v| v.to_str().ok())
            .and_then(|v| v.strip_prefix("Bearer "))
            .unwrap_or_default();
        if !constant_time_eq(presented.as_bytes(), expected.expose().as_bytes()) {
            return (
                StatusCode::UNAUTHORIZED,
                "this path requires a token. What now: send it as `authorization: Bearer <token>`; \
                 the value is the one in the profile's inbound_token.\n",
            )
                .into_response();
        }
    }

    let Ok(_permit) = state.permits.clone().try_acquire_owned() else {
        return (
            StatusCode::SERVICE_UNAVAILABLE,
            [("retry-after", "1")],
            "too many requests are in flight. What now: retry in a moment — this is a refusal, \
             not a loss, and your message has not been delivered.\n",
        )
            .into_response();
    };

    let mut failures = Vec::new();
    for profile in &state.profiles {
        match translate::prepare(profile, &body) {
            Err(e) => failures.push(format!("{e}")),
            Ok(delivery) => {
                let outcome = deliver_with_retry(
                    profile,
                    &delivery,
                    state.sink.as_ref(),
                    state.clock.as_ref(),
                )
                .await;
                if let Err(e) = outcome.result {
                    failures.push(format!("profile '{}': {e}", profile.name));
                }
            }
        }
    }

    if failures.is_empty() {
        (StatusCode::OK, "delivered\n").into_response()
    } else {
        // AR4: with several profiles on one path, all must succeed or the
        // sender is told it failed — and the documented consequence is
        // that their retry re-delivers the branch that did succeed.
        (
            StatusCode::BAD_GATEWAY,
            format!(
                "not delivered.\n{}\nWhat now: this message was NOT accepted; send it again once \
                 the cause above is addressed.\n",
                failures.join("\n")
            ),
        )
            .into_response()
    }
}

/// Token comparison that does not leak the answer through its own timing.
fn constant_time_eq(a: &[u8], b: &[u8]) -> bool {
    if a.len() != b.len() {
        return false;
    }
    a.iter().zip(b).fold(0u8, |acc, (x, y)| acc | (x ^ y)) == 0
}
