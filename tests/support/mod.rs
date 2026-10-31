// Each test binary compiles this whole module, so a double only one
// suite needs looks unused to the others (and so does its re-export).
#![allow(dead_code, unused_imports)]

//! Test doubles shared by the milestone suites.
//!
//! What these fakes cannot express, written down as standing rule 9
//! demands:
//!
//! * `FakeClock` does not pass time — it records what it was asked to
//!   wait. Anything that depends on real elapsed time (a lease actually
//!   expiring on the hub) needs the E2E suite against a real kyu, not
//!   this.
//! * `TestServer` speaks just enough HTTP to answer with a status and
//!   record what arrived. It does not do chunked encoding, keep-alive,
//!   redirects or TLS, so nothing here proves anything about those.

mod clock;
mod hub;
mod kyu;
mod server;
mod sink;

pub use clock::*;
pub use hub::*;
pub use kyu::*;
pub use server::*;
pub use sink::*;
