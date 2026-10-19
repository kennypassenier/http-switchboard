//! Per-hour history of what each profile did, for the last 24 hours
//! (feat-dash-kp11-30).
//!
//! The registry keeps counters since the start; a counter cannot say *when*
//! something happened, so "was it working at 02:00" had no answer. This
//! module keeps, per profile and per clock hour, how many messages were
//! received, delivered and failed, the time the deliveries took, and the
//! worst health the profile showed in that hour.
//!
//! **In memory only, and lost on restart.** Nothing is written to disk: a
//! history file is one more thing that grows and one more thing to repair,
//! and the page says plainly "History since the last start, at most
//! 24 hours". Hours before the service started are reported as `none`.
//!
//! **Fixed size, whatever happens.** 24 hour slots and [`RECENT_CAP`]
//! recent outcomes per profile, at most [`PROFILE_CAP`] profiles; a slot is
//! reused when its hour comes round again. Nothing here holds a payload, a
//! header, a URL or an error text (the text of a failed delivery can carry
//! a webhook path, and a webhook path is the credential, S2): only counts,
//! times and a short fixed cause word.
//!
//! Health model per hour (the same words the page draws):
//!
//! * `up`: delivered what came in, or nothing came in and nothing was wrong;
//! * `degraded`: some deliveries failed, or the profile was `failing`;
//! * `down`: every delivery failed, or the hub refused the credentials
//!   (`denied`) or could not be reached (`hub-down`);
//! * `none`: before the service started recording.
//!
//! An hour without traffic takes the health the profile had when the hour
//! began, so a profile denied for six hours reads red for six hours even
//! though nobody posted.

use std::collections::{BTreeMap, VecDeque};
use std::sync::Mutex;
use std::time::{SystemTime, UNIX_EPOCH};

use serde::Serialize;

use crate::pump::Health;

/// Hours kept, and cells drawn.
pub const HOURS: usize = 24;
/// Recent outcomes kept per profile.
pub const RECENT_CAP: usize = 50;
/// Profiles tracked. The set comes from the config, so this is a ceiling
/// that is never reached in practice, not a limit anyone meets.
pub const PROFILE_CAP: usize = 64;
const HOUR_SECS: u64 = 3600;

/// What went wrong in an hour, ordered by severity so `max` finds the
/// worst. The order is part of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Cause {
    Ok,
    Failing,
    Denied,
    HubDown,
}

impl Cause {
    fn of(health: Health) -> Self {
        match health {
            Health::Starting | Health::Working => Cause::Ok,
            Health::Failing => Cause::Failing,
            Health::Denied => Cause::Denied,
            Health::HubDown => Cause::HubDown,
        }
    }
}

#[derive(Debug, Clone, Copy)]
struct Bucket {
    /// Hours since the epoch.
    hour: u64,
    received: u32,
    delivered: u32,
    failed: u32,
    /// Time spent on the deliveries that succeeded.
    duration_ms: u64,
    /// The health when the hour began, the worst during it, and the last.
    first: Cause,
    worst: Cause,
    last: Cause,
}

#[derive(Debug, Clone, Copy)]
struct Recent {
    at: u64,
    ok: bool,
    ms: u64,
}

#[derive(Debug)]
struct Track {
    buckets: [Option<Bucket>; HOURS],
    recent: VecDeque<Recent>,
    last_failure_unix: Option<u64>,
    /// The health right now; what a new hour starts with.
    carry: Cause,
}

impl Track {
    fn new() -> Self {
        Self {
            buckets: [None; HOURS],
            recent: VecDeque::with_capacity(RECENT_CAP),
            last_failure_unix: None,
            carry: Cause::Ok,
        }
    }

    fn bucket(&mut self, hour: u64) -> &mut Bucket {
        let slot = &mut self.buckets[(hour % HOURS as u64) as usize];
        if slot.map(|b| b.hour) != Some(hour) {
            *slot = Some(Bucket {
                hour,
                received: 0,
                delivered: 0,
                failed: 0,
                duration_ms: 0,
                first: self.carry,
                worst: self.carry,
                last: self.carry,
            });
        }
        slot.as_mut().expect("the slot was just filled")
    }
}

/// The recorder. One per [`crate::obs::Registry`].
#[derive(Debug)]
pub struct History {
    started_unix: u64,
    tracks: Mutex<BTreeMap<String, Track>>,
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

fn profile_view(
    name: &str,
    track: &Track,
    first_hour: u64,
    current: u64,
    started_hour: u64,
) -> ProfileHistory {
    // What the profile was before the first hour with traffic: the first
    // such hour's opening health, else its health now.
    let mut running = (first_hour..=current)
        .filter_map(|h| track.buckets[(h % HOURS as u64) as usize].filter(|b| b.hour == h))
        .map(|b| b.first)
        .next()
        .unwrap_or(track.carry);
    let hours = (first_hour..=current)
        .map(|h| {
            let bucket = track.buckets[(h % HOURS as u64) as usize].filter(|b| b.hour == h);
            let start_unix = h * HOUR_SECS;
            if h < started_hour {
                return HourView::none(start_unix);
            }
            match bucket {
                Some(b) => {
                    running = b.last;
                    let (state, cause) = judge(b.received, b.delivered, b.failed, b.worst);
                    HourView {
                        start_unix,
                        received: b.received,
                        delivered: b.delivered,
                        failed: b.failed,
                        duration_ms: b.duration_ms,
                        state,
                        cause,
                    }
                }
                None => {
                    let (state, cause) = judge(0, 0, 0, running);
                    HourView {
                        start_unix,
                        received: 0,
                        delivered: 0,
                        failed: 0,
                        duration_ms: 0,
                        state,
                        cause,
                    }
                }
            }
        })
        .collect();
    ProfileHistory {
        name: name.to_string(),
        hours,
        recent: track
            .recent
            .iter()
            .rev()
            .map(|r| RecentView {
                at_unix: r.at,
                ok: r.ok,
                ms: r.ok.then_some(r.ms),
            })
            .collect(),
        last_failure_unix: track.last_failure_unix,
    }
}

/// An hour's state and the short cause word the page explains.
fn judge(
    received: u32,
    delivered: u32,
    failed: u32,
    worst: Cause,
) -> (&'static str, Option<&'static str>) {
    let by_health = match worst {
        Cause::Ok => None,
        Cause::Failing => Some(("degraded", "failing")),
        Cause::Denied => Some(("down", "denied")),
        Cause::HubDown => Some(("down", "hub-down")),
    };
    let by_count = if failed == 0 {
        None
    } else if delivered == 0 && received > 0 {
        Some(("down", "all-failed"))
    } else {
        Some(("degraded", "failing"))
    };
    match (by_health, by_count) {
        (Some(("down", c)), _) => ("down", Some(c)),
        (_, Some(("down", c))) => ("down", Some(c)),
        (Some((s, c)), _) | (None, Some((s, c))) => (s, Some(c)),
        (None, None) => ("up", None),
    }
}

#[derive(Debug, Serialize)]
pub struct HistoryView {
    pub now_unix: u64,
    /// When this recording began: nothing earlier is known.
    pub since_unix: u64,
    pub profiles: Vec<ProfileHistory>,
}

#[derive(Debug, Serialize)]
pub struct ProfileHistory {
    pub name: String,
    /// Oldest hour first, [`HOURS`] entries.
    pub hours: Vec<HourView>,
    /// Newest first, at most [`RECENT_CAP`].
    pub recent: Vec<RecentView>,
    pub last_failure_unix: Option<u64>,
}

#[derive(Debug, Serialize)]
pub struct HourView {
    pub start_unix: u64,
    pub received: u32,
    pub delivered: u32,
    pub failed: u32,
    /// Summed over the delivered messages.
    pub duration_ms: u64,
    /// `up`, `degraded`, `down` or `none`.
    pub state: &'static str,
    /// `failing`, `all-failed`, `denied` or `hub-down`; absent when up.
    pub cause: Option<&'static str>,
}

impl HourView {
    fn none(start_unix: u64) -> Self {
        Self {
            start_unix,
            received: 0,
            delivered: 0,
            failed: 0,
            duration_ms: 0,
            state: "none",
            cause: None,
        }
    }
}

#[derive(Debug, Serialize)]
pub struct RecentView {
    pub at_unix: u64,
    pub ok: bool,
    /// Delivery time; absent for a failure.
    pub ms: Option<u64>,
}

#[cfg(test)]
mod tests {
    use super::*;

    const H: u64 = HOUR_SECS;

    fn names() -> Vec<String> {
        vec!["a".to_string()]
    }

    #[test]
    fn hours_before_the_start_are_none_and_idle_hours_are_up() {
        let h = History::started_at(100 * H + 10);
        let view = h.view_at(103 * H + 5, &names());
        let hours = &view.profiles[0].hours;
        assert_eq!(hours.len(), HOURS);
        assert_eq!(hours[HOURS - 1].state, "up");
        assert_eq!(hours[HOURS - 4].state, "up");
        assert_eq!(hours[HOURS - 5].state, "none");
    }

    #[test]
    fn counts_fall_in_their_hour_and_failures_degrade_it() {
        let h = History::started_at(0);
        h.record_at("a", 50 * H + 1, true, 100);
        h.record_at("a", 50 * H + 2, true, 300);
        h.record_at("a", 50 * H + 3, false, 9);
        h.record_at("a", 51 * H + 3, false, 9);
        let view = h.view_at(51 * H + 10, &names());
        let hours = &view.profiles[0].hours;
        let before = &hours[HOURS - 2];
        assert_eq!(
            (before.received, before.delivered, before.failed),
            (3, 2, 1)
        );
        assert_eq!(before.duration_ms, 400);
        assert_eq!(before.state, "degraded");
        assert_eq!(hours[HOURS - 1].state, "down");
        assert_eq!(hours[HOURS - 1].cause, Some("all-failed"));
        assert_eq!(view.profiles[0].last_failure_unix, Some(51 * H + 3));
    }

    #[test]
    fn a_denied_profile_stays_red_through_quiet_hours_until_it_recovers() {
        let h = History::started_at(0);
        h.record_at("a", 10 * H, true, 5);
        h.note_health_at("a", 10 * H + 60, Health::Denied);
        h.note_health_at("a", 13 * H + 60, Health::Working);
        let view = h.view_at(14 * H, &names());
        let hours = &view.profiles[0].hours;
        let at = |hour: u64| &hours[hours.len() - 1 - (14 - hour) as usize];
        assert_eq!(at(10).state, "down");
        assert_eq!(at(11).state, "down");
        assert_eq!(at(12).state, "down");
        assert_eq!(at(13).state, "down");
        assert_eq!(at(13).cause, Some("denied"));
        assert_eq!(at(14).state, "up");
    }

    #[test]
    fn a_slot_is_reused_after_24_hours_and_memory_stays_fixed() {
        let h = History::started_at(0);
        h.record_at("a", 5 * H, true, 1);
        h.record_at("a", (5 + HOURS as u64) * H, true, 1);
        for i in 0..(RECENT_CAP as u64 * 3) {
            h.record_at("a", 30 * H + i, true, 1);
        }
        let view = h.view_at((5 + HOURS as u64) * H, &names());
        assert_eq!(view.profiles[0].hours[HOURS - 1].received, 1);
        let later = h.view_at(30 * H + 500, &names());
        assert_eq!(later.profiles[0].recent.len(), RECENT_CAP);
        for n in 0..(PROFILE_CAP + 10) {
            h.record_at(&format!("p{n}"), 30 * H, true, 1);
        }
        assert!(h.tracks.lock().unwrap().len() <= PROFILE_CAP);
    }
}
