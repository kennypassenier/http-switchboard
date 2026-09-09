//! What this service puts on the kit's dashboard (V1, 3.0.0).
//!
//! Since H1 moved the inbound door onto the kit's client tokens, this
//! service has a dashboard: a login, a page listing the senders that hold
//! a token, and the status page. Two things are worth adding to it.
//!
//! * A **Profiles** section, because "which profile is working" is the
//!   only question anybody asks this service, and until now the only way
//!   to ask it was to read `/healthz` as JSON.
//! * One button under that section, **Recheck profiles**. It is not
//!   decoration: a profile's health only changes when a message goes
//!   through it, so an HTTP-source profile that failed once stays red
//!   until a sender happens to post again — which, for a profile nobody
//!   uses yet, is never. The button puts them back to `starting`, the
//!   state that means "nothing has been tried since".
//!
//! There is deliberately **no button on a sender's row**. The kit already
//! puts Re-issue, Revoke and Delete there, and this service has no
//! per-sender operation of its own to add — a row button with nothing
//! behind it would be worse than an empty row.

use std::sync::Arc;

use axum::extract::State;
use axum::response::IntoResponse;
use chassis::{Section, SectionAction, StatusSection};

use crate::config::{Config, Sink, Source};
use crate::obs::Registry;
use crate::pump::Health;

/// The route the Recheck button posts to. Registered with
/// `dashboard_routes`, so it is behind the admin login, not a sender's
/// token.
pub const RECHECK_ROUTE: &str = "/profiles/recheck";

pub struct Profiles {
    registry: Arc<Registry>,
    /// Name, source and destination, in config order — the shape of the
    /// profile never changes while the service runs, so it is read once.
    shapes: Vec<(String, String)>,
}

impl Profiles {
    pub fn new(config: &Config, registry: Arc<Registry>) -> Self {
        let shapes = config
            .profiles
            .iter()
            .map(|p| {
                let from = match &p.source {
                    Source::Http { path } => format!("POST {path}"),
                    Source::Kyu { topic } => format!("kyu topic {topic}"),
                };
                let to = match &p.sink {
                    Sink::Url { url, .. } => shorten(url),
                    Sink::Kyu { topic } => format!("kyu topic {topic}"),
                };
                (p.name.clone(), format!("{from} → {to}"))
            })
            .collect();
        Self { registry, shapes }
    }
}

/// A destination URL on a status page, without its path: the path of a
/// Home Assistant webhook *is* the credential (S2), and this page is one
/// screenshot away from a chat window.
fn shorten(url: &str) -> String {
    match url.split_once("://") {
        Some((scheme, rest)) => {
            let host = rest.split('/').next().unwrap_or(rest);
            format!("{scheme}://{host}/…")
        }
        None => url.to_string(),
    }
}

impl StatusSection for Profiles {
    fn render(&self) -> Section {
        let snapshot = self.registry.snapshot();
        let rows = self
            .shapes
            .iter()
            .map(|(name, shape)| {
                let stats = snapshot.get(name);
                let state = match stats.and_then(|s| s.health) {
                    None | Some(Health::Starting) => "starting",
                    Some(Health::Working) => "working",
                    Some(Health::Failing) => "failing",
                    Some(Health::Denied) => "denied — the hub refused our credentials",
                    Some(Health::HubDown) => "hub-down — the hub could not be reached",
                };
                let delivered = stats.map(|s| s.delivered).unwrap_or(0);
                let failed = stats.map(|s| s.failed).unwrap_or(0);
                (
                    name.clone(),
                    format!("{state} · {shape} · {delivered} delivered, {failed} failed"),
                )
            })
            .collect();
        Section {
            title: "Profiles".to_string(),
            explain: "One profile is one source, one translation and one destination. \
                      A profile is 'starting' until a message has gone through it, and \
                      goes back to 'working' the moment one does."
                .to_string(),
            rows,
            html: None,
        }
    }

    fn actions(&self) -> Vec<SectionAction> {
        vec![SectionAction {
            label: "Recheck profiles".to_string(),
            route: RECHECK_ROUTE.to_string(),
            method: "POST".to_string(),
            destructive: false,
            confirm: None,
            busy_label: Some("Rechecking…".to_string()),
        }]
    }
}

/// The Recheck button's handler. Clears the remembered failure so a
/// profile nobody posts to stops reporting an outage that is over.
pub async fn recheck(State(registry): State<Arc<Registry>>) -> impl IntoResponse {
    let reset = registry.reset_health();
    crate::obs::log_warn(
        "*",
        "profiles_rechecked",
        &format!(
            "an operator reset the remembered health of {reset} profile(s) from the dashboard"
        ),
    );
    (
        axum::http::StatusCode::OK,
        [("content-type", "application/json")],
        format!(r#"{{"rechecked":{reset}}}"#),
    )
}
