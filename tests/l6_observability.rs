//! L6 — seeing that it works, not only that it runs (W5, W6, W7; AR11).

mod support;

use std::collections::HashMap;
use std::sync::Arc;

use http_switchboard::adapters::{HttpSink, TokioClock};
use http_switchboard::obs::Registry;
use http_switchboard::{config, inbound};
use support::TestServer;

fn env() -> impl config::EnvLookup {
    let map: HashMap<String, String> = HashMap::new();
    move |k: &str| map.get(k).cloned()
}

async fn serve(receiver: &str) -> (String, Arc<Registry>) {
    let text = format!(
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
    );
    let cfg = config::load("t.toml", &text, &env()).expect("config must load");
    let registry = Arc::new(Registry::new());
    for p in &cfg.profiles {
        registry.register(&p.name);
    }
    let router = inbound::router(
        &cfg,
        Arc::new(HttpSink::new(None, None, 2_000)),
        Arc::new(TokioClock),
        Arc::clone(&registry),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{addr}"), registry)
}

async fn get(url: &str) -> (u16, String) {
    let response = reqwest::get(url).await.expect("the endpoint must answer");
    (
        response.status().as_u16(),
        response.text().await.unwrap_or_default(),
    )
}

async fn post(url: &str, body: &str) -> u16 {
    reqwest::Client::new()
        .post(url)
        .header("content-type", "application/json")
        .body(body.to_string())
        .send()
        .await
        .unwrap()
        .status()
        .as_u16()
}

#[tokio::test]
async fn w5_healthz_answers_without_a_token_and_lists_the_profiles() {
    let receiver = TestServer::start(vec![200]).await;
    let (base, _registry) = serve(&receiver.base_url).await;

    let (status, body) = get(&format!("{base}/healthz")).await;

    assert_eq!(status, 200);
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON: {body}");
    assert_eq!(parsed["status"], "ok");
    assert_eq!(parsed["profiles"][0]["name"], "hook");
}

#[tokio::test]
async fn w5_a_failing_profile_is_visible_without_restarting_the_container() {
    // Phase 7, G2. Two callers want different answers from one fact:
    //  * the container's healthcheck asks "is this process alive" — it
    //    must stay 200 while Home Assistant is down, or the orchestrator
    //    restarts us for someone else's outage, and every restart resets
    //    the pump state (turning AR12's one failure event into one per
    //    restart);
    //  * Uptime Kuma asks "is it doing its job" and wants a non-2xx.
    let receiver = TestServer::start(vec![500]).await;
    let (base, _registry) = serve(&receiver.base_url).await;

    assert_eq!(get(&format!("{base}/healthz")).await.0, 200);
    assert_eq!(get(&format!("{base}/healthz?strict=1")).await.0, 200);

    assert_eq!(post(&format!("{base}/hook"), r#"{"x": 1}"#).await, 502);

    let (liveness, body) = get(&format!("{base}/healthz")).await;
    assert_eq!(
        liveness, 200,
        "liveness must not go red because a receiver is down: {body}"
    );
    assert!(body.contains(r#""status":"degraded""#), "{body}");
    assert!(body.contains(r#""state":"failing""#), "{body}");

    let (strict, body) = get(&format!("{base}/healthz?strict=1")).await;
    assert_eq!(
        strict, 503,
        "Uptime Kuma has to see this without reading the body: {body}"
    );
}

#[tokio::test]
async fn w6_metrics_move_after_a_success_and_a_failure() {
    let good = TestServer::start(vec![200]).await;
    let (base, _registry) = serve(&good.base_url).await;

    assert_eq!(post(&format!("{base}/hook"), r#"{"x": 1}"#).await, 200);
    // A body that cannot render counts as received and failed.
    assert_eq!(post(&format!("{base}/hook"), r#"{"nope": 1}"#).await, 502);

    let (status, text) = get(&format!("{base}/metrics")).await;

    assert_eq!(status, 200);
    assert!(
        text.contains(r#"switchboard_messages_received_total{profile="hook"} 2"#),
        "{text}"
    );
    assert!(
        text.contains(r#"switchboard_messages_delivered_total{profile="hook"} 1"#),
        "{text}"
    );
    assert!(
        text.contains(r#"switchboard_messages_failed_total{profile="hook"} 1"#),
        "{text}"
    );
    assert!(
        text.contains("switchboard_delivery_duration_ms_total"),
        "the duration metric is missing: {text}"
    );
}

#[tokio::test]
async fn ar11_neither_endpoint_echoes_anything_from_a_message() {
    let receiver = TestServer::start(vec![200]).await;
    let (base, _registry) = serve(&receiver.base_url).await;

    post(&format!("{base}/hook"), r#"{"x": "a-very-private-value"}"#).await;

    let (_, health) = get(&format!("{base}/healthz")).await;
    let (_, metrics) = get(&format!("{base}/metrics")).await;

    for body in [health, metrics] {
        assert!(
            !body.contains("a-very-private-value"),
            "an endpoint echoed message content: {body}"
        );
    }
}
