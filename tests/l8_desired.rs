//! L8 — the two Desired features (K7, W4).

mod support;

use std::collections::HashMap;

use http_switchboard::config::{self, Profile};
use http_switchboard::translate::{self, Target};

fn env() -> impl config::EnvLookup {
    let map: HashMap<String, String> = HashMap::new();
    move |k: &str| map.get(k).cloned()
}

fn profile(to: &str) -> Profile {
    let text = format!(
        r#"
[[profiles]]
name = "t"
from = {{ http_path = "/t" }}
to = {to}
content_type = "application/json"
body = '''{{"ok": true}}'''
"#
    );
    config::load("t.toml", &text, &env())
        .expect("test profile must load")
        .profiles
        .remove(0)
}

fn profile_with_method(to: &str, method: &str) -> Profile {
    let text = format!(
        r#"
[[profiles]]
name = "t"
from = {{ http_path = "/t" }}
to = {to}
method = "{method}"
content_type = "application/json"
body = '''{{"ok": true}}'''
"#
    );
    config::load("t.toml", &text, &env())
        .expect("test profile must load")
        .profiles
        .remove(0)
}

fn binary() -> std::path::PathBuf {
    let mut path = std::env::current_exe().unwrap();
    path.pop();
    path.pop();
    path.push("http-switchboard");
    path
}

#[test]
fn k7_a_path_segment_may_come_from_the_message() {
    let p = profile_with_method(
        r#"{ url = "http://127.0.0.1:9/devices/{{ id }}/state" }"#,
        "PUT",
    );
    let delivery = translate::prepare(&p, br#"{"id": "kitchen-lamp"}"#).unwrap();

    assert_eq!(
        delivery.target,
        Target::Url {
            url: "http://127.0.0.1:9/devices/kitchen-lamp/state".into(),
            method: "PUT".into()
        }
    );
}

#[test]
fn k7_a_value_cannot_add_a_path_segment_or_leave_the_host() {
    // AR10, made a property rather than a promise: every interpolated
    // value is percent-encoded by the engine, so a slash, a traversal or
    // a whole URL in the message stays one segment's worth of text.
    let p = profile(r#"{ url = "http://127.0.0.1:9/devices/{{ id }}/state" }"#);

    // Encoded into one harmless segment: a slash or a whole URL in the
    // value cannot add a segment or change the host.
    for (payload, must_not_contain) in [
        (&br#"{"id": "a/b"}"#[..], "/a/b/"),
        (&br#"{"id": "http://evil.example/x"}"#[..], "evil.example/x"),
    ] {
        let delivery = translate::prepare(&p, payload).unwrap();
        let Target::Url { url, .. } = &delivery.target else {
            panic!("expected a URL target");
        };
        assert!(
            url.starts_with("http://127.0.0.1:9/devices/"),
            "the address moved: {url}"
        );
        assert!(
            !url.contains(must_not_contain),
            "a value escaped its segment: {url}"
        );
    }

    // A traversal is refused outright rather than encoded and hoped
    // about: %2F decoding differs between servers, so fail closed.
    let e = translate::prepare(&p, br#"{"id": "../../admin"}"#).unwrap_err();
    let message = e.to_string();
    assert!(message.contains("What now:"), "no remedy: {message}");
}

#[test]
fn k7_a_templated_host_is_refused() {
    let p = profile(r#"{ url = "http://{{ host }}/x" }"#);
    let e = translate::prepare(&p, br#"{"host": "evil.example"}"#).unwrap_err();
    let message = e.to_string();
    assert!(message.contains("What now:"), "no remedy: {message}");
    assert!(message.contains("host"), "{message}");
}

#[test]
fn w4_the_dry_run_shows_the_result_and_sends_nothing() {
    // Against the shipped config and the recorded Alertmanager payload,
    // so this also proves the two agree with each other.
    let out = std::process::Command::new(binary())
        .args([
            "test",
            "--config",
            "deploy/config.example.toml",
            "--profile",
            "alertmanager",
            "--input",
            "tests/fixtures/alertmanager_firing.json",
        ])
        .env("KYU_TOKEN", "vault-value")
        .output()
        .expect("the binary must be built");

    let stdout = String::from_utf8_lossy(&out.stdout);
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(out.status.success(), "stdout: {stdout}\nstderr: {stderr}");

    let expected = std::fs::read_to_string("tests/fixtures/alertmanager_expected.json").unwrap();
    assert!(
        stdout.contains(expected.trim()),
        "the dry run must show exactly what the real path produces.\nexpected:\n{expected}\ngot:\n{stdout}"
    );
    assert!(stdout.contains("would send POST"), "{stdout}");
}

#[test]
fn w4_a_header_value_is_never_printed() {
    let dir = std::env::temp_dir().join(format!("hsw-l8-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let config = dir.join("with-header.toml");
    std::fs::write(
        &config,
        r#"
[[profiles]]
name = "t"
from = { http_path = "/t" }
to = { url = "http://127.0.0.1:9/x" }
content_type = "application/json"
headers = { authorization = "Bearer do-not-print-me" }
body = '''{"ok": true}'''
"#,
    )
    .unwrap();
    let input = dir.join("in.json");
    std::fs::write(&input, "{}").unwrap();

    let out = std::process::Command::new(binary())
        .args([
            "test",
            "--config",
            config.to_str().unwrap(),
            "--profile",
            "t",
            "--input",
            input.to_str().unwrap(),
        ])
        .output()
        .unwrap();

    let printed = format!(
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    assert!(out.status.success(), "{printed}");
    assert!(
        !printed.contains("do-not-print-me"),
        "a header value was printed: {printed}"
    );
    assert!(printed.contains("authorization: ***"), "{printed}");
}

#[test]
fn w4_an_unknown_profile_lists_the_ones_that_exist() {
    let out = std::process::Command::new(binary())
        .args([
            "test",
            "--config",
            "deploy/config.example.toml",
            "--profile",
            "nope",
            "--input",
            "tests/fixtures/alertmanager_firing.json",
        ])
        .env("KYU_TOKEN", "vault-value")
        .output()
        .unwrap();

    assert!(!out.status.success());
    let stderr = String::from_utf8_lossy(&out.stderr);
    assert!(stderr.contains("What now:"), "no remedy: {stderr}");
    assert!(stderr.contains("alertmanager"), "{stderr}");
}

#[test]
fn k7_awkward_values_in_a_templated_path_stay_one_segment() {
    // Phase 7, G7: the table covered the obvious hostile shapes; these are
    // the awkward ones — empty, structured, and with line breaks in.
    let p = profile(r#"{ url = "http://127.0.0.1:9/devices/{{ id }}/state" }"#);

    let empty = translate::prepare(&p, br#"{"id": ""}"#).unwrap();
    let Target::Url { url, .. } = &empty.target else {
        panic!("expected a URL target")
    };
    assert_eq!(
        url, "http://127.0.0.1:9/devices//state",
        "an empty value must not silently vanish into the path"
    );

    for payload in [
        &br#"{"id": {"a": 1}}"#[..],
        &br#"{"id": [1, 2]}"#[..],
        &br#"{"id": "line\r\nInjected: header"}"#[..],
        &br#"{"id": "?query=1"}"#[..],
        &br##"{"id": "#fragment"}"##[..],
    ] {
        let d = translate::prepare(&p, payload).unwrap();
        let Target::Url { url, .. } = &d.target else {
            panic!("expected a URL target")
        };
        assert!(
            url.starts_with("http://127.0.0.1:9/devices/") && url.ends_with("/state"),
            "the shape of the address must survive: {url}"
        );
        for forbidden in ['\r', '\n', '?', '#'] {
            assert!(
                !url.contains(forbidden),
                "{forbidden:?} must be encoded, not passed through: {url}"
            );
        }
    }
}
