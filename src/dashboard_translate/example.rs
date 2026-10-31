//! The example message offered as the box's starting text: built from the
//! fields the template reads, so it is a guess and the page says so.

use serde_json::{Map, Value};

use super::scanner::references;

/// Place an example value at `path` in `node`, creating objects (named
/// segments) and arrays (numeric segments) on the way. `list` marks a
/// field the template treats as a list, which gets a one-element array.
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

/// An example message built from the fields the template reads.
pub fn example_message(template: &str) -> Value {
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
