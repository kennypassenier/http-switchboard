use super::*;

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
