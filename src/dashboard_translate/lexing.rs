//! The character-level readers of the template scanner: where the code
//! regions are, and how to read a quoted string or a bracketed call.
//! They know nothing of the message; `scanner` decides what a name means.

/// The regions of a template that are code, as (first line, text).
pub(super) fn regions(template: &str) -> Vec<(usize, &str)> {
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
pub(super) fn read_string(chars: &[char], mut i: usize) -> (String, usize) {
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
pub(super) fn read_call(chars: &[char], mut i: usize) -> (String, usize) {
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

pub(super) fn attribute_in(args: &str) -> Option<String> {
    let at = args.find("attribute")? + "attribute".len();
    let rest = args[at..].trim_start().strip_prefix('=')?.trim_start();
    let chars: Vec<char> = rest.chars().collect();
    matches!(chars.first(), Some('"' | '\'')).then(|| read_string(&chars, 0).0)
}
