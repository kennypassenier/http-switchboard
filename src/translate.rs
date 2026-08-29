//! The pure translation core (L2 — K5, K12, K14; AR1, AR7).
//!
//! Pure by contract: no network, no clock, no environment. Give it a
//! profile and the bytes that arrived, get back a `Delivery` or an error.
//! That is what lets the hostile-payload suite (K14) and the recorded
//! Alertmanager fixture (K12) run as ordinary unit tests.
//!
//! Two decisions from AR7 live here, and both are mechanisms rather than
//! habits:
//!
//! * **JSON autoescape is on** for profiles that deliver JSON, so every
//!   interpolated value is escaped by the engine. Remembering `| tojson`
//!   in every template forever is not a security control.
//! * **Undefined is strict.** A field that disappeared from the source's
//!   payload is an error with a remedy, not an empty string that travels
//!   all the way to a phone as "Homelab-alarm: " while every layer
//!   reports success.

use std::collections::BTreeMap;

use minijinja::{AutoEscape, Environment, UndefinedBehavior};

use crate::config::{Profile, Sink};
use crate::secret::Secret;

/// What the sinks in L3 are asked to perform. Note what is NOT here: the
/// destination is copied from the profile, never rendered, so no incoming
/// message can influence where its own translation goes (K14, AR10).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Delivery {
    pub target: Target,
    pub content_type: String,
    pub headers: BTreeMap<String, Secret>,
    pub body: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Target {
    Url { url: String, method: String },
    KyuTopic { topic: String },
}

#[derive(Debug, thiserror::Error)]
pub enum RenderError {
    #[error("profile '{profile}': the incoming message is not JSON ({detail}). What now: check what the source actually sends — a form-encoded or plain-text body has no fields for a template to read, and rendering it would silently produce empty values.")]
    PayloadNotJson { profile: String, detail: String },

    #[error("profile '{profile}': the template could not be rendered — {detail}. What now: if a field is undefined, either the source stopped sending it or its name changed; write `| default(...)` to accept that explicitly, or fix the path. An empty value is never produced on your behalf.")]
    Render { profile: String, detail: String },

    #[error("profile '{profile}': the destination address could not be built — {detail}. What now: only path segments may be templated; the scheme, host and port always come from the config, so no incoming message can choose where its own translation goes.")]
    Target { profile: String, detail: String },

    #[error("profile '{profile}': the rendered body is not valid JSON ({detail}), while the profile declares {content_type}. What now: look at the template's punctuation — a missing comma or brace; the values themselves are escaped for you, so the fault is in the fixed text around them.")]
    InvalidJson {
        profile: String,
        content_type: String,
        detail: String,
    },
}

/// The mistake JSON autoescape invites, caught at startup rather than in
/// production: an interpolation written *inside* quotes. The engine
/// already emits a complete JSON value, so `"{{ x }}"` produces `""x""`.
/// Verified against minijinja 2.x rather than assumed.
pub fn check_template(body: &str, content_type: &str) -> Result<(), String> {
    if !is_json(content_type) {
        return Ok(());
    }
    if body.contains("\"{{") || body.contains("}}\"") {
        return Err(
            "a template value is wrapped in quotes (\"{{ … }}\"), which produces doubled quotes: \
             this profile delivers JSON, so values are already emitted as complete JSON values. \
             What now: write {\"summary\": {{ alerts.0.annotations.summary }}} \
             without the surrounding quotes"
                .to_string(),
        );
    }
    Ok(())
}

fn is_json(content_type: &str) -> bool {
    content_type.starts_with("application/json") || content_type.ends_with("+json")
}

/// Translate one incoming message into one delivery. Pure.
pub fn prepare(profile: &Profile, payload: &[u8]) -> Result<Delivery, RenderError> {
    let json: serde_json::Value =
        serde_json::from_slice(payload).map_err(|e| RenderError::PayloadNotJson {
            profile: profile.name.clone(),
            detail: e.to_string(),
        })?;
    prepare_value(profile, &json)
}

/// The same, for a payload the hub already handed over parsed (AR8: the
/// template's input is the parsed payload, never the envelope around it).
pub fn prepare_value(profile: &Profile, json: &serde_json::Value) -> Result<Delivery, RenderError> {
    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    let json_profile = is_json(&profile.content_type);
    if json_profile {
        env.set_auto_escape_callback(move |_| AutoEscape::Json);
    } else {
        env.set_auto_escape_callback(|_| AutoEscape::None);
    }

    let body = env
        .render_str(&profile.body, json)
        .map_err(|e| RenderError::Render {
            profile: profile.name.clone(),
            detail: describe(&e),
        })?;

    // AR7's second net: prove the result is what the profile claims it is
    // before anyone is asked to accept it. Non-mutating on purpose — K3's
    // bar is byte-identical bytes, so nothing is re-serialised.
    if json_profile {
        if let Err(e) = serde_json::from_str::<serde_json::Value>(&body) {
            return Err(RenderError::InvalidJson {
                profile: profile.name.clone(),
                content_type: profile.content_type.clone(),
                detail: e.to_string(),
            });
        }
    }

    Ok(Delivery {
        target: match &profile.sink {
            Sink::Url { url, method } => Target::Url {
                url: render_target(profile, url, json)?,
                method: method.clone(),
            },
            Sink::Kyu { topic } => Target::KyuTopic {
                topic: topic.clone(),
            },
        },
        content_type: profile.content_type.clone(),
        headers: profile.headers.clone(),
        body,
    })
}

/// minijinja's chain says which field was undefined; the top-level
/// message alone does not.
fn describe(err: &minijinja::Error) -> String {
    let mut parts = vec![err.to_string()];
    let mut source = std::error::Error::source(err);
    while let Some(e) = source {
        parts.push(e.to_string());
        source = e.source();
    }
    parts.join(": ")
}

/// K7: a destination whose *path* carries a value from the message —
/// `/devices/{{ id }}/state`. Everything about where the request goes
/// still comes from the config (AR10): the scheme, the host and the port
/// are split off before rendering and put back afterwards, so a template
/// cannot reach them. Each interpolated value is percent-encoded by the
/// engine rather than by the template author, the same "mechanism, not
/// habit" rule AR7 applies to JSON.
fn render_target(
    profile: &Profile,
    url: &str,
    json: &serde_json::Value,
) -> Result<String, RenderError> {
    if !url.contains("{{") {
        return Ok(url.to_string());
    }

    let (scheme, rest) = url.split_once("://").ok_or_else(|| RenderError::Target {
        profile: profile.name.clone(),
        detail: format!("'{url}' has no scheme"),
    })?;
    let (authority, path) = match rest.find('/') {
        Some(i) => (&rest[..i], &rest[i..]),
        None => (rest, ""),
    };
    if authority.contains("{{") {
        return Err(RenderError::Target {
            profile: profile.name.clone(),
            detail: "the host or port is templated".to_string(),
        });
    }

    let mut env = Environment::new();
    env.set_undefined_behavior(UndefinedBehavior::Strict);
    env.set_formatter(|out, _state, value| {
        let raw = match value.as_str() {
            Some(text) => text.to_string(),
            None => value.to_string(),
        };
        out.write_str(&percent_encode(&raw))?;
        Ok(())
    });

    let rendered = env
        .render_str(path, json)
        .map_err(|e| RenderError::Render {
            profile: profile.name.clone(),
            detail: describe(&e),
        })?;

    // Belt and braces: percent-encoding already makes these impossible,
    // and a future change to the encoder should not quietly re-open them.
    if rendered.contains("..") || rendered.contains("://") {
        return Err(RenderError::Target {
            profile: profile.name.clone(),
            detail: format!("the rendered path '{rendered}' escapes its own address"),
        });
    }

    Ok(format!("{scheme}://{authority}{rendered}"))
}

/// Percent-encode everything that is not unreserved. Deliberately strict:
/// a slash inside a *value* is encoded, so a value can never add a path
/// segment of its own.
fn percent_encode(value: &str) -> String {
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
