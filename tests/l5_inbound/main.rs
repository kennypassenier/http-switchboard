//! L5 — the inbound side (K1, W1, W8; AR3, AR9).

#[path = "../support/mod.rs"]
mod support;

use http_switchboard::adapters::{HttpSink, TokioClock};
use http_switchboard::config;
use http_switchboard::inbound;
use std::collections::HashMap;
use std::sync::Arc;
use support::TestServer;

fn env(pairs: &[(&str, &str)]) -> impl config::EnvLookup {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

/// Start the inbound server for a config and return its base URL.
async fn serve(config_text: &str) -> String {
    let cfg = config::load("t.toml", config_text, &env(&[("HOOK_TOKEN", "let-me-in")]))
        .expect("config must load");
    let router = inbound::router(
        &cfg,
        Arc::new(http_switchboard::reload::ProfileStore::new(&cfg)),
        Arc::new(HttpSink::new(None, None, 2_000)),
        Arc::new(TokioClock),
        Arc::new(http_switchboard::obs::Registry::new()),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    format!("http://{addr}")
}

fn one_profile(receiver: &str) -> String {
    format!(
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "{receiver}/in" }}
content_type = "application/json"
retries = 0
timeout_ms = 2000
body = '''{{"x": {{{{ x }}}}}}'''
"#
    )
}

/// A second profile on the SAME path, for the fan-out cases.
fn second_profile(receiver: &str) -> String {
    format!(
        r#"
[[profiles]]
name = "hook-log"
from = {{ http_path = "/hook" }}
to = {{ url = "{receiver}/log" }}
content_type = "application/json"
retries = 0
body = '''{{"copy": {{{{ x }}}}}}'''
"#
    )
}

async fn post(url: &str, body: &str, token: Option<&str>) -> (u16, String) {
    let mut req = reqwest::Client::new()
        .post(url)
        .header("content-type", "application/json")
        .body(body.to_string());
    if let Some(t) = token {
        req = req.header("authorization", format!("Bearer {t}"));
    }
    let response = req.send().await.expect("the server must answer");
    let status = response.status().as_u16();
    (status, response.text().await.unwrap_or_default())
}

mod answers;
mod fanout;
mod routing;
