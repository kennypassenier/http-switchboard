//! Tests of the translate page's pure parts: scanner, explanations,
//! defaults and the example message.

use super::example::example_message;
use super::explain::{defaults_filled, explain_missing};
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
    let why = explain_missing(ALERTS, 2, &message).expect("an explanation");
    assert!(why.contains("alerts.0.labels.alertname"), "{why}");
    assert!(why.contains("position 0"), "{why}");
}

#[test]
fn a_default_that_was_used_is_a_note() {
    let message = json!({ "alerts": [{ "labels": { "alertname": "x" } }] });
    let notes = defaults_filled(ALERTS, &message);
    assert_eq!(notes.len(), 1);
    assert_eq!(notes[0].what, "No severity in the message");
    assert!(notes[0].why.contains("\"warning\""));
}

#[test]
fn the_engine_line_is_read_from_its_sentence() {
    assert_eq!(line_of("undefined value (in <string>:12)"), Some(12));
    assert_eq!(line_of("nothing here"), None);
}
