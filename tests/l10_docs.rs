//! L10 — the documented command lines are the command lines the binary has
//! (correction C1, 2026-09-09).
//!
//! The 2.0.0 migration moved the config path from a positional argument to
//! `--config`, and the README's "Running it" block kept showing the old
//! form for three days and one release. Nothing caught it because a test
//! reads the code, and nobody re-ran the documentation.
//!
//! So: every `http-switchboard …` line in the two documents a user follows
//! is handed to the built binary, and the test fails when the argument
//! parser rejects it. It deliberately does not check that the command
//! *succeeds* — a documented path like `/etc/http-switchboard/config.toml`
//! does not exist here, and that refusal is the right answer. It checks
//! only that the binary understands what was asked.
//!
//! The measure is narrow on purpose: it proves the shape of a command, not
//! its outcome. What it cannot see is written down in TEST_PLAN.md.

use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// The documents a person follows to run this service.
const DOCUMENTED: [&str; 2] = ["README.md", "docs/USER_GUIDE.md"];

/// What clap says when it does not understand the command line. These are
/// the failures this test exists for; every other refusal is legitimate.
const REJECTIONS: [&str; 4] = [
    "unrecognized subcommand",
    "unexpected argument",
    "invalid value",
    "unexpected value",
];

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
}

fn binary() -> PathBuf {
    // The test binary lives in target/<profile>/deps; the service binary
    // is two levels up from there.
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("http-switchboard");
    path
}

/// Every line inside a fenced block that invokes this binary, with the
/// shell's line continuations folded back into one line.
fn documented_commands(markdown: &str) -> Vec<String> {
    let mut out = Vec::new();
    let mut fenced = false;
    let mut pending = String::new();
    for line in markdown.lines() {
        if line.trim_start().starts_with("```") {
            fenced = !fenced;
            pending.clear();
            continue;
        }
        if !fenced {
            continue;
        }
        let trimmed = line.trim();
        if pending.is_empty() && !trimmed.starts_with("http-switchboard ") {
            continue;
        }
        if let Some(head) = trimmed.strip_suffix('\\') {
            pending.push_str(strip_comment(head));
            pending.push(' ');
            continue;
        }
        pending.push_str(strip_comment(trimmed));
        out.push(pending.trim_end().to_string());
        pending.clear();
    }
    out
}

/// Everything before a shell comment. A ` #` in a documented command line
/// is a note to the reader, not an argument — and the binary, correctly,
/// refuses to parse one. Found by this very test on the first documentation
/// written after it existed (C1, fallback of field 8 not needed: the
/// extractor was incomplete, not brittle).
fn strip_comment(line: &str) -> &str {
    match line.find(" #") {
        Some(i) => line[..i].trim_end(),
        None => line.trim_end(),
    }
}

/// Run one documented command against a state directory and a config path
/// that do not exist, so a start-shaped command stops at the missing file
/// instead of binding a port on the machine running the tests.
fn parse_only(command: &str, scratch: &Path) -> String {
    let absent = scratch.join("absent.toml");
    let mut args: Vec<String> = command
        .split_whitespace()
        .skip(1)
        .map(String::from)
        .collect();
    // A documented --config points at a deployed path that may or may not
    // exist here; either way the test must not read it.
    if let Some(i) = args.iter().position(|a| a == "--config") {
        if i + 1 < args.len() {
            args[i + 1] = absent.display().to_string();
        }
    }
    let mut child = Command::new(binary())
        .args(&args)
        .env("HTTP_SWITCHBOARD_CONFIG", &absent)
        .env("HTTP_SWITCHBOARD_STATE_DIR", scratch)
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .expect("the binary must be built");

    // A command that nonetheless starts a server is killed rather than
    // allowed to hang the suite.
    let deadline = std::time::Instant::now() + std::time::Duration::from_secs(10);
    loop {
        match child.try_wait().expect("waiting on the child must work") {
            Some(_) => break,
            None if std::time::Instant::now() >= deadline => {
                let _ = child.kill();
                let _ = child.wait();
                break;
            }
            None => std::thread::sleep(std::time::Duration::from_millis(50)),
        }
    }

    let mut out = String::new();
    let mut err = String::new();
    if let Some(mut s) = child.stdout.take() {
        let _ = s.read_to_string(&mut out);
    }
    if let Some(mut s) = child.stderr.take() {
        let _ = s.read_to_string(&mut err);
    }
    format!("{out}{err}")
}

#[test]
fn l10_every_documented_command_is_one_the_binary_accepts() {
    let root = repo_root();
    let scratch = std::env::temp_dir().join(format!("hsw-docs-{}", std::process::id()));
    std::fs::create_dir_all(&scratch).unwrap();

    let mut checked = 0usize;
    for doc in DOCUMENTED {
        let text = std::fs::read_to_string(root.join(doc))
            .unwrap_or_else(|e| panic!("{doc} must be readable: {e}"));
        for command in documented_commands(&text) {
            let answer = parse_only(&command, &scratch);
            for rejection in REJECTIONS {
                assert!(
                    !answer.contains(rejection),
                    "{doc} documents a command this binary refuses to parse.\n  \
                     command: {command}\n  answer:  {}\nWhat now: fix the document, \
                     not this test — the binary is what ships.",
                    answer.trim()
                );
            }
            checked += 1;
        }
    }
    let _ = std::fs::remove_dir_all(&scratch);

    // A silent zero would make this test pass by finding nothing, which is
    // the failure mode of every test that greps for its own input.
    assert!(
        checked >= 4,
        "only {checked} documented commands were found; the extraction is broken \
         (fences changed shape?) and this test is proving nothing"
    );
}
