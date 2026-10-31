use super::*;

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
            attempts: 1,
            ack_failed: false
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

    match &step {
        Step::HandedBack {
            id,
            attempts,
            reason,
        } => {
            assert_eq!(id, "m1");
            assert_eq!(*attempts, 1);
            // The remedy travels with it now, instead of living only in
            // the source (Phase 7 audit, G5).
            assert!(reason.contains("What now:"), "no remedy: {reason}");
        }
        other => panic!("expected the message to be handed back, got {other:?}"),
    }
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
