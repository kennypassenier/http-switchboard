use super::*;

#[tokio::test]
async fn k1_a_post_on_a_configured_path_produces_exactly_one_delivery() {
    let receiver = TestServer::start(vec![200]).await;
    let base = serve(&one_profile(&receiver.base_url)).await;

    let (status, _) = post(&format!("{base}/hook"), r#"{"x": 1}"#, None).await;

    assert_eq!(status, 200);
    let seen = receiver.received();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].body, r#"{"x": 1}"#);
    assert_eq!(seen[0].path, "/in");
}

#[tokio::test]
async fn k1_an_unknown_path_answers_404_and_causes_nothing() {
    let receiver = TestServer::start(vec![200]).await;
    let base = serve(&one_profile(&receiver.base_url)).await;

    let (status, _) = post(&format!("{base}/nope"), r#"{"x": 1}"#, None).await;

    assert_eq!(status, 404);
    assert!(receiver.received().is_empty());
}

#[tokio::test]
async fn ar9_only_post_is_accepted() {
    let receiver = TestServer::start(vec![200]).await;
    let base = serve(&one_profile(&receiver.base_url)).await;

    let response = reqwest::Client::new()
        .get(format!("{base}/hook"))
        .send()
        .await
        .unwrap();

    assert_eq!(response.status().as_u16(), 405);
    assert!(receiver.received().is_empty());
}

#[tokio::test]
async fn w1_the_sender_is_told_the_truth_when_the_destination_refuses() {
    // The whole point of W1: this service stores nothing, so answering
    // "accepted" and failing afterwards would lose the message while the
    // sender believes it arrived.
    let receiver = TestServer::start(vec![500]).await;
    let base = serve(&one_profile(&receiver.base_url)).await;

    let (status, body) = post(&format!("{base}/hook"), r#"{"x": 1}"#, None).await;

    assert_eq!(status, 502);
    assert!(body.contains("What now:"), "no remedy: {body}");
    assert!(body.contains("NOT accepted"), "{body}");
}

#[tokio::test]
async fn w1_a_message_that_cannot_be_rendered_is_refused_not_swallowed() {
    let receiver = TestServer::start(vec![200]).await;
    let base = serve(&one_profile(&receiver.base_url)).await;

    let (status, body) = post(&format!("{base}/hook"), r#"{"other": 1}"#, None).await;

    assert_eq!(status, 502);
    assert!(body.contains("What now:"), "{body}");
    assert!(
        receiver.received().is_empty(),
        "nothing should have been sent"
    );
}
