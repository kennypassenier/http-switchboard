//! L7 — the service tells the house when it falls over (W11; AR12).
//!
//! The bar: a failing profile produces exactly ONE event, not one per
//! message, and recovery produces one more. Twenty minutes of Home
//! Assistant downtime with a backlog would otherwise arrive as a burst of
//! warnings in a house whose dispatcher exists to prevent exactly that.

#[path = "../support/mod.rs"]
mod support;

use http_switchboard::app::App;
use http_switchboard::config;
use std::collections::HashMap;
use support::{KyuHarness, TestServer};

fn env() -> impl config::EnvLookup {
    let map: HashMap<String, String> = HashMap::new();
    move |k: &str| map.get(k).cloned()
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

async fn read_topic(
    base_url: &str,
    token: &str,
    topic: &str,
    subscription: &str,
) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for n in 0..10 {
        // Only the first read asks for the history; the later ones carry on
        // where the subscription stands. Against kyu-latest 7.0.0 a reader
        // that sent `from=beginning` on every read got the events twice
        // (2026-11-04); kyu could not reproduce that with a plain ack loop,
        // so the cause is not pinned down, but asking once is all a reader
        // needs.
        let from = if n == 0 { "&from=beginning" } else { "" };
        let response = reqwest::Client::new()
            .get(format!(
                "{base_url}/t/{topic}/next?as={subscription}&envelope=json&wait=1{from}"
            ))
            .bearer_auth(token)
            .send()
            .await
            .unwrap();
        if response.status().as_u16() != 200 {
            break;
        }
        let envelope: serde_json::Value =
            serde_json::from_str(&response.text().await.unwrap()).unwrap();
        let id = envelope["id"].as_str().unwrap_or_default().to_string();
        out.push(envelope["payload"].clone());
        let _ = reqwest::Client::new()
            .post(format!("{base_url}/t/{topic}/ack/{id}?as={subscription}"))
            .bearer_auth(token)
            .send()
            .await;
    }
    out
}

mod inbound_source;
mod kyu_source;
