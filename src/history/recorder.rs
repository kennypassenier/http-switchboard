//! The recorder: where outcomes and health changes go in, and the 24-hour
//! view comes out. One per [`crate::obs::Registry`].

use std::collections::BTreeMap;
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use super::model::{Cause, Recent, Track};
use super::view::{profile_view, HistoryView};
use super::{HOURS, HOUR_SECS, PROFILE_CAP, RECENT_CAP};
use crate::pump::Health;

/// The recorder. One per [`crate::obs::Registry`].
#[derive(Debug)]
pub struct History {
    started_unix: u64,
    pub(super) tracks: Mutex<BTreeMap<String, Track>>,
}

impl Default for History {
    fn default() -> Self {
        Self::started_at(now_unix())
    }
}

fn now_unix() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

impl History {
    /// A recorder that began at `unix` seconds (tests; the service uses
    /// [`Default`]).
    pub fn started_at(unix: u64) -> Self {
        Self {
            started_unix: unix,
            tracks: Mutex::new(BTreeMap::new()),
        }
    }

    fn with_track<T>(&self, profile: &str, f: impl FnOnce(&mut Track) -> T) -> Option<T> {
        let mut tracks = self.tracks.lock().unwrap();
        if !tracks.contains_key(profile) && tracks.len() >= PROFILE_CAP {
            return None;
        }
        Some(f(tracks
            .entry(profile.to_string())
            .or_insert_with(Track::new)))
    }

    /// One message finished (delivered or not) at `at` unix seconds.
    pub fn record_at(&self, profile: &str, at: u64, delivered: bool, duration_ms: u64) {
        self.with_track(profile, |track| {
            let hour = at / HOUR_SECS;
            let bucket = track.bucket(hour);
            bucket.received = bucket.received.saturating_add(1);
            if delivered {
                bucket.delivered = bucket.delivered.saturating_add(1);
                bucket.duration_ms = bucket.duration_ms.saturating_add(duration_ms);
            } else {
                bucket.failed = bucket.failed.saturating_add(1);
            }
            if !delivered {
                track.last_failure_unix = Some(at);
            }
            if track.recent.len() == RECENT_CAP {
                track.recent.pop_front();
            }
            track.recent.push_back(Recent {
                at,
                ok: delivered,
                ms: if delivered { duration_ms } else { 0 },
            });
        });
    }

    pub fn record(&self, profile: &str, delivered: bool, duration_ms: u64) {
        self.record_at(profile, now_unix(), delivered, duration_ms);
    }

    /// The profile's health changed (or was reset) at `at`.
    pub fn note_health_at(&self, profile: &str, at: u64, health: Health) {
        self.with_track(profile, |track| {
            let cause = Cause::of(health);
            let bucket = track.bucket(at / HOUR_SECS);
            bucket.worst = bucket.worst.max(cause);
            bucket.last = cause;
            track.carry = cause;
        });
    }

    pub fn note_health(&self, profile: &str, health: Health) {
        self.note_health_at(profile, now_unix(), health);
    }

    /// The last 24 hours of every named profile, oldest hour first.
    /// Profiles never seen come back as 24 quiet hours.
    pub fn view_at(&self, now: u64, names: &[String]) -> HistoryView {
        let tracks = self.tracks.lock().unwrap();
        let current = now / HOUR_SECS;
        let first_hour = current.saturating_sub(HOURS as u64 - 1);
        let started_hour = self.started_unix / HOUR_SECS;
        let profiles = names
            .iter()
            .map(|name| {
                let empty = Track::new();
                let track = tracks.get(name).unwrap_or(&empty);
                profile_view(name, track, first_hour, current, started_hour)
            })
            .collect();
        HistoryView {
            now_unix: now,
            since_unix: self.started_unix,
            profiles,
        }
    }

    pub fn view(&self, names: &[String]) -> HistoryView {
        self.view_at(now_unix(), names)
    }
}
