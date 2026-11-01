//! The field table: for each field of the message a template reads, the
//! key of the result it lands under and the value this message gives it.
//!
//! This is what the line diff of a reshaped document could not say. The
//! rows come from the scanner (what the template reads) and the key
//! walker (where in the document each read sits), both lexical and
//! forgiving: a read the scanner misses is a row that is missing, never a
//! wrong render, because the engine alone produces the result.

use serde::Serialize;
use serde_json::Value;

use super::explain::resolve;
use super::keys::paths_at;
use super::scanner::references;

/// The longest a value is shown before it is cut with an ellipsis.
const SHOWN_CHARS: usize = 80;

/// One row of the table.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct FieldRow {
    /// The field of the message, as the template writes it.
    pub field: String,
    /// The key of the result it sits under; empty when the template reads
    /// it outside any key (the whole body).
    pub key: String,
    /// The value in this message, short; empty when there is none.
    pub value: String,
    /// `read` (the message has it), `default` (the template's default is
    /// used) or `missing` (the message lacks it and no default covers it).
    pub state: &'static str,
}

/// A value in a few words: scalars as JSON, containers by size.
fn shown(value: &Value) -> String {
    let text = match value {
        Value::Array(items) => return format!("list of {}", items.len()),
        Value::Object(map) => return format!("object with {} fields", map.len()),
        other => other.to_string(),
    };
    if text.chars().count() <= SHOWN_CHARS {
        return text;
    }
    let cut: String = text.chars().take(SHOWN_CHARS).collect();
    format!("{cut}…")
}

/// The table for `template` read against `message`.
pub fn field_table(template: &str, message: &Value) -> Vec<FieldRow> {
    let paths = paths_at(template);
    let mut rows: Vec<FieldRow> = Vec::new();
    for r in references(template) {
        let field = r.path.join(".");
        let key = paths.get(&r.at).cloned().unwrap_or_default();
        if rows.iter().any(|row| row.field == field && row.key == key) {
            continue;
        }
        let (value, state) = match (resolve(message, &r.path), &r.default) {
            (Ok(found), _) => (shown(found), "read"),
            (Err(_), Some(given)) => (given.clone(), "default"),
            (Err(_), None) => (String::new(), "missing"),
        };
        rows.push(FieldRow {
            field,
            key,
            value,
            state,
        });
    }
    rows
}
