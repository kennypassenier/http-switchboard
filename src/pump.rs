//! The hub side: poll, translate, deliver, and only then acknowledge
//! (L4 — K2; AR8).
//!
//! This is where messages get lost, so it is a state machine over traits
//! rather than a straight line of calls: `Hub`, `Sink` and `Clock` all
//! have fakes, and the E2E suite runs the same code against a real kyu.
//!
//! Four things here are not obvious, and each one comes from the Phase 4
//! critic pass:
//!
//! * **Topic-birth replay.** A kyu subscription only sees what is
//!   published after its first poll. A 404 means the topic does not exist
//!   yet, so the next successful poll asks for what the topic retained —
//!   otherwise the very first alert falls into the gap.
//! * **Nack, never wait out the lease.** Waiting costs one of kyu's five
//!   attempts and 30 seconds of invisibility per failure, which
//!   dead-letters a backlog inside a routine restart of the receiver.
//! * **Ack only after delivery.** The whole point of G7.
//! * **Denied is its own state.** A 401 is not "the hub is down"; it is a
//!   rotated token, and it must be visible as that.

use crate::adapters::{deliver_with_retry, BoxFuture, Clock, Sink};
use crate::config::{Profile, Source};
use crate::translate;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HubMessage {
    pub id: String,
    pub payload: serde_json::Value,
    pub attempt: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Poll {
    Message(Box<HubMessage>),
    /// The long poll closed with nothing waiting.
    Empty,
    /// The topic does not exist yet — nothing has ever published there.
    UnknownTopic,
    /// The message is on the hub but is not JSON, so no template can read
    /// it. Kept distinct from a delivery failure: retrying will not help.
    NotJson {
        id: String,
    },
}

#[derive(Debug, thiserror::Error)]
pub enum HubError {
    #[error("the hub refused our credentials. What now: the token was probably rotated without restarting this service — mint an app token on kyu's Apps page, put it in the environment (the homelab vault composes it from latch) and restart.")]
    Denied,

    #[error("the hub could not be reached: {detail}. What now: check that kyu is running and reachable from this container; messages are not lost while it is away, they wait on the hub.")]
    Unreachable { detail: String },

    #[error("the hub answered {status}. What now: check kyu's own logs — this is the hub objecting, not the network.")]
    Status { status: u16 },
}

/// The three verbs, plus the policy write. A trait so the pump can be
/// driven by a fake in unit tests and by the real hub in the E2E suite.
pub trait Hub: Send + Sync {
    fn next<'a>(
        &'a self,
        topic: &'a str,
        subscription: &'a str,
        from_beginning: bool,
    ) -> BoxFuture<'a, Result<Poll, HubError>>;

    fn ack<'a>(
        &'a self,
        topic: &'a str,
        subscription: &'a str,
        id: &'a str,
    ) -> BoxFuture<'a, Result<(), HubError>>;

    fn nack<'a>(
        &'a self,
        topic: &'a str,
        subscription: &'a str,
        id: &'a str,
        dead: bool,
    ) -> BoxFuture<'a, Result<(), HubError>>;

    fn set_policy<'a>(
        &'a self,
        topic: &'a str,
        subscription: &'a str,
        lease_ms: u64,
        max_attempts: u32,
    ) -> BoxFuture<'a, Result<(), HubError>>;
}

/// How a profile is doing, for `/healthz` and for the self-report events.
/// One fact, modelled once, so the endpoint, the log and the event cannot
/// disagree about it.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Health {
    Starting,
    Working,
    /// Deliveries are failing; messages are waiting on the hub.
    Failing,
    /// The hub is refusing our credentials.
    Denied,
    /// The hub cannot be reached.
    HubDown,
}

#[derive(Debug)]
pub struct PumpState {
    /// Set after a 404: the next successful poll asks for the retained
    /// history, so a message published before this subscription existed
    /// is not lost.
    pub replay_next: bool,
    pub health: Health,
    pub policy_pushed: bool,
}

impl Default for PumpState {
    fn default() -> Self {
        Self {
            replay_next: false,
            health: Health::Starting,
            policy_pushed: false,
        }
    }
}

/// What one turn of the pump did. Returned rather than logged, so the
/// caller decides what to record (AR11) and the tests can assert on it.
#[derive(Debug, PartialEq, Eq)]
pub enum Step {
    Delivered {
        id: String,
        attempts: u32,
        /// The delivery landed but the hub would not take our
        /// acknowledgement. AR4 calls this routine rather than exotic, and
        /// it has to be visible: the message will come back, and without
        /// this the log would say "delivered" and nothing would explain the
        /// duplicate (Phase 7 audit).
        ack_failed: bool,
    },
    /// Delivery failed; the message was handed back and the hub will
    /// offer it again. Carries the attempt count, because a log line that
    /// says "1" after three attempts misinforms exactly the person
    /// debugging a retry (Phase 7 audit).
    HandedBack {
        id: String,
        attempts: u32,
        /// Why it failed, carrying the remedy from the delivery error —
        /// which otherwise existed only in the source (Phase 7 audit).
        reason: String,
    },
    /// The message can never work as it stands (it does not render, or it
    /// is not JSON); dead-lettered so it is visible instead of looping.
    DeadLettered {
        id: String,
        reason: String,
    },
    Idle,
    TopicMissing,
    /// The hub refused our credentials. The detail carries the remedy that
    /// names kyu's Apps page — AR8.4 asked for it and nothing printed it.
    Denied {
        detail: String,
    },
    HubDown {
        detail: String,
    },
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
    let subscription = &profile.subscription;

    let poll = hub.next(topic, subscription, state.replay_next).await;

    // The policy is written AFTER a successful poll, never before. A kyu
    // subscription starts existing when it first polls, so a policy write
    // beforehand is refused for a subscription that does not exist yet —
    // measured against a real hub during the Phase 7 sweep, where the
    // effective lease turned out to be the hub's default 30 s and not the
    // 60 s this profile was validated against. Residual, and small: a
    // message that arrives on the very first poll (only possible on the
    // replay path) is still claimed under the default lease.
    if !state.policy_pushed
        && matches!(
            poll,
            Ok(Poll::Empty | Poll::Message(_) | Poll::NotJson { .. })
        )
        && hub
            .set_policy(topic, subscription, profile.lease_ms, profile.max_attempts)
            .await
            .is_ok()
    {
        state.policy_pushed = true;
    }

    match poll {
        Err(e @ HubError::Denied) => {
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
        Ok(Poll::Empty) => {
            state.replay_next = false;
            if state.health == Health::Starting
                || state.health == Health::HubDown
                || state.health == Health::Denied
            {
                state.health = Health::Working;
            }
            Step::Idle
        }
        Ok(Poll::NotJson { id }) => {
            state.replay_next = false;
            let reason =
                "the message on the hub is not JSON, so no template can read it".to_string();
            let _ = hub.nack(topic, subscription, &id, true).await;
            Step::DeadLettered { id, reason }
        }
        Ok(Poll::Message(message)) => {
            state.replay_next = false;
            match translate::prepare_value(profile, &message.payload) {
                Err(e) => {
                    // Rendering is pure: it will fail identically next
                    // time. Dead-letter it so it is visible on the
                    // dashboard instead of cycling until its attempts run
                    // out.
                    let reason = e.to_string();
                    let _ = hub.nack(topic, subscription, &message.id, true).await;
                    state.health = Health::Failing;
                    Step::DeadLettered {
                        id: message.id,
                        reason,
                    }
                }
                Ok(delivery) => {
                    let outcome = deliver_with_retry(profile, &delivery, sink, clock).await;
                    match outcome.result {
                        Ok(()) => {
                            // Ack failures are routine, not exotic: the
                            // hub may be busy. One retry, then let the
                            // lease do its work — at worst a duplicate,
                            // which the chain tolerates. It is reported,
                            // because an unexplained duplicate at 3 a.m. is
                            // worse than a noisy line.
                            let mut ack_failed = false;
                            if hub.ack(topic, subscription, &message.id).await.is_err() {
                                ack_failed =
                                    hub.ack(topic, subscription, &message.id).await.is_err();
                            }
                            state.health = Health::Working;
                            Step::Delivered {
                                id: message.id,
                                attempts: outcome.attempts,
                                ack_failed,
                            }
                        }
                        Err(e) => {
                            // Hand it back at once rather than sitting on
                            // the claim: the hub's own backoff is better
                            // at waiting than we are.
                            let _ = hub.nack(topic, subscription, &message.id, false).await;
                            state.health = Health::Failing;
                            Step::HandedBack {
                                id: message.id,
                                attempts: outcome.attempts,
                                reason: e.to_string(),
                            }
                        }
                    }
                }
            }
        }
    }
}
