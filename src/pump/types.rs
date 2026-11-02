//! The vocabulary of the pump: what the hub hands over, how it can fail,
//! the `Hub` port, a profile's health, the pump's own memory between turns,
//! and what one turn did.
//!
//! Kept apart from the turn itself (`pump/turn.rs`) because every other
//! module (the health endpoint, the history, the dashboard, the hub
//! adapter) needs these types and none of them needs the state machine.

use crate::adapters::BoxFuture;

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
    /// The message is on the hub but bigger than this service reads. Only
    /// its id was recovered; it is given up on, never read in full.
    TooLarge {
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

impl Health {
    /// The one word for this state, shared by `/healthz`, the status page
    /// and the self-report events so they can never spell it differently.
    pub fn word(self) -> &'static str {
        match self {
            Health::Starting => "starting",
            Health::Working => "working",
            Health::Failing => "failing",
            Health::Denied => "denied",
            Health::HubDown => "hub-down",
        }
    }

    /// The word for a remembered health; nothing remembered yet reads as
    /// "starting", the state that means "nothing tried since start".
    pub fn word_of(health: Option<Health>) -> &'static str {
        health.unwrap_or(Health::Starting).word()
    }
}

#[derive(Debug)]
pub struct PumpState {
    /// Set after a 404: the next successful poll asks for the retained
    /// history, so a message published before this subscription existed
    /// is not lost.
    pub replay_next: bool,
    pub health: Health,
    pub policy_pushed: bool,
    /// A reload changed the lease or attempt cap after a push had already
    /// succeeded: the subscription exists, so the new policy is written
    /// BEFORE the next poll. Best effort: a failed write is retried every
    /// 30 s and logged once, and the poll goes ahead meanwhile, so a message
    /// can still be claimed under the old lease until a write succeeds.
    pub policy_before_poll: bool,
    /// When the policy write was last tried, and the error text it last
    /// failed with: together they keep a refusing hub from being asked and
    /// logged about every turn.
    pub policy_last_try: Option<std::time::Instant>,
    pub policy_last_error: Option<String>,
    /// Whether the message of the latest turn is new to us: the hub's
    /// `attempt` was 1. A message handed back and offered again is a
    /// further attempt, not a further message.
    pub first_sight: bool,
}

impl Default for PumpState {
    fn default() -> Self {
        Self {
            replay_next: false,
            health: Health::Starting,
            policy_pushed: false,
            policy_before_poll: false,
            policy_last_try: None,
            policy_last_error: None,
            first_sight: true,
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
        /// Why, with the remedy: for the log.
        reason: String,
        /// One plain line for the dashboard. Written from fixed words (plus
        /// a status number), never from the error text, which can carry a
        /// destination path or a receiver's own words.
        summary: String,
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
