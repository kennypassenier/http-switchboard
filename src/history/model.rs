//! The in-memory shapes the recorder keeps: one fixed-size [`Track`] per
//! profile, made of hour [`Bucket`]s and a short list of recent outcomes.

use std::collections::VecDeque;

use super::{HOURS, RECENT_CAP};
use crate::pump::Health;

/// What went wrong in an hour, ordered by severity so `max` finds the
/// worst. The order is part of the contract.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub(super) enum Cause {
    Ok,
    Failing,
    Denied,
    HubDown,
}

impl Cause {
    pub(super) fn of(health: Health) -> Self {
        match health {
            Health::Starting | Health::Working => Cause::Ok,
            Health::Failing => Cause::Failing,
            Health::Denied => Cause::Denied,
            Health::HubDown => Cause::HubDown,
        }
    }
}

/// One clock hour of one profile.
#[derive(Debug, Clone, Copy)]
pub(super) struct Bucket {
    /// Hours since the epoch.
    pub(super) hour: u64,
    pub(super) received: u32,
    pub(super) delivered: u32,
    pub(super) failed: u32,
    /// Time spent on the deliveries that succeeded.
    pub(super) duration_ms: u64,
    /// The health when the hour began, the worst during it, and the last.
    pub(super) first: Cause,
    pub(super) worst: Cause,
    pub(super) last: Cause,
}

/// One recent outcome, for the "last messages" list.
#[derive(Debug, Clone, Copy)]
pub(super) struct Recent {
    pub(super) at: u64,
    pub(super) ok: bool,
    pub(super) ms: u64,
}

/// Everything remembered about one profile; its size is fixed.
#[derive(Debug)]
pub(super) struct Track {
    pub(super) buckets: [Option<Bucket>; HOURS],
    pub(super) recent: VecDeque<Recent>,
    pub(super) last_failure_unix: Option<u64>,
    /// The health right now; what a new hour starts with.
    pub(super) carry: Cause,
}

impl Track {
    pub(super) fn new() -> Self {
        Self {
            buckets: [None; HOURS],
            recent: VecDeque::with_capacity(RECENT_CAP),
            last_failure_unix: None,
            carry: Cause::Ok,
        }
    }

    pub(super) fn bucket(&mut self, hour: u64) -> &mut Bucket {
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
