//! The pre-2.0.0 stand-alone router: the profile paths plus `/healthz`,
//! `/metrics` and the body cap on one listener. The binary gets those three
//! from the kit since 2.0.0; this stays for the in-process test suites,
//! which drive the service without the kit.

use std::sync::Arc;

use axum::extract::DefaultBodyLimit;
use axum::http::StatusCode;
use axum::Router;

use super::{profile_router, MAX_BODY_BYTES};
use crate::adapters::{Clock, Sink};
use crate::config::Config;
use crate::obs::Registry;
use crate::reload::ProfileStore;

/// The pre-2.0.0 stand-alone router: profile paths plus `/healthz`,
/// `/metrics` and the body cap. Kept for the in-process suites; the binary
/// gets these three from the kit.
pub fn router(
    config: &Config,
    store: Arc<ProfileStore>,
    sink: Arc<dyn Sink>,
    clock: Arc<dyn Clock>,
    registry: Arc<Registry>,
) -> Router {
    let router = profile_router(config, store, sink, clock, Arc::clone(&registry));

    // AR11: the same listener, on paths the config refuses to let a
    // profile claim. Neither answers anything a message put there.
    let health_registry = Arc::clone(&registry);
    let metrics_registry = Arc::clone(&registry);
    router
        .route(
            "/healthz",
            axum::routing::get(
                move |axum::extract::Query(q): axum::extract::Query<
                    std::collections::HashMap<String, String>,
                >| {
                    let registry = Arc::clone(&health_registry);
                    async move {
                        let (healthy, body) = registry.healthz();
                        // Plain /healthz answers liveness: the process is
                        // up and can serve. ?strict=1 answers "is every
                        // profile doing its job", for a monitor that should
                        // alarm rather than restart (Phase 7, G2).
                        let strict = matches!(
                            q.get("strict").map(String::as_str),
                            Some("1" | "true" | "yes")
                        );
                        let status = if healthy || !strict {
                            StatusCode::OK
                        } else {
                            StatusCode::SERVICE_UNAVAILABLE
                        };
                        (status, [("content-type", "application/json")], body)
                    }
                },
            ),
        )
        .route(
            "/metrics",
            axum::routing::get(move || {
                let registry = Arc::clone(&metrics_registry);
                async move {
                    (
                        [("content-type", "text/plain; version=0.0.4")],
                        registry.metrics(),
                    )
                }
            }),
        )
        .layer(DefaultBodyLimit::max(MAX_BODY_BYTES))
}
