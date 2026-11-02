//! The door's half of the refusal streak. It has no queue to protect, so
//! after `REFUSAL_STREAK_LIMIT` permanent refusals in a row it only turns
//! the profile failing (with the usual transition event) and keeps
//! answering every request.

use super::Inbound;
use crate::config::Profile;
use crate::pump::streak::REFUSAL_DETAIL;
use crate::pump::Health;

/// The destination refused a message for good: extend the run, and at the
/// limit make the profile failing.
pub(super) async fn refused(state: &Inbound, profile: &Profile) {
    if !state.registry.note_door_refusal(&profile.name) {
        return;
    }
    let before = state.registry.swap_health(&profile.name, Health::Failing);
    crate::app::announce_transition(
        state.sink.as_ref(),
        state.reporting.as_deref(),
        &profile.name,
        before,
        Health::Failing,
        REFUSAL_DETAIL,
    )
    .await;
}
