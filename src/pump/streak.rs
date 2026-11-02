//! The refusal streak: how many messages in a row the destination has
//! permanently refused. A destination that refuses everything (a wrong
//! path, a template that no longer fits) would otherwise drain a whole
//! queue into dead letters while the profile stays green. After
//! [`REFUSAL_STREAK_LIMIT`] refusals in a row the profile is failing and a
//! kyu pump stops claiming messages, so the rest wait on the hub.
//!
//! Only destination refusals count. A message that is not JSON, is too
//! large or does not render is a fact about that message, not about the
//! route, so it neither extends nor breaks the streak. A temporary failure
//! (it is handed back, not given up) is neutral for the same reason: it
//! says nothing about whether the destination would accept the message.

/// How many permanent refusals in a row make a profile failing (and pause
/// a kyu pump).
pub const REFUSAL_STREAK_LIMIT: u32 = 3;

use super::types::{Health, PumpState};

/// The one plain line that says why. Fixed words: no path, header or
/// payload can reach a snapshot, a log line or a self-report through it.
pub const REFUSAL_DETAIL: &str = "the destination refuses every message";

/// A run of consecutive destination refusals, and whether it has reached
/// the limit.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RefusalStreak {
    count: u32,
}

impl RefusalStreak {
    /// One more refusal. True once the run has reached the limit (and for
    /// every further refusal, so the door keeps saying failing).
    pub fn refused(&mut self) -> bool {
        self.count = self.count.saturating_add(1);
        self.at_limit()
    }

    /// A delivery got through: the destination accepts messages again.
    pub fn delivered(&mut self) {
        self.count = 0;
    }

    /// Whether the run has reached the limit.
    pub fn at_limit(&self) -> bool {
        self.count >= REFUSAL_STREAK_LIMIT
    }
}

/// The destination permanently refused the message just given up on. At
/// the limit the profile is failing and the pump pauses: this message
/// stays dead-lettered (it was already refused), the rest wait on the hub.
pub(super) fn note_refusal(state: &mut PumpState) {
    if state.refusals.refused() {
        state.health = Health::Failing;
        state.paused = true;
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_limit_is_reached_on_the_third_refusal_in_a_row() {
        let mut streak = RefusalStreak::default();
        assert!(!streak.refused());
        assert!(!streak.refused());
        assert!(streak.refused());
    }

    #[test]
    fn a_delivery_starts_the_count_again() {
        let mut streak = RefusalStreak::default();
        streak.refused();
        streak.refused();
        streak.delivered();
        assert!(!streak.refused());
        assert!(!streak.at_limit());
    }
}
