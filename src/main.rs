//! The binary (L6b), on chassis since 2.0.0: the kit owns the command
//! line, configuration layers, logging, `/healthz`, `/metrics`, the
//! graceful shutdown and signed self-update; this file assembles the
//! switchboard on top of it — one health subsystem per profile, the pumps
//! started after the bind and stopped in the shutdown window. The `test`
//! dry-run subcommand is the switchboard's own and is dispatched before
//! the kit sees the arguments.
//!
//! 3.0.0 (H1, V1): the inbound routes are the kit's API routes, so a
//! sender presents a kit client token and the per-path `inbound_token` is
//! gone. That brings the kit's dashboard with it, which this file dresses:
//! a client is called a *sender* here, and the status page carries the
//! Profiles section from `dashboard.rs`.

use std::process::ExitCode;
use std::sync::Arc;

use axum::Router;
use chassis::{App, Control};
use http_switchboard::app::App as Switchboard;
use http_switchboard::cli;

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // W4: render a profile against a recorded message and print what would
    // go out, without sending anything. The switchboard's own subcommand;
    // the kit's parser does not know it, so it goes first.
    if args.first().map(String::as_str) == Some("test") {
        return cli::dry_run(&args[1..]);
    }

    let mut app = match start_app() {
        Ok(app) => app,
        Err(code) => return code,
    };
    // Only a real start and `--check` need the switchboard's own config;
    // `--version`, `gen-secret`, `--healthcheck`, `--print-config`, `update`
    // and `rekey` are the kit's alone and must work without the file.
    let Some(loaded) = app.loaded.as_ref() else {
        return app.run().await;
    };
    if !matches!(app.control, None | Some(Control::Check)) {
        return app.run().await;
    }
    let path = loaded.file_path.display().to_string();
    let config = match cli::config_from_app(&app, &path) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let profiles = config.profiles.len();
    let check_path = path.clone();
    app.on_check(move || {
        println!("{check_path}: ok, {profiles} profile(s)");
        Ok(())
    });

    let switchboard = Switchboard::from_config(config);
    cli::wire_service(&mut app, &switchboard);
    if let Err(e) = cli::wire_browser_prefs(&mut app) {
        eprintln!("{e}");
        return ExitCode::FAILURE;
    }

    let switchboard = Arc::new(switchboard);
    if app.control.is_none() {
        cli::spawn_reload_on_hangup(path, Arc::clone(&switchboard));
    }
    cli::wire_lifecycle(&mut app, switchboard);
    app.run().await
}

/// Start the kit from the environment and arguments; on failure print why
/// and hand back the exit code.
fn start_app() -> Result<App, ExitCode> {
    let mut app = App::from_env_and_args(cli::spec(), Router::new()).map_err(|e| {
        eprintln!("{e}");
        ExitCode::FAILURE
    })?;
    // fix-15 (chassis-rs 3.2.0): without this the kit's dashboard pages and
    // layout would show the binary name ("http-switchboard") as the brand;
    // "HTTPSwitchboard" is the name the service uses everywhere else
    // (README).
    app.brand_title("HTTPSwitchboard");
    Ok(app)
}
