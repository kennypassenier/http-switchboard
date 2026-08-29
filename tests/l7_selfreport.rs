//! L7 — the service tells the house when it falls over (W11; AR12).
//!
//! The bar: a failing profile produces exactly ONE event, not one per
//! message, and recovery produces one more. Twenty minutes of Home
//! Assistant downtime with a backlog would otherwise arrive as a burst of
//! warnings in a house whose dispatcher exists to prevent exactly that.

mod support;

use std::collections::HashMap;

use http_switchboard::app::App;
use http_switchboard::config;
use support::{KyuHarness, TestServer};

fn env() -> impl config::EnvLookup {
    let map: HashMap<String, String> = HashMap::new();
    move |k: &str| map.get(k).cloned()
}

fn free_port() -> u16 {
    let listener = std::net::TcpListener::bind("127.0.0.1:0").unwrap();
    listener.local_addr().unwrap().port()
}

async fn read_topic(base_url: &str, topic: &str, subscription: &str) -> Vec<serde_json::Value> {
    let mut out = Vec::new();
    for _ in 0..10 {
        let response = reqwest::Client::new()
            .get(format!(
                "{base_url}/t/{topic}/next?as={subscription}&envelope=json&wait=1&from=beginning"
            ))
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
            .send()
            .await;
    }
    out
}

#[tokio::test]
async fn w11_e2e_a_failing_profile_reports_once_and_recovery_reports_once() {
    let Some(hub) = KyuHarness::start().await else {
        eprintln!("skipped: set KYU_IMAGE to run this against a real kyu");
        return;
    };
    // The receiver refuses the first three messages, then accepts.
    let receiver = TestServer::start(vec![500, 500, 500, 200]).await;
    let port = free_port();
    let text = format!(
        r#"
[kyu]
base_url = "{}"

[reporting]
topic = "switchboard.events"

[[profiles]]
name = "reporting-profile"
from = {{ kyu_topic = "report.raw" }}
to = {{ url = "{}/in" }}
content_type = "application/json"
retries = 0
timeout_ms = 2000
body = '''{{"alert": {{{{ name }}}}}}'''
"#,
        hub.base_url, receiver.base_url
    );
    let cfg = config::load("t.toml", &text, &env()).unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        App::from_config(cfg)
            .run(format!("127.0.0.1:{port}").parse().unwrap(), async {
                let _ = stop_rx.await;
            })
            .await
            .unwrap();
    });
    for _ in 0..60 {
        if reqwest::get(format!("http://127.0.0.1:{port}/healthz"))
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    // Three messages, all refused at first: the profile falls over once,
    // is redelivered, and eventually gets through.
    for i in 0..3 {
        hub.publish("report.raw", &format!(r#"{{"name": "alert{i}"}}"#))
            .await;
    }

    // Wait until the receiver has accepted one, which means the profile
    // has been through failing and back.
    let mut recovered = false;
    for _ in 0..60 {
        if receiver
            .received()
            .len()
            .checked_sub(3)
            .map(|n| n >= 1)
            .unwrap_or(false)
        {
            recovered = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    assert!(
        recovered,
        "the receiver should have accepted a message after the refusals: {:?}",
        receiver.received().len()
    );
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let events = read_topic(&hub.base_url, "switchboard.events", "reader").await;
    let failing: Vec<_> = events
        .iter()
        .filter(|e| e["event"] == "profile.failing")
        .collect();
    let recovered_events: Vec<_> = events
        .iter()
        .filter(|e| e["event"] == "profile.recovered")
        .collect();

    assert_eq!(
        failing.len(),
        1,
        "exactly one failure event, not one per message: {events:?}"
    );
    assert_eq!(
        recovered_events.len(),
        1,
        "and exactly one recovery event: {events:?}"
    );
    assert_eq!(failing[0]["profile"], "reporting-profile");
    assert!(
        failing[0]["detail"].as_str().unwrap_or_default().len() > 5,
        "the event says what happened: {events:?}"
    );
    for event in &events {
        let text = event.to_string();
        assert!(
            !text.contains("alert0") && !text.contains("alert1"),
            "an event must never carry the payload: {text}"
        );
    }

    let _ = stop_tx.send(());
}

#[tokio::test]
async fn w11_e2e_an_inbound_profile_reports_itself_too() {
    // Phase 7, G3: the transition logic lived only in the pump loop, so a
    // profile fed by a webhook fell over in silence — and that is the
    // source shape every second profile tends to use.
    let Some(hub) = KyuHarness::start().await else {
        eprintln!("skipped: set KYU_IMAGE to run this against a real kyu");
        return;
    };
    let receiver = TestServer::start(vec![500, 200]).await;
    let port = free_port();
    let text = format!(
        r#"
[kyu]
base_url = "{}"

[reporting]
topic = "switchboard.events"

[[profiles]]
name = "inbound-profile"
from = {{ http_path = "/hook" }}
to = {{ url = "{}/in" }}
content_type = "application/json"
retries = 0
timeout_ms = 2000
body = '''{{"alert": {{{{ name }}}}}}'''
"#,
        hub.base_url, receiver.base_url
    );
    let cfg = config::load("t.toml", &text, &env()).unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        App::from_config(cfg)
            .run(format!("127.0.0.1:{port}").parse().unwrap(), async {
                let _ = stop_rx.await;
            })
            .await
            .unwrap();
    });
    for _ in 0..60 {
        if reqwest::get(format!("http://127.0.0.1:{port}/healthz"))
            .await
            .map(|r| r.status().is_success())
            .unwrap_or(false)
        {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(100)).await;
    }

    let post = |body: &'static str| async move {
        reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/hook"))
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    };

    assert_eq!(
        post(r#"{"name": "first"}"#).await,
        502,
        "the receiver refuses"
    );
    assert_eq!(post(r#"{"name": "second"}"#).await, 200, "and then accepts");
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let events = read_topic(&hub.base_url, "switchboard.events", "reader2").await;
    let failing = events
        .iter()
        .filter(|e| e["event"] == "profile.failing")
        .count();
    let recovered = events
        .iter()
        .filter(|e| e["event"] == "profile.recovered")
        .count();

    assert_eq!(failing, 1, "one failure event: {events:?}");
    assert_eq!(recovered, 1, "one recovery event: {events:?}");
    assert_eq!(events[0]["profile"], "inbound-profile");
    for event in &events {
        assert!(
            !event.to_string().contains("first"),
            "an event must never carry the payload: {event}"
        );
    }

    let _ = stop_tx.send(());
}
