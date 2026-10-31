use super::*;

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

/// fix-8 (2026-10-04): a destination that never answers is cut off at the
/// profile's own timeout_ms, not at the sink's 30 s ceiling. Before the fix
/// one attempt against a dropped connection took 30 s while the config's
/// budget check had counted 500 ms against the lease.
#[tokio::test]
async fn fix_8_the_profile_timeout_bounds_each_attempt() {
    struct NeverAnswers;
    impl Sink for NeverAnswers {
        fn deliver<'a>(
            &'a self,
            _delivery: &'a Delivery,
        ) -> http_switchboard::adapters::BoxFuture<'a, Result<(), DeliverError>> {
            Box::pin(std::future::pending())
        }
    }
    let p = profile(
        "retries = 0\ntimeout_ms = 300",
        r#"{ url = "http://127.0.0.1:9/in" }"#,
    );
    let started = std::time::Instant::now();
    let outcome = deliver_with_retry(
        &p,
        &delivery_to("http://127.0.0.1:9/in", &[]),
        &NeverAnswers,
        &FakeClock::default(),
    )
    .await;
    assert!(
        matches!(
            outcome.result,
            Err(DeliverError::Timeout { timeout_ms: 300 })
        ),
        "{:?}",
        outcome.result
    );
    assert!(
        started.elapsed() < std::time::Duration::from_secs(2),
        "the attempt must end at the profile's 300 ms, it took {:?}",
        started.elapsed()
    );
}
