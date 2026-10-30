//! Reload on SIGHUP (feat-reload-1).

use std::sync::Arc;

use http_switchboard::app::App as Switchboard;

use super::config_file::load_config_file;

/// feat-reload-1: on SIGHUP (`systemctl reload http-switchboard`), read
/// the file again and apply it if — and only if — it is a complete, valid
/// config of the same shape. Registering the handler also means a stray
/// hangup no longer ends the process, which is the signal's default.
pub(crate) fn spawn_reload_on_hangup(path: String, switchboard: Arc<Switchboard>) {
    use tokio::signal::unix::{signal, SignalKind};
    let mut hangup = match signal(SignalKind::hangup()) {
        Ok(s) => s,
        Err(e) => {
            http_switchboard::obs::log_warn(
                "-",
                "reload_unavailable",
                &format!("cannot listen for SIGHUP ({e}); a config change needs a restart"),
            );
            return;
        }
    };
    tokio::spawn(async move {
        while hangup.recv().await.is_some() {
            let loaded = load_config_file(&path);
            let outcome =
                http_switchboard::reload::apply(&switchboard.config, &switchboard.profiles, loaded);
            switchboard.registry.record_reload(outcome.label());
            let line = http_switchboard::reload::describe(&outcome);
            match outcome {
                http_switchboard::reload::Outcome::Applied { .. } => {
                    http_switchboard::obs::log_info("-", "config_reloaded", &line)
                }
                _ => http_switchboard::obs::log_warn("-", "config_not_reloaded", &line),
            }
        }
    });
}
