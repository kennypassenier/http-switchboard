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
//! * an **example message** built from the fields a template reads, for the
//!   box's starting text and the alert's fix button. It is a guess from the
//!   template's text and says so on the page.
//!
//! The scanner below reads the template's `{{ … }}` and `{% … %}` regions
//! by hand. It is deliberately forgiving and never decides whether a
//! message renders (the engine does that); it only explains, and a missed
//! reference degrades an explanation to the engine's own sentence.

use std::sync::Arc;

use axum::body::Bytes;
use axum::extract::{DefaultBodyLimit, Path, State};
use axum::http::StatusCode;
use axum::response::{Html, IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Extension, Router};
use serde::Serialize;
use serde_json::{json, Map, Value};

use crate::config::Profile;
use crate::dashboard::{Profiles, WEB_MOUNT};
use crate::translate::{self, RenderError};

/// The page.
pub const PAGE_ROUTE: &str = "/translate";
/// Where the page posts a message: `{name}` is the profile.
pub const PREVIEW_ROUTE: &str = "/translate/{name}";
/// The most a message may weigh. A sample is one recorded message; this is
/// generous for that and small enough that a paste of a whole log is
/// refused with a sentence instead of rendered.
pub const MAX_MESSAGE_BYTES: usize = 256 * 1024;

/// What a successful preview carries.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Preview {
    pub profile: String,
    pub body: String,
    pub content_type: String,
    pub bytes: usize,
    pub notes: Vec<Note>,
}

/// A default the template filled in.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Note {
    pub key: String,
    pub what: String,
    pub why: String,
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
}

/// Render `profile`'s template against `payload` and say what came out.
/// Pure: no sink, no network.
pub fn preview(profile: &Profile, payload: &[u8]) -> Result<Preview, Failure> {
    if payload.len() > MAX_MESSAGE_BYTES {
        return Err(Failure {
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
        });
    }
    match translate::prepare(profile, payload) {
        Ok(delivery) => {
            let message: Value = serde_json::from_slice(payload).unwrap_or(Value::Null);
            Ok(Preview {
                profile: profile.name.clone(),
                bytes: delivery.body.len(),
                content_type: delivery.content_type,
                notes: defaults_filled(&profile.body, &message),
                body: delivery.body,
            })
        }
        Err(error) => Err(failure_of(profile, payload, &error)),
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
fn line_of(detail: &str) -> Option<usize> {
    let at = detail.find("<string>:")? + "<string>:".len();
    let digits: String = detail[at..]
        .chars()
        .take_while(char::is_ascii_digit)
        .collect();
    digits.parse().ok()
}

fn failure_of(profile: &Profile, payload: &[u8], error: &RenderError) -> Failure {
    let remedy = remedy_of(error);
    match error {
        RenderError::PayloadNotJson { detail, .. } => Failure {
            error: "The message is not JSON".to_string(),
            why: format!("{detail}. A template reads fields, and plain text or a form has none."),
            remedy,
            kind: "message",
            fixable: true,
            line: None,
        },
        RenderError::Render { detail, .. } => {
            let line = line_of(detail);
            let message: Value = serde_json::from_slice(payload).unwrap_or(Value::Null);
            let why = line
                .and_then(|n| explain_missing(&profile.body, n, &message))
                .unwrap_or_else(|| detail.replace("<string>", "the template"));
            Failure {
                error: "The template could not be rendered".to_string(),
                why,
                remedy,
                kind: "template",
                fixable: true,
                line,
            }
        }
        // The detail of these may carry the destination's path, which is
        // the credential of a webhook: it is never repeated here.
        RenderError::Target { .. } => Failure {
            error: "The destination address could not be built".to_string(),
            why: "The destination's path uses a value this message does not provide, or builds \
                  an address that is not allowed. The address itself is never shown."
                .to_string(),
            remedy,
            kind: "template",
            fixable: true,
            line: None,
        },
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
        },
    }
}

// ── the template scanner ───────────────────────────────────────────────

/// One reference to the message that a template makes.
#[derive(Debug, Clone, PartialEq, Eq)]
struct Reference {
    path: Vec<String>,
    line: usize,
    /// Used as a list: counted, joined, mapped, iterated.
    list: bool,
    /// The text inside `default(...)`, as written.
    default: Option<String>,
    /// `attribute="a.b"` given to a filter that follows.
    attributes: Vec<String>,
}

const KEYWORDS: &[&str] = &[
    "if",
    "else",
    "elif",
    "endif",
    "and",
    "or",
    "not",
    "in",
    "is",
    "for",
    "endfor",
    "set",
    "true",
    "false",
    "none",
    "True",
    "False",
    "None",
    "with",
    "endwith",
    "macro",
    "endmacro",
    "call",
    "block",
    "endblock",
    "extends",
    "include",
    "import",
    "from",
    "as",
    "filter",
    "endfilter",
    "raw",
    "endraw",
    "loop",
    "self",
    "range",
    "dict",
    "namespace",
    "cycler",
    "joiner",
    "lipsum",
];

const LIST_FILTERS: &[&str] = &[
    "length",
    "count",
    "map",
    "join",
    "first",
    "last",
    "selectattr",
    "rejectattr",
    "sort",
    "reverse",
    "sum",
    "list",
    "batch",
    "slice",
    "unique",
    "min",
    "max",
];

/// The regions of a template that are code, as (first line, text).
fn regions(template: &str) -> Vec<(usize, &str)> {
    let mut found = Vec::new();
    let bytes = template.as_bytes();
    let mut at = 0;
    while at + 1 < bytes.len() {
        let close = match (bytes[at], bytes[at + 1]) {
            (b'{', b'{') => Some("}}"),
            (b'{', b'%') => Some("%}"),
            (b'{', b'#') => Some("#}"),
            _ => None,
        };
        let Some(close) = close else {
            at += 1;
            continue;
        };
        let start = at + 2;
        let end = template[start..]
            .find(close)
            .map_or(template.len(), |n| start + n);
        if close != "#}" {
            let line = 1 + template[..start].bytes().filter(|b| *b == b'\n').count();
            found.push((line, &template[start..end]));
        }
        at = end + close.len();
    }
    found
}

/// Read a quoted string from `chars[i]` (the opening quote); returns the
/// text and the index after the closing quote.
fn read_string(chars: &[char], mut i: usize) -> (String, usize) {
    let quote = chars[i];
    let mut text = String::new();
    i += 1;
    while i < chars.len() && chars[i] != quote {
        if chars[i] == '\\' && i + 1 < chars.len() {
            i += 1;
        }
        text.push(chars[i]);
        i += 1;
    }
    (text, (i + 1).min(chars.len()))
}

/// Read `(...)` from `chars[i]` (the opening bracket); returns the text
/// between the brackets and the index after the closing one.
fn read_call(chars: &[char], mut i: usize) -> (String, usize) {
    let mut depth = 0usize;
    let mut text = String::new();
    while i < chars.len() {
        match chars[i] {
            '"' | '\'' => {
                let (s, next) = read_string(chars, i);
                let quote = chars[i];
                text.push(quote);
                text.push_str(&s);
                text.push(quote);
                i = next;
                continue;
            }
            '(' => {
                depth += 1;
                if depth > 1 {
                    text.push('(');
                }
            }
            ')' => {
                depth -= 1;
                if depth == 0 {
                    return (text, i + 1);
                }
                text.push(')');
            }
            c => text.push(c),
        }
        i += 1;
    }
    (text, i)
}

fn attribute_in(args: &str) -> Option<String> {
    let at = args.find("attribute")? + "attribute".len();
    let rest = args[at..].trim_start().strip_prefix('=')?.trim_start();
    let chars: Vec<char> = rest.chars().collect();
    matches!(chars.first(), Some('"' | '\'')).then(|| read_string(&chars, 0).0)
}

/// Every reference to the message in `template`.
fn references(template: &str) -> Vec<Reference> {
    let mut found: Vec<Reference> = Vec::new();
    let mut locals: Vec<String> = Vec::new();
    for (first_line, text) in regions(template) {
        let chars: Vec<char> = text.chars().collect();
        let mut i = 0;
        let mut line = first_line;
        let mut after_pipe = false;
        let mut binding = false;
        let mut after_is = false;
        let mut last: Option<usize> = None;
        while i < chars.len() {
            let c = chars[i];
            if c == '\n' {
                line += 1;
                i += 1;
            } else if c == '"' || c == '\'' {
                i = read_string(&chars, i).1;
                after_pipe = false;
            } else if c == '|' {
                after_pipe = true;
                i += 1;
            } else if c.is_ascii_alphabetic() || c == '_' {
                let mut path = Vec::new();
                loop {
                    let from = i;
                    while i < chars.len() && (chars[i].is_ascii_alphanumeric() || chars[i] == '_') {
                        i += 1;
                    }
                    path.push(chars[from..i].iter().collect::<String>());
                    let more = i + 1 < chars.len()
                        && chars[i] == '.'
                        && (chars[i + 1].is_ascii_alphanumeric() || chars[i + 1] == '_');
                    if !more {
                        break;
                    }
                    i += 1;
                }
                let mut peek = i;
                while peek < chars.len() && chars[peek].is_whitespace() {
                    peek += 1;
                }
                let called = chars.get(peek) == Some(&'(');
                let root = path[0].clone();
                if after_pipe {
                    // A filter: it says how the reference before it is used.
                    let name = path.join(".");
                    let args = if called {
                        let (args, next) = read_call(&chars, peek);
                        i = next;
                        args
                    } else {
                        String::new()
                    };
                    if let Some(r) = last.and_then(|n| found.get_mut(n)) {
                        if LIST_FILTERS.contains(&name.as_str()) {
                            r.list = true;
                        }
                        if name == "default" || name == "d" {
                            r.default = Some(args.trim().to_string());
                        }
                        if let Some(a) = attribute_in(&args) {
                            r.attributes.push(a);
                        }
                    }
                    after_pipe = false;
                    continue;
                }
                after_pipe = false;
                if called {
                    if root == "default" || !KEYWORDS.contains(&root.as_str()) {
                        let (_, next) = read_call(&chars, peek);
                        i = next;
                    }
                    continue;
                }
                if after_is {
                    after_is = false;
                    continue;
                }
                match root.as_str() {
                    "for" | "set" => binding = true,
                    "in" | "=" => binding = false,
                    "is" => after_is = true,
                    _ => {}
                }
                if KEYWORDS.contains(&root.as_str()) {
                    continue;
                }
                if binding {
                    locals.push(root);
                    continue;
                }
                if locals.contains(&root) {
                    continue;
                }
                found.push(Reference {
                    path,
                    line,
                    list: false,
                    default: None,
                    attributes: Vec::new(),
                });
                last = Some(found.len() - 1);
            } else {
                if c == '=' {
                    binding = false;
                }
                if !c.is_whitespace() {
                    after_pipe = false;
                }
                i += 1;
            }
        }
    }
    found
}

/// Follow `path` through `value`; `Err(n)` is the index of the first
/// segment that is not there.
fn resolve<'a>(value: &'a Value, path: &[String]) -> Result<&'a Value, usize> {
    let mut at = value;
    for (n, segment) in path.iter().enumerate() {
        let next = match at {
            Value::Object(map) => map.get(segment),
            Value::Array(list) => segment.parse::<usize>().ok().and_then(|i| list.get(i)),
            _ => None,
        };
        at = next.ok_or(n)?;
    }
    Ok(at)
}

/// "Line 2 reads alerts.0, but the message has no alerts at position 0":
/// the first reference on `line` that the message lacks, in words.
fn explain_missing(template: &str, line: usize, message: &Value) -> Option<String> {
    for r in references(template).iter().filter(|r| r.line == line) {
        if r.default.is_some() {
            continue;
        }
        if let Err(n) = resolve(message, &r.path) {
            let shown = r.path.join(".");
            let before = if n == 0 {
                "the message".to_string()
            } else {
                r.path[..n].join(".")
            };
            let missing = &r.path[n];
            return Some(if missing.parse::<usize>().is_ok() {
                format!(
                    "Line {line} reads {shown}, but {before} has nothing at position {missing}, \
                     so there is nothing there to read."
                )
            } else {
                format!(
                    "Line {line} reads {shown}, but {before} has no \"{missing}\", so there is \
                     nothing there to read."
                )
            });
        }
    }
    None
}

/// The defaults the template filled in for this message.
fn defaults_filled(template: &str, message: &Value) -> Vec<Note> {
    let mut notes: Vec<Note> = Vec::new();
    for r in references(template) {
        let Some(given) = &r.default else { continue };
        if resolve(message, &r.path).is_ok() {
            continue;
        }
        let key = format!("{}@{}", r.path.join("."), r.line);
        if notes.iter().any(|n| n.key == key) {
            continue;
        }
        let name = r.path.last().cloned().unwrap_or_default();
        notes.push(Note {
            key,
            what: format!("No {name} in the message"),
            why: if given.is_empty() {
                format!("The template filled in an empty default (line {}).", r.line)
            } else {
                format!("The template filled in {given} (line {}).", r.line)
            },
        });
    }
    notes
}

/// An example message built from the fields the template reads.
pub fn example_message(template: &str) -> Value {
    fn put(node: &mut Value, path: &[String], list: bool, label: &str) {
        let Some((head, rest)) = path.split_first() else {
            let text = Value::String(format!("{label} example"));
            if node.is_null() {
                *node = if list { Value::Array(vec![text]) } else { text };
            } else if list && node.is_string() {
                *node = Value::Array(vec![text]);
            }
            return;
        };
        if let Ok(index) = head.parse::<usize>() {
            if !node.is_array() {
                *node = Value::Array(Vec::new());
            }
            if let Value::Array(items) = node {
                while items.len() <= index {
                    items.push(Value::Null);
                }
                put(&mut items[index], rest, list, label);
            }
        } else {
            if !node.is_object() {
                *node = Value::Object(Map::new());
            }
            if let Value::Object(map) = node {
                put(
                    map.entry(head.clone()).or_insert(Value::Null),
                    rest,
                    list,
                    label,
                );
            }
        }
    }
    let mut root = Value::Object(Map::new());
    for r in references(template) {
        let label = r.path.last().cloned().unwrap_or_default();
        put(&mut root, &r.path, r.list, &label);
        for attribute in &r.attributes {
            let mut deeper = r.path.clone();
            deeper.push("0".to_string());
            deeper.extend(attribute.split('.').map(str::to_string));
            put(
                &mut root,
                &deeper,
                false,
                attribute.rsplit('.').next().unwrap_or(""),
            );
        }
    }
    root
}

// ── routes ─────────────────────────────────────────────────────────────

/// Register the page, its place in the navigation and the route it posts
/// to. Called from `dashboard::install`.
pub fn install(app: &mut chassis::App, profiles: Arc<Profiles>) {
    app.page(chassis::shell::pages::Page::new(
        "translate",
        "Try a translation",
        PAGE_ROUTE,
    ));
    let router = Router::new()
        .route(PAGE_ROUTE, get(page))
        .route(PREVIEW_ROUTE, post(preview_route))
        // Twice the cap: a message over the cap reaches `preview` and is
        // refused in words; only something far larger gets the bare 413.
        .layer(DefaultBodyLimit::max(MAX_MESSAGE_BYTES * 2))
        .with_state(profiles);
    app.dashboard_routes(router);
}

fn json(status: StatusCode, body: String) -> Response {
    (status, [("content-type", "application/json")], body).into_response()
}

async fn preview_route(
    State(profiles): State<Arc<Profiles>>,
    Path(name): Path<String>,
    body: Bytes,
) -> Response {
    let Some(profile) = profiles.store().get(&name) else {
        let names = profiles
            .snapshot()
            .iter()
            .map(|p| p.name.clone())
            .collect::<Vec<_>>()
            .join(", ");
        return json(
            StatusCode::NOT_FOUND,
            json!({
                "error": format!("there is no profile named '{name}'"),
                "remedy": format!("pick one of: {names}"),
            })
            .to_string(),
        );
    };
    match preview(&profile, &body) {
        Ok(ok) => json(
            StatusCode::OK,
            serde_json::to_string(&ok).unwrap_or_default(),
        ),
        Err(failure) => {
            let status = if failure.kind == "size" {
                StatusCode::PAYLOAD_TOO_LARGE
            } else {
                StatusCode::UNPROCESSABLE_ENTITY
            };
            json(status, serde_json::to_string(&failure).unwrap_or_default())
        }
    }
}

/// JSON that is safe inside a `<script type="application/json">`.
fn script_safe(json: &str) -> String {
    json.replace('<', "\\u003c")
}

async fn page(
    Extension(dash): Extension<chassis::Dashboard>,
    State(profiles): State<Arc<Profiles>>,
) -> Result<Html<String>, chassis::Error> {
    let views = profiles.snapshot();
    let entries: Vec<Value> = views
        .iter()
        .filter_map(|view| {
            let profile = profiles.store().get(&view.name)?;
            let delivers = match view.sink.kind {
                "url" => {
                    "delivers to its destination (the address is a credential and is never shown)"
                        .to_string()
                }
                _ => format!("publishes to the hub ({})", view.sink.label),
            };
            Some(json!({
                "name": view.name,
                "takes": view.source.label,
                "delivers": delivers,
                "content_type": profile.content_type,
                "template": profile.body,
                "example": example_message(&profile.body),
            }))
        })
        .collect();
    let data = script_safe(
        &json!({
            "profiles": entries,
            "routes": { "preview": PREVIEW_ROUTE },
            "maxBytes": MAX_MESSAGE_BYTES,
        })
        .to_string(),
    );
    dash.render_project(
        PAGE_ROUTE,
        include_str!("../web/translate.html"),
        json!({
            "data": data,
            "profiles": entries.iter().map(|e| e["name"].clone()).collect::<Vec<_>>(),
            "mount": WEB_MOUNT,
            "version": crate::dashboard::asset_version(),
        }),
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    const ALERTS: &str = "{\n  \"alert\": {{ alerts.0.labels.alertname | tojson }},\n  \"severity\": {{ alerts.0.labels.severity | default(\"warning\") | tojson }},\n  \"count\": {{ alerts | length }},\n  \"summary\": {{ alerts | map(attribute=\"annotations.summary\") | join(\"; \") | tojson }}\n}";

    #[test]
    fn the_scanner_finds_paths_defaults_and_attributes() {
        let refs = references(ALERTS);
        assert_eq!(refs[0].path, ["alerts", "0", "labels", "alertname"]);
        assert_eq!(refs[0].line, 2);
        assert_eq!(refs[1].default.as_deref(), Some("\"warning\""));
        assert!(refs[2].list);
        assert_eq!(refs[3].attributes, ["annotations.summary"]);
    }

    #[test]
    fn a_loop_variable_is_not_a_field_of_the_message() {
        let refs = references("{% for a in alerts %}{{ a.labels.x }}{% endfor %}");
        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].path, ["alerts"]);
    }

    #[test]
    fn the_example_has_every_field_the_template_reads() {
        let example = example_message(ALERTS);
        let alert = &example["alerts"][0];
        assert!(alert["labels"]["alertname"].is_string());
        assert!(alert["annotations"]["summary"].is_string());
    }

    #[test]
    fn a_missing_field_is_named_in_words() {
        let message = json!({ "alerts": [] });
        let why = explain_missing(ALERTS, 2, &message).expect("an explanation");
        assert!(why.contains("alerts.0.labels.alertname"), "{why}");
        assert!(why.contains("position 0"), "{why}");
    }

    #[test]
    fn a_default_that_was_used_is_a_note() {
        let message = json!({ "alerts": [{ "labels": { "alertname": "x" } }] });
        let notes = defaults_filled(ALERTS, &message);
        assert_eq!(notes.len(), 1);
        assert_eq!(notes[0].what, "No severity in the message");
        assert!(notes[0].why.contains("\"warning\""));
    }

    #[test]
    fn the_engine_line_is_read_from_its_sentence() {
        assert_eq!(line_of("undefined value (in <string>:12)"), Some(12));
        assert_eq!(line_of("nothing here"), None);
    }
}
