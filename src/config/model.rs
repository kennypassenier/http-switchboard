//! The validated model: what the rest of the service sees once a file has
//! passed every rule. Nothing here parses or checks; see `load`.

use std::collections::BTreeMap;

use super::defaults::BACKOFF_BASE_MS;
use crate::secret::Secret;

#[derive(Debug, Clone)]
pub struct Config {
    pub kyu: Option<Kyu>,
    pub reporting: Option<Reporting>,
    pub profiles: Vec<Profile>,
}

#[derive(Debug, Clone)]
pub struct Kyu {
    pub base_url: String,
    pub token: Option<Secret>,
}

#[derive(Debug, Clone)]
pub struct Reporting {
    pub topic: String,
}

#[derive(Debug, Clone)]
pub struct Profile {
    pub name: String,
    /// The kyu subscription this profile consumes under. Its own key,
    /// never an accident of the profile name: renaming a profile would
    /// otherwise create a fresh subscription and abandon everything still
    /// unacked (AR2).
    pub subscription: String,
    pub source: Source,
    pub sink: Sink,
    pub content_type: String,
    pub body: String,
    pub headers: BTreeMap<String, Secret>,
    pub timeout_ms: u64,
    pub retries: u32,
    pub lease_ms: u64,
    pub max_attempts: u32,
    /// W12 (mini-round MR2, 2026-08-30, Kenny's idea): include the
    /// receiver's own error text in the answer to the sender. Off by
    /// default and refused on a kyu source, where there is nobody to
    /// answer — a receiver's error page is not ours and can carry its
    /// internal addresses, which is the mistake this project just fixed
    /// in the other direction.
    pub forward_error_body: bool,
}

impl Profile {
    /// Wall-clock a delivery may consume before the ack must be in, worst
    /// case: every attempt burning its full timeout, plus the pauses
    /// between them. The pauses are part of the budget — leaving them out
    /// is how a design that looks like it fits the lease does not.
    pub fn retry_budget_ms(&self) -> u64 {
        let tries = u64::from(self.retries).saturating_add(1);
        let attempts = self.timeout_ms.saturating_mul(tries);
        (0..self.retries).fold(attempts, |sum, i| sum.saturating_add(self.pause_ms(i)))
    }

    /// The pause before retry number `index` (0-based): 1 s, 2 s, 4 s,
    /// doubling. Saturating, so an out-of-range index cannot overflow.
    pub fn pause_ms(&self, index: u32) -> u64 {
        BACKOFF_BASE_MS.saturating_mul(2u64.saturating_pow(index))
    }

    /// All the pauses, in order, for the dashboard that draws them. One
    /// definition (`pause_ms`) for the budget, the retry loop and the
    /// page, so they cannot disagree with the check that guards the lease.
    pub fn pauses_ms(&self) -> Vec<u64> {
        (0..self.retries).map(|i| self.pause_ms(i)).collect()
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Source {
    Http { path: String },
    Kyu { topic: String },
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Sink {
    Url { url: String, method: String },
    Kyu { topic: String },
}
