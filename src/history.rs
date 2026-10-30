//! Per-hour history of what each profile did, for the last 24 hours
//! (feat-dash-kp11-30).
//!
//! The registry keeps counters since the start; a counter cannot say *when*
//! something happened, so "was it working at 02:00" had no answer. This
//! module keeps, per profile and per clock hour, how many messages were
//! received, delivered and failed, the time the deliveries took, and the
//! worst health the profile showed in that hour.
//!
//! **In memory only, and lost on restart.** Nothing is written to disk: a
//! history file is one more thing that grows and one more thing to repair,
//! and the page says plainly "History since the last start, at most
//! 24 hours". Hours before the service started are reported as `none`.
//!
//! **Fixed size, whatever happens.** 24 hour slots and [`RECENT_CAP`]
//! recent outcomes per profile, at most [`PROFILE_CAP`] profiles; a slot is
//! reused when its hour comes round again. Nothing here holds a payload, a
//! header, a URL or an error text (the text of a failed delivery can carry
//! a webhook path, and a webhook path is the credential, S2): only counts,
//! times and a short fixed cause word.
//!
//! Health model per hour (the same words the page draws):
//!
//! * `up`: delivered what came in, or nothing came in and nothing was wrong;
//! * `degraded`: some deliveries failed, or the profile was `failing`;
//! * `down`: every delivery failed, or the hub refused the credentials
//!   (`denied`) or could not be reached (`hub-down`);
//! * `none`: before the service started recording.
//!
//! An hour without traffic takes the health the profile had when the hour
//! began, so a profile denied for six hours reads red for six hours even
//! though nobody posted.

mod model;
mod recorder;
#[cfg(test)]
mod tests;
mod view;

pub use recorder::History;
pub use view::{HistoryView, HourView, ProfileHistory, RecentView};

/// Hours kept, and cells drawn.
pub const HOURS: usize = 24;
/// Recent outcomes kept per profile.
pub const RECENT_CAP: usize = 50;
/// Profiles tracked. The set comes from the config, so this is a ceiling
/// that is never reached in practice, not a limit anyone meets.
pub const PROFILE_CAP: usize = 64;
/// Seconds in a clock hour.
const HOUR_SECS: u64 = 3600;
