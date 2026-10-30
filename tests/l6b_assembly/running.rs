use super::*;

#[tokio::test]
async fn l6b_a_message_travels_through_the_running_service() {
    // The whole way: a POST arrives at the service, the profile
    // translates it, and the receiver sees the result — with the service
    // started the way the binary starts it.
    let receiver = TestServer::start(vec![200]).await;
    let port = free_port();
    let text = format!(
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "{}/in" }}
content_type = "application/json"
retries = 0
body = '''{{"alert": {{{{ name }}}}}}'''
"#,
        receiver.base_url
    );
    let cfg = config::load("t.toml", &text, &env(&[])).unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        App::from_config(cfg)
            .run(format!("127.0.0.1:{port}").parse().unwrap(), async {
                let _ = stop_rx.await;
            })
            .await
            .unwrap();
    });
    wait_until_healthy(port).await;

    let response = reqwest::Client::new()
        .post(format!("http://127.0.0.1:{port}/hook"))
        .header("content-type", "application/json")
        .body(r#"{"name": "HostDown"}"#)
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 200);
    assert_eq!(receiver.received()[0].body, r#"{"alert": "HostDown"}"#);

    // And it stops when asked.
    let _ = stop_tx.send(());
}

#[tokio::test]
async fn l6b_e2e_the_running_service_pumps_the_hub_by_itself() {
    // No test driving pump_once by hand: the service's own loop picks the
    // message up, translates it and delivers it.
    let Some(hub) = KyuHarness::start().await else {
        unreachable!("KyuHarness::start always starts a kyu");
    };
    let receiver = TestServer::start(vec![200]).await;
    let port = free_port();
    let text = format!(
        r#"
[kyu]
base_url = "{}"
token = "{}"

[[profiles]]
name = "pumped"
from = {{ kyu_topic = "assembly.raw" }}
to = {{ url = "{}/in" }}
content_type = "application/json"
retries = 0
body = '''{{"alert": {{{{ name }}}}}}'''
"#,
        hub.base_url, hub.token, receiver.base_url
    );
    let cfg = config::load("t.toml", &text, &env(&[])).unwrap();
    let (stop_tx, stop_rx) = tokio::sync::oneshot::channel::<()>();
    tokio::spawn(async move {
        App::from_config(cfg)
            .run(format!("127.0.0.1:{port}").parse().unwrap(), async {
                let _ = stop_rx.await;
            })
            .await
            .unwrap();
    });
    wait_until_healthy(port).await;

    hub.publish("assembly.raw", r#"{"name": "FilesystemFull"}"#)
        .await;

    let mut seen = false;
    for _ in 0..40 {
        if !receiver.received().is_empty() {
            seen = true;
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(500)).await;
    }
    assert!(seen, "the service should have pumped the message by itself");
    assert_eq!(
        receiver.received()[0].body,
        r#"{"alert": "FilesystemFull"}"#
    );

    let metrics = reqwest::get(format!("http://127.0.0.1:{port}/metrics"))
        .await
        .unwrap()
        .text()
        .await
        .unwrap();
    assert!(
        metrics.contains(r#"switchboard_messages_delivered_total{profile="pumped"} 1"#),
        "the running service should have counted it: {metrics}"
    );

    let _ = stop_tx.send(());
}
