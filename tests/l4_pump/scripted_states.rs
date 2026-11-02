use super::*;

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

    match pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await {
        Step::Denied { detail } => assert!(
            detail.contains("Apps page"),
            "the remedy AR8.4 asked for must travel with the step: {detail}"
        ),
        other => panic!("expected Denied, got {other:?}"),
    }
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

    match pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await {
        Step::HubDown { detail } => assert!(detail.contains("What now:"), "no remedy: {detail}"),
        other => panic!("expected HubDown, got {other:?}"),
    }
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
        Step::DeadLettered { id, reason, .. } => {
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
