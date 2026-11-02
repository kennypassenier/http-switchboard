//! The registry: the counters and remembered health every endpoint
//! answers from. Counters only; no payloads, no header values, nothing
//! that could carry a secret into a scrape.

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use crate::pump::Health;

#[derive(Debug, Default, Clone)]
pub struct ProfileStats {
    pub received: u64,
    pub delivered: u64,
    pub failed: u64,
    pub duration_ms_total: u64,
    pub last_success_unix: Option<u64>,
    pub health: Option<Health>,
}

/// Everything the endpoints answer from. Counters only — no payloads, no
/// header values, nothing that could carry a secret into a scrape.
#[derive(Debug, Default)]
pub struct Registry {
    pub(super) profiles: Mutex<BTreeMap<String, ProfileStats>>,
    /// feat-reload-1: reloads per outcome (`applied`, `needs-restart`,
    /// `invalid`), so a reload that did nothing is visible in a scrape.
    pub(super) reloads: Mutex<BTreeMap<&'static str, u64>>,
    /// The last 24 hours per profile, in memory (feat-dash-kp11-30).
    pub history: crate::history::History,
}

impl Registry {
    pub fn new() -> Self {
        Self::default()
    }

    /// Make a profile visible before it has done anything, so a service
    /// that is failing from the first second still lists it.
    pub fn register(&self, profile: &str) {
        self.profiles
            .lock()
            .unwrap()
            .entry(profile.to_string())
            .or_default();
    }

    pub fn record_reload(&self, outcome: &'static str) {
        *self.reloads.lock().unwrap().entry(outcome).or_default() += 1;
    }

    pub fn record(&self, profile: &str, delivered: bool, duration_ms: u64) {
        let mut guard = self.profiles.lock().unwrap();
        let stats = guard.entry(profile.to_string()).or_default();
        stats.received += 1;
        stats.duration_ms_total += duration_ms;
        if delivered {
            stats.delivered += 1;
            stats.last_success_unix = Some(now_unix());
        } else {
            stats.failed += 1;
        }
        drop(guard);
        self.history.record(profile, delivered, duration_ms);
    }

    pub fn set_health(&self, profile: &str, health: Health) {
        self.history.note_health(profile, health);
        self.profiles
            .lock()
            .unwrap()
            .entry(profile.to_string())
            .or_default()
            .health = Some(health);
    }

    /// Record `health` and return what it replaced, in one locked step.
    /// A caller that reads the old value, awaits something and then writes
    /// can lose a transition to a concurrent request; this cannot. Nothing
    /// remembered reads as `Starting`.
    pub fn swap_health(&self, profile: &str, health: Health) -> Health {
        let previous = {
            let mut guard = self.profiles.lock().unwrap();
            let stats = guard.entry(profile.to_string()).or_default();
            stats.health.replace(health)
        };
        self.history.note_health(profile, health);
        previous.unwrap_or(Health::Starting)
    }

    /// Put every profile back to `Starting` (H1/V1, 3.0.0): the dashboard's
    /// "Recheck profiles" button.
    ///
    /// Health only ever changes when a message goes through, so a profile
    /// whose source is an HTTP path stays `Failing` for as long as nobody
    /// posts to it — long after the receiver that caused it came back.
    /// `Starting` is the honest state for "nothing has been tried since",
    /// and it is the one state `/healthz?strict=1` does not alarm on.
    /// Returns how many profiles were reset.
    pub fn reset_health(&self) -> usize {
        let mut profiles = self.profiles.lock().unwrap();
        for (name, stats) in profiles.iter_mut() {
            stats.health = Some(Health::Starting);
            self.history.note_health(name, Health::Starting);
        }
        profiles.len()
    }

    /// Put one profile back to `Starting` (the entity card's and the row
    /// menu's "Recheck <name>"). `false` when no such profile is
    /// registered, so the route can answer 404 instead of inventing one.
    pub fn reset_health_of(&self, profile: &str) -> bool {
        match self.profiles.lock().unwrap().get_mut(profile) {
            Some(stats) => {
                stats.health = Some(Health::Starting);
                self.history.note_health(profile, Health::Starting);
                true
            }
            None => false,
        }
    }

    /// The state a profile is in right now, so a caller can tell a
    /// transition from a repeat.
    pub fn health_of(&self, profile: &str) -> Option<Health> {
        self.profiles
            .lock()
            .unwrap()
            .get(profile)
            .and_then(|s| s.health)
    }

    pub fn snapshot(&self) -> BTreeMap<String, ProfileStats> {
        self.profiles.lock().unwrap().clone()
    }
}

pub(super) fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0)
}
