//! Structured log events: the K11 contract, and what a Loki query filters
//! on. All of them ride the kit's subscriber under one target.

use crate::pump::Health;

/// One event per message — the K11 contract, and what a Loki query filters
/// on. Since 2.0.0 (D-H1) these ride the kit's subscriber as structured
/// fields, so `HTTP_SWITCHBOARD_LOG_FORMAT=json` yields ONE shape for the
/// kit's lines and the switchboard's: `fields.profile`, `fields.outcome`,
/// … under the target `http_switchboard::events`. Per-attempt detail
/// belongs at debug level; this is the summary.
pub fn log_message(profile: &str, source: &str, outcome: &str, duration_ms: u64, attempts: u32) {
    tracing::info!(
        target: "http_switchboard::events",
        profile,
        source,
        outcome,
        duration_ms,
        attempts,
        "message"
    );
}

/// A state change, logged once rather than on every attempt: a hub that
/// is away for an hour should produce two lines, not thousands.
pub fn log_transition(profile: &str, from: Health, to: Health, detail: &str) {
    tracing::warn!(
        target: "http_switchboard::events",
        profile,
        event = "state_change",
        from = ?from,
        to = ?to,
        detail,
        "state change"
    );
}

/// An operator-visible event that is not a problem (a reload applied).
pub fn log_info(profile: &str, event: &str, detail: &str) {
    tracing::info!(target: "http_switchboard::events", profile, event, detail, "event");
}

/// Something went wrong that is NOT a state change. Kept separate on
/// purpose: the first version of this reported a failed self-report by
/// logging a Working -> Failing transition that never happened, which is
/// a log line lying about the state it exists to describe (found while
/// smoke-testing the container image).
pub fn log_warn(profile: &str, event: &str, detail: &str) {
    tracing::warn!(target: "http_switchboard::events", profile, event, detail, "warning");
}
