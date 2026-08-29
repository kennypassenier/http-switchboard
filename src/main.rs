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
         http-switchboard --healthcheck [http://127.0.0.1:8080/healthz]\n\
         http-switchboard test --config <config.toml> --profile <name> --input <message.json>\n"
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
        // W4: render a profile against a recorded message and print what
        // would go out, without sending anything. The difference between a
        // config file you dare to edit and one you avoid.
        Some("test") => {
            let mut config_path = "config.toml".to_string();
            let mut profile_name = String::new();
            let mut input_path = String::new();
            let mut i = 1;
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

            let config = match load(&config_path) {
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

            return match http_switchboard::translate::prepare(profile, &payload) {
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
                        // The names, never the values: a header may carry a
                        // token, and this output ends up in terminals and
                        // pasted into chats.
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
