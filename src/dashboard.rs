//! What this service puts on the kit's dashboard (V1, 3.0.0; kp-themes 11
//! views, 3.5.0).
//!
//! Since H1 moved the inbound door onto the kit's client tokens, this
//! service has a dashboard: a login, a page listing the senders that hold
//! a token, and the status page. The status page carries four sections of
//! this service's own, in the order a reader asks the questions:
//!
//! 1. **Profiles**: "which profile is working", as one card per profile
//!    (the default) or as a map of sources and destinations.
//! 2. **Path of a profile**: where one message stops.
//! 3. **Settings and retry budget**: what a profile does, and whether its
//!    retries fit inside the hub's lease.
//! 4. **Recheck**: the button that puts every profile back to `starting`,
//!    and the same per profile in a row menu.
//!
//! The recheck is not decoration: a profile's health only changes when a
//! message goes through it, so an HTTP-source profile that failed once
//! stays red until a sender happens to post again, which, for a profile
//! nobody uses yet, is never. `starting` is the state that means "nothing
//! has been tried since".
//!
//! # How the page reaches the browser
//!
//! The kit renders a section's `html` raw but allows no inline script, so
//! the components are drawn by one small ES module (`web/dashboard.js`,
//! embedded, served by the kit's web app under [`WEB_MOUNT`] behind the
//! admin login). The first section carries the `<script>` and `<link>`
//! tags and a `<script type="application/json">` with the first snapshot,
//! so the cards are there on first paint; the module then asks
//! [`SNAPSHOT_ROUTE`] for fresh data.
//!
//! **Freshness is a light poll, not a Live push** (every 10 s while the
//! tab is visible, and straight after a recheck). A push would need a hook
//! in the pump for every health change and counter, and the counters move
//! on every message; a poll of a few hundred bytes behind the login costs
//! less than the code that would replace it, and a missed event cannot
//! leave the page stale for longer than one interval.
//!
//! # Secrets
//!
//! The snapshot is the only thing the module reads, and it carries no
//! header value, no payload and no webhook path: the path of a Home
//! Assistant webhook *is* the credential (S2), and this page is one
//! screenshot away from a chat window. A destination is shown as host and
//! port only; user info in a URL is cut as well.
//!
//! There is deliberately **no button on a sender's row**. The kit already
//! puts Re-issue, Revoke and Delete there, and this service has no
//! per-sender operation of its own to add.

use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use axum::extract::{Path, State};
use axum::http::StatusCode;
use axum::response::IntoResponse;
use axum::routing::{get, post};
use axum::Router;
use chassis::shell::webapp::WebApp;
use chassis::{Section, StatusSection};
use serde::Serialize;

use crate::config::{Config, Profile, Sink, Source};
use crate::obs::{ProfileStats, Registry};
use crate::pump::Health;
use crate::reload::ProfileStore;

/// The route "Recheck profiles" posts to. Registered with
/// `dashboard_routes`, so it is behind the admin login, not a sender's
/// token.
pub const RECHECK_ROUTE: &str = "/profiles/recheck";
/// "Recheck this profile": `{name}` is the profile's name, URL-encoded.
pub const RECHECK_ONE_ROUTE: &str = "/profiles/{name}/recheck";
/// The JSON the cards, map, trace and settings are drawn from.
pub const SNAPSHOT_ROUTE: &str = "/profiles.json";
/// Where the dashboard module and its stylesheet are served.
pub const WEB_MOUNT: &str = "/app/hsb";

const WEB_FILES: &[(&str, &[u8])] = &[
    ("index.html", include_bytes!("../web/index.html")),
    ("dashboard.js", include_bytes!("../web/dashboard.js")),
    ("dashboard.css", include_bytes!("../web/dashboard.css")),
    ("history.js", include_bytes!("../web/history.js")),
    ("history.css", include_bytes!("../web/history.css")),
    ("profiles.js", include_bytes!("../web/profiles.js")),
    ("profiles.css", include_bytes!("../web/profiles.css")),
    ("health.js", include_bytes!("../web/health.js")),
    ("health.css", include_bytes!("../web/health.css")),
    ("reloads.js", include_bytes!("../web/reloads.js")),
    ("reloads.css", include_bytes!("../web/reloads.css")),
    ("translate.js", include_bytes!("../web/translate.js")),
    ("translate.css", include_bytes!("../web/translate.css")),
];

/// The `?v=` of the module and the stylesheet: the kit serves a year of
/// `immutable` for a versioned URL, so the version has to move when the
/// bytes do, not only when the package version does.
pub(crate) fn asset_version() -> String {
    let mut hasher = DefaultHasher::new();
    for (name, bytes) in WEB_FILES {
        name.hash(&mut hasher);
        bytes.hash(&mut hasher);
    }
    format!("{:x}", hasher.finish())
}

/// What a profile looks like to the browser. Names, counters and shapes;
/// nothing that could carry a secret.
#[derive(Debug, Serialize)]
pub struct ProfileView {
    pub name: String,
    /// `starting`, `working`, `failing`, `denied` or `hub-down`.
    pub state: &'static str,
    pub received: u64,
    pub delivered: u64,
    pub failed: u64,
    /// Mean delivery time; `None` before the first message.
    pub avg_ms: Option<u64>,
    pub last_success_unix: Option<u64>,
    pub source: EndView,
    pub sink: EndView,
    pub content_type: String,
    /// A short label for the template (its content type and field names,
    /// never the body): the hover card on a profile name shows it.
    pub template: String,
    pub timeout_ms: u64,
    pub retries: u32,
    /// The pauses between tries, in order.
    pub pauses_ms: Vec<u64>,
    /// Worst-case time one delivery may take.
    pub budget_ms: u64,
    /// Only a kyu source holds a lease and a hand-out limit.
    pub lease_ms: Option<u64>,
    pub max_attempts: Option<u32>,
}

/// One end of a profile.
#[derive(Debug, Serialize)]
pub struct EndView {
    /// `kyu` or `http` for a source; `kyu` or `url` for a destination.
    pub kind: &'static str,
    /// The words on a card or a node: `kyu topic alerts.raw`,
    /// `POST /uptime-kuma`, `homeassistant.lan:8123`.
    pub label: String,
    /// A second line: `subscription switchboard`, `HTTP, no hub`.
    pub sub: String,
    /// A source's path (HTTP) or topic (kyu); a destination's topic.
    /// Never a destination's path.
    pub path: Option<String>,
}

/// The host and port of a URL, without scheme, user info, path or query.
/// The path of a Home Assistant webhook is the credential (S2).
pub fn host_of(url: &str) -> String {
    let rest = url.split_once("://").map_or(url, |(_, rest)| rest);
    let authority = rest.split(['/', '?', '#']).next().unwrap_or(rest);
    authority
        .rsplit_once('@')
        .map_or(authority, |(_, host)| host)
        .to_string()
}

fn state_word(health: Option<Health>) -> &'static str {
    match health {
        None | Some(Health::Starting) => "starting",
        Some(Health::Working) => "working",
        Some(Health::Failing) => "failing",
        Some(Health::Denied) => "denied",
        Some(Health::HubDown) => "hub-down",
    }
}

fn view_of(profile: &Profile, stats: Option<&ProfileStats>) -> ProfileView {
    let kyu_source = matches!(profile.source, Source::Kyu { .. });
    let received = stats.map_or(0, |s| s.received);
    let source = match &profile.source {
        Source::Http { path } => EndView {
            kind: "http",
            label: format!("POST {path}"),
            sub: "HTTP, no hub".to_string(),
            path: Some(path.clone()),
        },
        Source::Kyu { topic } => EndView {
            kind: "kyu",
            label: format!("kyu topic {topic}"),
            sub: format!("subscription {}", profile.subscription),
            path: Some(topic.clone()),
        },
    };
    let sink = match &profile.sink {
        Sink::Url { url, .. } => EndView {
            kind: "url",
            label: host_of(url),
            sub: "path hidden".to_string(),
            path: None,
        },
        Sink::Kyu { topic } => EndView {
            kind: "kyu",
            label: format!("kyu topic {topic}"),
            sub: "published to the hub".to_string(),
            path: Some(topic.clone()),
        },
    };
    ProfileView {
        name: profile.name.clone(),
        state: state_word(stats.and_then(|s| s.health)),
        received,
        delivered: stats.map_or(0, |s| s.delivered),
        failed: stats.map_or(0, |s| s.failed),
        avg_ms: stats
            .filter(|_| received > 0)
            .map(|s| s.duration_ms_total / received),
        last_success_unix: stats.and_then(|s| s.last_success_unix),
        source,
        sink,
        content_type: profile.content_type.clone(),
        template: crate::dashboard_profiles::template_label(&profile.body, &profile.content_type),
        timeout_ms: profile.timeout_ms,
        retries: profile.retries,
        pauses_ms: profile.pauses_ms(),
        budget_ms: profile.retry_budget_ms(),
        lease_ms: kyu_source.then_some(profile.lease_ms),
        max_attempts: kyu_source.then_some(profile.max_attempts),
    }
}

pub struct Profiles {
    registry: Arc<Registry>,
    store: Arc<ProfileStore>,
    /// Profile names in config order. The set never changes while the
    /// service runs; a destination can (feat-reload-1), so each snapshot
    /// reads the profile from the store.
    order: Vec<String>,
}

impl Profiles {
    pub fn new(config: &Config, store: Arc<ProfileStore>, registry: Arc<Registry>) -> Self {
        let order = config.profiles.iter().map(|p| p.name.clone()).collect();
        Self {
            registry,
            store,
            order,
        }
    }

    /// The profiles as they stand now (the translate page renders from them).
    pub(crate) fn store(&self) -> &Arc<ProfileStore> {
        &self.store
    }

    /// Every profile in config order, as the browser sees it.
    pub fn snapshot(&self) -> Vec<ProfileView> {
        let stats = self.registry.snapshot();
        self.order
            .iter()
            .filter_map(|name| self.store.get(name))
            .map(|p| view_of(&p, stats.get(&p.name)))
            .collect()
    }

    /// The JSON that is both the first embedded snapshot and the poll's
    /// answer.
    fn snapshot_json(&self) -> String {
        serde_json::json!({ "profiles": self.snapshot() }).to_string()
    }

    fn routes(self: &Arc<Self>) -> Router {
        Router::new()
            .route(SNAPSHOT_ROUTE, get(snapshot))
            .route(RECHECK_ROUTE, post(recheck))
            .route(RECHECK_ONE_ROUTE, post(recheck_one))
            .with_state(Arc::clone(self))
    }
}

/// Register everything this service puts on the dashboard: the four
/// status sections, their routes behind the admin login, and the module
/// that draws them. `main.rs` and the door tests call this one function,
/// so what runs is what is tested.
pub fn install(app: &mut chassis::App, profiles: Arc<Profiles>) {
    // First, so what needs a person stands above everything else.
    app.status_section(crate::dashboard_health::HealthSection);
    app.status_section(ProfilesSection(Arc::clone(&profiles)));
    app.status_section(TraceSection);
    app.status_section(SettingsSection);
    app.status_section(RecheckSection(Arc::clone(&profiles)));
    // The last 24 hours: key figures, uptime strips, refresh chooser.
    crate::dashboard_history::install(app, Arc::clone(&profiles.registry), profiles.order.clone());
    app.dashboard_routes(profiles.routes());
    crate::dashboard_reloads::install(app, Arc::clone(&profiles.store));
    crate::dashboard_translate::install(app, Arc::clone(&profiles));
    app.webapp(WebApp::embedded(WEB_FILES).at(WEB_MOUNT));
}

// ── routes ─────────────────────────────────────────────────────────────

fn json(status: StatusCode, body: String) -> impl IntoResponse {
    (status, [("content-type", "application/json")], body)
}

async fn snapshot(State(profiles): State<Arc<Profiles>>) -> impl IntoResponse {
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

/// "Recheck this profile": the same reset for one profile.
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

// ── sections ───────────────────────────────────────────────────────────

fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// JSON that is safe inside a `<script type="application/json">`: a `<`
/// can never close the element, whatever a profile is called.
pub(crate) fn script_safe(json: &str) -> String {
    json.replace('<', "\\u003c")
}

struct ProfilesSection(Arc<Profiles>);

impl StatusSection for ProfilesSection {
    fn render(&self) -> Section {
        let version = asset_version();
        let data = serde_json::json!({
            "profiles": self.0.snapshot(),
            "routes": {
                "snapshot": SNAPSHOT_ROUTE,
                "recheckAll": RECHECK_ROUTE,
                "recheckOne": RECHECK_ONE_ROUTE,
            },
        })
        .to_string();
        let plain: String = self
            .0
            .snapshot()
            .iter()
            .map(|p| {
                format!(
                    "<li>{}: {} ({} delivered, {} failed)</li>",
                    esc(&p.name),
                    p.state,
                    p.delivered,
                    p.failed
                )
            })
            .collect();
        let html = format!(
            r##"<link rel="stylesheet" href="{WEB_MOUNT}/dashboard.css?v={version}">
<link rel="stylesheet" href="{WEB_MOUNT}/profiles.css?v={version}">
<script type="application/json" id="hsb-data">{data}</script>
<script type="module" src="{WEB_MOUNT}/dashboard.js?v={version}"></script>
<section class="kp-card hsb-area" id="hsb-area" data-kp-state="ready" data-kp-loading-words="Reading the profiles…" aria-label="Profiles">
  <div class="hsb-controls" role="group" aria-label="View" id="hsb-view">
    <b>View</b>
    <button type="button" class="kp-button kp-button--sm" aria-pressed="true" data-hsb-view="cards">Cards</button>
    <button type="button" class="kp-button kp-button--sm" aria-pressed="false" data-hsb-view="map">Map</button>
  </div>
  <div class="kp-card__stage">
    <div class="kp-card__data">
      <div class="hsb-frame hsb-frame--profiles">
        <div class="kp-entity-grid" data-kp-entity-grid data-kp-entity-min="22rem" data-kp-entity-meters="1" id="hsb-grid"></div>
        <div class="kp-matrix" data-kp-matrix data-kp-view="lines" data-kp-matrix-owned data-kp-remember="hsb-profiles" aria-label="Which source feeds which destination" id="hsb-map" hidden></div>
      </div>
    </div>
    <div class="kp-card__state" hidden></div>
  </div>
</section>
<noscript><ul>{plain}</ul><p>The cards, the map and the buttons need JavaScript.</p></noscript>"##,
            data = script_safe(&data),
        );
        Section {
            title: "Profiles".to_string(),
            explain:
                "One profile is one source, one translation and one destination. \
                      A profile is 'starting' until a message has gone through it, and \
                      goes back to 'working' the moment one does. The page refreshes \
                      itself as often as the refresh chooser says (every 30 seconds to start with)."
                    .to_string(),
            rows: Vec::new(),
            html: Some(html),
        }
    }
}

struct TraceSection;

impl StatusSection for TraceSection {
    fn render(&self) -> Section {
        Section {
            title: "Path of a profile".to_string(),
            explain: "Follows one message from its sender to the destination and stops at \
                      the first hop that refuses it, saying which one and why."
                .to_string(),
            rows: Vec::new(),
            html: Some(
                r#"<div class="hsb-frame hsb-trace">
  <div class="hsb-controls" role="group" aria-label="Profile" id="hsb-trace-pick" hidden></div>
  <div class="hsb-trace__head"><b id="hsb-trace-name"></b><span class="kp-badge" id="hsb-trace-badge"></span></div>
  <div class="kp-trace" data-kp-trace data-kp-trace-copy id="hsb-trace"></div>
</div>"#
                    .to_string(),
            ),
        }
    }
}

struct SettingsSection;

impl StatusSection for SettingsSection {
    fn render(&self) -> Section {
        Section {
            title: "Settings and retry budget".to_string(),
            explain: "What each profile does in plain rows, and one bar for how long its \
                      retries can take compared to the time the hub holds a message for it \
                      (the lease). The destination is shown as host and port; its path is a \
                      credential and is never shown."
                .to_string(),
            rows: Vec::new(),
            html: Some(r#"<div class="hsb-stage" id="hsb-settings"></div>"#.to_string()),
        }
    }
}

struct RecheckSection(Arc<Profiles>);

impl StatusSection for RecheckSection {
    fn render(&self) -> Section {
        let rows: String = self
            .0
            .order
            .iter()
            .map(|name| {
                let n = esc(name);
                format!(
                    r#"<tr data-kp-row-key="{n}" data-hsb-row="{n}">
  <td data-label="Profile"><b data-kp-hovercard="" data-hsb-profile="{n}" tabindex="0">{n}</b></td>
  <td data-label="State"><span class="kp-badge" data-hsb-state></span></td>
  <td data-label="Retry budget" data-kp-sort-value="0" data-hsb-budget></td>
  <td data-label="Actions" class="hsb-col-act">
    <div class="kp-menu-button" data-kp-menu-button data-kp-key="hsb-menu-{n}">
      <button type="button" class="kp-button kp-button--sm" aria-label="Actions for {n}">Actions</button>
      <div class="kp-menu kp-menu--rich" role="menu" aria-label="Actions for {n}" hidden></div>
    </div>
  </td>
</tr>"#
                )
            })
            .collect();
        let html = format!(
            r##"<div class="hsb-bar">
  <button type="button" class="kp-button" id="hsb-recheck-all" data-kp-flash-button data-kp-busy-label="Rechecking…" data-kp-flash-word="Done" data-kp-flash-words="Done,Refused" data-kp-refusal="line" data-kp-flash-line="#hsb-line">Recheck profiles</button>
  <p class="hsb-line" id="hsb-line" hidden></p>
</div>
<div class="kp-datatable" data-kp-datatable data-kp-key="hsb-profiles">
  <div class="kp-table-wrap">
    <table class="kp-table hsb-table">
      <caption class="kp-sr-only">Profiles</caption>
      <thead><tr>
        <th scope="col" data-kp-sort="text">Profile</th>
        <th scope="col" data-kp-sort="text">State</th>
        <th scope="col" data-kp-sort="number">Retry budget</th>
        <th scope="col" class="hsb-col-act">Actions</th>
      </tr></thead>
      <tbody>{rows}</tbody>
    </table>
  </div>
</div>"##
        );
        Section {
            title: "Recheck".to_string(),
            explain: "Puts every profile, or just one, back to 'starting' so the service \
                      tries its destination again. A profile's health only changes when a \
                      message goes through it, so one that failed once stays red until a \
                      sender posts again; this is the way out."
                .to_string(),
            rows: Vec::new(),
            html: Some(html),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_destination_is_host_and_port_only() {
        assert_eq!(
            host_of("http://homeassistant.lan:8123/api/webhook/SECRET-ID"),
            "homeassistant.lan:8123"
        );
        assert_eq!(host_of("https://user:pw@ha.lan/x?token=1"), "ha.lan");
        assert_eq!(host_of("ha.lan/api/webhook/x"), "ha.lan");
    }

    #[test]
    fn json_in_a_script_cannot_close_it() {
        assert!(!script_safe(r#"{"name":"</script><b>"}"#).contains('<'));
    }
}
