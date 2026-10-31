use super::*;

#[tokio::test]
async fn w11_e2e_an_inbound_profile_reports_itself_too() {
    // Phase 7, G3: the transition logic lived only in the pump loop, so a
    // profile fed by a webhook fell over in silence — and that is the
    // source shape every second profile tends to use.
    let Some(hub) = KyuHarness::start().await else {
        unreachable!("KyuHarness::start always starts a kyu");
    };
    let receiver = TestServer::start(vec![500, 200]).await;
    let port = free_port();
    let text = format!(
        r#"
[kyu]
base_url = "{}"
token = "{}"

[reporting]
topic = "switchboard.events"

[[profiles]]
name = "inbound-profile"
from = {{ http_path = "/hook" }}
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

    let post = |body: &'static str| async move {
        reqwest::Client::new()
            .post(format!("http://127.0.0.1:{port}/hook"))
            .header("content-type", "application/json")
            .body(body)
            .send()
            .await
            .unwrap()
            .status()
            .as_u16()
    };

    assert_eq!(
        post(r#"{"name": "first"}"#).await,
        502,
        "the receiver refuses"
    );
    assert_eq!(post(r#"{"name": "second"}"#).await, 200, "and then accepts");
    tokio::time::sleep(std::time::Duration::from_millis(500)).await;

    let events = read_topic(&hub.base_url, &hub.token, "switchboard.events", "reader2").await;
    let failing = events
        .iter()
        .filter(|e| e["event"] == "profile.failing")
        .count();
    let recovered = events
        .iter()
        .filter(|e| e["event"] == "profile.recovered")
        .count();

    assert_eq!(failing, 1, "one failure event: {events:?}");
    assert_eq!(recovered, 1, "one recovery event: {events:?}");
    assert_eq!(events[0]["profile"], "inbound-profile");
    for event in &events {
        assert!(
            !event.to_string().contains("first"),
            "an event must never carry the payload: {event}"
        );
    }

    let _ = stop_tx.send(());
}
