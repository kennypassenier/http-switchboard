//! The registry's side of the refusal streak (see `pump/streak.rs`): the
//! pause flag the dashboard shows, and the door's own streak. The door has
//! no queue to protect, so it only goes failing; a kyu pump keeps its
//! streak in its own state and publishes just the flag here.

use super::registry::ProfileStats;
use super::Registry;

impl Registry {
    /// A kyu pump paused (or resumed): shown as `paused` in the snapshot.
    pub fn set_paused(&self, profile: &str, paused: bool) {
        self.profiles
            .lock()
            .unwrap()
            .entry(profile.to_string())
            .or_default()
            .paused = paused;
    }

    /// The inbound door gave up on a message because the destination
    /// refused it for good. True when this is the refusal that reaches the
    /// limit (or any later one in the same run).
    pub fn note_door_refusal(&self, profile: &str) -> bool {
        let mut guard = self.profiles.lock().unwrap();
        guard
            .entry(profile.to_string())
            .or_default()
            .door_refusals
            .refused()
    }

    /// The inbound door delivered a message: the run of refusals is over.
    pub fn note_door_delivered(&self, profile: &str) {
        let mut guard = self.profiles.lock().unwrap();
        guard
            .entry(profile.to_string())
            .or_default()
            .door_refusals
            .delivered();
    }
}

/// A recheck forgets the run and lifts the pause; the pump notices the
/// reset health and starts claiming again.
pub(super) fn clear(stats: &mut ProfileStats) {
    stats.paused = false;
    stats.door_refusals.delivered();
}
