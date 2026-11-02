//! One turn of the pump: poll, translate, deliver, settle.
//!
//! Split into one function per outcome of the poll so each rule (replay on
//! a missing topic, dead-letter what can never render, ack only after
//! delivery, nack at once on a failed delivery) reads on its own.

use super::policy;
use super::types::{Health, Hub, HubMessage, Poll, PumpState, Step};
use crate::adapters::{deliver_with_retry, Clock, Sink};
use crate::config::{Profile, Source};
use crate::translate;

/// Where a turn talks to the hub: the topic and subscription of one
/// profile, borrowed for the length of the turn.
struct Lane<'a> {
    hub: &'a dyn Hub,
    profile: &'a Profile,
    topic: &'a str,
}

impl Lane<'_> {
    fn subscription(&self) -> &str {
        &self.profile.subscription
    }

    /// Hand a message back to the hub. A failed nack is not lost work —
    /// the lease runs out and the hub offers the message again — but it is
    /// logged, so a redelivery after a dead-letter has an explanation.
    async fn nack(&self, id: &str, dead: bool) {
        if let Err(e) = self
            .hub
            .nack(self.topic, self.subscription(), id, dead)
            .await
        {
            crate::obs::log_warn(
                &self.profile.name,
                "nack_failed",
                &format!("could not hand message {id} back to the hub (dead letter: {dead}); the lease will return it: {e}"),
            );
        }
    }

    /// Acknowledge a delivered message, retrying once. Returns whether the
    /// acknowledgement finally failed.
    async fn ack(&self, id: &str) -> bool {
        // Ack failures are routine, not exotic: the hub may be busy. One
        // retry, then let the lease do its work — at worst a duplicate,
        // which the chain tolerates. It is reported (Step::Delivered's
        // ack_failed), because an unexplained duplicate at 3 a.m. is worse
        // than a noisy line.
        if self
            .hub
            .ack(self.topic, self.subscription(), id)
            .await
            .is_ok()
        {
            return false;
        }
        self.hub
            .ack(self.topic, self.subscription(), id)
            .await
            .is_err()
    }
}

/// One turn: poll, translate, deliver, settle. Never more than one
/// message, so a profile drains its backlog in publish order (AR3).
pub async fn pump_once(
    profile: &Profile,
    hub: &dyn Hub,
    sink: &dyn Sink,
    clock: &dyn Clock,
    state: &mut PumpState,
) -> Step {
    let Source::Kyu { topic } = &profile.source else {
        return Step::Idle;
    };
    let lane = Lane {
        hub,
        profile,
        topic,
    };
    policy::push_before_poll(hub, profile, topic, state).await;
    let poll = hub
        .next(topic, lane.subscription(), state.replay_next)
        .await;
    policy::push_after_poll(hub, profile, topic, &poll, state).await;

    match poll {
        Err(e @ crate::pump::HubError::Denied) => {
            state.health = Health::Denied;
            Step::Denied {
                detail: e.to_string(),
            }
        }
        Err(e) => {
            state.health = Health::HubDown;
            Step::HubDown {
                detail: e.to_string(),
            }
        }
        Ok(Poll::UnknownTopic) => {
            // Nothing has ever published here. Ask for the history on the
            // next poll so the first message is not missed.
            state.replay_next = true;
            Step::TopicMissing
        }
        Ok(Poll::Empty) => idle(state),
        Ok(Poll::NotJson { id }) => {
            state.replay_next = false;
            let reason =
                "the message on the hub is not JSON, so no template can read it".to_string();
            lane.nack(&id, true).await;
            Step::DeadLettered { id, reason }
        }
        Ok(Poll::Message(message)) => {
            state.replay_next = false;
            handle_message(&lane, *message, sink, clock, state).await
        }
    }
}

/// The long poll closed with nothing waiting: the hub is reachable and
/// accepted us, so a profile that was starting or cut off is working.
fn idle(state: &mut PumpState) -> Step {
    state.replay_next = false;
    if matches!(
        state.health,
        Health::Starting | Health::HubDown | Health::Denied
    ) {
        state.health = Health::Working;
    }
    Step::Idle
}

/// Translate one message and deliver it, then settle it with the hub.
async fn handle_message(
    lane: &Lane<'_>,
    message: HubMessage,
    sink: &dyn Sink,
    clock: &dyn Clock,
    state: &mut PumpState,
) -> Step {
    let delivery = match translate::prepare_value(lane.profile, &message.payload) {
        Ok(delivery) => delivery,
        Err(e) => {
            // Rendering is pure: it will fail identically next time.
            // Dead-letter it so it is visible on the dashboard instead of
            // cycling until its attempts run out.
            lane.nack(&message.id, true).await;
            state.health = Health::Failing;
            return Step::DeadLettered {
                id: message.id,
                reason: e.to_string(),
            };
        }
    };
    let outcome = deliver_with_retry(lane.profile, &delivery, sink, clock).await;
    match outcome.result {
        Ok(()) => {
            let ack_failed = lane.ack(&message.id).await;
            state.health = Health::Working;
            Step::Delivered {
                id: message.id,
                attempts: outcome.attempts,
                ack_failed,
            }
        }
        Err(e) => {
            // Hand it back at once rather than sitting on the claim: the
            // hub's own backoff is better at waiting than we are.
            lane.nack(&message.id, false).await;
            state.health = Health::Failing;
            Step::HandedBack {
                id: message.id,
                attempts: outcome.attempts,
                reason: e.to_string(),
            }
        }
    }
}
