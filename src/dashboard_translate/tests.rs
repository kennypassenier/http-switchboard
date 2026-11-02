//! Tests of the translate page's pure parts: scanner, explanations,
//! defaults and the example message.

use super::example::example_message;
use super::explain::{defaults_filled, explain_missing};
use super::fields::Analysis;
use super::preview::line_of;
use super::scanner::references;
use serde_json::json;

const ALERTS: &str = "{\n  \"alert\": {{ alerts.0.labels.alertname | tojson }},\n  \"severity\": {{ alerts.0.labels.severity | default(\"warning\") | tojson }},\n  \"count\": {{ alerts | length }},\n  \"summary\": {{ alerts | map(attribute=\"annotations.summary\") | join(\"; \") | tojson }}\n}";

#[test]
fn the_scanner_finds_paths_defaults_and_attributes() {
    let refs = references(ALERTS);
    assert_eq!(refs[0].path, ["alerts", "0", "labels", "alertname"]);
    assert_eq!(refs[0].line, 2);
    assert_eq!(refs[1].default.as_deref(), Some("\"warning\""));
    assert!(refs[2].list);
    assert_eq!(refs[3].attributes, ["annotations.summary"]);
}

#[test]
fn a_loop_variable_is_not_a_field_of_the_message() {
    let refs = references("{% for a in alerts %}{{ a.labels.x }}{% endfor %}");
    assert_eq!(refs.len(), 1);
    assert_eq!(refs[0].path, ["alerts"]);
}

#[test]
fn the_example_has_every_field_the_template_reads() {
    let example = example_message(ALERTS);
    let alert = &example["alerts"][0];
    assert!(alert["labels"]["alertname"].is_string());
    assert!(alert["annotations"]["summary"].is_string());
}

#[test]
fn a_missing_field_is_named_in_words() {
    let message = json!({ "alerts": [] });
    let why = explain_missing(&references(ALERTS), 2, &message).expect("an explanation");
    assert!(why.contains("alerts.0.labels.alertname"), "{why}");
    assert!(why.contains("position 0"), "{why}");
}

#[test]
fn a_default_that_was_used_is_a_note() {
    let message = json!({ "alerts": [{ "labels": { "alertname": "x" } }] });
    let notes = defaults_filled(&references(ALERTS), &message);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].what, "No severity in the message");
    assert!(notes[0].why.contains("\"warning\""));
}

#[test]
fn the_engine_line_is_read_from_its_sentence() {
    assert_eq!(line_of("undefined value (in <string>:12)"), Some(12));
    assert_eq!(line_of("nothing here"), None);
}

#[test]
fn a_field_lands_under_the_key_the_template_writes_it_at() {
    let template = "{\"title\": {{ a.b }},\n \"inner\": {\"n\": {{ c | length }}, \"s\": {{ d | default(\"x\") }}},\n \"list\": [{{ e }}, {{ f }}]}";
    let message = json!({ "a": { "b": "hi" }, "c": [1, 2], "e": 1, "f": 2 });
    let rows = Analysis::of(template).field_table(&message);
    let seen: Vec<(&str, &str, &str)> = rows
        .iter()
        .map(|r| (r.field.as_str(), r.key.as_str(), r.state))
        .collect();
    assert_eq!(
        seen,
        [
            ("a.b", "title", "read"),
            ("c", "inner.n", "read"),
            ("d", "inner.s", "default"),
            ("e", "list.0", "read"),
            ("f", "list.1", "read"),
        ]
    );
    assert_eq!(rows[0].value, "\"hi\"");
    assert_eq!(rows[1].value, "list of 2");
}

#[test]
fn a_missing_field_is_a_row_not_an_error() {
    let rows = Analysis::of("{\"k\": {{ nope.here }}}").field_table(&json!({}));
    assert_eq!(rows[0].state, "missing");
    assert_eq!(rows[0].key, "k");
}

#[test]
fn a_draft_template_is_checked_like_the_config_checks_it() {
    use super::scratch::with_template;
    let profile = crate::config::Profile {
        name: "p".into(),
        subscription: "p".into(),
        source: crate::config::Source::Http { path: "/p".into() },
        sink: crate::config::Sink::Kyu { topic: "t".into() },
        content_type: "application/json".into(),
        body: "{}".into(),
        headers: Default::default(),
        timeout_ms: 1,
        retries: 0,
        lease_ms: 1,
        max_attempts: 1,
        forward_error_body: false,
    };
    assert!(with_template(&profile, "{\"a\": {{ x }}}").is_ok());
    let quoted = with_template(&profile, "{\"a\": \"{{ x }}\"}").unwrap_err();
    assert_eq!(quoted.kind, "template");
    assert!(quoted.remedy.starts_with("Write"), "{}", quoted.remedy);
    assert_eq!(with_template(&profile, "  ").unwrap_err().kind, "template");
}

fn url_profile(url: &str, body: &str) -> crate::config::Profile {
    crate::config::Profile {
        name: "p".into(),
        subscription: "p".into(),
        source: crate::config::Source::Http { path: "/p".into() },
        sink: crate::config::Sink::Url {
            url: url.into(),
            method: "POST".into(),
        },
        content_type: "application/json".into(),
        body: body.into(),
        headers: Default::default(),
        timeout_ms: 1,
        retries: 0,
        lease_ms: 1,
        max_attempts: 1,
        forward_error_body: false,
    }
}

#[test]
fn a_path_error_is_not_pinned_to_a_line_of_the_body() {
    use super::preview::preview;
    let profile = url_profile("http://h/{{ nope }}", "{\n  \"a\": {{ a }}\n}");
    let failure = preview(&profile, br#"{"a": 1}"#).unwrap_err();
    assert_eq!(failure.line, None, "{failure:?}");
    assert!(!failure.why.contains("Line"), "{}", failure.why);
}

#[test]
fn a_body_error_still_names_its_line_when_the_path_is_templated() {
    use super::preview::preview;
    let profile = url_profile("http://h/{{ id }}", "{\n  \"a\": {{ gone }}\n}");
    let failure = preview(&profile, br#"{"id": 1}"#).unwrap_err();
    assert_eq!(failure.line, Some(2), "{failure:?}");
    assert!(failure.why.contains("Line 2 reads gone"), "{}", failure.why);
}

#[test]
fn regions_report_the_line_they_start_on() {
    let template = "{{ a }}\n\n{% if b %}\n{# note\n#}{{ c }}";
    let lines: Vec<usize> = references(template).iter().map(|r| r.line).collect();
    assert_eq!(lines, [1, 3, 5]);
}

#[test]
fn a_large_template_is_scanned_without_quadratic_cost() {
    let template = "{% set v = a %}\n{{ v }}{{ b.c }}\n".repeat(2_000);
    let started = std::time::Instant::now();
    let analysis = Analysis::of(&template);
    let rows = analysis.field_table(&json!({}));
    assert_eq!(rows.len(), 2);
    assert!(started.elapsed() < std::time::Duration::from_secs(2));
}
