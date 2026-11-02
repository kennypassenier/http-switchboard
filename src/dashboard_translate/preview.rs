//! The answer to "try this message": a [`Preview`] or a [`Failure`], built
//! from [`translate::prepare`] and nothing else (no sink, no network).

use serde::Serialize;
use serde_json::Value;

use super::explain::{defaults_filled, explain_missing, Note};
use super::fields::{Analysis, FieldRow};
use super::scanner::Reference;
use super::MAX_MESSAGE_BYTES;
use crate::config::{Profile, Sink};
use crate::translate::{self, RenderError};

/// What a successful preview carries.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Preview {
    pub profile: String,
    pub body: String,
    pub content_type: String,
    pub bytes: usize,
    pub notes: Vec<Note>,
    /// The fields the template reads and where each lands in the result.
    pub fields: Vec<FieldRow>,
}

/// What a refused preview carries: the kit's `{ error, remedy }` plus the
/// reason.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Failure {
    pub error: String,
    pub why: String,
    pub remedy: String,
    /// `message` (the box), `template` or `size`.
    pub kind: &'static str,
    /// Whether putting the example message back could mend it.
    pub fixable: bool,
    /// The template line the engine named.
    pub line: Option<usize>,
    /// The fields the template reads, when the message could be read, so
    /// the table can say which one is missing.
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub fields: Vec<FieldRow>,
}

/// Render `profile`'s template against `payload` and say what came out.
/// Pure: no sink, no network.
pub fn preview(profile: &Profile, payload: &[u8]) -> Result<Preview, Box<Failure>> {
    if payload.len() > MAX_MESSAGE_BYTES {
        return Err(Box::new(Failure {
            error: "The message is too large to try".to_string(),
            why: format!(
                "It is {} KiB and the limit is {} KiB.",
                payload.len().div_ceil(1024),
                MAX_MESSAGE_BYTES / 1024
            ),
            remedy: "Paste one recorded message instead of a whole log; a template only reads \
                     the fields of a single message."
                .to_string(),
            kind: "size",
            fixable: false,
            line: None,
            fields: Vec::new(),
        }));
    }
    // One scan of the template, shared by the table, the notes and the
    // explanation of a failure.
    let analysis = Analysis::of(&profile.body);
    match translate::prepare(profile, payload) {
        Ok(delivery) => {
            let message: Value = serde_json::from_slice(payload).unwrap_or(Value::Null);
            Ok(Preview {
                profile: profile.name.clone(),
                bytes: delivery.body.len(),
                content_type: delivery.content_type,
                notes: defaults_filled(&analysis.refs, &message),
                fields: analysis.field_table(&message),
                body: delivery.body,
            })
        }
        Err(error) => {
            let mut failure = Box::new(failure_of(profile, payload, &error, &analysis));
            if let Ok(message) = serde_json::from_slice::<Value>(payload) {
                failure.fields = analysis.field_table(&message);
            }
            Err(failure)
        }
    }
}

/// The remedy sentence every `RenderError` ends with.
fn remedy_of(error: &RenderError) -> String {
    let text = error.to_string();
    let remedy = text
        .split_once("What now: ")
        .map_or(text.as_str(), |(_, r)| r);
    let mut chars = remedy.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}

/// The line the engine named in `… (in <string>:2)`.
pub(super) fn line_of(detail: &str) -> Option<usize> {
    let at = detail.find("<string>:")? + "<string>:".len();
    let digits: String = detail[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn failure_of(
    profile: &Profile,
    payload: &[u8],
    error: &RenderError,
    analysis: &Analysis,
) -> Failure {
    let remedy = remedy_of(error);
    match error {
        RenderError::PayloadNotJson { detail, .. } => Failure {
            error: "The message is not JSON".to_string(),
            why: format!("{detail}. A template reads fields, and plain text or a form has none."),
            remedy,
            kind: "message",
            fixable: true,
            line: None,
            fields: Vec::new(),
        },
        // `prepare` renders the body first and the destination's path
        // after it, and both raise `Render`: only an error of the body
        // carries a line of the body template.
        RenderError::Render { detail, .. } if body_fails(profile, payload) => {
            render_failure(&analysis.refs, payload, detail, remedy)
        }
        // The detail of these may carry the destination's path, which is
        // the credential of a webhook: it is never repeated here.
        RenderError::Render { .. } | RenderError::Target { .. } => target_failure(remedy),
        RenderError::InvalidJson { detail, .. } => Failure {
            error: "The translated body is not valid JSON".to_string(),
            why: format!(
                "The profile delivers {} but what the template wrote does not parse: {detail}.",
                profile.content_type
            ),
            remedy,
            kind: "template",
            fixable: false,
            line: None,
            fields: Vec::new(),
        },
    }
}

fn target_failure(remedy: String) -> Failure {
    Failure {
        error: "The destination address could not be built".to_string(),
        why: "The destination's path uses a value this message does not provide, or builds \
              an address that is not allowed. The address itself is never shown."
            .to_string(),
        remedy,
        kind: "template",
        fixable: true,
        line: None,
        fields: Vec::new(),
    }
}

/// Whether the body template alone fails to render: the profile is tried
/// again with a sink that has no templated path.
fn body_fails(profile: &Profile, payload: &[u8]) -> bool {
    let mut body_only = profile.clone();
    body_only.sink = Sink::Kyu {
        topic: String::new(),
    };
    matches!(
        translate::prepare(&body_only, payload),
        Err(RenderError::Render { .. })
    )
}

/// The failure for a template the engine could not render: the line it
/// named and, when the cause is a field the message lacks, which field.
fn render_failure(refs: &[Reference], payload: &[u8], detail: &str, remedy: String) -> Failure {
    let line = line_of(detail);
    let message: Value = serde_json::from_slice(payload).unwrap_or(Value::Null);
    let why = line
        .and_then(|n| explain_missing(refs, n, &message))
        .unwrap_or_else(|| detail.replace("<string>", "the template"));
    // The engine's remedy is about a field the message lacks; a syntax error
    // (most often a draft being typed) needs the line fixed instead.
    let remedy = if detail.contains("syntax error") {
        "Fix the template on the line named: a tag or a brace is not closed or is out of \
         place. The profile's own template is not changed by anything typed here."
            .to_string()
    } else {
        remedy
    };
    Failure {
        error: "The template could not be rendered".to_string(),
        why,
        remedy,
        kind: "template",
        fixable: true,
        line,
        fields: Vec::new(),
    }
}
