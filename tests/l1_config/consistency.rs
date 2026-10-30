use super::*;

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

#[test]
fn k10_a_quoted_interpolation_in_a_json_profile_is_refused_at_startup() {
    // AR7: with JSON autoescape on, the engine emits a complete JSON
    // value, so "{{ x }}" produces doubled quotes. Measured against
    // minijinja 2.x, then made a startup error rather than a surprise on
    // the first real alert.
    let text = GOOD.replace(
        "{\"alert\": {{ alerts.0.labels.alertname }}}",
        "{\"alert\": \"{{ alerts.0.labels.alertname }}\"}",
    );
    assert_usable(&err(&text), &["alertmanager", "quotes"]);
}

#[test]
fn k11_every_config_error_carries_a_remedy() {
    // K11's bar, walked rather than sampled: each of these is a different
    // variant, and none may reach Kenny without a "what now".
    let cases: Vec<String> = vec![
        "not toml at all {{{".to_string(),
        "[kyu]\nbase_url = \"http://127.0.0.1:8080\"\n".to_string(),
        GOOD.replace(r#"name = "uptime-kuma""#, r#"name = "alertmanager""#),
        GOOD.replace(r#"name = "alertmanager""#, r#"name = "alertmanager-HA""#),
        GOOD.replace(
            r#"to = { kyu_topic = "alerts.homelab" }"#,
            r#"to = { kyu_topic = "kyu.alerts" }"#,
        ),
        GOOD.replace(
            r#"from = { http_path = "/uptime-kuma" }"#,
            r#"from = { http_path = "uptime-kuma" }"#,
        ),
        GOOD.replace(
            r#"from = { http_path = "/uptime-kuma" }"#,
            r#"from = { http_path = "/metrics" }"#,
        ),
        GOOD.replace(
            r#"name = "alertmanager""#,
            "name = \"alertmanager\"\ninbound_token = \"x\"",
        ),
        GOOD.replace(
            r#"to = { kyu_topic = "alerts.homelab" }"#,
            "to = { kyu_topic = \"alerts.homelab\" }\nmethod = \"PUT\"",
        ),
        GOOD.replace(
            "content_type = \"application/json\"\nbody = '''",
            "body = '''",
        ),
        GOOD.replace(
            r#"name = "alertmanager""#,
            "name = \"alertmanager\"\nlease_ms = 20000\ntimeout_ms = 10000\nretries = 2",
        ),
        GOOD.replace(
            r#"from = { kyu_topic = "alerts.raw" }"#,
            r#"from = { kyu_topic = "switchboard.events" }"#,
        ),
        GOOD.replace(r#"token = "${KYU_TOKEN}""#, r#"token = "${NOWHERE}""#),
        GOOD.replace(
            r#"base_url = "http://127.0.0.1:8080""#,
            r#"base_url = "127.0.0.1:8080""#,
        ),
        GOOD.replace(
            r#"to = { url = "http://127.0.0.1:9999/hook" }"#,
            r#"to = { url = "nope" }"#,
        ),
        GOOD.replace(
            r#"from = { kyu_topic = "alerts.raw" }"#,
            r#"from = { kyu_topic = "alerts.raw", http_path = "/x" }"#,
        ),
        GOOD.replace(
            "{\"alert\": {{ alerts.0.labels.alertname }}}",
            "{\"alert\": \"{{ alerts.0.labels.alertname }}\"}",
        ),
    ];

    let mut variants: Vec<std::mem::Discriminant<ConfigError>> = Vec::new();
    for text in &cases {
        let e = match load(text) {
            Ok(_) => panic!("this config should have been refused:\n{text}"),
            Err(e) => e,
        };
        let message = e.to_string();
        assert!(
            message.contains("What now:"),
            "no remedy on {e:?}: {message}"
        );
        let kind = std::mem::discriminant(&e);
        if !variants.contains(&kind) {
            variants.push(kind);
        }
    }
    assert!(
        variants.len() >= 15,
        "the walk should cover distinct variants, got {}",
        variants.len()
    );
}
