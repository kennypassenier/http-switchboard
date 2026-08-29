//! L2 — the translation core (K5, K12, K14; AR1, AR7).
//!
//! Everything here runs without a server, a hub or a clock. That is the
//! point of AR1: the mapping and the hostile-payload suite are ordinary
//! unit tests, not an afternoon of manual poking.

use std::collections::HashMap;

use http_switchboard::config::{self, Profile};
use http_switchboard::translate::{self, Target};

fn env(pairs: &[(&str, &str)]) -> impl config::EnvLookup {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

/// The configuration that ships with the project, loaded by the test so
/// it cannot rot unnoticed (K9's bar).
fn shipped_profile() -> Profile {
    let path = "deploy/config.example.toml";
    let text = std::fs::read_to_string(path).expect("the shipped example config must exist");
    let cfg = config::load(path, &text, &env(&[("KYU_TOKEN", "vault-value")]))
        .expect("the shipped example config must load");
    cfg.profiles
        .into_iter()
        .find(|p| p.name == "alertmanager")
        .expect("the example config must carry the alertmanager profile")
}

fn profile_with(body: &str, content_type: &str) -> Profile {
    let text = format!(
        r#"
[[profiles]]
name = "t"
from = {{ http_path = "/t" }}
to = {{ url = "http://127.0.0.1:9/hook" }}
content_type = "{content_type}"
body = '''{body}'''
"#
    );
    config::load("t.toml", &text, &env(&[]))
        .expect("test profile must load")
        .profiles
        .remove(0)
}

#[test]
fn k12_the_recorded_alertmanager_payload_renders_byte_for_byte() {
    // The payload was produced by prom/alertmanager itself and captured
    // verbatim; the expected output is pinned beside it. Standing rule 9:
    // synthetic vectors only prove we agree with ourselves.
    let payload = std::fs::read("tests/fixtures/alertmanager_firing.json").unwrap();
    let expected = std::fs::read_to_string("tests/fixtures/alertmanager_expected.json").unwrap();

    let delivery =
        translate::prepare(&shipped_profile(), &payload).expect("the real payload must render");

    assert_eq!(delivery.body, expected);
    assert_eq!(
        delivery.target,
        Target::KyuTopic {
            topic: "alerts.homelab".into()
        }
    );
    assert_eq!(delivery.content_type, "application/json");
}

#[test]
fn k5_field_access_defaults_arithmetic_and_conditionals() {
    let payload = br#"{"bytes": 1073741824, "sev": "critical", "labels": {"name": "disk"}}"#;

    let p = profile_with(
        r#"{"gb": {{ (bytes | float / 1073741824) | round(2) }}}"#,
        "application/json",
    );
    assert_eq!(
        translate::prepare(&p, payload).unwrap().body,
        r#"{"gb": 1.0}"#
    );

    let p = profile_with(r#"{"n": {{ labels.name }}}"#, "application/json");
    assert_eq!(
        translate::prepare(&p, payload).unwrap().body,
        r#"{"n": "disk"}"#
    );

    let p = profile_with(
        r#"{"m": {{ missing | default("none") }}}"#,
        "application/json",
    );
    assert_eq!(
        translate::prepare(&p, payload).unwrap().body,
        r#"{"m": "none"}"#
    );

    let p = profile_with(
        r#"{"p": {{ "high" if sev == "critical" else "low" }}}"#,
        "application/json",
    );
    assert_eq!(
        translate::prepare(&p, payload).unwrap().body,
        r#"{"p": "high"}"#
    );
}

#[test]
fn ar7_a_quote_in_an_alert_summary_cannot_rewrite_the_document() {
    // The critic's scenario, made a test: an exporter's free-text
    // annotation that tries to add a field of its own. Escaping is done
    // by the engine, so this is structure, not discipline.
    let hostile = br#"{"alerts":[{"status":"firing","labels":{"alertname":"X","severity":"critical"},"annotations":{"summary":"disk full\", \"severity\": \"info"}}]}"#;

    let p = profile_with(
        r#"{"summary": {{ alerts.0.annotations.summary }}, "severity": {{ alerts.0.labels.severity }}}"#,
        "application/json",
    );
    let body = translate::prepare(&p, hostile).unwrap().body;
    let parsed: serde_json::Value = serde_json::from_str(&body).unwrap();

    assert_eq!(
        parsed["severity"], "critical",
        "the sender must not pick the severity: {body}"
    );
    assert_eq!(
        parsed["summary"], "disk full\", \"severity\": \"info",
        "the text must survive intact, as text"
    );
    assert_eq!(
        parsed.as_object().unwrap().len(),
        2,
        "no field was smuggled in: {body}"
    );
}

#[test]
fn ar7_a_missing_field_is_an_error_with_a_remedy_not_an_empty_value() {
    // The failure mode the throwaway script was rejected for: a renamed
    // field renders empty and every layer reports success.
    let p = profile_with(
        r#"{"a": {{ alerts.0.labels.alertname }}}"#,
        "application/json",
    );
    let e = translate::prepare(&p, br#"{"alerts":[{"labels":{}}]}"#).unwrap_err();
    let message = e.to_string();
    assert!(message.contains("What now:"), "no remedy: {message}");
    assert!(message.contains('t'), "the profile is named: {message}");
}

#[test]
fn ar7_a_rendered_body_that_is_not_valid_json_is_refused_before_it_is_sent() {
    let p = profile_with(r#"{"a": {{ x }} "#, "application/json");
    let e = translate::prepare(&p, br#"{"x": 1}"#).unwrap_err();
    let message = e.to_string();
    assert!(message.contains("valid JSON"), "{message}");
    assert!(message.contains("What now:"), "no remedy: {message}");
}

#[test]
fn ar8_a_payload_that_is_not_json_is_an_error_with_a_remedy() {
    // curl -d sends form encoding by default; kyu's own docs record that
    // trap. Better a per-message error than a template that reads nothing
    // and renders empty.
    let p = profile_with(r#"{"a": {{ x }}}"#, "application/json");
    let e = translate::prepare(&p, b"x=1&y=2").unwrap_err();
    let message = e.to_string();
    assert!(message.contains("not JSON"), "{message}");
    assert!(message.contains("What now:"), "no remedy: {message}");
}

#[test]
fn k14_no_payload_can_change_where_its_translation_goes() {
    // The property, over deliberately hostile shapes. The destination is
    // copied from the profile and never rendered, so this holds by
    // construction — the test is here to keep it that way.
    let hostiles: [&[u8]; 6] = [
        br#"{"url": "http://evil.example/steal", "x": 1}"#,
        br#"{"host": "10.10.10.4", "x": 1}"#,
        br#"{"x": "http://user@evil.example/"}"#,
        br#"{"x": "../../admin"}"#,
        br#"{"x": "javascript:alert(1)"}"#,
        br#"{"x": "http://127.0.0.1:8080/t/kyu.events"}"#,
    ];
    for payload in hostiles {
        let p = profile_with(r#"{"x": {{ x | default("-") }}}"#, "application/json");
        let delivery = translate::prepare(&p, payload).expect("should render");
        assert_eq!(
            delivery.target,
            Target::Url {
                url: "http://127.0.0.1:9/hook".into(),
                method: "POST".into()
            },
            "the destination moved for payload {:?}",
            String::from_utf8_lossy(payload)
        );
    }
}

#[test]
fn k14_a_non_json_profile_still_escapes_nothing_and_stays_on_target() {
    let p = profile_with("{{ x }}", "text/plain");
    let d = translate::prepare(&p, br#"{"x": "plain <b> text"}"#).unwrap();
    assert_eq!(d.body, "plain <b> text");
    assert_eq!(d.content_type, "text/plain");
}
