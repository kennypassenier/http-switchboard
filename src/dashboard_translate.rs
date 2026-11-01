//! "Try a translation": the page that shows what a profile makes of a
//! message without sending anything (kp-themes 11 adoption, row 9).
//!
//! Until now the only way to see what a template makes of a message was to
//! send one through to the destination, or to run `http-switchboard test`
//! on the host. This page does the second from the browser.
//!
//! # Nothing is delivered
//!
//! The route renders the profile's template with [`translate::prepare`],
//! the function the pump and the door use, and the `test` subcommand prints
//! the result of. It never touches a sink, a hub or the network: `prepare`
//! is pure by contract. What the route answers with is the rendered body,
//! its content type, and notes; **never** the delivery's headers (a header
//! may carry a token) or its target (the path of a Home Assistant webhook
//! *is* the credential, see `dashboard.rs`). The template text is
//! configuration, not a secret, and is shown.
//!
//! # What this module adds around `prepare`
//!
//! * a **size cap** on the message ([`MAX_MESSAGE_BYTES`]), refused with a
//!   sentence that says what to do;
//! * an **error in three parts** (what, why, remedy) built from
//!   [`RenderError`], with the template line the engine named and, when the
//!   cause is a field the message does not have, which field;
//! * **notes** for the defaults the template filled in (`| default(...)`
//!   on a path the message lacks), so a silent fallback is visible;
//! * a **field table**: each field of the message the template reads, the
//!   key of the result it lands under and its value in this message;
//! * a **scratch template**: the page may post a draft template with the
//!   message; it is rendered by the same code for that one request and
//!   never stored (see `scratch`);
//! * an **example message** built from the fields a template reads, for the
//!   box's starting text and the alert's fix button. It is a guess from the
//!   template's text and says so on the page.
//!
//! The scanner below reads the template's `{{ … }}` and `{% … %}` regions
//! by hand. It is deliberately forgiving and never decides whether a
//! message renders (the engine does that); it only explains, and a missed
//! reference degrades an explanation to the engine's own sentence.

mod example;
mod explain;
mod fields;
mod keys;
mod lexing;
mod preview;
mod routes;
mod scanner;
mod scratch;
#[cfg(test)]
mod tests;
mod words;

pub use example::example_message;
pub use explain::Note;
pub use preview::{preview, Failure, Preview};
pub use routes::install;

/// The page.
pub const PAGE_ROUTE: &str = "/translate";
/// Where the page posts a message: `{name}` is the profile.
pub const PREVIEW_ROUTE: &str = "/translate/{name}";
/// The most a message may weigh. A sample is one recorded message; this is
/// generous for that and small enough that a paste of a whole log is
/// refused with a sentence instead of rendered.
pub const MAX_MESSAGE_BYTES: usize = 256 * 1024;
/// The most a draft template may weigh. The config has no cap of its own;
/// a template is one document with a few fields, so this is generous and
/// still refuses a paste of something else with a sentence.
pub const MAX_TEMPLATE_BYTES: usize = 64 * 1024;
