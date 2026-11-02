//! The status page's history sections (feat-dash-kp11-30..39): the key
//! figures of the last 24 hours with their trend, one uptime strip of 24
//! hour cells per profile, and the View options popover (how the tiles
//! look, what pressing an hour opens). How often the page reads again is
//! the kit's refresh menu in the page header.
//!
//! The data is [`crate::history::History`], read through [`HISTORY_ROUTE`]
//! behind the admin login (a route of its own: the profiles snapshot is
//! polled by the cards and stays small). The first paint comes from a
//! `<script type="application/json">` in the first section, like the
//! profiles; `web/history.js` is started by `web/dashboard.js` and reads
//! again on every poll of the page.
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
<section class="kp-card hsb-flat hsb-h-card" id="hsb-h-card" data-chassis-owned data-kp-state="loading" aria-label="Last 24 hours" data-kp-loading-words="Reading the last 24 hours">
  <div class="hsb-h-toolbar">
    <p class="kp-card__body" id="hsb-h-body"></p>
    <div class="kp-card__actions">
      <button type="button" class="kp-button kp-button--sm" id="hsb-h-options-open" popovertarget="hsb-h-options" aria-haspopup="dialog" style="anchor-name: --hsb-h-options" hidden>View options ▾</button>
      <div popover="auto" id="hsb-h-options" class="kp-popover hsb-h-options" style="position-anchor: --hsb-h-options" role="dialog" aria-label="View options">
        <div class="kp-formgrid">
          <div class="kp-formgrid__row">
            <b class="kp-formgrid__label">Tiles</b>
            <div class="kp-formgrid__control">
              <div class="kp-segmented" data-kp-segmented="single" role="radiogroup" aria-label="Tiles" id="hsb-h-tiles">
                <button type="button" class="kp-segmented__item" data-value="trend" role="radio" aria-checked="true"><span class="kp-segmented__label">Trend</span></button>
                <button type="button" class="kp-segmented__item" data-value="spark" role="radio" aria-checked="false"><span class="kp-segmented__label">Sparklines</span></button>
              </div>
              <p class="kp-field__help">Trend draws each figure's hourly chart; Sparklines keeps the tiles small, with Failed as a filter.</p>
            </div>
          </div>
          <div class="kp-formgrid__row">
            <b class="kp-formgrid__label">Press an hour</b>
            <div class="kp-formgrid__control">
              <div class="kp-segmented" data-kp-segmented="single" role="radiogroup" aria-label="Press an hour of the uptime strip" id="hsb-h-press">
                <button type="button" class="kp-segmented__item" data-value="info" role="radio" aria-checked="true"><span class="kp-segmented__label">Quick info</span></button>
                <button type="button" class="kp-segmented__item" data-value="hover" role="radio" aria-checked="false"><span class="kp-segmented__label">Hover card only</span></button>
              </div>
              <p class="kp-field__help">What pressing an hour of the uptime strip below opens.</p>
            </div>
          </div>
        </div>
      </div>
    </div>
  </div>
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
            explain: "Received, delivered, failed and average delivery time of the profile \
                      chosen in Path of a profile, counted per hour over the last 24 hours."
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
            explain: "One strip per profile, one cell per hour of the last 24: green when it went \
                      well, amber when some deliveries failed, red when all failed or the \
                      hub could not be used, grey when nothing was received or recorded."
                .to_string(),
            rows: Vec::new(),
            html: Some(r#"<div class="hsb-h-strips" id="hsb-h-strips"></div>"#.to_string()),
        }
    }
}
