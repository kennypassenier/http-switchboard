//! L1 — configuration and every check on it (K8, K9, K10; AR2, AR5, AR6).
//!
//! The bar agreed at the Phase 2 gate: for every class of config error a
//! test that startup stops AND that the message names the file, the
//! profile and a remedy. "Startup stops" is `load` returning `Err`; the
//! binary turns that into a non-zero exit.

use std::collections::HashMap;

use http_switchboard::config::{self, ConfigError, Sink, Source};

fn env(pairs: &[(&str, &str)]) -> impl config::EnvLookup {
    let map: HashMap<String, String> = pairs
        .iter()
        .map(|(k, v)| ((*k).to_string(), (*v).to_string()))
        .collect();
    move |k: &str| map.get(k).cloned()
}

fn load(text: &str) -> Result<config::Config, ConfigError> {
    config::load(
        "config.toml",
        text,
        &env(&[("KYU_TOKEN", "t0ken-from-the-vault")]),
    )
}

fn err(text: &str) -> String {
    match load(text) {
        Ok(_) => panic!("expected this config to be refused, but it loaded"),
        Err(e) => e.to_string(),
    }
}

/// Every refusal must name the file and carry a remedy — the K10 bar,
/// applied to whatever the caller just produced.
fn assert_usable(message: &str, must_mention: &[&str]) {
    assert!(
        message.contains("config.toml"),
        "error does not name the file: {message}"
    );
    assert!(
        message.contains("What now:"),
        "error carries no remedy (standing rule 11): {message}"
    );
    for needle in must_mention {
        assert!(
            message.contains(needle),
            "error does not mention {needle:?}: {message}"
        );
    }
}

const GOOD: &str = r#"
[kyu]
base_url = "http://127.0.0.1:8080"
token = "${KYU_TOKEN}"

[reporting]
topic = "switchboard.events"

[[profiles]]
name = "alertmanager"
from = { kyu_topic = "alerts.raw" }
to = { kyu_topic = "alerts.homelab" }
content_type = "application/json"
body = '''
{"alert": "{{ alerts.0.labels.alertname }}"}
'''

[[profiles]]
name = "uptime-kuma"
from = { http_path = "/uptime-kuma" }
to = { url = "http://127.0.0.1:9999/hook" }
content_type = "application/json"
method = "PUT"
timeout_ms = 4000
headers = { authorization = "${KYU_TOKEN}" }
body = "{}"
"#;

#[test]
fn k9_a_valid_config_loads_with_both_source_and_sink_kinds() {
    let cfg = load(GOOD).expect("this config should load");
    assert_eq!(cfg.profiles.len(), 2);

    let am = &cfg.profiles[0];
    assert_eq!(
        am.source,
        Source::Kyu {
            topic: "alerts.raw".into()
        }
    );
    assert_eq!(
        am.sink,
        Sink::Kyu {
            topic: "alerts.homelab".into()
        }
    );

    let uk = &cfg.profiles[1];
    assert_eq!(
        uk.source,
        Source::Http {
            path: "/uptime-kuma".into()
        }
    );
    assert_eq!(
        uk.sink,
        Sink::Url {
            url: "http://127.0.0.1:9999/hook".into(),
            method: "PUT".into()
        }
    );
    assert_eq!(uk.timeout_ms, 4000);
}

#[test]
fn k9_several_profiles_may_share_one_inbound_path() {
    // The fan-out model (scope G2): one path, two profiles, each with its
    // own destination and its own failure handling.
    let cfg = load(GOOD).unwrap();
    let text = GOOD.to_string()
        + r#"
[[profiles]]
name = "uptime-kuma-log"
from = { http_path = "/uptime-kuma" }
to = { kyu_topic = "ops.log" }
content_type = "application/json"
body = "{}"
"#;
    let two = load(&text).expect("two profiles on one path is legal");
    assert_eq!(cfg.profiles.len(), 2);
    assert_eq!(two.profiles.len(), 3);
    let paths: Vec<_> = two
        .profiles
        .iter()
        .filter_map(|p| match &p.source {
            Source::Http { path } => Some(path.as_str()),
            _ => None,
        })
        .collect();
    assert_eq!(paths, vec!["/uptime-kuma", "/uptime-kuma"]);
}

#[test]
fn k8_an_env_reference_resolves_and_a_literal_stays_literal() {
    let cfg = load(GOOD).unwrap();
    assert_eq!(
        cfg.kyu.as_ref().unwrap().token.as_ref().unwrap().expose(),
        "t0ken-from-the-vault"
    );
    let literal = GOOD.replace(r#"token = "${KYU_TOKEN}""#, r#"token = "plain-value""#);
    let cfg = load(&literal).unwrap();
    assert_eq!(
        cfg.kyu.as_ref().unwrap().token.as_ref().unwrap().expose(),
        "plain-value"
    );
}

#[test]
fn k8_a_missing_environment_variable_stops_startup_and_names_the_variable() {
    let text = GOOD.replace(
        r#"token = "${KYU_TOKEN}""#,
        r#"token = "${NOT_SET_ANYWHERE}""#,
    );
    let message = err(&text);
    assert_usable(&message, &["NOT_SET_ANYWHERE"]);
}

#[test]
fn k8_no_secret_value_appears_in_any_config_error() {
    // Standing rule 10, asserted rather than assumed: the value behind a
    // reference must not travel into an error message.
    let text = GOOD.replace(r#"name = "uptime-kuma""#, r#"name = "Uptime-Kuma""#);
    let message = err(&text);
    assert!(
        !message.contains("t0ken-from-the-vault"),
        "a secret leaked into an error: {message}"
    );
}

#[test]
fn k10_an_unknown_key_is_refused_rather_than_ignored() {
    let text = GOOD.replace(
        r#"content_type = "application/json""#,
        "bodyy = \"oops\"\ncontent_type = \"application/json\"",
    );
    let message = err(&text);
    assert_usable(&message, &["bodyy"]);
}

#[test]
fn k10_a_config_without_profiles_is_refused() {
    let message = err("[kyu]\nbase_url = \"http://127.0.0.1:8080\"\n");
    assert_usable(&message, &["profiles"]);
}

#[test]
fn k10_a_duplicate_profile_name_is_refused() {
    let text = GOOD.replace(r#"name = "uptime-kuma""#, r#"name = "alertmanager""#);
    let message = err(&text);
    assert_usable(&message, &["alertmanager"]);
}

#[test]
fn k10_a_name_kyu_would_refuse_is_refused_here_first() {
    // The scenario: an upper-case profile name starts fine and then fails
    // every poll, while the service reports itself healthy.
    let text = GOOD.replace(r#"name = "alertmanager""#, r#"name = "alertmanager-HA""#);
    let message = err(&text);
    assert_usable(&message, &["alertmanager-HA"]);
}

#[test]
fn k10_the_subscription_is_its_own_key_and_is_validated_too() {
    let cfg = load(GOOD).unwrap();
    assert_eq!(cfg.profiles[0].subscription, "alertmanager");

    let text = GOOD.replace(
        r#"name = "alertmanager""#,
        "name = \"alertmanager\"\nsubscription = \"switchboard\"",
    );
    let cfg = load(&text).unwrap();
    assert_eq!(cfg.profiles[0].subscription, "switchboard");

    let bad = GOOD.replace(
        r#"name = "alertmanager""#,
        "name = \"alertmanager\"\nsubscription = \"Switch Board\"",
    );
    assert_usable(&err(&bad), &["Switch Board", "alertmanager"]);
}

#[test]
fn k10_a_topic_in_kyus_reserved_space_is_refused() {
    let text = GOOD.replace(
        r#"to = { kyu_topic = "alerts.homelab" }"#,
        r#"to = { kyu_topic = "kyu.alerts" }"#,
    );
    let message = err(&text);
    assert_usable(&message, &["kyu.alerts", "alertmanager", "403"]);
}

#[test]
fn k10_a_path_without_a_leading_slash_is_refused() {
    let text = GOOD.replace(
        r#"from = { http_path = "/uptime-kuma" }"#,
        r#"from = { http_path = "uptime-kuma" }"#,
    );
    assert_usable(&err(&text), &["uptime-kuma"]);
}

#[test]
fn k10_a_profile_may_not_claim_healthz_or_metrics() {
    for reserved in ["/healthz", "/metrics"] {
        let text = GOOD.replace(
            r#"from = { http_path = "/uptime-kuma" }"#,
            &format!(r#"from = {{ http_path = "{reserved}" }}"#),
        );
        assert_usable(&err(&text), &[reserved, "uptime-kuma"]);
    }
}

#[test]
fn k10_an_inbound_token_on_a_kyu_source_is_refused_not_ignored() {
    let text = GOOD.replace(
        r#"name = "alertmanager""#,
        "name = \"alertmanager\"\ninbound_token = \"${KYU_TOKEN}\"",
    );
    assert_usable(&err(&text), &["inbound_token", "alertmanager"]);
}

#[test]
fn k10_a_method_on_a_kyu_destination_is_refused() {
    let text = GOOD.replace(
        r#"to = { kyu_topic = "alerts.homelab" }"#,
        "to = { kyu_topic = \"alerts.homelab\" }\nmethod = \"PUT\"",
    );
    assert_usable(&err(&text), &["method", "alertmanager"]);
}

#[test]
fn k10_a_missing_content_type_is_refused() {
    let text = GOOD.replace(
        "content_type = \"application/json\"\nbody = '''",
        "body = '''",
    );
    assert_usable(&err(&text), &["content_type", "alertmanager"]);
}

#[test]
fn k10_a_kyu_endpoint_without_a_kyu_section_is_refused() {
    let text = GOOD
        .replace(
            "[kyu]\nbase_url = \"http://127.0.0.1:8080\"\ntoken = \"${KYU_TOKEN}\"\n",
            "",
        )
        .replace(r#"headers = { authorization = "${KYU_TOKEN}" }"#, "");
    assert_usable(&err(&text), &["alertmanager", "base_url"]);
}

#[test]
fn k10_a_retry_budget_that_does_not_fit_the_lease_is_refused() {
    // AR8's arithmetic, made a config error instead of a duplicate in
    // production: 3 attempts x 10 s does not fit in a 30 s lease.
    let text = GOOD.replace(
        r#"name = "alertmanager""#,
        "name = \"alertmanager\"\ntimeout_ms = 10000\nretries = 2\nlease_ms = 20000",
    );
    let message = err(&text);
    assert_usable(&message, &["alertmanager", "lease"]);
    assert!(
        message.contains("38000") && message.contains("20000"),
        "the error should show the arithmetic: {message}"
    );
}

#[test]
fn k10_a_retry_budget_that_fits_is_accepted() {
    let text = GOOD.replace(
        r#"name = "alertmanager""#,
        "name = \"alertmanager\"\ntimeout_ms = 8000\nretries = 2",
    );
    let cfg = load(&text).expect("24 s of attempts + 3 s of pauses + 5 s margin fits a 60 s lease");
    assert_eq!(cfg.profiles[0].retry_budget_ms(), 27_000);
}

#[test]
fn k10_a_profile_may_not_consume_its_own_failure_events() {
    let text = GOOD.replace(
        r#"from = { kyu_topic = "alerts.raw" }"#,
        r#"from = { kyu_topic = "switchboard.events" }"#,
    );
    assert_usable(&err(&text), &["switchboard.events", "alertmanager"]);
}

#[test]
fn k10_an_ambiguous_endpoint_is_refused() {
    let text = GOOD.replace(
        r#"from = { kyu_topic = "alerts.raw" }"#,
        r#"from = { kyu_topic = "alerts.raw", http_path = "/also-this" }"#,
    );
    assert_usable(&err(&text), &["alertmanager", "from"]);
}

#[test]
fn k10_a_destination_that_is_not_an_http_address_is_refused() {
    let text = GOOD.replace(
        r#"to = { url = "http://127.0.0.1:9999/hook" }"#,
        r#"to = { url = "127.0.0.1:9999/hook" }"#,
    );
    assert_usable(&err(&text), &["uptime-kuma", "scheme"]);
}
