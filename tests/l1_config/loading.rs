use super::*;

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
