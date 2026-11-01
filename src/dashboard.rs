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

mod assets;
mod escape;
mod handlers;
mod profiles;
mod profiles_section;
mod recheck_section;
mod static_sections;
#[cfg(test)]
mod tests;
mod view;

use std::sync::Arc;

use chassis::shell::webapp::WebApp;

use assets::WEB_FILES;
use profiles_section::ProfilesSection;
use recheck_section::RecheckSection;
use static_sections::{SettingsSection, TraceSection};

pub use handlers::{recheck, recheck_one};
pub use profiles::Profiles;
pub use view::{host_of, EndView, ProfileView};

pub(crate) use assets::asset_version;
pub(crate) use escape::script_safe;

/// The route "Recheck profiles" posts to. Registered with
/// `dashboard_routes`, so it is behind the admin login, not a sender's
/// token.
pub const RECHECK_ROUTE: &str = "/profiles/recheck";
/// "Recheck <name>": `{name}` is the profile's name, URL-encoded.
pub const RECHECK_ONE_ROUTE: &str = "/profiles/{name}/recheck";
/// The JSON the cards, map, trace and settings are drawn from.
pub const SNAPSHOT_ROUTE: &str = "/profiles.json";
/// Where the dashboard module and its stylesheet are served.
pub const WEB_MOUNT: &str = "/app/hsb";

/// Register everything this service puts on the dashboard: the four
/// status sections, their routes behind the admin login, and the module
/// that draws them. `main.rs` and the door tests call this one function,
/// so what runs is what is tested.
pub fn install(app: &mut chassis::App, profiles: Arc<Profiles>) {
    // First, so what needs a person stands above everything else.
    app.status_section(crate::dashboard_health::HealthSection);
    app.status_section(ProfilesSection(Arc::clone(&profiles)));
    // The per-profile sections stand together under the one profile
    // choice in Path: the path, then the last 24 hours (key figures and
    // uptime strips) of the same profile.
    app.status_section(TraceSection);
    crate::dashboard_history::install(app, Arc::clone(&profiles.registry), profiles.order.clone());
    app.status_section(SettingsSection);
    app.status_section(RecheckSection(Arc::clone(&profiles)));
    app.dashboard_routes(profiles.routes());
    crate::dashboard_reloads::install(app, Arc::clone(profiles.store()));
    crate::dashboard_translate::install(app, Arc::clone(&profiles));
    app.webapp(WebApp::embedded(WEB_FILES).at(WEB_MOUNT));
}
