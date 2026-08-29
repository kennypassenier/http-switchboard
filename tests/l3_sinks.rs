//! L3 — delivering to a URL and to a kyu topic (K3, K4, K6, W2, W3).
//!
//! Against a real socket, not a mock of HTTP: the fake receiver is a
//! genuine TCP listener, so the client's timeout, headers and body are
//! exercised for real (standing rule 9).

mod support;

use std::collections::HashMap;

use http_switchboard::adapters::{deliver_with_retry, DeliverError, HttpSink, Sink};
use http_switchboard::config::{self, Profile};
use http_switchboard::secret::Secret;
use http_switchboard::translate::{self, Delivery, Target};
use support::{FakeClock, TestServer};

fn env(pairs: &[(&str, &str)]) -> impl config::EnvLookup {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

fn profile(extra: &str, to: &str) -> Profile {
    let text = format!(
        r#"
[kyu]
base_url = "http://127.0.0.1:1"
token = "${{KYU_TOKEN}}"

[[profiles]]
name = "t"
from = {{ http_path = "/t" }}
to = {to}
content_type = "application/json"
body = '''{{"x": {{{{ x }}}}}}'''
{extra}
"#
    );
    config::load("t.toml", &text, &env(&[("KYU_TOKEN", "s3cr3t-hub-token")]))
        .expect("test profile must load")
        .profiles
        .remove(0)
}

fn delivery_to(url: &str, headers: &[(&str, &str)]) -> Delivery {
    Delivery {
        target: Target::Url {
            url: url.to_string(),
            method: "POST".to_string(),
        },
        content_type: "application/json".to_string(),
        headers: headers
            .iter()
            .map(|(k, v)| ((*k).to_string(), Secret::new(*v)))
            .collect(),
        body: r#"{"x": 1}"#.to_string(),
    }
}

#[tokio::test]
async fn k3_the_receiver_gets_exactly_what_the_translation_produced() {
    let server = TestServer::start(vec![200]).await;
    let sink = HttpSink::new(None, None, 2_000);
    let delivery = delivery_to(&format!("{}/hook", server.base_url), &[]);

    sink.deliver(&delivery).await.expect("should be delivered");

    let seen = server.received();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/hook");
    assert_eq!(seen[0].body, r#"{"x": 1}"#);
    assert_eq!(seen[0].header("content-type"), Some("application/json"));
}

#[tokio::test]
async fn k6_headers_from_the_profile_reach_the_receiver() {
    let server = TestServer::start(vec![200]).await;
    let sink = HttpSink::new(None, None, 2_000);
    let delivery = delivery_to(
        &format!("{}/hook", server.base_url),
        &[
            ("authorization", "Bearer abc123"),
            ("x-source", "switchboard"),
        ],
    );

    sink.deliver(&delivery).await.unwrap();

    let seen = server.received();
    assert_eq!(seen[0].header("authorization"), Some("Bearer abc123"));
    assert_eq!(seen[0].header("x-source"), Some("switchboard"));
}

#[tokio::test]
async fn k4_publishing_to_a_kyu_topic_hits_the_publish_verb_with_the_token() {
    let server = TestServer::start(vec![200]).await;
    let sink = HttpSink::new(
        Some(server.base_url.clone()),
        Some(Secret::new("s3cr3t-hub-token")),
        2_000,
    );
    let delivery = Delivery {
        target: Target::KyuTopic {
            topic: "alerts.homelab".to_string(),
        },
        content_type: "application/json".to_string(),
        headers: Default::default(),
        body: r#"{"x": 1}"#.to_string(),
    };

    sink.deliver(&delivery).await.unwrap();

    let seen = server.received();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/t/alerts.homelab");
    assert_eq!(
        seen[0].header("authorization"),
        Some("Bearer s3cr3t-hub-token")
    );
}

#[tokio::test]
async fn w2_a_receiver_that_never_answers_fails_with_a_remedy_and_frees_the_profile() {
    let server = TestServer::start(vec![0]).await; // 0 = never answer
    let sink = HttpSink::new(None, None, 300);
    let delivery = delivery_to(&format!("{}/hook", server.base_url), &[]);

    let err = sink.deliver(&delivery).await.unwrap_err();
    let message = err.to_string();
    assert!(matches!(err, DeliverError::Timeout { .. }), "{message}");
    assert!(message.contains("What now:"), "no remedy: {message}");

    // And the sink is usable again immediately afterwards.
    let ok_server = TestServer::start(vec![200]).await;
    sink.deliver(&delivery_to(&format!("{}/hook", ok_server.base_url), &[]))
        .await
        .expect("the next message must go through");
}

#[tokio::test]
async fn w3_two_failures_then_success_is_one_delivery_and_three_attempts() {
    // The exact bar from the Phase 2 gate. The pauses are measured with a
    // fake clock, so this test does not wait three seconds.
    let server = TestServer::start(vec![503, 503, 200]).await;
    let sink = HttpSink::new(None, None, 2_000);
    let clock = FakeClock::default();
    let p = profile("retries = 2", r#"{ url = "http://127.0.0.1:1/x" }"#);
    let delivery = delivery_to(&format!("{}/hook", server.base_url), &[]);

    let outcome = deliver_with_retry(&p, &delivery, &sink, &clock).await;

    assert!(outcome.result.is_ok(), "{:?}", outcome.result);
    assert_eq!(outcome.attempts, 3);
    assert_eq!(
        server.received().len(),
        3,
        "three attempts reached the receiver"
    );
    assert_eq!(clock.pauses(), vec![1_000, 2_000], "1 s then 2 s");
}

#[tokio::test]
async fn w3_a_permanent_client_error_is_not_retried() {
    // A 400 will be a 400 again; retrying it only delays the failure and
    // burns the lease budget.
    let server = TestServer::start(vec![400]).await;
    let sink = HttpSink::new(None, None, 2_000);
    let clock = FakeClock::default();
    let p = profile("retries = 2", r#"{ url = "http://127.0.0.1:1/x" }"#);

    let outcome = deliver_with_retry(
        &p,
        &delivery_to(&format!("{}/hook", server.base_url), &[]),
        &sink,
        &clock,
    )
    .await;

    assert_eq!(outcome.attempts, 1);
    assert!(clock.pauses().is_empty(), "nothing should have waited");
    let message = outcome.result.unwrap_err().to_string();
    assert!(message.contains("What now:"), "no remedy: {message}");
}

#[tokio::test]
async fn w3_a_failure_that_never_recovers_stops_after_the_configured_attempts() {
    let server = TestServer::start(vec![503]).await;
    let sink = HttpSink::new(None, None, 2_000);
    let clock = FakeClock::default();
    let p = profile("retries = 2", r#"{ url = "http://127.0.0.1:1/x" }"#);

    let outcome = deliver_with_retry(
        &p,
        &delivery_to(&format!("{}/hook", server.base_url), &[]),
        &sink,
        &clock,
    )
    .await;

    assert_eq!(outcome.attempts, 3);
    assert!(outcome.result.is_err());
    assert_eq!(clock.pauses(), vec![1_000, 2_000]);
}

#[tokio::test]
async fn k6_a_secret_header_value_appears_in_no_error_message() {
    // Standing rule 10, asserted: the failure path is where secrets leak.
    let sink = HttpSink::new(
        Some("http://127.0.0.1:1".to_string()),
        Some(Secret::new("s3cr3t-hub-token")),
        200,
    );
    let delivery = Delivery {
        target: Target::KyuTopic {
            topic: "alerts.homelab".to_string(),
        },
        content_type: "application/json".to_string(),
        headers: [("authorization".to_string(), Secret::new("Bearer leak-me"))]
            .into_iter()
            .collect(),
        body: "{}".to_string(),
    };

    let err = sink.deliver(&delivery).await.unwrap_err();
    let message = format!("{err} {err:?}");
    assert!(
        !message.contains("s3cr3t-hub-token"),
        "hub token leaked: {message}"
    );
    assert!(
        !message.contains("leak-me"),
        "header value leaked: {message}"
    );
}

#[tokio::test]
async fn k3_the_whole_path_from_payload_to_receiver_holds_together() {
    // Translation and delivery, end to end against a real socket: the
    // receiver sees exactly what the template produced.
    let server = TestServer::start(vec![200]).await;
    let p = profile("", &format!(r#"{{ url = "{}/hook" }}"#, server.base_url));
    let delivery = translate::prepare(&p, br#"{"x": "with \" quote"}"#).unwrap();
    let sink = HttpSink::new(None, None, 2_000);

    sink.deliver(&delivery).await.unwrap();

    assert_eq!(server.received()[0].body, r#"{"x": "with \" quote"}"#);
}
