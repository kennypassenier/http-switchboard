//! The "Recheck" status section: a button for all profiles and a table
//! with a row menu for each.

use std::sync::Arc;

use chassis::{Section, StatusSection};

use super::escape::esc;
use super::profiles::Profiles;

pub(super) struct RecheckSection(pub(super) Arc<Profiles>);

impl StatusSection for RecheckSection {
    fn render(&self) -> Section {
        let rows: String = self.0.order.iter().map(|name| row(&esc(name))).collect();
        let html = markup(&rows);
        Section {
            title: "Recheck".to_string(),
            explain: "Puts a profile, or all of them, back to 'starting' so the next message decides again, which is the way out of a failure that stays red until a sender posts."
                .to_string(),
            rows: Vec::new(),
            html: Some(html),
        }
    }
}

/// One table row for the profile `n` (already HTML-escaped): its name, a
/// state badge and budget cell the module fills, and the row menu.
fn row(n: &str) -> String {
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
}

/// The section: the "Recheck profiles" button and the table of `rows`.
fn markup(rows: &str) -> String {
    format!(
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
      <div class="kp-empty" data-kp-datatable-empty hidden>
        <p class="kp-empty__title">No profiles are configured</p>
        <p class="kp-empty__body">A profile says where messages come from and where they go. Add one to the configuration and reload.</p>
      </div>
    </div>"##
    )
}
