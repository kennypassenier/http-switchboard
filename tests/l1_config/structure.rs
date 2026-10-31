use super::*;

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
