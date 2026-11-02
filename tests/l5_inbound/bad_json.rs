use super::*;

use http_switchboard::obs::Registry;

/// Like `serve`, but hands back the registry so health and counters can
/// be read.
async fn serve_with_registry(config_text: &str) -> (String, Arc<Registry>) {
    let cfg = config::load("t.toml", config_text, &env(&[])).expect("config must load");
    let registry = Arc::new(Registry::new());
    let router = inbound::router(
        &cfg,
        Arc::new(http_switchboard::reload::ProfileStore::new(&cfg)),
        Arc::new(HttpSink::new(None, None, 2_000)),
        Arc::new(TokioClock),
        Arc::clone(&registry),
    );
    let listener = tokio::net::TcpListener::bind("127.0.0.1:0").await.unwrap();
    let addr = listener.local_addr().unwrap();
    tokio::spawn(async move {
        axum::serve(listener, router).await.unwrap();
    });
    (format!("http://{addr}"), registry)
}

#[tokio::test]
async fn malformed_json_is_a_400_that_explains_itself_and_never_echoes_the_body() {
    let receiver = TestServer::start(vec![200]).await;
    let (base, _registry) = serve_with_registry(&one_profile(&receiver.base_url)).await;

    let (status, body) = post(&format!("{base}/hook"), "{\"x\": SECRET-PAYLOAD", None).await;

    assert_eq!(status, 400);
    assert!(body.contains("not valid JSON"), "what is wrong: {body}");
    assert!(body.contains("Why:"), "{body}");
    assert!(body.contains("What now:"), "{body}");
    assert!(
        !body.contains("SECRET-PAYLOAD"),
        "the body was echoed to the sender: {body}"
    );
    assert!(receiver.received().is_empty(), "nothing may be delivered");
}

#[tokio::test]
async fn malformed_json_does_not_change_health_and_is_counted_as_given_up() {
    let receiver = TestServer::start(vec![200]).await;
    let (base, registry) = serve_with_registry(&one_profile(&receiver.base_url)).await;
    assert_eq!(
        post(&format!("{base}/hook"), r#"{"x": 1}"#, None).await.0,
        200
    );
    let before = registry.health_of("hook");

    let (status, _) = post(&format!("{base}/hook"), "not json at all", None).await;

    assert_eq!(status, 400);
    assert_eq!(registry.health_of("hook"), before, "health must not move");
    let stats = &registry.snapshot()["hook"];
    assert_eq!((stats.received, stats.attempts), (2, 2));
    assert_eq!((stats.delivered, stats.failed, stats.given_up), (1, 0, 1));
    let reason = stats.last_given_up_reason.as_deref().unwrap();
    assert!(reason.contains("not JSON"), "{reason}");
    assert!(
        !reason.contains("not json at all"),
        "payload text: {reason}"
    );
    assert!(stats.last_given_up_unix.is_some());
}

#[tokio::test]
async fn a_permanent_refusal_is_given_up_without_failing_the_profile() {
    let receiver = TestServer::start(vec![422]).await;
    let (base, registry) = serve_with_registry(&one_profile(&receiver.base_url)).await;

    let (status, _) = post(&format!("{base}/hook"), r#"{"x": 1}"#, None).await;

    assert_eq!(status, 502);
    assert_eq!(registry.health_of("hook"), None, "health must not move");
    let stats = &registry.snapshot()["hook"];
    assert_eq!((stats.failed, stats.given_up), (0, 1));
    let reason = stats.last_given_up_reason.as_deref().unwrap();
    assert!(reason.contains("422"), "{reason}");
    assert!(
        !reason.contains("/in"),
        "the destination path leaked: {reason}"
    );
}

#[tokio::test]
async fn a_temporary_refusal_still_fails_the_profile() {
    let receiver = TestServer::start(vec![503]).await;
    let (base, registry) = serve_with_registry(&one_profile(&receiver.base_url)).await;

    post(&format!("{base}/hook"), r#"{"x": 1}"#, None).await;

    assert_eq!(
        registry.health_of("hook"),
        Some(http_switchboard::pump::Health::Failing)
    );
    assert_eq!(registry.snapshot()["hook"].given_up, 0);
}
