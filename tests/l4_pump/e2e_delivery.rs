use super::*;

#[tokio::test]
async fn k2_e2e_a_message_published_before_the_first_poll_still_arrives() {
    // scope-flagship-1's scenario against the real hub: publish first, subscribe after.
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

    let hub = hub_container.hub(2);
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

    let hub = hub_container.hub(2);
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
    let hub = hub_container.hub(2);
    let sink = HttpSink::new(
        Some(hub_container.base_url.clone()),
        Some(http_switchboard::secret::Secret::new(
            hub_container.token.clone(),
        )),
        2_000,
    );
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
        .bearer_auth(&hub_container.token)
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
