//! The two status sections that are only markup: the trace of one
//! message and the settings stage. Their content is drawn client-side.

use chassis::{Section, StatusSection};

pub(super) struct TraceSection;

impl StatusSection for TraceSection {
    fn render(&self) -> Section {
        Section {
            title: "Path of a profile".to_string(),
            explain: "Follows one message from its sender to the destination and stops at \
                      the first hop that refuses it, saying which one and why. The profile \
                      chosen here is also the one Last 24 hours shows."
                .to_string(),
            rows: Vec::new(),
            html: Some(
                r#"<div class="hsb-frame hsb-trace">
  <div class="kp-formgrid">
    <div class="kp-formgrid__row" id="hsb-trace-pick-row" hidden>
      <b class="kp-formgrid__label">Profile</b>
      <div class="kp-formgrid__control"><div class="hsb-pick" id="hsb-trace-pick"></div></div>
    </div>
    <div class="kp-formgrid__row">
      <b class="kp-formgrid__label" id="hsb-trace-name"></b>
      <div class="kp-formgrid__control"><span class="kp-badge hsb-badge" id="hsb-trace-badge"></span></div>
    </div>
  </div>
  <div class="kp-trace" data-kp-trace data-kp-trace-copy id="hsb-trace"></div>
</div>"#
                    .to_string(),
            ),
        }
    }
}

pub(super) struct SettingsSection;

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
            html: Some(
                r#"<div class="hsb-stage" id="hsb-settings" data-kp-formgrid-stage></div>"#
                    .to_string(),
            ),
        }
    }
}
