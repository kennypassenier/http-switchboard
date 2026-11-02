//! The destination address of a url sink, with its templated path.
//!
//! The path of a webhook IS its credential (a Home Assistant webhook id),
//! so no error produced here repeats the url, the template or the
//! rendered path: they travel back to the sender and into the logs.

use super::limits::template_env;
use super::{Profile, RenderError};

/// K7: a destination whose *path* carries a value from the message —
/// `/devices/{{ id }}/state`. Everything about where the request goes
/// still comes from the config (AR10): the scheme, the host and the port
/// are split off before rendering and put back afterwards, so a template
/// cannot reach them. Each interpolated value is percent-encoded by the
/// engine rather than by the template author, the same "mechanism, not
/// habit" rule AR7 applies to JSON.
pub(super) fn render_target(
    profile: &Profile,
    url: &str,
    json: &serde_json::Value,
) -> Result<String, RenderError> {
    if !url.contains("{{") {
        return Ok(url.to_string());
    }
    let refuse = |detail: &str| RenderError::Target {
        profile: profile.name.clone(),
        detail: detail.to_string(),
    };

    let (scheme, rest) = url
        .split_once("://")
        .ok_or_else(|| refuse("the configured address has no scheme"))?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if authority.contains("{{") {
        return Err(refuse("the host or port is templated"));
    }

    let mut env = template_env(false);
    env.set_formatter(|out, _state, value| {
        let raw = match value.as_str() {
            Some(text) => text.to_string(),
            None => value.to_string(),
        };
        out.write_str(&percent_encode(&raw))?;
        Ok(())
    });
    // Only the error kind is kept: the engine's text can quote the template.
    let rendered = env.render_str(path, json).map_err(|e| {
        refuse(&format!(
            "the address template could not be rendered ({})",
            e.kind()
        ))
    })?;

    if escapes_address(&rendered) {
        return Err(refuse(
            "a value in the address template would leave the address",
        ));
    }
    Ok(format!("{scheme}://{authority}{rendered}"))
}

/// Belt and braces: percent-encoding already makes these impossible, and
/// a future change to the encoder should not quietly re-open them. A
/// segment of exactly `.` or `..` is traversal; a longer one that merely
/// contains dots ("Waiting...") is an ordinary value.
fn escapes_address(rendered: &str) -> bool {
    let path = rendered.split(['?', '#']).next().unwrap_or_default();
    rendered.contains("://") || path.split('/').any(|seg| seg == "." || seg == "..")
}

/// Percent-encode everything that is not unreserved. Deliberately strict:
/// a slash inside a *value* is encoded, so a value can never add a path
/// segment of its own.
pub fn percent_encode(value: &str) -> String {
    let mut out = String::with_capacity(value.len());
    for byte in value.as_bytes() {
        match byte {
            b'A'..=b'Z' | b'a'..=b'z' | b'0'..=b'9' | b'-' | b'_' | b'.' | b'~' => {
                out.push(*byte as char)
            }
            other => out.push_str(&format!("%{other:02X}")),
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dots_inside_a_segment_are_fine_but_dot_segments_are_not() {
        assert!(!escapes_address("/status/Waiting.../x"));
        assert!(escapes_address("/a/../b"));
        assert!(escapes_address("/a/./b"));
        assert!(escapes_address("/a/.."));
    }

    fn profile(url: &str) -> Profile {
        Profile {
            name: "p".into(),
            subscription: "p".into(),
            source: crate::config::Source::Http { path: "/p".into() },
            sink: crate::config::Sink::Url {
                url: url.into(),
                method: "POST".into(),
            },
            content_type: "application/json".into(),
            body: "{}".into(),
            headers: Default::default(),
            timeout_ms: 1,
            retries: 0,
            lease_ms: 1,
            max_attempts: 1,
            forward_error_body: false,
        }
    }

    #[test]
    fn a_refused_value_never_appears_in_the_error() {
        let url = "http://h:1/api/webhook/SECRETID/{{ v }}";
        let err = render_target(&profile(url), url, &serde_json::json!({"v": ".."}))
            .unwrap_err()
            .to_string();
        assert!(err.contains("would leave the address"), "{err}");
        assert!(
            !err.contains("SECRETID") && !err.contains("webhook"),
            "{err}"
        );
    }

    #[test]
    fn an_undefined_value_does_not_quote_the_template() {
        let url = "http://h:1/api/webhook/SECRETID/{{ v }}";
        let err = render_target(&profile(url), url, &serde_json::json!({}))
            .unwrap_err()
            .to_string();
        assert!(!err.contains("SECRETID"), "{err}");
    }

    #[test]
    fn a_value_with_dots_is_accepted() {
        let url = "http://h:1/s/{{ v }}";
        let ok = render_target(&profile(url), url, &serde_json::json!({"v": "Waiting..."}));
        assert_eq!(ok.unwrap(), "http://h:1/s/Waiting...");
    }
}
