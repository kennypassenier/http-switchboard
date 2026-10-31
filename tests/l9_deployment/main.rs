//! Phase 7 — the wrapping, not the app (G9, and part of G7).
//!
//! The deployment artifacts were touched by no test: the container's
//! healthcheck depends on `--healthcheck`, which nothing ran, and a
//! broken one presents as a restart loop inside an image with no shell to
//! debug it. The CLI verbs are the runbook's own procedures, so a
//! regression there lands in the emergency path.

#[path = "../support/mod.rs"]
mod support;

use std::io::Write;
use std::process::{Child, Command, Stdio};
use support::TestServer;

/// The two secrets a dashboard-carrying service needs before it will run
/// (3.0.0, H1). Fixed values, because a test is not a deployment: on a
/// host they come from `http-switchboard gen-secret` into the env file.
const TEST_TOKEN: &str = "test-admin-token-not-a-real-one";

const TEST_SECRET_KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

fn binary() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("http-switchboard");
    path
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

struct Killed(Child);

impl Drop for Killed {
    fn drop(&mut self) {
        let _ = self.0.kill();
        let _ = self.0.wait();
    }
}

fn tempdir(tag: &str) -> std::path::PathBuf {
    let dir = std::env::temp_dir().join(format!("hsw-{tag}-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    dir
}

mod cli;
mod healthcheck;
mod secrets;
