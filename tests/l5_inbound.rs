//! L5 — the inbound side (K1, W1, W8; AR3, AR9).

mod support;

use std::collections::HashMap;
use std::sync::Arc;

use http_switchboard::adapters::{HttpSink, TokioClock};
use http_switchboard::config;
use http_switchboard::inbound;
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
        Arc::new(HttpSink::new(None, None, 2_000)),
        Arc::new(TokioClock),
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

#[tokio::test]
async fn k1_a_post_on_a_configured_path_produces_exactly_one_delivery() {
    let receiver = TestServer::start(vec![200]).await;
    let base = serve(&one_profile(&receiver.base_url)).await;

    let (status, _) = post(&format!("{base}/hook"), r#"{"x": 1}"#, None).await;

    assert_eq!(status, 200);
    let seen = receiver.received();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].body, r#"{"x": 1}"#);
    assert_eq!(seen[0].path, "/in");
}

#[tokio::test]
async fn k1_an_unknown_path_answers_404_and_causes_nothing() {
    let receiver = TestServer::start(vec![200]).await;
    let base = serve(&one_profile(&receiver.base_url)).await;

    let (status, _) = post(&format!("{base}/nope"), r#"{"x": 1}"#, None).await;

    assert_eq!(status, 404);
    assert!(receiver.received().is_empty());
}

#[tokio::test]
async fn ar9_only_post_is_accepted() {
    let receiver = TestServer::start(vec![200]).await;
    let base = serve(&one_profile(&receiver.base_url)).await;

    let response = reqwest::Client::new()
        .get(format!("{base}/hook"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 405);
    assert!(receiver.received().is_empty());
}

#[tokio::test]
async fn w1_the_sender_is_told_the_truth_when_the_destination_refuses() {
    // The whole point of W1: this service stores nothing, so answering
    // "accepted" and failing afterwards would lose the message while the
    // sender believes it arrived.
    let receiver = TestServer::start(vec![500]).await;
    let base = serve(&one_profile(&receiver.base_url)).await;

    let (status, body) = post(&format!("{base}/hook"), r#"{"x": 1}"#, None).await;

    assert_eq!(status, 502);
    assert!(body.contains("What now:"), "no remedy: {body}");
    assert!(body.contains("NOT accepted"), "{body}");
}

#[tokio::test]
async fn w1_a_message_that_cannot_be_rendered_is_refused_not_swallowed() {
    let receiver = TestServer::start(vec![200]).await;
    let base = serve(&one_profile(&receiver.base_url)).await;

    let (status, body) = post(&format!("{base}/hook"), r#"{"other": 1}"#, None).await;

    assert_eq!(status, 502);
    assert!(body.contains("What now:"), "{body}");
    assert!(
        receiver.received().is_empty(),
        "nothing should have been sent"
    );
}

#[tokio::test]
async fn k9_two_profiles_on_one_path_both_deliver_without_a_startup_panic() {
    // The bar K9 was frozen with — and the case the critic pointed out
    // would have made the router panic in main.
    let first = TestServer::start(vec![200]).await;
    let second = TestServer::start(vec![200]).await;
    let text = format!(
        "{}\n{}",
        one_profile(&first.base_url),
        second_profile(&second.base_url)
    );
    let base = serve(&text).await;

    let (status, _) = post(&format!("{base}/hook"), r#"{"x": 7}"#, None).await;

    assert_eq!(status, 200);
    assert_eq!(first.received()[0].body, r#"{"x": 7}"#);
    assert_eq!(second.received()[0].body, r#"{"copy": 7}"#);
}

#[tokio::test]
async fn ar4_one_failing_branch_makes_the_whole_request_fail() {
    // AR4's decision, made visible: with two profiles on a path there is
    // one answer, so all must succeed or the sender is told it failed.
    let ok = TestServer::start(vec![200]).await;
    let bad = TestServer::start(vec![500]).await;
    let text = format!(
        "{}\n{}",
        one_profile(&ok.base_url),
        second_profile(&bad.base_url)
    );
    let base = serve(&text).await;

    let (status, body) = post(&format!("{base}/hook"), r#"{"x": 7}"#, None).await;

    assert_eq!(status, 502);
    assert!(
        body.contains("hook-log"),
        "the failing profile is named: {body}"
    );
    assert_eq!(ok.received().len(), 1, "the branch that worked still ran");
}

#[tokio::test]
async fn w8_without_the_token_the_door_stays_shut() {
    let receiver = TestServer::start(vec![200]).await;
    let text = format!(
        "{}\ninbound_token = \"${{HOOK_TOKEN}}\"\n",
        one_profile(&receiver.base_url).trim_end()
    );
    let base = serve(&text).await;

    let (status, body) = post(&format!("{base}/hook"), r#"{"x": 1}"#, None).await;
    assert_eq!(status, 401);
    assert!(body.contains("What now:"), "no remedy: {body}");
    assert!(receiver.received().is_empty());

    let (status, _) = post(&format!("{base}/hook"), r#"{"x": 1}"#, Some("wrong")).await;
    assert_eq!(status, 401);
    assert!(receiver.received().is_empty());

    let (status, _) = post(&format!("{base}/hook"), r#"{"x": 1}"#, Some("let-me-in")).await;
    assert_eq!(status, 200);
    assert_eq!(receiver.received().len(), 1);
}

#[tokio::test]
async fn w8_the_token_itself_never_appears_in_an_answer() {
    let receiver = TestServer::start(vec![200]).await;
    let text = format!(
        "{}\ninbound_token = \"${{HOOK_TOKEN}}\"\n",
        one_profile(&receiver.base_url).trim_end()
    );
    let base = serve(&text).await;

    let (_, body) = post(&format!("{base}/hook"), r#"{"x": 1}"#, None).await;

    assert!(!body.contains("let-me-in"), "the token leaked: {body}");
}

#[tokio::test]
async fn ar9_a_body_over_the_cap_is_refused() {
    let receiver = TestServer::start(vec![200]).await;
    let base = serve(&one_profile(&receiver.base_url)).await;
    let huge = format!(r#"{{"x": "{}"}}"#, "a".repeat(inbound::MAX_BODY_BYTES + 10));

    let (status, _) = post(&format!("{base}/hook"), &huge, None).await;

    assert_eq!(status, 413);
    assert!(receiver.received().is_empty());
}

#[tokio::test]
async fn k10_profiles_sharing_a_path_must_agree_on_the_token() {
    // One path is one door: the check happens once per request, so two
    // different expectations behind it would half-deliver.
    let text = r#"
[[profiles]]
name = "a"
from = { http_path = "/hook" }
to = { url = "http://127.0.0.1:1/x" }
content_type = "application/json"
inbound_token = "one"
body = "{}"

[[profiles]]
name = "b"
from = { http_path = "/hook" }
to = { url = "http://127.0.0.1:1/y" }
content_type = "application/json"
inbound_token = "two"
body = "{}"
"#;
    let err = config::load("t.toml", text, &env(&[]))
        .unwrap_err()
        .to_string();
    assert!(err.contains("What now:"), "no remedy: {err}");
    assert!(err.contains("/hook"), "{err}");
    assert!(
        !err.contains("one") || !err.contains("two"),
        "tokens must not be echoed: {err}"
    );
}
