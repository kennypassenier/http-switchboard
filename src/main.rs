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

use axum::routing::post;
use axum::Router;
use chassis::{App, AppSpec, Control};
use http_switchboard::app::App as Switchboard;
use http_switchboard::config::{self, ProcessEnv, Source};
use http_switchboard::obs::{ProfileSubsystem, RegistryMetrics};

/// The kit's description of this service. One definition, because the
/// dry-run needs the same knob and section names the real start does.
fn spec() -> AppSpec {
    AppSpec {
        name: "http-switchboard",
        version: env!("CARGO_PKG_VERSION"),
        repository: Some("kennypassenier/http-switchboard"),
        ..Default::default()
    }
}

fn usage() -> String {
    "http-switchboard [--config <config.toml>] [--listen host:port] [--state-dir <dir>]\n\
     http-switchboard --check [--config <config.toml>]\n\
     http-switchboard --healthcheck [http://127.0.0.1:8080/healthz]\n\
     http-switchboard test --config <config.toml> --profile <name> --input <message.json>\n\
     Every knob is also an environment variable (HTTP_SWITCHBOARD_CONFIG, _LISTEN, …); --help lists them.\n"
        .to_string()
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();
    // W4: render a profile against a recorded message and print what would
    // go out, without sending anything. The switchboard's own subcommand;
    // the kit's parser does not know it, so it goes first.
    if args.first().map(String::as_str) == Some("test") {
        return dry_run(&args[1..]);
    }

    let spec = spec();
    let mut app = match App::from_env_and_args(spec, Router::new()) {
        Ok(app) => app,
        Err(e) => {
            eprintln!("{e}");
            return ExitCode::FAILURE;
        }
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
    // The switchboard's config shares the file with the kit's knobs: read
    // the file, drop the kit's keys (and its [[notify.webhook]] tables) and
    // validate the rest with the switchboard's own rules intact
    // (deny_unknown_fields, reserved paths, `${VAR}` from the environment).
    let path = loaded.file_path.display().to_string();
    // feat-config-1 (kit 2.0.0): the kit hands over its own half of the
    // shared file — its knob keys AND its table sections. What this replaced
    // was nineteen hand-written lines, one of which stripped `notify` on
    // knowledge that lived nowhere and would have gone silently wrong the
    // day the kit gained a second section.
    let config = match app
        .project_table()
        .map_err(|e| e.to_string())
        .and_then(|table| {
            let own =
                toml::to_string(&table).map_err(|e| format!("{path}: cannot re-serialise: {e}"))?;
            config::load(&path, &own, &ProcessEnv).map_err(|e| e.to_string())
        }) {
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
    for profile in &switchboard.config.profiles {
        app.subsystem(ProfileSubsystem::new(
            &profile.name,
            Arc::clone(&switchboard.registry),
        ));
        if let Source::Http { path } = &profile.source {
            // A delivery may legitimately take longer than the kit's request
            // timeout (retries × timeout_ms + settle); the sender waits for
            // the real answer (W1), so the kit's clock stays out of it.
            app.exempt_from_timeout(path.clone());
        }
    }
    app.metrics_source(RegistryMetrics(Arc::clone(&switchboard.registry)));
    // H1 (3.0.0): the profile paths are API routes, so the kit checks an
    // `Authorization: Bearer <client token>` before this service sees the
    // body, and revoking a sender's token locks it out the same second.
    // `chassis clients issue <sender> --url … --token-env …` mints one
    // without opening the dashboard.
    app.api_routes(switchboard.profile_router());
    // V1: what a "client" is called here. Presentation only — the routes,
    // the JSON keys and the log fields keep saying `client`.
    app.vocabulary("sender", "senders");
    app.status_section(http_switchboard::dashboard::Profiles::new(
        &switchboard.config,
        Arc::clone(&switchboard.registry),
    ));
    app.dashboard_routes(Router::new().route(
        http_switchboard::dashboard::RECHECK_ROUTE,
        post(http_switchboard::dashboard::recheck).with_state(Arc::clone(&switchboard.registry)),
    ));

    let switchboard = Arc::new(switchboard);
    let stop: Arc<std::sync::Mutex<Option<tokio::sync::broadcast::Sender<()>>>> =
        Arc::new(std::sync::Mutex::new(None));
    {
        let switchboard = Arc::clone(&switchboard);
        let stop = Arc::clone(&stop);
        app.on_start(move || {
            let tx = switchboard.spawn_pumps();
            *stop.lock().expect("stop lock") = Some(tx);
        });
    }
    app.on_flush(move || {
        // Stopping accepting is the shutdown; the pumps hold nothing, so
        // being cut off mid-poll costs at worst a duplicate (S3).
        if let Some(tx) = stop.lock().expect("stop lock").take() {
            let _ = tx.send(());
        }
    });
    app.run().await
}

/// Read a config file for the `test` subcommand.
///
/// This verb runs before the kit parses anything, so there is no `App` to
/// ask for `project_table()`. It does the same strip from the spec, which
/// names both halves itself (`knob_keys` since 1.2.0, `kit_sections` since
/// 2.0.0) — so this is the kit's list, not knowledge kept here.
///
/// fix-3: without the strip this verb answered "not valid TOML" on a file
/// that is valid TOML and that the service starts from happily, which is
/// exactly the file the operations runbook tells an operator to point it at.
fn load_for_dry_run(path: &str) -> Result<config::Config, String> {
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

fn dry_run(args: &[String]) -> ExitCode {
    let mut config_path = "config.toml".to_string();
    let mut profile_name = String::new();
    let mut input_path = String::new();
    let mut i = 0;
    while i < args.len() {
        match args[i].as_str() {
            "--config" => config_path = args.get(i + 1).cloned().unwrap_or_default(),
            "--profile" => profile_name = args.get(i + 1).cloned().unwrap_or_default(),
            "--input" => input_path = args.get(i + 1).cloned().unwrap_or_default(),
            other => {
                eprintln!("unknown option '{other}'.\n{}", usage());
                return ExitCode::FAILURE;
            }
        }
        i += 2;
    }
    if profile_name.is_empty() || input_path.is_empty() {
        eprintln!("{}", usage());
        return ExitCode::FAILURE;
    }
    let config = match load_for_dry_run(&config_path) {
        Ok(c) => c,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };
    let Some(profile) = config.profiles.iter().find(|p| p.name == profile_name) else {
        let names: Vec<&str> = config.profiles.iter().map(|p| p.name.as_str()).collect();
        eprintln!(
            "{config_path}: there is no profile named '{profile_name}'. What now: pick one of: {}.",
            names.join(", ")
        );
        return ExitCode::FAILURE;
    };
    let payload = match std::fs::read(&input_path) {
        Ok(bytes) => bytes,
        Err(e) => {
            eprintln!(
                "{input_path}: cannot be read ({e}). What now: point --input at a file \
                 holding one recorded message."
            );
            return ExitCode::FAILURE;
        }
    };
    match http_switchboard::translate::prepare(profile, &payload) {
        Ok(delivery) => {
            match &delivery.target {
                http_switchboard::translate::Target::Url { url, method } => {
                    println!("would send {method} {url}")
                }
                http_switchboard::translate::Target::KyuTopic { topic } => {
                    println!("would publish to kyu topic {topic}")
                }
            }
            println!("content-type: {}", delivery.content_type);
            for name in delivery.headers.keys() {
                // The names, never the values: a header may carry a token,
                // and this output ends up in terminals and pasted into chats.
                println!("header: {name}: ***");
            }
            println!("---");
            println!("{}", delivery.body);
            ExitCode::SUCCESS
        }
        Err(e) => {
            eprintln!("{e}");
            ExitCode::FAILURE
        }
    }
}
