//! The door has no queue to pause: three permanent refusals in a row make
//! the profile failing, and every request is still answered.

use super::*;
use http_switchboard::obs::Registry;
use http_switchboard::pump::Health;

async fn serve_counted(config_text: &str) -> (String, Arc<Registry>) {
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
async fn three_refusals_in_a_row_fail_the_profile_and_the_door_keeps_answering() {
    let receiver = TestServer::start(vec![422]).await;
    let (base, registry) = serve_counted(&one_profile(&receiver.base_url)).await;
    let url = format!("{base}/hook");

    for _ in 0..2 {
        assert_eq!(post(&url, r#"{"x": 1}"#, None).await.0, 502);
    }
    assert_eq!(
        registry.health_of("hook"),
        None,
        "two refusals are not enough"
    );
    assert_eq!(post(&url, r#"{"x": 1}"#, None).await.0, 502);
    assert_eq!(registry.health_of("hook"), Some(Health::Failing));

    let (status, _) = post(&url, r#"{"x": 1}"#, None).await;
    assert_eq!(status, 502, "the door still answers, and still tries");
    assert_eq!(receiver.received().len(), 4);
    assert!(!registry.snapshot()["hook"].paused, "the door never pauses");
}

#[tokio::test]
async fn a_success_resets_the_door_streak() {
    let receiver = TestServer::start(vec![422, 422, 200, 422, 422]).await;
    let (base, registry) = serve_counted(&one_profile(&receiver.base_url)).await;
    let url = format!("{base}/hook");
    for _ in 0..5 {
        post(&url, r#"{"x": 1}"#, None).await;
    }
    assert_eq!(registry.health_of("hook"), Some(Health::Working));
}
