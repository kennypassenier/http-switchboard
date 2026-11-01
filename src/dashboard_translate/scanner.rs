//! The template scanner: finds every reference to the message in a
//! template (`alerts.0.labels.severity`), with the line it is on and how
//! it is used. It is deliberately forgiving and never decides whether a
//! message renders (the engine does that); it only explains, so a missed
//! reference degrades an explanation to the engine's own sentence.

use super::lexing::{attribute_in, read_call, read_string, regions};
use super::words::{KEYWORDS, LIST_FILTERS};

/// One reference to the message that a template makes.
#[derive(Debug, Clone, PartialEq, Eq)]
pub(super) struct Reference {
    pub(super) path: Vec<String>,
    pub(super) line: usize,
    /// Byte offset in the template of the region the reference is in.
    pub(super) at: usize,
    /// Used as a list: counted, joined, mapped, iterated.
    pub(super) list: bool,
    /// The text inside `default(...)`, as written.
    pub(super) default: Option<String>,
    /// `attribute="a.b"` given to a filter that follows.
    pub(super) attributes: Vec<String>,
}

/// Where the scan of one code region stands. A region is the text between
/// one `{{ }}` or `{% %}` pair; the flags carry what the previous token
/// said about the next one.
struct Cursor {
    i: usize,
    line: usize,
    /// The previous token was `|`: the next name is a filter.
    after_pipe: bool,
    /// Inside the names a `for` or `set` binds (locals, not message fields).
    binding: bool,
    /// The previous token was `is`: the next name is a test, not a field.
    after_is: bool,
    /// Index in `found` of the latest reference, which a filter qualifies.
    last: Option<usize>,
}

impl Cursor {
    /// Keywords that open or close a run of bound names: `for a in`,
    /// `set a =`; and `is`, which makes the next name a test.
    fn track_keyword(&mut self, root: &str) {
        match root {
            "for" | "set" => self.binding = true,
            "in" | "=" => self.binding = false,
            "is" => self.after_is = true,
            _ => {}
        }
    }
}

/// The scan across all regions: the references found and the local names
/// (loop variables, `set` targets) that must not count as message fields.
struct Scan {
    found: Vec<Reference>,
    locals: Vec<String>,
    /// Where the region being scanned starts in the template.
    at: usize,
}

/// Every reference to the message in `template`.
pub(super) fn references(template: &str) -> Vec<Reference> {
    let mut scan = Scan {
        found: Vec::new(),
        locals: Vec::new(),
        at: 0,
    };
    for region in regions(template) {
        let chars: Vec<char> = region.text.chars().collect();
        scan.at = region.start;
        scan.region(&chars, region.line);
    }
    scan.found
}

fn is_name_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

/// Read a dotted name (`a.b.0.c`) starting at `chars[*i]`; leaves `*i`
/// after it.
fn read_path(chars: &[char], i: &mut usize) -> Vec<String> {
    let mut path = Vec::new();
    loop {
        let from = *i;
        while *i < chars.len() && is_name_char(chars[*i]) {
            *i += 1;
        }
        path.push(chars[from..*i].iter().collect::<String>());
        let more = *i + 1 < chars.len() && chars[*i] == '.' && is_name_char(chars[*i + 1]);
        if !more {
            return path;
        }
        *i += 1;
    }
}

impl Scan {
    fn region(&mut self, chars: &[char], first_line: usize) {
        let mut cur = Cursor {
            i: 0,
            line: first_line,
            after_pipe: false,
            binding: false,
            after_is: false,
            last: None,
        };
        while cur.i < chars.len() {
            let c = chars[cur.i];
            if c == '\n' {
                cur.line += 1;
                cur.i += 1;
            } else if c == '"' || c == '\'' {
                cur.i = read_string(chars, cur.i).1;
                cur.after_pipe = false;
            } else if c == '|' {
                cur.after_pipe = true;
                cur.i += 1;
            } else if c.is_ascii_alphabetic() || c == '_' {
                self.name(chars, &mut cur);
            } else {
                if c == '=' {
                    cur.binding = false;
                }
                if !c.is_whitespace() {
                    cur.after_pipe = false;
                }
                cur.i += 1;
            }
        }
    }

    /// One name at the cursor: a filter, a call, a keyword, a binding or a
    /// reference to the message.
    fn name(&mut self, chars: &[char], cur: &mut Cursor) {
        let path = read_path(chars, &mut cur.i);
        let mut peek = cur.i;
        while peek < chars.len() && chars[peek].is_whitespace() {
            peek += 1;
        }
        let called = chars.get(peek) == Some(&'(');
        let root = path[0].clone();
        if cur.after_pipe {
            self.filter(chars, cur, &path, called.then_some(peek));
            return;
        }
        if called {
            if root == "default" || !KEYWORDS.contains(&root.as_str()) {
                cur.i = read_call(chars, peek).1;
            }
            return;
        }
        if cur.after_is {
            cur.after_is = false;
            return;
        }
        cur.track_keyword(&root);
        if KEYWORDS.contains(&root.as_str()) {
            return;
        }
        if cur.binding {
            self.locals.push(root);
        } else if !self.locals.contains(&root) {
            self.found.push(Reference {
                path,
                line: cur.line,
                at: self.at,
                list: false,
                default: None,
                attributes: Vec::new(),
            });
            cur.last = Some(self.found.len() - 1);
        }
    }

    /// A filter says how the reference before it is used: as a list, with
    /// a default, or by an attribute.
    fn filter(
        &mut self,
        chars: &[char],
        cur: &mut Cursor,
        path: &[String],
        call_at: Option<usize>,
    ) {
        let name = path.join(".");
        let args = match call_at {
            Some(at) => {
                let (args, next) = read_call(chars, at);
                cur.i = next;
                args
            }
            None => String::new(),
        };
        if let Some(r) = cur.last.and_then(|n| self.found.get_mut(n)) {
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
        cur.after_pipe = false;
    }
}
