//! The top of the status page: what needs a person, said before anything
//! else (kp-themes 11: attention band, problem alerts, the kyu hub's
//! connection banner).
//!
//! This section is registered first, so it stands above the profiles. It
//! carries no data of its own: `web/health.js` reads the same snapshot the
//! profile cards poll (`dashboard.js` announces every refresh), so there is
//! one poll for the whole page, and a state that is in the band is in the
//! cards at the same moment.

use chassis::{Section, StatusSection};

use crate::dashboard::{asset_version, WEB_MOUNT};

/// The section above the profiles.
pub struct HealthSection;

impl StatusSection for HealthSection {
    fn render(&self) -> Section {
        let version = asset_version();
        let html = format!(
            r##"<link rel="stylesheet" href="{WEB_MOUNT}/health.css?v={version}">
<script type="module" src="{WEB_MOUNT}/health.js?v={version}"></script>
<div class="hsbh-frame">
  <div id="hsbh-connection" data-kp-connection data-kp-connection-source="app" data-kp-connection-grace="1500" data-kp-placement="top"></div>
  <div class="hsbh-grid">
    <div class="hsbh-attention">
      <div class="kp-attention" id="hsbh-band" role="region" aria-label="Profiles that need attention"></div>
      <div class="kp-empty hsbh-clear" id="hsbh-clear" data-kp-art="done" data-kp-art-size="sm" hidden>
        <p class="kp-empty__title">All profiles are working</p>
        <p class="kp-empty__body" id="hsbh-clear-body"></p>
      </div>
    </div>
    <div class="kp-problems hsbh-problems" id="hsbh-problems" data-kp-problems data-kp-problems-mode="each" hidden></div>
  </div>
</div>"##
        );
        Section {
            title: "Needs attention".to_string(),
            explain: "The profiles that are not well, worst first, with what is wrong, why and \
                      what to do. A profile leaves this list the moment it works again. When \
                      the kyu hub cannot be reached or refuses our token, a banner says so."
                .to_string(),
            rows: Vec::new(),
            html: Some(html),
        }
    }
}
