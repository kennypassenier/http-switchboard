//! Profile configuration: the whole model of the service (L1 — K8, K9,
//! K10; AR2, AR5, AR6).
//!
//! Loading is pure: the caller supplies the file's text and a lookup for
//! environment variables, so every validation rule is a unit test with no
//! ambient I/O (AR1). Nothing here starts half-working — a config that
//! does not hold up stops the process, and every error says what to do
//! about it (K10, standing rule 11).

mod checks;
mod defaults;
mod env;
mod error;
mod load;
mod model;
mod profile;
mod raw;

pub use env::{EnvLookup, ProcessEnv};
pub use error::ConfigError;
pub use load::load;
pub use model::{Config, Kyu, Profile, Reporting, Sink, Source};

impl std::fmt::Display for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{} profile(s)", self.profiles.len())
    }
}
