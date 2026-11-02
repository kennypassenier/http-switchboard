//! Putting a template's references next to a message: which field is
//! missing (in words), and which defaults the template filled in.

use serde::Serialize;
use serde_json::Value;

use std::collections::HashSet;

use super::scanner::Reference;

/// A default the template filled in.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct Note {
    pub key: String,
    pub what: String,
    pub why: String,
}

/// Follow `path` through `value`; `Err(n)` is the index of the first
/// segment that is not there.
pub(super) fn resolve<'a>(value: &'a Value, path: &[String]) -> Result<&'a Value, usize> {
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
pub(super) fn explain_missing(refs: &[Reference], line: usize, message: &Value) -> Option<String> {
    for r in refs.iter().filter(|r| r.line == line) {
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
pub(super) fn defaults_filled(refs: &[Reference], message: &Value) -> Vec<Note> {
    let mut notes: Vec<Note> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for r in refs {
        let Some(given) = &r.default else { continue };
        if resolve(message, &r.path).is_ok() {
            continue;
        }
        let key = format!("{}@{}", r.path.join("."), r.line);
        if !seen.insert(key.clone()) {
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
