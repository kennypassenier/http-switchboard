//! The binary (L6b), on chassis since 2.0.0: the kit owns the command
//! line, configuration layers, logging, `/healthz`, `/metrics`, the
//! graceful shutdown and signed self-update; this file assembles the
//! switchboard on top of it — inbound routes as public routes (third-party
//! senders keep their per-path `inbound_token`), one health subsystem per
//! profile, the pumps started after the bind and stopped in the shutdown
//! window. The `test` dry-run subcommand is the switchboard's own and is
//! dispatched before the kit sees the arguments.

use std::process::ExitCode;
use std::sync::Arc;

use axum::Router;
use chassis::{App, AppSpec, Control};
use http_switchboard::app::App as Switchboard;
use http_switchboard::config::{self, ProcessEnv, Source};
use http_switchboard::obs::{ProfileSubsystem, RegistryMetrics};

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

    let spec = AppSpec {
        name: "http-switchboard",
        version: env!("CARGO_PKG_VERSION"),
        repository: Some("kennypassenier/http-switchboard"),
        ..Default::default()
    };
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
    let config = match load_own(&path, &app.spec.knob_keys()) {
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
    // Inbound webhooks are PUBLIC routes: third-party senders cannot carry a
    // kit client token, and the per-path inbound_token check lives in the
    // handler. Without the dashboard feature the kit merges these as they
    // are; /healthz and /metrics are the kit's.
    app.api_routes(switchboard.profile_router());

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

/// Read the shared config file and hand the switchboard its part.
fn load_own(path: &str, kit_keys: &[&str]) -> Result<config::Config, String> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "{path}: cannot be read ({e}). What now: check the path (--config / \
             HTTP_SWITCHBOARD_CONFIG) and that the file is readable by the user this service \
             runs as."
        )
    })?;
    let mut table: toml::Table = toml::from_str(&text).map_err(|e| {
        format!("{path}: is not valid TOML: {e}. What now: fix the file; the kit's and the switchboard's keys share it.")
    })?;
    for key in kit_keys {
        table.remove(*key);
    }
    table.remove("notify");
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
    let config = match load_own(&config_path, &[]) {
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
