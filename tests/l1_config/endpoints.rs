use super::*;

#[test]
fn k10_a_retired_inbound_token_is_refused_not_ignored() {
    // Until 3.0.0 this asserted that an inbound token on a kyu source was
    // a config error. H1 retired the field entirely, so the refusal now
    // applies to every source kind — and it names the command that took
    // its place, which is the part that makes it useful to whoever hits it.
    let text = GOOD.replace(
        r#"name = "alertmanager""#,
        "name = \"alertmanager\"\ninbound_token = \"${KYU_TOKEN}\"",
    );
    let message = err(&text);
    assert_usable(&message, &["inbound_token", "alertmanager"]);
    assert!(
        message.contains("chassis clients issue"),
        "the replacement command is named: {message}"
    );
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
        .replace("[reporting]\ntopic = \"switchboard.events\"\n", "")
        .replace(r#"headers = { authorization = "${KYU_TOKEN}" }"#, "");
    assert_usable(&err(&text), &["alertmanager", "base_url"]);
}

#[test]
fn k10_the_self_report_topic_is_validated_like_any_other() {
    // Phase 7 audit, G6: it was not validated at all, so a name kyu
    // reserves started up green and then 403'd on every publish — the one
    // feature whose job is to make silence visible, failing silently.
    let reserved = GOOD.replace(
        r#"topic = "switchboard.events""#,
        r#"topic = "kyu.switchboard""#,
    );
    assert_usable(&err(&reserved), &["kyu.switchboard", "403"]);

    let bad_name = GOOD.replace(
        r#"topic = "switchboard.events""#,
        r#"topic = "Switch Board!!""#,
    );
    assert_usable(&err(&bad_name), &["Switch Board!!"]);

    let no_hub = GOOD
        .replace(
            "[kyu]\nbase_url = \"http://127.0.0.1:8080\"\ntoken = \"${KYU_TOKEN}\"\n",
            "",
        )
        .replace(
            r#"from = { kyu_topic = "alerts.raw" }"#,
            r#"from = { http_path = "/am" }"#,
        )
        .replace(
            r#"to = { kyu_topic = "alerts.homelab" }"#,
            r#"to = { url = "http://127.0.0.1:9/x" }"#,
        )
        .replace(r#"headers = { authorization = "${KYU_TOKEN}" }"#, "");
    assert_usable(&err(&no_hub), &["reporting", "base_url"]);
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
