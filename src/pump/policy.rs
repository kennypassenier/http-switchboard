//! Writing a subscription's lease and attempt policy to the hub.
//!
//! Two moments: after the first successful poll (a kyu subscription only
//! exists once it has polled, so a write before that is refused), and
//! before the next poll after a reload changed the policy (the subscription
//! exists by then, and a poll first would claim a message under the old
//! lease). The before-poll write is best effort: if it fails the poll still
//! goes ahead, the write is retried every 30 s and the failure is logged
//! once, so until a write succeeds a message may be claimed under the old
//! lease.

use std::time::{Duration, Instant};

use super::types::{Hub, HubError, Poll, PumpState};
use crate::config::Profile;

/// The least time between two policy writes while the hub keeps refusing
/// them, so a broken policy endpoint costs one call per interval, not one
/// per poll.
const POLICY_RETRY_INTERVAL: Duration = Duration::from_secs(30);

/// Write the policy now if it is wanted and the retry interval allows.
/// A refusal is logged as a warning on the first failure and whenever the
/// error text changes, and the write is tried again after the interval.
pub(super) async fn try_push(hub: &dyn Hub, profile: &Profile, topic: &str, state: &mut PumpState) {
    if let Some(last) = state.policy_last_try {
        if last.elapsed() < POLICY_RETRY_INTERVAL {
            return;
        }
    }
    state.policy_last_try = Some(Instant::now());
    let result = hub
        .set_policy(
            topic,
            &profile.subscription,
            profile.lease_ms,
            profile.max_attempts,
        )
        .await;
    match result {
        Ok(()) => {
            state.policy_pushed = true;
            state.policy_before_poll = false;
            state.policy_last_error = None;
            state.policy_last_try = None;
        }
        Err(e) => note_failure(profile, &e, state),
    }
}

fn note_failure(profile: &Profile, error: &HubError, state: &mut PumpState) {
    let text = error.to_string();
    if state.policy_last_error.as_deref() != Some(text.as_str()) {
        crate::obs::log_warn(
            &profile.name,
            "policy_push_failed",
            &format!(
                "could not write the lease and attempt policy to the hub; messages are claimed \
                 under the hub's own defaults until it succeeds (tried again every {} s): {text}",
                POLICY_RETRY_INTERVAL.as_secs()
            ),
        );
        state.policy_last_error = Some(text);
    }
}

/// Before a poll: a reload asked for the new policy to be in place first.
/// Best effort: a failed write does not hold the poll back.
pub(super) async fn push_before_poll(
    hub: &dyn Hub,
    profile: &Profile,
    topic: &str,
    state: &mut PumpState,
) {
    if state.policy_before_poll {
        try_push(hub, profile, topic, state).await;
    }
}

/// After a poll: the first successful poll creates the subscription, so
/// only then is the first write possible.
pub(super) async fn push_after_poll(
    hub: &dyn Hub,
    profile: &Profile,
    topic: &str,
    poll: &Result<Poll, HubError>,
    state: &mut PumpState,
) {
    let polled = matches!(
        poll,
        Ok(Poll::Empty | Poll::Message(_) | Poll::NotJson { .. } | Poll::TooLarge { .. })
    );
    if !state.policy_pushed && polled {
        try_push(hub, profile, topic, state).await;
    }
}
