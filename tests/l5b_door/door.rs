use super::*;

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
