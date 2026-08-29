//! L4 — the hub side (K2; AR8). The heart of the project.
//!
//! Two layers, on purpose. The scripted-hub tests pin the ORDER — poll,
//! deliver, only then acknowledge — because that ordering is what loses
//! messages and it must be provable without a container. The E2E tests
//! then run the same code against a real kyu, because a fake hub has no
//! leases, no redelivery and no dead letters, which are the three things
//! this milestone actually rests on.

mod support;

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use http_switchboard::adapters::{HttpSink, KyuHub};
use http_switchboard::config::{self, Profile};
use http_switchboard::pump::{pump_once, Health, HubMessage, Poll, PumpState, Step};
use support::{FakeClock, FakeHub, FakeSink, HubErrorKind, KyuHarness, TestServer};

fn env(pairs: &[(&str, &str)]) -> impl config::EnvLookup {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

fn kyu_profile(base_url: &str, topic: &str, to: &str, body: &str) -> Profile {
    let text = format!(
        r#"
[kyu]
base_url = "{base_url}"

[[profiles]]
name = "p"
subscription = "switchboard"
from = {{ kyu_topic = "{topic}" }}
to = {to}
content_type = "application/json"
retries = 0
timeout_ms = 2000
body = '''{body}'''
"#
    );
    config::load("t.toml", &text, &env(&[]))
        .expect("test profile must load")
        .profiles
        .remove(0)
}

fn message(id: &str, payload: serde_json::Value) -> Poll {
    Poll::Message(Box::new(HubMessage {
        id: id.to_string(),
        payload,
        attempt: 1,
    }))
}

// ── the ordering, with a scripted hub ──────────────────────────────────

#[tokio::test]
async fn k2_a_message_is_acknowledged_only_after_it_was_delivered() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let hub = FakeHub::with_calls(
        vec![Ok(message("m1", serde_json::json!({"x": 1})))],
        Arc::clone(&calls),
    );
    let sink = FakeSink::new(vec![true], Arc::clone(&calls));
    let p = kyu_profile(
        "http://127.0.0.1:1",
        "t",
        r#"{ url = "http://127.0.0.1:1/x" }"#,
        r#"{"x": {{ x }}}"#,
    );
    let mut state = PumpState::default();

    let step = pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await;

    assert_eq!(
        step,
        Step::Delivered {
            id: "m1".into(),
            attempts: 1
        }
    );
    let recorded = calls.lock().unwrap().clone();
    let deliver_at = recorded
        .iter()
        .position(|c| c.starts_with("deliver"))
        .unwrap();
    let ack_at = recorded.iter().position(|c| c.starts_with("ack")).unwrap();
    assert!(
        deliver_at < ack_at,
        "the ack must come after the delivery, not before: {recorded:?}"
    );
    assert_eq!(state.health, Health::Working);
}

#[tokio::test]
async fn k2_a_refused_delivery_is_handed_back_and_never_acknowledged() {
    let calls = Arc::new(Mutex::new(Vec::new()));
    let hub = FakeHub::with_calls(
        vec![Ok(message("m1", serde_json::json!({"x": 1})))],
        Arc::clone(&calls),
    );
    let sink = FakeSink::new(vec![false], Arc::clone(&calls));
    let p = kyu_profile(
        "http://127.0.0.1:1",
        "t",
        r#"{ url = "http://127.0.0.1:1/x" }"#,
        r#"{"x": {{ x }}}"#,
    );
    let mut state = PumpState::default();

    let step = pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await;

    assert_eq!(step, Step::HandedBack { id: "m1".into() });
    let recorded = calls.lock().unwrap().clone();
    assert!(
        recorded.iter().any(|c| c == "nack(m1, dead=false)"),
        "the message must be handed back at once, not left to the lease: {recorded:?}"
    );
    assert!(
        !recorded.iter().any(|c| c.starts_with("ack(")),
        "nothing may be acknowledged here: {recorded:?}"
    );
    assert_eq!(state.health, Health::Failing);
}

#[tokio::test]
async fn ar8_an_unknown_topic_makes_the_next_poll_ask_for_the_history() {
    // The flagship scenario: Alertmanager fires before this subscription
    // has ever polled. Without the replay, that first alert is gone.
    let hub = FakeHub::new(vec![
        Ok(Poll::UnknownTopic),
        Ok(message("m1", serde_json::json!({"x": 1}))),
    ]);
    let calls = Arc::clone(&hub.calls);
    let sink = FakeSink::new(vec![true], Arc::clone(&calls));
    let p = kyu_profile(
        "http://127.0.0.1:1",
        "t",
        r#"{ url = "http://127.0.0.1:1/x" }"#,
        r#"{"x": {{ x }}}"#,
    );
    let mut state = PumpState::default();

    let first = pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await;
    assert_eq!(first, Step::TopicMissing);
    assert!(state.replay_next, "the next poll must ask for the history");

    let second = pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await;
    assert!(matches!(second, Step::Delivered { .. }));
    assert!(
        hub.calls()
            .contains(&"next(from_beginning=true)".to_string()),
        "the second poll should have asked from the beginning: {:?}",
        hub.calls()
    );
    assert!(!state.replay_next, "and only once");
}

#[tokio::test]
async fn ar8_a_refused_token_is_its_own_state_not_a_hub_outage() {
    let hub = FakeHub::new(vec![Err(HubErrorKind::Denied)]);
    let sink = FakeSink::new(vec![true], Arc::clone(&hub.calls));
    let p = kyu_profile(
        "http://127.0.0.1:1",
        "t",
        r#"{ url = "http://127.0.0.1:1/x" }"#,
        r#"{"x": {{ x }}}"#,
    );
    let mut state = PumpState::default();

    assert_eq!(
        pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await,
        Step::Denied
    );
    assert_eq!(state.health, Health::Denied);
}

#[tokio::test]
async fn ar8_an_unreachable_hub_is_a_state_the_pump_survives() {
    let hub = FakeHub::new(vec![Err(HubErrorKind::Unreachable), Ok(Poll::Empty)]);
    let sink = FakeSink::new(vec![true], Arc::clone(&hub.calls));
    let p = kyu_profile(
        "http://127.0.0.1:1",
        "t",
        r#"{ url = "http://127.0.0.1:1/x" }"#,
        r#"{"x": {{ x }}}"#,
    );
    let mut state = PumpState::default();

    assert_eq!(
        pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await,
        Step::HubDown
    );
    assert_eq!(state.health, Health::HubDown);

    assert_eq!(
        pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await,
        Step::Idle
    );
    assert_eq!(
        state.health,
        Health::Working,
        "recovery is a transition too"
    );
}

#[tokio::test]
async fn ar7_a_message_that_can_never_render_is_dead_lettered_not_looped() {
    // Rendering is pure: it will fail identically on every redelivery.
    // Cycling it until its attempts run out only delays the moment it
    // becomes visible.
    let hub = FakeHub::new(vec![Ok(message("m1", serde_json::json!({"other": 1})))]);
    let sink = FakeSink::new(vec![true], Arc::clone(&hub.calls));
    let p = kyu_profile(
        "http://127.0.0.1:1",
        "t",
        r#"{ url = "http://127.0.0.1:1/x" }"#,
        r#"{"x": {{ x }}}"#,
    );
    let mut state = PumpState::default();

    let step = pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await;

    match step {
        Step::DeadLettered { id, reason } => {
            assert_eq!(id, "m1");
            assert!(reason.contains("What now:"), "no remedy: {reason}");
        }
        other => panic!("expected a dead letter, got {other:?}"),
    }
    assert!(hub.calls().contains(&"nack(m1, dead=true)".to_string()));
}

#[tokio::test]
async fn ar8_the_subscription_policy_is_pushed_once() {
    let hub = FakeHub::new(vec![Ok(Poll::Empty), Ok(Poll::Empty)]);
    let sink = FakeSink::new(vec![true], Arc::clone(&hub.calls));
    let p = kyu_profile(
        "http://127.0.0.1:1",
        "t",
        r#"{ url = "http://127.0.0.1:1/x" }"#,
        r#"{"x": {{ x }}}"#,
    );
    let mut state = PumpState::default();

    pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await;
    pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await;

    let policies = hub
        .calls()
        .iter()
        .filter(|c| c.starts_with("policy("))
        .count();
    assert_eq!(policies, 1, "the policy is a write, not a heartbeat");
    assert!(hub
        .calls()
        .iter()
        .any(|c| c.contains("lease=60000") && c.contains("attempts=5")));
}

// ── the same code against a real kyu ───────────────────────────────────

macro_rules! require_hub {
    ($harness:ident) => {
        let Some($harness) = KyuHarness::start().await else {
            eprintln!("skipped: set KYU_IMAGE to run the end-to-end suite against a real kyu");
            return;
        };
    };
}

#[tokio::test]
async fn k2_e2e_a_message_published_before_the_first_poll_still_arrives() {
    // S1's scenario against the real hub: publish first, subscribe after.
    // Without AR8's replay this is the alert that vanishes.
    require_hub!(hub_container);
    let receiver = TestServer::start(vec![200]).await;
    let p = kyu_profile(
        &hub_container.base_url,
        "alerts.raw",
        &format!(r#"{{ url = "{}/hook" }}"#, receiver.base_url),
        r#"{"alert": {{ alertname }}}"#,
    );
    hub_container
        .publish("alerts.raw", r#"{"alertname": "FilesystemFull"}"#)
        .await;

    let hub = KyuHub::new(hub_container.base_url.clone(), None, 2);
    let sink = HttpSink::new(None, None, 2_000);
    let mut state = PumpState::default();

    // First poll: the topic exists (something published), but this
    // subscription is new, so it sees nothing — then the replay kicks in.
    let mut delivered = false;
    for _ in 0..4 {
        if let Step::Delivered { .. } =
            pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await
        {
            delivered = true;
            break;
        }
        state.replay_next = true;
    }

    assert!(delivered, "the first alert must not be lost");
    assert_eq!(
        receiver.received()[0].body,
        r#"{"alert": "FilesystemFull"}"#
    );
}

#[tokio::test]
async fn k2_e2e_a_refused_delivery_comes_back_and_a_delivered_one_does_not() {
    // Both halves of G7 against the real hub: the first attempt is
    // refused, so the message must return; the second succeeds, so it
    // must not.
    require_hub!(hub_container);
    let receiver = TestServer::start(vec![500, 200]).await;
    let p = kyu_profile(
        &hub_container.base_url,
        "alerts.retry",
        &format!(r#"{{ url = "{}/hook" }}"#, receiver.base_url),
        r#"{"alert": {{ alertname }}}"#,
    );

    let hub = KyuHub::new(hub_container.base_url.clone(), None, 2);
    let sink = HttpSink::new(None, None, 2_000);
    let mut state = PumpState::default();

    // Create the subscription first, then publish into it.
    pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await;
    hub_container
        .publish("alerts.retry", r#"{"alertname": "HostDown"}"#)
        .await;

    let mut steps = Vec::new();
    for _ in 0..6 {
        steps.push(pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await);
    }

    assert!(
        steps.iter().any(|s| matches!(s, Step::HandedBack { .. })),
        "the refused delivery must be handed back: {steps:?}"
    );
    assert!(
        steps.iter().any(|s| matches!(s, Step::Delivered { .. })),
        "the redelivery must then succeed: {steps:?}"
    );
    let delivered = steps
        .iter()
        .filter(|s| matches!(s, Step::Delivered { .. }))
        .count();
    assert_eq!(delivered, 1, "and it must not keep coming back after that");
    assert_eq!(receiver.received().len(), 2, "one refusal, one acceptance");
}

#[tokio::test]
async fn k4_e2e_the_translation_is_published_back_onto_a_topic() {
    // The flagship chain's second hop: translate and publish, then read
    // it back off the hub as hub-bridge would.
    require_hub!(hub_container);
    let p = kyu_profile(
        &hub_container.base_url,
        "chain.raw",
        r#"{ kyu_topic = "chain.out" }"#,
        r#"{"alert": {{ alerts.0.labels.alertname }}}"#,
    );
    let hub = KyuHub::new(hub_container.base_url.clone(), None, 2);
    let sink = HttpSink::new(Some(hub_container.base_url.clone()), None, 2_000);
    let mut state = PumpState::default();

    pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await;
    let real = std::fs::read_to_string("tests/fixtures/alertmanager_firing.json").unwrap();
    hub_container.publish("chain.raw", &real).await;

    let mut delivered = false;
    for _ in 0..5 {
        if let Step::Delivered { .. } =
            pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await
        {
            delivered = true;
            break;
        }
    }
    assert!(delivered, "the translation should have been published");

    let body = reqwest::Client::new()
        .get(format!(
            "{}/t/chain.out/next?as=reader&envelope=json&wait=2&from=beginning",
            hub_container.base_url
        ))
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    let envelope: serde_json::Value = serde_json::from_str(&body).unwrap();
    assert_eq!(
        envelope["payload"]["alert"], "FilesystemFull",
        "what hub-bridge would pick up: {body}"
    );
}
