//! The one-line, plain-English reasons a message is given up on, as the
//! dashboard shows them. Built from fixed words only (plus a status
//! number): the full error text can hold a destination path, a header
//! name or a receiver's own words, and none of that may reach a snapshot.

use crate::translate::RenderError;

pub(crate) const NOT_JSON: &str = "the message was not JSON, so no template could read it";

/// Why a message could not be turned into a request.
pub(crate) fn render_summary(error: &RenderError) -> &'static str {
    match error {
        RenderError::PayloadNotJson { .. } => NOT_JSON,
        RenderError::Target { .. } => {
            "the destination address could not be built from this message"
        }
        RenderError::Render { .. } | RenderError::InvalidJson { .. } => {
            "the template could not be filled in from this message"
        }
    }
}

/// Why a destination's permanent refusal ended a message.
pub(crate) fn refused_summary(status: u16) -> String {
    format!(
        "the destination refused this message with status {status}, which retrying will not change"
    )
}
