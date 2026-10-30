//! The binary's building blocks, kept out of `main.rs` so the entry point
//! reads as the sequence of steps it is: dry-run dispatch, kit start-up,
//! config, wiring, run.

mod config_file;
mod dry_run;
mod hangup;
mod spec;
mod wiring;

pub(crate) use config_file::config_from_app;
pub(crate) use dry_run::dry_run;
pub(crate) use hangup::spawn_reload_on_hangup;
pub(crate) use spec::spec;
pub(crate) use wiring::{wire_browser_prefs, wire_lifecycle, wire_service};
