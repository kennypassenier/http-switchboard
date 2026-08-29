//! The binary: read the config, start the service, stop cleanly (L6b).

use std::net::SocketAddr;
use std::process::ExitCode;

use http_switchboard::app::App;
use http_switchboard::config::{self, ProcessEnv};

const DEFAULT_LISTEN: &str = "0.0.0.0:8080";

fn usage() -> String {
    format!(
        "http-switchboard <config.toml> [--listen {DEFAULT_LISTEN}]\n\
         http-switchboard --check-config <config.toml>\n\
         http-switchboard --healthcheck [http://127.0.0.1:8080/healthz]\n"
    )
}

#[tokio::main]
async fn main() -> ExitCode {
    let args: Vec<String> = std::env::args().skip(1).collect();

    match args.first().map(String::as_str) {
        // AR13: a distroless image has no shell and no curl, so the
        // container's healthcheck is the binary asking itself.
        Some("--healthcheck") => {
            let url = args
                .get(1)
                .cloned()
                .unwrap_or_else(|| "http://127.0.0.1:8080/healthz".to_string());
            return match reqwest::get(&url).await {
                Ok(r) if r.status().is_success() => ExitCode::SUCCESS,
                Ok(r) => {
                    eprintln!("unhealthy: {url} answered {}", r.status());
                    ExitCode::FAILURE
                }
                Err(e) => {
                    eprintln!("unhealthy: {url} could not be reached: {e}");
                    ExitCode::FAILURE
                }
            };
        }
        Some("--check-config") => {
            let Some(path) = args.get(1) else {
                eprintln!("{}", usage());
                return ExitCode::FAILURE;
            };
            return match load(path) {
                Ok(config) => {
                    println!("{path}: ok, {} profile(s)", config.profiles.len());
                    ExitCode::SUCCESS
                }
                Err(message) => {
                    eprintln!("{message}");
                    ExitCode::FAILURE
                }
            };
        }
        None | Some("--help") | Some("-h") => {
            eprintln!("{}", usage());
            return ExitCode::FAILURE;
        }
        _ => {}
    }

    let path = &args[0];
    let listen = match args.iter().position(|a| a == "--listen") {
        Some(i) => args.get(i + 1).cloned(),
        None => None,
    }
    .unwrap_or_else(|| DEFAULT_LISTEN.to_string());
    let listen: SocketAddr = match listen.parse() {
        Ok(addr) => addr,
        Err(e) => {
            eprintln!(
                "--listen {listen} is not an address: {e}. What now: write it as host:port, \
                 for example {DEFAULT_LISTEN}."
            );
            return ExitCode::FAILURE;
        }
    };

    // K10: a config that does not hold up stops the process here, with
    // the remedy in the message, rather than starting half-working.
    let config = match load(path) {
        Ok(config) => config,
        Err(message) => {
            eprintln!("{message}");
            return ExitCode::FAILURE;
        }
    };

    match App::from_config(config)
        .run(listen, shutdown_signal())
        .await
    {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn load(path: &str) -> Result<http_switchboard::config::Config, String> {
    let text = std::fs::read_to_string(path).map_err(|e| {
        format!(
            "{path}: cannot be read ({e}). What now: check the path and that the file is \
             readable by the user this service runs as."
        )
    })?;
    config::load(path, &text, &ProcessEnv).map_err(|e| e.to_string())
}

/// SIGTERM is what the container runtime sends; ctrl-c is what a person
/// sends. Both mean the same thing here.
async fn shutdown_signal() {
    let ctrl_c = async {
        let _ = tokio::signal::ctrl_c().await;
    };
    #[cfg(unix)]
    let terminate = async {
        if let Ok(mut signal) =
            tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())
        {
            signal.recv().await;
        }
    };
    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => {}
        _ = terminate => {}
    }
}
