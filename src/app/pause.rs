//! The pause of a kyu pump whose destination refuses every message: when
//! it starts, when it ends, and how long a paused pump rests between looks.

use crate::obs::{self, Registry};
use crate::pump::streak::{REFUSAL_DETAIL, REFUSAL_STREAK_LIMIT};
use crate::pump::{Health, PumpState};

/// How long a paused pump rests before it looks for a recheck again. It
/// claims nothing meanwhile, so this only bounds how late a recheck lands.
pub(super) const PAUSED_POLL_MS: u64 = 1_000;

/// Publish and log a pause that the last step began.
pub(super) fn announce_pause(
    registry: &Registry,
    profile: &str,
    state: &PumpState,
    was_paused: bool,
) {
    if state.paused && !was_paused {
        registry.set_paused(profile, true);
        obs::log_warn(
            profile,
            "pump_paused",
            &format!(
                "paused after {REFUSAL_STREAK_LIMIT} refused messages in a row: {REFUSAL_DETAIL}. \
                 What now: fix the destination, then recheck the profile; the rest of the \
                 messages wait on the hub."
            ),
        );
    }
}

/// A recheck puts the registry's health back to `Starting` while the pump
/// still remembers something else: forget the streak and resume.
pub(super) fn resume_after_recheck(registry: &Registry, name: &str, state: &mut PumpState) {
    if registry.health_of(name) == Some(Health::Starting) && state.health != Health::Starting {
        resume(name, state, "recheck");
    }
}

/// Start claiming again, logging once if the pump had been paused.
pub(super) fn resume(name: &str, state: &mut PumpState, why: &str) {
    if state.resume() {
        obs::log_info(
            name,
            "pump_resumed",
            &format!("claiming messages again after a {why}"),
        );
    }
}
