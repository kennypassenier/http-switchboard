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
//!
//! Layout: `types` holds the vocabulary every other module shares (the
//! `Hub` port, `Health`, `Step`); `turn` holds the state machine. The
//! public paths (`crate::pump::Health`, `crate::pump::pump_once`, …) stay
//! as they were.

pub(crate) mod give_up;
mod policy;
pub mod streak;
mod turn;
mod types;

pub use turn::pump_once;
pub use types::{Health, Hub, HubError, HubMessage, Poll, PumpState, Step};
