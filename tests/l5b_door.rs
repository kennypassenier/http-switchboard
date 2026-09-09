//! L5b — the door, as the kit holds it (H1, W8 amended; 3.0.0).
//!
//! Until 3.0.0 this service checked a per-path `inbound_token` of its own.
//! It now registers the profile paths as the kit's API routes, so the kit
//! checks an `Authorization: Bearer <client token>` before the body is
//! ever read, and a revoked token is out the same second.
//!
//! These tests run the real thing: `chassis::testing::TestApp` starts this
//! service in-process on a free port with a temporary state directory and
//! fresh secrets — the same assembly `main.rs` builds — and drives it over
//! HTTP. That is the kit harness of 1.8.0 (T1); the fakes in
//! `tests/support` stay where the subject is this project's own domain
//! (the pump's ordering, the hub, the retry ladder), which the harness
//! knows nothing about.

mod support;

use std::collections::HashMap;
use std::sync::Arc;

use axum::Router;
use chassis::testing::TestApp;
use chassis::AppSpec;
use http_switchboard::app::App as Switchboard;
use http_switchboard::config;
use reqwest::Method;
use support::TestServer;

fn env(pairs: &[(&str, &str)]) -> impl config::EnvLookup {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

fn one_http_profile(receiver: &str) -> String {
    format!(
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "{receiver}/in" }}
content_type = "application/json"
body = '{{"x": {{{{ x }}}}}}'
"#
    )
}

/// Start this service the way `main.rs` does, under the kit's test
/// harness: profile paths as API routes, the Profiles section on the
/// status page, senders as the word for a client.
async fn start(config_text: &str) -> TestApp {
    let cfg = config::load("t.toml", config_text, &env(&[])).expect("config must load");
    let switchboard = Switchboard::from_config(cfg);
    let registry = Arc::clone(&switchboard.registry);
    let profiles =
        http_switchboard::dashboard::Profiles::new(&switchboard.config, Arc::clone(&registry));
    let subsystems: Vec<http_switchboard::obs::ProfileSubsystem> = switchboard
        .config
        .profiles
        .iter()
        .map(|p| http_switchboard::obs::ProfileSubsystem::new(&p.name, Arc::clone(&registry)))
        .collect();
    let routes = switchboard.profile_router();
    let spec = AppSpec {
        name: "http-switchboard",
        version: env!("CARGO_PKG_VERSION"),
        repository: Some("kennypassenier/http-switchboard"),
        ..Default::default()
    };
    TestApp::start_with(spec, Router::new(), move |app| {
        app.api_routes(routes);
        for s in subsystems {
            app.subsystem(s);
        }
        app.vocabulary("sender", "senders");
        app.status_section(profiles);
        app.dashboard_routes(Router::new().route(
            http_switchboard::dashboard::RECHECK_ROUTE,
            axum::routing::post(http_switchboard::dashboard::recheck).with_state(registry),
        ));
    })
    .await
}

#[tokio::test]
async fn w8_a_sender_without_a_token_never_reaches_the_translation() {
    let receiver = TestServer::start(vec![200]).await;
    let app = start(&one_http_profile(&receiver.base_url)).await;

    let (status, body) = TestApp::send_text(
        app.request(Method::POST, "/hook")
            .header("content-type", "application/json")
            .body(r#"{"x": 1}"#.to_string()),
    )
    .await;

    assert_eq!(status, 401, "no token must not get in: {body}");
    assert!(
        body.contains("remedy") && body.contains("Bearer"),
        "the kit's refusal must say what to send: {body}"
    );
    assert!(
        receiver.received().is_empty(),
        "nothing may be delivered for a request that never got through the door"
    );
}

#[tokio::test]
async fn w8_a_senders_token_opens_the_door_and_a_wrong_one_does_not() {
    let receiver = TestServer::start(vec![200]).await;
    let mut app = start(&one_http_profile(&receiver.base_url)).await;
    app.login().await;
    let sender = app.issue_client("alertmanager", &[]).await;

    let (status, _) = TestApp::send_text(
        app.bearer(Method::POST, "/hook", "not-the-token")
            .header("content-type", "application/json")
            .body(r#"{"x": 1}"#.to_string()),
    )
    .await;
    assert_eq!(status, 401, "a wrong token is still a closed door");
    assert!(receiver.received().is_empty());

    let (status, body) = TestApp::send_text(
        app.bearer(Method::POST, "/hook", &sender.token)
            .header("content-type", "application/json")
            .body(r#"{"x": 1}"#.to_string()),
    )
    .await;
    assert_eq!(status, 200, "the issued token must get in: {body}");
    assert_eq!(receiver.received().len(), 1, "and the message is delivered");
}

#[tokio::test]
async fn w8_revoking_a_sender_shuts_the_door_on_the_next_request() {
    // The reason the door moved (H1): a static token in a config file can
    // only be withdrawn by editing that file and restarting the service.
    let receiver = TestServer::start(vec![200, 200]).await;
    let mut app = start(&one_http_profile(&receiver.base_url)).await;
    app.login().await;
    let sender = app.issue_client("alertmanager", &[]).await;

    let (status, _) = TestApp::send_text(
        app.bearer(Method::POST, "/hook", &sender.token)
            .header("content-type", "application/json")
            .body(r#"{"x": 1}"#.to_string()),
    )
    .await;
    assert_eq!(status, 200);

    let (status, _) = app
        .post_json(
            &format!("/api/clients/{}/revoke", sender.id),
            serde_json::json!({}),
        )
        .await;
    assert!((200..300).contains(&status), "revoke must be accepted");

    let (status, _) = TestApp::send_text(
        app.bearer(Method::POST, "/hook", &sender.token)
            .header("content-type", "application/json")
            .body(r#"{"x": 2}"#.to_string()),
    )
    .await;
    assert_eq!(status, 401, "a revoked token is out on the next request");
    assert_eq!(
        receiver.received().len(),
        1,
        "only the request from before the revoke was delivered"
    );
}

#[tokio::test]
async fn w8_the_token_never_appears_in_any_answer() {
    let receiver = TestServer::start(vec![200]).await;
    let mut app = start(&one_http_profile(&receiver.base_url)).await;
    app.login().await;
    let sender = app.issue_client("alertmanager", &[]).await;

    let (_, refusal) = TestApp::send_text(
        app.request(Method::POST, "/hook")
            .header("content-type", "application/json")
            .body(r#"{"x": 1}"#.to_string()),
    )
    .await;
    assert!(
        !refusal.contains(&sender.token),
        "the token leaked: {refusal}"
    );

    let (_, page) = app.page("/").await;
    assert!(
        !page.contains(&sender.token),
        "the status page shows a token"
    );
}

#[tokio::test]
async fn v1_the_status_page_names_the_profiles_and_calls_a_client_a_sender() {
    let receiver = TestServer::start(vec![200]).await;
    let mut app = start(&one_http_profile(&receiver.base_url)).await;
    app.login().await;

    let (status, page) = app.page("/").await;
    assert_eq!(status, 200, "the status page must render");
    assert!(
        page.contains("Profiles"),
        "the Profiles section is there: {page}"
    );
    assert!(page.contains("hook"), "the profile is named");
    assert!(
        page.contains("Recheck profiles"),
        "the section's own button is rendered"
    );
    assert!(
        page.to_lowercase().contains("sender"),
        "the kit speaks this project's word for a client"
    );
}

#[tokio::test]
async fn v1_recheck_clears_a_failure_a_profile_can_no_longer_clear_itself() {
    // An http-source profile only changes health when somebody posts to
    // it. One failed delivery therefore keeps /healthz?strict=1 red for as
    // long as nobody sends anything — which is what the button is for.
    let receiver = TestServer::start(vec![500]).await;
    let mut app = start(&one_http_profile(&receiver.base_url)).await;
    app.login().await;
    let sender = app.issue_client("alertmanager", &[]).await;

    let (status, _) = TestApp::send_text(
        app.bearer(Method::POST, "/hook", &sender.token)
            .header("content-type", "application/json")
            .body(r#"{"x": 1}"#.to_string()),
    )
    .await;
    assert_eq!(
        status, 502,
        "the receiver refused, so the sender is told so"
    );

    let (_, health) = app.get_json("/healthz?strict=1").await;
    assert_eq!(
        health["status"], "degraded",
        "the profile is remembered as failing: {health}"
    );

    let (status, answer) = app
        .post_json(
            http_switchboard::dashboard::RECHECK_ROUTE,
            serde_json::json!({}),
        )
        .await;
    assert_eq!(status, 200, "the button's route answers: {answer}");
    assert_eq!(answer["rechecked"], 1);

    let (_, health) = app.get_json("/healthz?strict=1").await;
    assert_eq!(
        health["status"], "ok",
        "after the recheck the stale failure is gone: {health}"
    );
}
