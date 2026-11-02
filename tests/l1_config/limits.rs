//! Numbers and HTTP parts a profile is held to before anything computes
//! with them (review 2026-11-02).

use super::*;

fn with_http_profile_line(line: &str) -> String {
    GOOD.replace(
        r#"timeout_ms = 4000"#,
        &format!("timeout_ms = 4000\n{line}"),
    )
}

#[test]
fn too_many_retries_are_refused_before_any_arithmetic() {
    let text = with_http_profile_line("retries = 4000000000");
    assert_usable(&err(&text), &["uptime-kuma", "retries", "at most 10"]);
}

#[test]
fn a_zero_timeout_is_refused() {
    let text = GOOD.replace("timeout_ms = 4000", "timeout_ms = 0");
    assert_usable(&err(&text), &["uptime-kuma", "timeout_ms"]);
}

#[test]
fn a_zero_lease_or_attempt_cap_is_refused() {
    let lease = GOOD.replace(
        r#"body = '''
{"alert""#,
        "lease_ms = 0\nbody = '''\n{\"alert\"",
    );
    assert_usable(&err(&lease), &["alertmanager", "lease_ms"]);
    let attempts = GOOD.replace(
        r#"body = '''
{"alert""#,
        "max_attempts = 0\nbody = '''\n{\"alert\"",
    );
    assert_usable(&err(&attempts), &["alertmanager", "max_attempts"]);
}

#[test]
fn a_bad_method_or_header_is_refused_at_load() {
    let method = GOOD.replace(r#"method = "PUT""#, r#"method = "NOT A METHOD""#);
    assert_usable(&err(&method), &["uptime-kuma", "method"]);
    let name = GOOD.replace(
        r#"headers = { authorization"#,
        r#"headers = { "bad name" = "x", authorization"#,
    );
    assert_usable(&err(&name), &["uptime-kuma", "header name"]);
}

#[test]
fn a_header_value_is_never_echoed_when_it_is_refused() {
    let text = GOOD.replace(
        r#"headers = { authorization = "${KYU_TOKEN}" }"#,
        r#"headers = { authorization = "line\nbreak-SECRET" }"#,
    );
    let message = err(&text);
    assert_usable(&message, &["uptime-kuma", "authorization"]);
    assert!(!message.contains("SECRET"), "{message}");
}

#[test]
fn the_largest_allowed_retry_count_still_has_a_finite_budget() {
    let text = with_http_profile_line("retries = 10");
    let config = load(&text).expect("ten retries are allowed");
    let profile = config
        .profiles
        .iter()
        .find(|p| p.name == "uptime-kuma")
        .unwrap();
    assert_eq!(profile.pauses_ms().len(), 10);
    assert!(profile.retry_budget_ms() < u64::MAX);
}
