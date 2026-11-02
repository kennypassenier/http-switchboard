//! Tests of the delivery error rules.

use super::error::{advice_for, DeliverError};

#[test]
fn k11_every_status_remedy_is_present_and_says_something_different() {
    // Phase 7, G7: five branches existed, tests rendered two — and not
    // the three an operator actually meets.
    let cases = [401, 403, 404, 413, 400, 422, 500, 503];
    let mut seen: Vec<String> = Vec::new();
    for status in cases {
        let e = DeliverError::Status {
            status,
            advice: advice_for(status),
            answer: String::new(),
        };
        let message = e.to_string();
        assert!(
            message.contains(&status.to_string()),
            "the status must be in the message: {message}"
        );
        assert!(
            message.len() > 60,
            "a remedy must actually say something: {message}"
        );
        seen.push(advice_for(status));
    }
    // The credential, the unknown address and the too-large body each
    // get their own advice rather than the generic one.
    assert_ne!(seen[0], seen[3], "401 and 413 must not share advice");
    assert_ne!(
        seen[2], seen[4],
        "404 and a generic 4xx must not share advice"
    );
    assert_ne!(seen[4], seen[6], "a 4xx and a 5xx must not share advice");
    assert_eq!(seen[0], seen[1], "401 and 403 are the same problem");
}

#[test]
fn w3_only_the_errors_worth_retrying_are_retried() {
    assert!(DeliverError::Timeout { timeout_ms: 1 }.is_retryable());
    assert!(DeliverError::Transport { detail: "x".into() }.is_retryable());
    for status in [500, 502, 503, 429] {
        assert!(
            DeliverError::Status {
                status,
                advice: String::new(),
                answer: String::new()
            }
            .is_retryable(),
            "{status} should be retried"
        );
    }
    for status in [400, 401, 403, 404, 413, 422] {
        assert!(
            !DeliverError::Status {
                status,
                advice: String::new(),
                answer: String::new()
            }
            .is_retryable(),
            "{status} will not get better by asking again"
        );
    }
}

#[test]
fn a_request_we_could_not_build_is_not_retried() {
    assert!(!DeliverError::Request {
        detail: "bad header".into()
    }
    .is_retryable());
}

#[test]
fn a_missing_attempt_reads_as_the_first_offer() {
    use super::kyu_hub::envelope::parse_envelope_bytes;
    use crate::pump::Poll;
    let poll = parse_envelope_bytes(br#"{"id":"m1","payload":{"x":1}}"#).unwrap();
    match poll {
        Poll::Message(m) => assert_eq!((m.id.as_str(), m.attempt), ("m1", 1)),
        other => panic!("expected a message, got {other:?}"),
    }
    let poll = parse_envelope_bytes(br#"{"id":"m1","payload":{},"attempt":3}"#).unwrap();
    match poll {
        Poll::Message(m) => assert_eq!(m.attempt, 3),
        other => panic!("expected a message, got {other:?}"),
    }
}

#[test]
fn the_id_of_an_oversize_answer_is_found_near_its_start_or_not_at_all() {
    use super::kyu_hub::envelope::scan_id;
    assert_eq!(
        scan_id(br#"{"id": "abc-1","payload":{"big":"xxxx"#).as_deref(),
        Some("abc-1")
    );
    assert_eq!(scan_id(br#"{"payload":{"big":"xxxx"#), None);
    assert_eq!(scan_id(br#"{"id":"a\"b","payload":1"#), None);
    assert_eq!(scan_id(br#"{"id":"","payload":1"#), None);
}

#[test]
fn an_id_inside_the_payload_is_never_taken_for_the_messages_own() {
    use super::kyu_hub::envelope::scan_id;
    assert_eq!(scan_id(br#"{"payload":{"id":"evil","x":"#), None);
}
