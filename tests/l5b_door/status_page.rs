use super::*;

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
