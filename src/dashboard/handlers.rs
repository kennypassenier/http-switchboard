//! The three routes behind the admin login: the snapshot the page polls,
//! and the two recheck buttons.

use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;

use super::profiles::Profiles;

fn json(status: StatusCode, body: String) -> impl IntoResponse {
    (status, [("content-type", "application/json")], body)
}

pub(super) async fn snapshot(State(profiles): State<Arc<Profiles>>) -> impl IntoResponse {
    json(StatusCode::OK, profiles.snapshot_json())
}

/// The Recheck button's handler. Clears the remembered failure so a
/// profile nobody posts to stops reporting an outage that is over. The
/// answer carries the count the toast says ("3 profiles back to starting").
pub async fn recheck(State(profiles): State<Arc<Profiles>>) -> impl IntoResponse {
    let reset = profiles.registry.reset_health();
    crate::obs::log_warn(
        "*",
        "profiles_rechecked",
        &format!(
            "an operator reset the remembered health of {reset} profile(s) from the dashboard"
        ),
    );
    json(StatusCode::OK, format!(r#"{{"rechecked":{reset}}}"#))
}

/// "Recheck <name>": the same reset for one profile.
pub async fn recheck_one(
    State(profiles): State<Arc<Profiles>>,
    Path(name): Path<String>,
) -> impl IntoResponse {
    if !profiles.order.contains(&name) || !profiles.registry.reset_health_of(&name) {
        let names = profiles.order.join(", ");
        return json(
            StatusCode::NOT_FOUND,
            serde_json::json!({
                "error": format!("there is no profile named '{name}'"),
                "remedy": format!("pick one of: {names}"),
            })
            .to_string(),
        );
    }
    crate::obs::log_warn(
        &name,
        "profile_rechecked",
        "an operator reset the remembered health of this profile from the dashboard",
    );
    json(
        StatusCode::OK,
        serde_json::json!({ "rechecked": 1, "profile": name }).to_string(),
    )
}
