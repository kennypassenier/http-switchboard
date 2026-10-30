//! Tests of the health body and the metrics text.

use super::Registry;
use crate::pump::Health;

#[test]
fn w5_a_denied_profile_makes_the_service_report_degraded() {
    let registry = Registry::new();
    registry.register("a");
    registry.set_health("a", Health::Working);
    let (healthy, body) = registry.healthz();
    assert!(healthy, "{body}");
    assert!(body.contains(r#""state":"working""#), "{body}");

    registry.set_health("a", Health::Denied);
    let (healthy, body) = registry.healthz();
    assert!(
        !healthy,
        "a denied profile must not be reported as healthy: {body}"
    );
    assert!(body.contains("degraded"), "{body}");
    assert!(body.contains(r#""state":"denied""#), "{body}");
}

#[test]
fn w6_the_counters_and_the_duration_move_as_expected() {
    let registry = Registry::new();
    registry.record("a", true, 12);
    registry.record("a", false, 30);
    let text = registry.metrics();
    assert!(
        text.contains(r#"switchboard_messages_received_total{profile="a"} 2"#),
        "{text}"
    );
    assert!(
        text.contains(r#"switchboard_messages_delivered_total{profile="a"} 1"#),
        "{text}"
    );
    assert!(
        text.contains(r#"switchboard_messages_failed_total{profile="a"} 1"#),
        "{text}"
    );
    assert!(
        text.contains(r#"switchboard_delivery_duration_ms_total{profile="a"} 42"#),
        "the duration W6 asks for is missing: {text}"
    );
}

#[test]
fn w7_a_health_body_carries_names_and_never_content() {
    let registry = Registry::new();
    registry.register("alertmanager");
    registry.record("alertmanager", true, 5);
    let (_, body) = registry.healthz();
    let parsed: serde_json::Value = serde_json::from_str(&body).expect("valid JSON");
    assert_eq!(parsed["profiles"][0]["name"], "alertmanager");
    assert!(parsed["profiles"][0]["last_success_age_s"].is_number());
}
