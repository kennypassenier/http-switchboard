//! feat-reload-1 — a new config read while the service keeps running
//! (Kenny, 2026-09-26: Later → Essential, "with tests for a broken and a
//! half-written config").
//!
//! The bar: a valid change reaches the next message; anything short of a
//! complete, valid config of the same shape changes nothing, and the
//! service keeps delivering with what it had.

#[path = "../support/mod.rs"]
mod support;

use http_switchboard::adapters::{HttpSink, TokioClock};
use http_switchboard::config;
use http_switchboard::inbound;
use http_switchboard::obs::Registry;
use http_switchboard::reload::{self, Outcome, ProfileStore};
use std::collections::HashMap;
use std::io::Write;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use support::TestServer;

fn env() -> impl config::EnvLookup {
    let map: HashMap<String, String> = HashMap::new();
    move |k: &str| map.get(k).cloned()
}

fn profile(receiver: &str, body: &str) -> String {
    format!(
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "{receiver}/in" }}
content_type = "application/json"
retries = 0
timeout_ms = 2000
body = '''{body}'''
"#
    )
}

fn load(text: &str) -> Result<config::Config, String> {
    config::load("t.toml", text, &env()).map_err(|e| e.to_string())
}

/// The inbound side over a store, as the binary wires it.
async fn serve(cfg: &config::Config, store: Arc<ProfileStore>) -> String {
    let router = inbound::router(
        cfg,
        store,
        Arc::new(HttpSink::new(None, None, 2_000)),
        Arc::new(TokioClock),
        Arc::new(Registry::new()),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    format!("http://{addr}")
}

async fn post(url: &str) -> u16 {
    reqwest::Client::new()
        .post(url)
        .header("content-type", "application/json")
        .body(r#"{"x": 7}"#)
        .send()
        .await
        .expect("the server must answer")
        .status()
        .as_u16()
}

const OLD: &str = r#"{"old": {{ x }}}"#;

const NEW: &str = r#"{"new": {{ x }}}"#;

mod kyu_side;
mod outcomes;
mod signal;
