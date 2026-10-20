//! [feat-dash-kp11-41] The "Reloads" section of the status page: the
//! history of the profile file being read again, newest first.
//!
//! The data is [`crate::reload_log`]'s capped, in-memory record; this
//! module only serves it (`GET /reloads.json`, behind the admin login)
//! and renders the card's frame. `web/reloads.js` draws it and refreshes
//! on the dashboard's existing poll, so there is no second timer.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use chassis::{Section, StatusSection};

use crate::dashboard::{asset_version, script_safe, WEB_MOUNT};
use crate::reload::ProfileStore;

/// The JSON the card is drawn from.
pub const RELOADS_ROUTE: &str = "/reloads.json";

struct ReloadsSection;

async fn reloads(State(store): State<Arc<ProfileStore>>) -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        serde_json::to_string(&store.log.view()).unwrap_or_else(|_| "{}".to_string()),
    )
}

/// Register the section and its route. Called by `dashboard::install`.
pub fn install(app: &mut chassis::App, store: Arc<ProfileStore>) {
    app.status_section(ReloadsSection);
    app.dashboard_routes(
        Router::new()
            .route(RELOADS_ROUTE, get(reloads))
            .with_state(store),
    );
}

impl StatusSection for ReloadsSection {
    fn render(&self) -> Section {
        let version = asset_version();
        let data = serde_json::json!({
            "route": RELOADS_ROUTE,
        })
        .to_string();
        let html = format!(
            r##"<link rel="stylesheet" href="{WEB_MOUNT}/reloads.css?v={version}">
<script type="application/json" id="hsb-reloads-data">{data}</script>
<script type="module" src="{WEB_MOUNT}/reloads.js?v={version}"></script>
<section class="kp-card hsb-reloads-card" id="hsb-reloads" data-kp-state="loading" aria-labelledby="hsb-reloads-t">
  <div class="kp-card__header">
    <h3 class="kp-card__title" id="hsb-reloads-t">Reloads</h3>
    <p class="hsb-reloads-counts" id="hsb-reloads-counts" aria-live="polite"></p>
  </div>
  <div class="kp-card__stage">
    <div class="kp-card__data hsb-reloads">
      <div class="kp-problems" data-kp-problems data-kp-problems-mode="each" id="hsb-reloads-alert"></div>
      <div class="hsb-reloads__host">
        <ol class="kp-timeline hsb-reloads__list" id="hsb-reloads-list" data-kp-follow="head" data-kp-pill-unit="reload,reloads" tabindex="0" aria-label="Reloads of the profile file, newest first"></ol>
      </div>
    </div>
  </div>
</section>
<noscript><p>The history of reloads needs JavaScript.</p></noscript>"##,
            data = script_safe(&data),
        );
        Section {
            title: "Reloads".to_string(),
            explain: "Every time the service read its profile file again (after \
                      `systemctl reload http-switchboard`), newest first: what changed, what \
                      needs a restart, and what was refused. The history is kept in memory, \
                      holds the last 50 reloads and starts empty after every restart."
                .to_string(),
            rows: Vec::new(),
            html: Some(html),
        }
    }
}
