//! HTTPSwitchboard — accepts a message in one shape and delivers it in
//! another, to a destination the configuration chooses.
//!
//! Module layout follows AR1: `translate` is pure (no network, no clock,
//! no environment) so every mapping test runs without a server or a hub;
//! everything that touches the outside world lives behind the traits in
//! `adapters`, so the pump's ordering — poll, deliver, only then ack — is
//! testable with fakes. That ordering is where messages get lost, which
//! is why it does not live in the pure half.

pub mod adapters;
pub mod app;
pub mod config;
pub mod inbound;
pub mod obs;
pub mod pump;
pub mod secret;
pub mod translate;
