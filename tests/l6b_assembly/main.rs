//! L6b — the parts, assembled into something that runs.
//!
//! Exit criterion: the binary loads a real config, a message travels the
//! whole way through the running service, and a broken config stops it
//! with a non-zero exit and a remedy.

#[path = "../support/mod.rs"]
mod support;

use http_switchboard::app::App;
use http_switchboard::config;
use std::collections::HashMap;
use support::{KyuHarness, TestServer};

fn env(pairs: &[(&str, &str)]) -> impl config::EnvLookup {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

/// The two secrets a dashboard-carrying service needs before it will run
/// (3.0.0, H1). Fixed values, because a test is not a deployment: on a
/// host they come from `http-switchboard gen-secret` into the env file.
const TEST_TOKEN: &str = "test-admin-token-not-a-real-one";

const TEST_SECRET_KEY: &str = "00112233445566778899aabbccddeeff00112233445566778899aabbccddeeff";

fn binary() -> std::path::PathBuf {
    // The test binary lives in target/<profile>/deps; the service binary
    // is two levels up from there.
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("http-switchboard");
    path
}

async fn wait_until_healthy(port: u16) {
    for _ in 0..60 {
        if let Ok(r) = reqwest::get(format!("http://127.0.0.1:{port}/healthz")).await {
            if r.status().is_success() {
                return;
            }
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }
    panic!("the service did not become healthy");
}

mod check_cli;
mod running;
