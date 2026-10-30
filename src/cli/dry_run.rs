//! `http-switchboard test`: render a profile against a recorded message
//! and print what would go out, without sending anything (W4).

use std::process::ExitCode;

use crate::translate::{self, Delivery, Target};

use super::config_file::load_config_file;
use super::spec::usage;

/// The options of the `test` subcommand.
struct Args {
    config_path: String,
    profile_name: String,
    input_path: String,
}

/// The config file the dry run reads when `--config` is not given.
const DEFAULT_CONFIG_PATH: &str = "config.toml";

/// Parse `--config`, `--profile`, `--input`; an unknown option, or a
/// missing profile or input, is an error carrying the text to print.
fn parse_args(args: &[String]) -> Result<Args, String> {
    let mut parsed = Args {
        config_path: DEFAULT_CONFIG_PATH.to_string(),
        profile_name: String::new(),
        input_path: String::new(),
    };
    let mut i = 0;
    while i < args.len() {
        // A flag without a value reads as empty and is caught below
        // (profile and input) or by the file read (config).
        let value = args.get(i + 1).cloned().unwrap_or_default();
        match args[i].as_str() {
            "--config" => parsed.config_path = value,
            "--profile" => parsed.profile_name = value,
            "--input" => parsed.input_path = value,
            other => return Err(format!("unknown option '{other}'.\n{}", usage())),
        }
        i += 2;
    }
    if parsed.profile_name.is_empty() || parsed.input_path.is_empty() {
        return Err(usage());
    }
    Ok(parsed)
}

/// Print what `delivery` would do. Header names only, never the values: a
/// header may carry a token, and this output ends up in terminals and
/// pasted into chats.
fn print_delivery(delivery: &Delivery) {
    match &delivery.target {
        Target::Url { url, method } => println!("would send {method} {url}"),
        Target::KyuTopic { topic } => println!("would publish to kyu topic {topic}"),
    }
    println!("content-type: {}", delivery.content_type);
    for name in delivery.headers.keys() {
        println!("header: {name}: ***");
    }
    println!("---");
    println!("{}", delivery.body);
}

/// Run the subcommand; the exit code is the result.
pub fn dry_run(args: &[String]) -> ExitCode {
    match run(args) {
        Ok(()) => ExitCode::SUCCESS,
        Err(message) => {
            eprintln!("{message}");
            ExitCode::FAILURE
        }
    }
}

fn run(args: &[String]) -> Result<(), String> {
    let args = parse_args(args)?;
    let config = load_config_file(&args.config_path)?;
    let Some(profile) = config.profiles.iter().find(|p| p.name == args.profile_name) else {
        let names: Vec<&str> = config.profiles.iter().map(|p| p.name.as_str()).collect();
        return Err(format!(
            "{}: there is no profile named '{}'. What now: pick one of: {}.",
            args.config_path,
            args.profile_name,
            names.join(", ")
        ));
    };
    let payload = std::fs::read(&args.input_path).map_err(|e| {
        format!(
            "{}: cannot be read ({e}). What now: point --input at a file \
             holding one recorded message.",
            args.input_path
        )
    })?;
    let delivery = translate::prepare(profile, &payload).map_err(|e| e.to_string())?;
    print_delivery(&delivery);
    Ok(())
}
