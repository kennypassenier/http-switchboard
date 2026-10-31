//! L1 — configuration and every check on it (K8, K9, K10; AR2, AR5, AR6).
//!
//! The bar agreed at the Phase 2 gate: for every class of config error a
//! test that startup stops AND that the message names the file, the
//! profile and a remedy. "Startup stops" is `load` returning `Err`; the
//! binary turns that into a non-zero exit.

use http_switchboard::config::{self, ConfigError, Sink, Source};
use std::collections::HashMap;

fn env(pairs: &[(&str, &str)]) -> impl config::EnvLookup {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

fn load(text: &str) -> Result<config::Config, ConfigError> {
    config::load(
        "config.toml",
        text,
        &env(&[("KYU_TOKEN", "t0ken-from-the-vault")]),
    )
}

fn err(text: &str) -> String {
    match load(text) {
        Ok(_) => panic!("expected this config to be refused, but it loaded"),
        Err(e) => e.to_string(),
    }
}

/// Every refusal must name the file and carry a remedy — the K10 bar,
/// applied to whatever the caller just produced.
fn assert_usable(message: &str, must_mention: &[&str]) {
    assert!(
        message.contains("config.toml"),
        "error does not name the file: {message}"
    );
    assert!(
        message.contains("What now:"),
        "error carries no remedy (standing rule 11): {message}"
    );
    for needle in must_mention {
        assert!(
            message.contains(needle),
            "error does not mention {needle:?}: {message}"
        );
    }
}

const GOOD: &str = r#"
[kyu]
base_url = "http://127.0.0.1:8080"
token = "${KYU_TOKEN}"

[reporting]
topic = "switchboard.events"

[[profiles]]
name = "alertmanager"
from = { kyu_topic = "alerts.raw" }
to = { kyu_topic = "alerts.homelab" }
content_type = "application/json"
body = '''
{"alert": {{ alerts.0.labels.alertname }}}
'''

[[profiles]]
name = "uptime-kuma"
from = { http_path = "/uptime-kuma" }
to = { url = "http://127.0.0.1:9999/hook" }
content_type = "application/json"
method = "PUT"
timeout_ms = 4000
headers = { authorization = "${KYU_TOKEN}" }
body = "{}"
"#;

mod consistency;
mod endpoints;
mod loading;
mod structure;
