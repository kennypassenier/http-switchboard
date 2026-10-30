use super::*;

#[tokio::test]
async fn w1_the_answer_never_tells_the_sender_where_the_message_was_going() {
    // Phase 7, found twice independently — by the test-gap audit and by
    // the security review. reqwest's error Display appends "for url (…)",
    // and that text was travelling back to whoever POSTed. The shipped
    // destination is a Home Assistant webhook whose id IS the credential.
    // Port 9 (discard) with nothing listening: the delivery fails at the
    // transport layer, which is the path that carried the URL.
    let dead = "http://127.0.0.1:9".to_string();
    let text = format!(
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "{dead}/api/webhook/SECRET-WEBHOOK-ID-abc123" }}
content_type = "application/json"
retries = 0
timeout_ms = 500
body = '''{{"x": {{{{ x }}}}}}'''
"#
    );
    let base = serve(&text).await;

    let (status, body) = post(&format!("{base}/hook"), r#"{"x": 1}"#, None).await;

    assert_eq!(status, 502);
    assert!(
        !body.contains("SECRET-WEBHOOK-ID-abc123"),
        "the destination leaked to the sender: {body}"
    );
    assert!(
        !body.contains(&dead),
        "the destination host leaked to the sender: {body}"
    );
    assert!(
        body.contains("What now:"),
        "the remedy must survive: {body}"
    );
}

#[tokio::test]
async fn ar3_a_burst_gets_clean_refusals_instead_of_an_unbounded_queue() {
    // Phase 7, G5: the bound and its 503 were reached by no test at all,
    // while L5's exit criterion claimed a burst gives clean refusals.
    //
    // Honest about what this proves: the semaphore is acquired AFTER the
    // body has been read, so what it bounds is concurrent DELIVERIES, not
    // the memory held by bodies in flight. The exit criterion's wording
    // was looser than the code; this test pins the code.
    let hanging = TestServer::start(vec![0]).await; // never answers
    let text = format!(
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "{}/in" }}
content_type = "application/json"
retries = 0
timeout_ms = 30000
body = '''{{"x": {{{{ x }}}}}}'''
"#,
        hanging.base_url
    );
    let base = serve(&text).await;

    // More requests than the bound, all of which will occupy a permit
    // because the receiver never answers.
    let mut handles = Vec::new();
    for _ in 0..(http_switchboard::inbound::MAX_IN_FLIGHT + 8) {
        let url = format!("{base}/hook");
        handles.push(tokio::spawn(async move {
            post(&url, r#"{"x": 1}"#, None).await
        }));
    }

    let mut refusals = 0;
    for handle in handles {
        let (status, body) = handle.await.unwrap();
        if status == 503 {
            refusals += 1;
            assert!(
                body.contains("What now:"),
                "a refusal must say what to do: {body}"
            );
            assert!(
                body.contains("not a loss"),
                "and must not read as a lost message: {body}"
            );
        }
    }

    assert!(
        refusals >= 8,
        "past the bound the answer must be a refusal, not a queue: only {refusals} refusals"
    );
}

#[tokio::test]
async fn w12_the_receivers_own_words_come_back_only_when_the_profile_asks() {
    // Kenny's idea (mini-round MR2, 2026-08-30), and its guard rail. By
    // default the receiver's text is not ours to hand on — a moment ago
    // this same answer was leaking the destination address.
    let receiver = TestServer::start(vec![400]).await;
    let quiet = format!(
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "{}/in" }}
content_type = "application/json"
retries = 0
body = '''{{"x": {{{{ x }}}}}}'''
"#,
        receiver.base_url
    );
    let base = serve(&quiet).await;
    let (status, body) = post(&format!("{base}/hook"), r#"{"x": 1}"#, None).await;
    assert_eq!(status, 502);
    assert!(
        !body.contains("It said:"),
        "by default the receiver's answer stays with the receiver: {body}"
    );
    assert!(
        body.contains("status 400"),
        "the status still comes back: {body}"
    );

    let loud = format!(
        "{}
forward_error_body = true
",
        quiet.trim_end()
    );
    let base = serve(&loud).await;
    let (status, body) = post(&format!("{base}/hook"), r#"{"x": 1}"#, None).await;
    assert_eq!(status, 502);
    assert!(body.contains("status 400"), "{body}");
    assert!(body.contains("What now:"), "the remedy survives: {body}");
}

#[tokio::test]
async fn w12_a_receiver_answer_is_bounded_and_stripped_of_control_characters() {
    // The text is not ours and cannot be vouched for, so it is trimmed to
    // one bounded line before it is repeated.
    let receiver = TestServer::start(vec![400]).await;
    let text = format!(
        r#"
[[profiles]]
name = "hook"
from = {{ http_path = "/hook" }}
to = {{ url = "{}/in" }}
content_type = "application/json"
retries = 0
forward_error_body = true
body = '''{{"x": {{{{ x }}}}}}'''
"#,
        receiver.base_url
    );
    let base = serve(&text).await;

    let (_, body) = post(&format!("{base}/hook"), r#"{"x": 1}"#, None).await;

    assert!(
        !body.contains('\r'),
        "no control characters may be repeated: {body:?}"
    );
    assert!(
        body.len() < 4096,
        "and the answer stays bounded: {}",
        body.len()
    );
}

#[test]
fn k10_forwarding_is_refused_where_there_is_nobody_to_answer() {
    let text = r#"
[kyu]
base_url = "http://127.0.0.1:8080"

[[profiles]]
name = "from-hub"
from = { kyu_topic = "alerts.raw" }
to = { url = "http://127.0.0.1:9/x" }
content_type = "application/json"
forward_error_body = true
body = "{}"
"#;
    let e = config::load("t.toml", text, &env(&[]))
        .unwrap_err()
        .to_string();
    assert!(e.contains("What now:"), "no remedy: {e}");
    assert!(e.contains("forward_error_body"), "{e}");
    assert!(e.contains("nobody"), "the reason is named: {e}");
}
