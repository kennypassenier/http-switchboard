//! The binary's building blocks: dry-run dispatch, kit start-up, config,
//! wiring. They live in the library (not in `main.rs`) so integration tests
//! can call them directly, e.g. `load_config_file` or `dry_run`, without
//! starting a process (Kenny's choice, 30/10/2026).

mod config_file;
mod dry_run;
mod hangup;
mod spec;
mod wiring;

pub use config_file::{config_from_app, load_config_file};
pub use dry_run::dry_run;
pub use hangup::spawn_reload_on_hangup;
pub use spec::spec;
pub use wiring::{wire_browser_prefs, wire_lifecycle, wire_service};
