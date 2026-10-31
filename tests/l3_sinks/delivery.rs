use super::*;

#[tokio::test]
async fn k3_the_receiver_gets_exactly_what_the_translation_produced() {
    let server = TestServer::start(vec![200]).await;
    let sink = HttpSink::new(None, None, 2_000);
    let delivery = delivery_to(&format!("{}/hook", server.base_url), &[]);

    sink.deliver(&delivery).await.expect("should be delivered");

    let seen = server.received();
    assert_eq!(seen.len(), 1);
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/hook");
    assert_eq!(seen[0].body, r#"{"x": 1}"#);
    assert_eq!(seen[0].header("content-type"), Some("application/json"));
}

#[tokio::test]
async fn k6_headers_from_the_profile_reach_the_receiver() {
    let server = TestServer::start(vec![200]).await;
    let sink = HttpSink::new(None, None, 2_000);
    let delivery = delivery_to(
        &format!("{}/hook", server.base_url),
        &[
            ("authorization", "Bearer abc123"),
            ("x-source", "switchboard"),
        ],
    );

    sink.deliver(&delivery).await.unwrap();

    let seen = server.received();
    assert_eq!(seen[0].header("authorization"), Some("Bearer abc123"));
    assert_eq!(seen[0].header("x-source"), Some("switchboard"));
}

#[tokio::test]
async fn k4_publishing_to_a_kyu_topic_hits_the_publish_verb_with_the_token() {
    let server = TestServer::start(vec![200]).await;
    let sink = HttpSink::new(
        Some(server.base_url.clone()),
        Some(Secret::new("s3cr3t-hub-token")),
        2_000,
    );
    let delivery = Delivery {
        target: Target::KyuTopic {
            topic: "alerts.homelab".to_string(),
        },
        content_type: "application/json".to_string(),
        headers: Default::default(),
        body: r#"{"x": 1}"#.to_string(),
        forward_error_body: false,
    };

    sink.deliver(&delivery).await.unwrap();

    let seen = server.received();
    assert_eq!(seen[0].method, "POST");
    assert_eq!(seen[0].path, "/t/alerts.homelab");
    assert_eq!(
        seen[0].header("authorization"),
        Some("Bearer s3cr3t-hub-token")
    );
}

#[tokio::test]
async fn k6_a_secret_header_value_appears_in_no_error_message() {
    // Standing rule 10, asserted: the failure path is where secrets leak.
    let sink = HttpSink::new(
        Some("http://127.0.0.1:1".to_string()),
        Some(Secret::new("s3cr3t-hub-token")),
        200,
    );
    let delivery = Delivery {
        target: Target::KyuTopic {
            topic: "alerts.homelab".to_string(),
        },
        content_type: "application/json".to_string(),
        headers: [("authorization".to_string(), Secret::new("Bearer leak-me"))]
            .into_iter()
            .collect(),
        body: "{}".to_string(),
        forward_error_body: false,
    };

    let err = sink.deliver(&delivery).await.unwrap_err();
    let message = format!("{err} {err:?}");
    assert!(
        !message.contains("s3cr3t-hub-token"),
        "hub token leaked: {message}"
    );
    assert!(
        !message.contains("leak-me"),
        "header value leaked: {message}"
    );
}

#[tokio::test]
async fn k3_the_whole_path_from_payload_to_receiver_holds_together() {
    // Translation and delivery, end to end against a real socket: the
    // receiver sees exactly what the template produced.
    let server = TestServer::start(vec![200]).await;
    let p = profile("", &format!(r#"{{ url = "{}/hook" }}"#, server.base_url));
    let delivery = translate::prepare(&p, br#"{"x": "with \" quote"}"#).unwrap();
    let sink = HttpSink::new(None, None, 2_000);

    sink.deliver(&delivery).await.unwrap();

    assert_eq!(server.received()[0].body, r#"{"x": "with \" quote"}"#);
}
