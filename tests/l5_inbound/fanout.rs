use super::*;

#[tokio::test]
async fn k9_two_profiles_on_one_path_both_deliver_without_a_startup_panic() {
    // The bar K9 was frozen with — and the case the critic pointed out
    // would have made the router panic in main.
    let first = TestServer::start(vec![200]).await;
    let second = TestServer::start(vec![200]).await;
    let text = format!(
        "{}\n{}",
        one_profile(&first.base_url),
        second_profile(&second.base_url)
    );
    let base = serve(&text).await;

    let (status, _) = post(&format!("{base}/hook"), r#"{"x": 7}"#, None).await;

    assert_eq!(status, 200);
    assert_eq!(first.received()[0].body, r#"{"x": 7}"#);
    assert_eq!(second.received()[0].body, r#"{"copy": 7}"#);
}

#[tokio::test]
async fn ar4_one_failing_branch_makes_the_whole_request_fail() {
    // AR4's decision, made visible: with two profiles on a path there is
    // one answer, so all must succeed or the sender is told it failed.
    let ok = TestServer::start(vec![200]).await;
    let bad = TestServer::start(vec![500]).await;
    let text = format!(
        "{}\n{}",
        one_profile(&ok.base_url),
        second_profile(&bad.base_url)
    );
    let base = serve(&text).await;

    let (status, body) = post(&format!("{base}/hook"), r#"{"x": 7}"#, None).await;

    assert_eq!(status, 502);
    assert!(
        body.contains("hook-log"),
        "the failing profile is named: {body}"
    );
    assert_eq!(ok.received().len(), 1, "the branch that worked still ran");
}

#[tokio::test]
async fn ar9_a_body_over_the_cap_is_refused() {
    let receiver = TestServer::start(vec![200]).await;
    let base = serve(&one_profile(&receiver.base_url)).await;
    let huge = format!(r#"{{"x": "{}"}}"#, "a".repeat(inbound::MAX_BODY_BYTES + 10));

    let (status, _) = post(&format!("{base}/hook"), &huge, None).await;

    assert_eq!(status, 413);
    assert!(receiver.received().is_empty());
}

#[tokio::test]
async fn k10_a_two_x_inbound_token_is_refused_with_the_command_that_replaces_it() {
    // 3.0.0 (H1): the switchboard stopped guarding its own door. A config
    // carrying the old field must not be silently ignored, and must not
    // fail with serde's bare "unknown field" either — the person reading
    // that message needs to know what to do instead.
    let text = r#"
[[profiles]]
name = "a"
from = { http_path = "/hook" }
to = { url = "http://127.0.0.1:1/x" }
content_type = "application/json"
inbound_token = "one"
body = "{}"
"#;
    let err = config::load("t.toml", text, &env(&[]))
        .unwrap_err()
        .to_string();
    assert!(err.contains("What now:"), "no remedy: {err}");
    assert!(
        err.contains("chassis clients issue"),
        "the replacement command is named: {err}"
    );
    assert!(err.contains("'a'"), "the profile is named: {err}");
    assert!(
        !err.contains("\"one\""),
        "the retired token must not be echoed: {err}"
    );
}
