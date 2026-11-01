//! Which output key a spot in the template sits under.
//!
//! A JSON template is the document with `{{ … }}` holes in it, so the key
//! a hole lands under can be read from the text around it: walk the text,
//! keep a stack of the objects and arrays that are open, remember the
//! latest `"key":` of each object, and at a hole the stack is the path
//! (`labels.name`, `items.0`). It is a lexical reading, not an evaluation:
//! a hole inside a `{% for %}` is placed where the loop is written, and a
//! hole outside any key (a bare `{{ x }}` as the whole body) has the
//! empty path. The engine, not this walk, decides what renders.

use std::collections::HashMap;

/// One open container of the document being walked.
enum Frame {
    /// An object and the key most recently written in it.
    Object(Option<String>),
    /// An array and the position of the element being written.
    Array(usize),
}

/// What the walker is in the middle of.
enum Mode {
    Plain,
    /// Inside a string literal; the text so far and whether the last
    /// character was a backslash.
    Text(String, bool),
}

struct Walker {
    stack: Vec<Frame>,
    mode: Mode,
}

impl Walker {
    fn path(&self) -> String {
        let parts: Vec<String> = self
            .stack
            .iter()
            .filter_map(|frame| match frame {
                Frame::Object(key) => key.clone(),
                Frame::Array(at) => Some(at.to_string()),
            })
            .collect();
        parts.join(".")
    }

    /// One character of the fixed text. `colon_follows` says whether the
    /// next non-space character is `:`, which makes a string a key.
    fn step(&mut self, c: char, colon_follows: impl Fn() -> bool) {
        if let Mode::Text(text, escaped) = &mut self.mode {
            if *escaped {
                text.push(c);
                *escaped = false;
            } else if c == '\\' {
                *escaped = true;
            } else if c == '"' {
                let key = std::mem::take(text);
                self.mode = Mode::Plain;
                if let (Some(Frame::Object(slot)), true) = (self.stack.last_mut(), colon_follows())
                {
                    *slot = Some(key);
                }
            } else {
                text.push(c);
            }
            return;
        }
        match c {
            '"' => self.mode = Mode::Text(String::new(), false),
            '{' => self.stack.push(Frame::Object(None)),
            '[' => self.stack.push(Frame::Array(0)),
            '}' | ']' => {
                self.stack.pop();
            }
            ',' => match self.stack.last_mut() {
                Some(Frame::Array(at)) => *at += 1,
                Some(Frame::Object(key)) => *key = None,
                None => {}
            },
            _ => {}
        }
    }
}

/// The closing pair that belongs to an opening one.
fn closer(open: [u8; 2]) -> Option<&'static str> {
    match &open {
        b"{{" => Some("}}"),
        b"{%" => Some("%}"),
        b"{#" => Some("#}"),
        _ => None,
    }
}

/// For the start offset of every `{{ }}` / `{% %}` region, the key path
/// the region sits under. Offsets are the ones [`super::lexing::regions`]
/// reports.
pub(super) fn paths_at(template: &str) -> HashMap<usize, String> {
    let bytes = template.as_bytes();
    let mut walker = Walker {
        stack: Vec::new(),
        mode: Mode::Plain,
    };
    let mut found = HashMap::new();
    let mut at = 0;
    while at < template.len() {
        if let Some(close) = bytes.get(at..at + 2).and_then(|p| closer([p[0], p[1]])) {
            let start = at + 2;
            if bytes[at + 1] != b'#' {
                found.insert(start, walker.path());
            }
            at = template[start..]
                .find(close)
                .map_or(template.len(), |n| start + n + close.len());
            continue;
        }
        let Some(c) = template[at..].chars().next() else {
            break;
        };
        let rest = &template[at + c.len_utf8()..];
        walker.step(c, || rest.trim_start().starts_with(':'));
        at += c.len_utf8();
    }
    found
}
