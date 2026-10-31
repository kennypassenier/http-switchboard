use super::*;

#[tokio::test]
async fn w11_e2e_a_failing_profile_reports_once_and_recovery_reports_once() {
    let Some(hub) = KyuHarness::start().await else {
        unreachable!("KyuHarness::start always starts a kyu");
    };
    // The receiver refuses the first three messages, then accepts.
    let receiver = TestServer::start(vec![500, 500, 500, 200]).await;
    let port = free_port();
    let text = format!(
        r#"
[kyu]
base_url = "{}"
token = "{}"

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
        hub.base_url, hub.token, receiver.base_url
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

    let events = read_topic(&hub.base_url, &hub.token, "switchboard.events", "reader").await;
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
