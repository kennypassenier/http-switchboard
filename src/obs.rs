//! Health, metrics and structured logging (L6 — W5, W6, W7; AR11).
//!
//! One fact, modelled once. A profile's state is the same thing
//! `/healthz` reports, the log line records and — later — the self-report
//! event announces; three separate notions of "is it working" is how they
//! come to disagree.
//!
//! `/healthz` deliberately reports more than "the process is alive". A
//! liveness-only endpoint answers 200 while every profile has been denied
//! for six hours, which is exactly the silent death W5 was raised to
//! Essential against.

mod events;
mod glue;
mod registry;
mod report;
mod streak;
#[cfg(test)]
mod tests;

pub use events::{log_info, log_message, log_transition, log_warn};
pub use glue::{ProfileSubsystem, RegistryMetrics};
pub use registry::{ProfileStats, Registry};
