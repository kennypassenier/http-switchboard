use super::*;

#[tokio::test]
async fn ar8_e2e_the_policy_is_in_force_after_the_first_successful_poll() {
    // Found by the Phase 7 reasoned-vs-measured sweep: kyu refuses a
    // policy write for a subscription that does not exist yet, and a
    // subscription only starts existing when it first polls. Writing the
    // policy BEFORE the first poll therefore always failed, leaving the
    // first message claimed under the hub's default 30 s lease — the very
    // arithmetic the config's budget check exists to prevent.
    require_hub!(hub_container);
    let receiver = TestServer::start(vec![200]).await;
    let p = kyu_profile(
        &hub_container.base_url,
        "policy.raw",
        &format!(r#"{{ url = "{}/hook" }}"#, receiver.base_url),
        r#"{"alert": {{ name }}}"#,
    );
    // The topic has to exist, or the poll 404s and no subscription is made.
    hub_container
        .publish("policy.raw", r#"{"name": "x"}"#)
        .await;

    let hub = hub_container.hub(2);
    let sink = HttpSink::new(None, None, 2_000);
    let mut state = PumpState::default();

    // ONE turn: poll, and the policy must be in force afterwards.
    pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await;

    let policy = reqwest::Client::new()
        .get(format!(
            "{}/api/t/policy.raw/subs/switchboard/policy",
            hub_container.base_url
        ))
        .bearer_auth(support::KYU_ADMIN_TOKEN)
        .send()
        .await
        .unwrap()
        .text()
        .await
        .unwrap();

    assert!(
        policy.contains("60000"),
        "the lease this profile was validated against must be the one in force: {policy}"
    );
    assert!(state.policy_pushed, "and the pump must know it succeeded");
}

#[tokio::test]
async fn k2_e2e_a_message_that_can_never_work_is_settled_not_looped() {
    // Phase 7, G4: the dead-letter road was proven only against a fake
    // hub — which has no leases, no redelivery and no dead letters, the
    // three things the E2E layer exists for. Two shapes that can never
    // work: a body that is not JSON at all, and JSON whose field the
    // template needs is absent.
    require_hub!(hub_container);
    let receiver = TestServer::start(vec![200]).await;
    let p = kyu_profile(
        &hub_container.base_url,
        "dead.raw",
        &format!(r#"{{ url = "{}/hook" }}"#, receiver.base_url),
        r#"{"alert": {{ name }}}"#,
    );
    let hub = hub_container.hub(2);
    let sink = HttpSink::new(None, None, 2_000);
    let mut state = PumpState::default();

    // Create the subscription first, then publish what cannot work.
    pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await;
    reqwest::Client::new()
        .post(format!("{}/t/dead.raw", hub_container.base_url))
        .bearer_auth(&hub_container.token)
        .header("content-type", "text/plain")
        .body("this is not json at all")
        .send()
        .await
        .unwrap();
    hub_container
        .publish("dead.raw", r#"{"something_else": 1}"#)
        .await;

    let mut dead = 0;
    for _ in 0..8 {
        if let Step::DeadLettered { reason, .. } =
            pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await
        {
            assert!(reason.len() > 10, "the event says why: {reason}");
            dead += 1;
        }
    }

    assert_eq!(
        dead, 2,
        "both messages must be dead-lettered exactly once, not cycled"
    );
    assert!(
        receiver.received().is_empty(),
        "and neither may reach the destination"
    );

    // Settled means settled: nothing comes back.
    for _ in 0..3 {
        let step = pump_once(&p, &hub, &sink, &FakeClock::default(), &mut state).await;
        assert_eq!(step, Step::Idle, "a settled message must not return");
    }
}
