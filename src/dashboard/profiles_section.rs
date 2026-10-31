//! The "Profiles" status section: cards or map, drawn client-side from
//! the first snapshot embedded in the page.

use std::sync::Arc;

use chassis::{Section, StatusSection};

use super::assets::asset_version;
use super::escape::{esc, script_safe};
use super::profiles::Profiles;
use super::view::ProfileView;
use super::{RECHECK_ONE_ROUTE, RECHECK_ROUTE, SNAPSHOT_ROUTE, WEB_MOUNT};

pub(super) struct ProfilesSection(pub(super) Arc<Profiles>);

impl StatusSection for ProfilesSection {
    fn render(&self) -> Section {
        // One snapshot for both the embedded data and the no-JS list, so
        // the two cannot disagree.
        let profiles = self.0.snapshot();
        let data = serde_json::json!({
            "profiles": profiles,
            "routes": {
                "snapshot": SNAPSHOT_ROUTE,
                "recheckAll": RECHECK_ROUTE,
                "recheckOne": RECHECK_ONE_ROUTE,
            },
        })
        .to_string();
        let html = markup(&data, &no_script_list(&profiles));
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

/// The list shown when JavaScript is off: a name, a state and counters.
fn no_script_list(profiles: &[ProfileView]) -> String {
    profiles
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
        .collect()
}

/// The section's HTML: stylesheets, the embedded snapshot, the module tag
/// and the empty frames the module fills.
fn markup(data: &str, plain: &str) -> String {
    let version = asset_version();
    format!(
        r##"<link rel="stylesheet" href="{WEB_MOUNT}/dashboard.css?v={version}">
<link rel="stylesheet" href="{WEB_MOUNT}/profiles.css?v={version}">
<script type="application/json" id="hsb-data">{data}</script>
<script type="module" src="{WEB_MOUNT}/dashboard.js?v={version}"></script>
<section class="kp-card hsb-area" id="hsb-area" data-chassis-owned data-kp-state="loading" data-kp-loading-words="Reading the profiles…" aria-label="Profiles">
  <div class="kp-formgrid">
    <div class="kp-formgrid__row">
      <b class="kp-formgrid__label" id="hsb-view-l">View</b>
      <div class="kp-formgrid__control">
        <div class="hsb-choices" role="group" aria-labelledby="hsb-view-l" id="hsb-view">
          <button type="button" class="kp-button kp-button--sm" aria-pressed="true" data-hsb-view="cards">Cards</button>
          <button type="button" class="kp-button kp-button--sm" aria-pressed="false" data-hsb-view="map">Map</button>
        </div>
      </div>
    </div>
  </div>
  <div class="kp-card__stage">
    <div class="kp-card__data">
      <div class="hsb-frame hsb-frame--profiles">
        <div class="kp-entity-grid" data-kp-entity-grid data-kp-entity-min="22rem" data-kp-entity-meters="1" id="hsb-grid"></div>
        <div class="kp-matrix" data-kp-matrix data-kp-view="lines" data-kp-matrix-owned data-kp-remember="hsb-profiles" aria-label="Which source feeds which destination" id="hsb-map" hidden></div>
      </div>
    </div>
  </div>
</section>
<noscript><ul>{plain}</ul><p>The cards, the map and the buttons need JavaScript.</p></noscript>"##,
        data = script_safe(data),
    )
}
