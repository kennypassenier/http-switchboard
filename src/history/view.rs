//! The 24-hour view the page draws: per profile, one cell per hour with a
//! state word and a short cause, plus the most recent outcomes.

use serde::Serialize;

use super::model::{Bucket, Cause, Track};
use super::{HOURS, HOUR_SECS};

/// Build one profile's view of the hours `first_hour..=current`.
pub(super) fn profile_view(
    name: &str,
    track: &Track,
    first_hour: u64,
    current: u64,
    started_hour: u64,
) -> ProfileHistory {
    // What the profile was before the first hour with traffic: the first
    // such hour's opening health, else its health now.
    let mut running = (first_hour..=current)
        .filter_map(|h| bucket_of(track, h))
        .map(|b| b.first)
        .next()
        .unwrap_or(track.carry);
    let hours = (first_hour..=current)
        .map(|h| {
            let start_unix = h * HOUR_SECS;
            if h < started_hour {
                return HourView::none(start_unix);
            }
            match bucket_of(track, h) {
                Some(b) => {
                    running = b.last;
                    HourView::busy(start_unix, &b)
                }
                None => HourView::quiet(start_unix, running),
            }
        })
        .collect();
    ProfileHistory {
        name: name.to_string(),
        hours,
        recent: recent_views(track),
        last_failure_unix: track.last_failure_unix,
    }
}

/// The recent outcomes, newest first.
fn recent_views(track: &Track) -> Vec<RecentView> {
    track
        .recent
        .iter()
        .rev()
        .map(|r| RecentView {
            at_unix: r.at,
            ok: r.ok,
            ms: r.ok.then_some(r.ms),
        })
        .collect()
}

/// The bucket recorded for clock hour `h`, if its slot still holds that
/// hour (a slot is reused when its hour comes round again).
fn bucket_of(track: &Track, h: u64) -> Option<Bucket> {
    track.buckets[(h % HOURS as u64) as usize].filter(|b| b.hour == h)
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
    } else if delivered == 0 && (received > 0 || failed > 0) {
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
    /// An hour with traffic.
    fn busy(start_unix: u64, b: &Bucket) -> Self {
        let (state, cause) = judge(b.received, b.delivered, b.failed, b.worst);
        Self {
            start_unix,
            received: b.received,
            delivered: b.delivered,
            failed: b.failed,
            duration_ms: b.duration_ms,
            state,
            cause,
        }
    }

    /// An hour without traffic: it takes the health the profile had when
    /// the hour began.
    fn quiet(start_unix: u64, running: Cause) -> Self {
        let (state, cause) = judge(0, 0, 0, running);
        Self {
            start_unix,
            received: 0,
            delivered: 0,
            failed: 0,
            duration_ms: 0,
            state,
            cause,
        }
    }

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
