//! The scratch template: what the page may send besides a message.
//!
//! The page can post an envelope `{ "message": "<text>", "template":
//! "<text>" }` instead of the bare message. The `template` is a draft the
//! person is editing; it replaces the profile's template for that one
//! preview only. Nothing is stored: the profile in the running config is
//! never touched, the draft lives in the request and is gone with it.
//!
//! The draft is rendered by exactly the code the pump uses
//! ([`crate::translate::prepare`]) after exactly the check the config
//! loader runs ([`crate::translate::check_template`]), and it is held to a
//! size cap like the message. A draft that does not parse or render comes
//! back as a [`Failure`], never as a 500.

use serde::Deserialize;

use super::preview::Failure;
use super::MAX_TEMPLATE_BYTES;
use crate::config::Profile;

/// The content type of the envelope. A request of any other type is a
/// bare message, as before.
pub const ENVELOPE_TYPE: &str = "application/vnd.hsb.try+json";

/// The envelope.
#[derive(Debug, Deserialize)]
pub struct Envelope {
    pub message: String,
    #[serde(default)]
    pub template: Option<String>,
}

pub(super) fn refused(error: &str, why: String, remedy: &str, kind: &'static str) -> Box<Failure> {
    Box::new(Failure {
        error: error.to_string(),
        why,
        remedy: remedy.to_string(),
        kind,
        fixable: false,
        line: None,
        fields: Vec::new(),
    })
}

/// Read the envelope; a body that is not one is refused in words.
pub fn open(body: &[u8]) -> Result<Envelope, Box<Failure>> {
    serde_json::from_slice(body).map_err(|_| {
        refused(
            "The request was not understood",
            "The page sends the message and the draft template together, and this request \
             was not in that shape."
                .to_string(),
            "Reload the page and try again.",
            "request",
        )
    })
}

/// `profile` with `template` in place of its own; the destination, the
/// headers and the content type stay the profile's.
pub fn with_template(profile: &Profile, template: &str) -> Result<Profile, Box<Failure>> {
    if template.len() > MAX_TEMPLATE_BYTES {
        return Err(refused(
            "The template is too large to try",
            format!(
                "It is {} KiB and the limit is {} KiB.",
                template.len().div_ceil(1024),
                MAX_TEMPLATE_BYTES / 1024
            ),
            "Try a smaller template; a profile's template is one document with a few fields.",
            "size",
        ));
    }
    if template.trim().is_empty() {
        let mut failure = refused(
            "The template is empty",
            "A profile delivers what its template writes, and an empty template writes nothing."
                .to_string(),
            "Write the document the destination should receive, or reset to the profile's template.",
            "template",
        );
        failure.fixable = false;
        return Err(failure);
    }
    crate::translate::check_template(template, &profile.content_type).map_err(|problem| {
        let (why, remedy) = problem
            .split_once("What now: ")
            .map_or((problem.as_str(), "Fix the template."), |(w, r)| (w, r));
        refused(
            "The template cannot be used",
            format!("{}.", sentence(why.trim().trim_end_matches('.'))),
            &format!("{}.", sentence(remedy.trim().trim_end_matches('.'))),
            "template",
        )
    })?;
    let mut draft = profile.clone();
    draft.body = template.to_string();
    Ok(draft)
}

/// Capitalise the first letter.
fn sentence(text: &str) -> String {
    let mut chars = text.chars();
    chars
        .next()
        .map(|first| first.to_uppercase().chain(chars).collect())
        .unwrap_or_default()
}
