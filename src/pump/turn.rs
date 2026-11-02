//! One turn of the pump: poll, translate, deliver, settle.
//!
//! Split into one function per outcome of the poll so each rule (replay on
//! a missing topic, dead-letter what can never render, ack only after
//! delivery, nack at once on a failed delivery, give up at once on a
//! permanent refusal, pause after a run of refusals) reads on its own.

use super::give_up;
use super::policy;
use super::streak;
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
    if state.paused {
        return Step::Paused; // nothing is claimed; messages wait on the hub
    }
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
        Err(e) => hub_failure(e, state),
        Ok(poll) => settle_poll(&lane, poll, sink, clock, state).await,
    }
}

/// A poll that failed: the hub is refusing us or away.
fn hub_failure(error: crate::pump::HubError, state: &mut PumpState) -> Step {
    let detail = error.to_string();
    if matches!(error, crate::pump::HubError::Denied) {
        state.health = Health::Denied;
        Step::Denied { detail }
    } else {
        state.health = Health::HubDown;
        Step::HubDown { detail }
    }
}

/// A poll the hub answered: act on what it holds.
async fn settle_poll(
    lane: &Lane<'_>,
    poll: Poll,
    sink: &dyn Sink,
    clock: &dyn Clock,
    state: &mut PumpState,
) -> Step {
    match poll {
        Poll::UnknownTopic => {
            // Nothing has ever published here. Ask for the history on the
            // next poll so the first message is not missed.
            state.replay_next = true;
            Step::TopicMissing
        }
        Poll::Empty => idle(state),
        Poll::NotJson { id } => {
            hub_answered(state);
            state.replay_next = false;
            // A dead letter ends it, so it is seen once.
            state.first_sight = true;
            lane.nack(&id, true).await;
            Step::DeadLettered {
                id,
                reason: "the message on the hub is not JSON, so no template can read it"
                    .to_string(),
                summary: give_up::NOT_JSON.to_string(),
            }
        }
        Poll::TooLarge { id } => {
            hub_answered(state);
            state.replay_next = false;
            state.first_sight = true;
            lane.nack(&id, true).await;
            Step::DeadLettered {
                id,
                reason: "the message on the hub is larger than this service reads, so it was given up on unread"
                    .to_string(),
                summary: give_up::TOO_LARGE.to_string(),
            }
        }
        Poll::Message(message) => {
            hub_answered(state);
            state.replay_next = false;
            state.first_sight = message.attempt <= 1;
            handle_message(lane, *message, sink, clock, state).await
        }
    }
}

/// The hub answered the poll and accepted us, so a profile that was
/// starting or cut off is working again.
fn hub_answered(state: &mut PumpState) {
    if matches!(
        state.health,
        Health::Starting | Health::HubDown | Health::Denied
    ) {
        state.health = Health::Working;
    }
}

/// The long poll closed with nothing waiting: the hub is reachable and
/// accepted us, so a profile that was starting or cut off is working.
fn idle(state: &mut PumpState) -> Step {
    state.replay_next = false;
    hub_answered(state);
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
            // This message is the problem, not the destination.
            lane.nack(&message.id, true).await;
            return Step::DeadLettered {
                id: message.id,
                reason: e.to_string(),
                summary: give_up::render_summary(&e).to_string(),
            };
        }
    };
    let outcome = deliver_with_retry(lane.profile, &delivery, sink, clock).await;
    settle_delivery(lane, message.id, outcome, state).await
}

/// Settle a message with the hub by how its delivery ended.
async fn settle_delivery(
    lane: &Lane<'_>,
    id: String,
    outcome: crate::adapters::Outcome,
    state: &mut PumpState,
) -> Step {
    match outcome.result {
        Ok(()) => {
            let ack_failed = lane.ack(&id).await;
            state.health = Health::Working;
            state.refusals.delivered();
            Step::Delivered {
                id,
                attempts: outcome.attempts,
                ack_failed,
            }
        }
        Err(e) if e.is_permanent() => {
            // The destination will answer the same to the same bytes, so
            // re-offering only burns the hub's attempts. Give up at once;
            // the receiver's status is in the log, health stays as it was.
            lane.nack(&id, true).await;
            streak::note_refusal(state);
            let status = e.permanent_status().unwrap_or_default();
            Step::DeadLettered {
                id,
                reason: e.to_string(),
                summary: give_up::refused_summary(status),
            }
        }
        Err(e) => {
            // Hand it back at once rather than sitting on the claim: the
            // hub's own backoff is better at waiting than we are.
            lane.nack(&id, false).await;
            state.health = Health::Failing;
            Step::HandedBack {
                id,
                attempts: outcome.attempts,
                reason: e.to_string(),
            }
        }
    }
}
