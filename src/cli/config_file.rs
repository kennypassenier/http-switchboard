//! Reading the switchboard's half of the shared config file.

use crate::config::{self, ProcessEnv};

use super::spec::spec;

/// Read the switchboard's half of a config file: the `test` subcommand
/// and a reload (feat-reload-1).
///
/// Neither has the kit's start-up `App` to ask for `project_table()`, so
/// this does the same strip from the spec, which
/// names both halves itself (`knob_keys` since 1.2.0, `kit_sections` since
/// 2.0.0) — so this is the kit's list, not knowledge kept here.
///
/// fix-3: without the strip this verb answered "not valid TOML" on a file
/// that is valid TOML and that the service starts from happily, which is
/// exactly the file the operations runbook tells an operator to point it at.
pub fn load_config_file(path: &str) -> Result<config::Config, String> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "{path}: cannot be read ({e}). What now: check the path (--config / \
             HTTP_SWITCHBOARD_CONFIG) and that the file is readable by the user this service \
             runs as."
        )
    })?;
    let mut table: toml::Table = toml::from_str(&text).map_err(|e| {
        format!("{path}: is not valid TOML: {e}. What now: fix the syntax the parser points at.")
    })?;
    let spec = spec();
    for key in spec.knob_keys().iter().chain(spec.kit_sections()) {
        table.remove(*key);
    }
    let own = toml::to_string(&table).map_err(|e| format!("{path}: cannot re-serialise: {e}"))?;
    config::load(path, &own, &ProcessEnv).map_err(|e| e.to_string())
}

/// The switchboard's config from a started kit `App`: the kit hands over
/// its own half of the shared file (feat-config-1, kit 2.0.0), its knob
/// keys AND its table sections, so what is left is validated with the
/// switchboard's own rules intact (deny_unknown_fields, reserved paths,
/// `${VAR}` from the environment). This replaced nineteen hand-written
/// lines, one of which stripped `notify` on knowledge that lived nowhere
/// and would have gone silently wrong the day the kit gained a second
/// section.
pub fn config_from_app(app: &chassis::App, path: &str) -> Result<config::Config, String> {
    let table = app.project_table().map_err(|e| e.to_string())?;
    let own = toml::to_string(&table).map_err(|e| format!("{path}: cannot re-serialise: {e}"))?;
    config::load(path, &own, &ProcessEnv).map_err(|e| e.to_string())
}
