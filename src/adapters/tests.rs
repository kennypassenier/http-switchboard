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
