//! The status page's history sections (feat-dash-kp11-30..39): the key
//! figures of the last 24 hours with their trend, one uptime strip of 24
//! hour cells per profile, and the refresh chooser that drives how often
//! the whole page reads again.
//!
//! The data is [`crate::history::History`], read through [`HISTORY_ROUTE`]
//! behind the admin login (a route of its own: the profiles snapshot is
//! polled by the cards and stays small). The first paint comes from a
//! `<script type="application/json">` in the first section, like the
//! profiles; `web/history.js` is started by `web/dashboard.js`, which owns
//! the poll the chooser takes over.
//!
//! The history is in memory and lost on restart; the page says so ("History
//! since the last start, at most 24 hours"). It carries counts, times and a
//! cause word, never a payload, a header, a URL or an error text.

use std::sync::Arc;

use axum::extract::State;
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::get;
use axum::Router;
use chassis::{Section, StatusSection};

use crate::dashboard::{asset_version, WEB_MOUNT};
use crate::obs::Registry;

/// The JSON the figures and strips are drawn from.
pub const HISTORY_ROUTE: &str = "/history.json";

struct Source {
    registry: Arc<Registry>,
    /// Profile names in config order.
    order: Vec<String>,
}

impl Source {
    fn json(&self) -> String {
        serde_json::to_string(&self.registry.history.view(&self.order))
            .unwrap_or_else(|_| r#"{"profiles":[]}"#.to_string())
    }
}

/// JSON that is safe inside a `<script type="application/json">`.
fn script_safe(json: &str) -> String {
    json.replace('<', "\\u003c")
}

/// Register the two sections and the route. Called from
/// [`crate::dashboard::install`], which also serves `history.js` and
/// `history.css` from its embedded web files.
pub fn install(app: &mut chassis::App, registry: Arc<Registry>, order: Vec<String>) {
    let source = Arc::new(Source { registry, order });
    app.status_section(FiguresSection(Arc::clone(&source)));
    app.status_section(UptimeSection);
    app.dashboard_routes(
        Router::new()
            .route(HISTORY_ROUTE, get(history))
            .with_state(source),
    );
}

async fn history(State(source): State<Arc<Source>>) -> impl IntoResponse {
    (
        StatusCode::OK,
        [("content-type", "application/json")],
        source.json(),
    )
}

struct FiguresSection(Arc<Source>);

impl StatusSection for FiguresSection {
    fn render(&self) -> Section {
        let version = asset_version();
        let data = serde_json::json!({
            "history": self.0.registry.history.view(&self.0.order),
            "route": HISTORY_ROUTE,
        })
        .to_string();
        let html = format!(
            r##"<link rel="stylesheet" href="{WEB_MOUNT}/history.css?v={version}">
<script type="application/json" id="hsb-history-data">{data}</script>
<div class="kp-formgrid" id="hsb-h-grid">
  <div class="kp-refresh" id="hsb-h-refresh" data-kp-refresh="30" data-kp-refresh-label="Page refresh" data-kp-remember="hsb-refresh" data-kp-look="segmented"></div>
  <div class="kp-formgrid__row">
    <span class="kp-formgrid__label" aria-hidden="true"></span>
    <div class="kp-formgrid__control" data-kp-formgrid-span="wide"><p class="hsb-h-note">History since the last start, at most 24 hours.</p></div>
  </div>
  <div class="kp-formgrid__row" id="hsb-h-pick-row" hidden>
    <b class="kp-formgrid__label" id="hsb-h-pick-l">Profile</b>
    <div class="kp-formgrid__control"><div class="hsb-choices" role="group" aria-labelledby="hsb-h-pick-l" id="hsb-h-pick"></div></div>
  </div>
  <div class="kp-formgrid__row" id="hsb-h-tiles" hidden>
    <b class="kp-formgrid__label" id="hsb-h-tiles-l">Tiles</b>
    <div class="kp-formgrid__control">
      <div class="hsb-choices" role="group" aria-labelledby="hsb-h-tiles-l">
        <button type="button" class="kp-button kp-button--sm" aria-pressed="true" data-hsb-tiles="trend">Trend</button>
        <button type="button" class="kp-button kp-button--sm" aria-pressed="false" data-hsb-tiles="spark">Sparklines</button>
      </div>
    </div>
  </div>
</div>
<section class="kp-card hsb-h-card" id="hsb-h-card" data-kp-state="ready" aria-labelledby="hsb-h-title" data-kp-loading-words="Reading the last 24 hours">
  <div class="kp-card__header">
    <h3 class="kp-card__title" id="hsb-h-title">Last 24 hours</h3>
    <div class="kp-card__actions"><span class="kp-timestamp" id="hsb-h-read"></span></div>
  </div>
  <p class="kp-card__body" id="hsb-h-body"></p>
  <div class="kp-card__stage">
    <div class="kp-card__data hsb-h-data">
      <div class="kp-kpis" data-kp-kpis-columns="4 2 1" role="group" aria-label="Key figures" id="hsb-h-strip"></div>
      <div class="hsb-h-recent">
        <h4 class="hsb-h-recent__title" id="hsb-h-recent-title">Latest deliveries</h4>
        <ul class="hsb-h-recent__list" aria-label="Deliveries" id="hsb-h-recent"></ul>
      </div>
    </div>
  </div>
</section>
<noscript><p>The key figures need JavaScript.</p></noscript>"##,
            data = script_safe(&data),
        );
        Section {
            title: "Last 24 hours".to_string(),
            explain: "Received, delivered, failed and the average delivery time of one \
                      profile over the last 24 hours, each with its trend by the hour. \
                      The history lives in memory: it starts empty when the service \
                      starts and keeps at most 24 hours. The page refresh chooser sets \
                      how often the whole page reads again, or pauses it."
                .to_string(),
            rows: Vec::new(),
            html: Some(html),
        }
    }
}

struct UptimeSection;

impl StatusSection for UptimeSection {
    fn render(&self) -> Section {
        Section {
            title: "Uptime by the hour".to_string(),
            explain: "One strip per profile: each cell is an hour of the last 24, green when \
                      it went well, amber when some deliveries failed, red when every \
                      delivery failed or the hub refused or could not be reached, grey \
                      before the service started. Press a cell for the hour's details."
                .to_string(),
            rows: Vec::new(),
            html: Some(
                r#"<div class="kp-formgrid">
  <div class="kp-formgrid__row" id="hsb-h-press" hidden>
    <b class="kp-formgrid__label" id="hsb-h-press-l">Press a cell</b>
    <div class="kp-formgrid__control">
      <div class="hsb-choices" role="group" aria-labelledby="hsb-h-press-l">
        <button type="button" class="kp-button kp-button--sm" aria-pressed="true" data-hsb-press="info">Quick info</button>
        <button type="button" class="kp-button kp-button--sm" aria-pressed="false" data-hsb-press="hover">Hover card only</button>
      </div>
    </div>
  </div>
</div>
<div class="hsb-h-strips" id="hsb-h-strips"></div>"#
                    .to_string(),
            ),
        }
    }
}
