//! What the profile cards need that the config only implies (kp-themes 11,
//! hover card on a profile name).
//!
//! The hover card names a profile's *template* in a short label. The
//! template body itself is never sent to the browser: it can name entity
//! ids, and an operator may paste anything into it. What the label carries
//! is the shape: the content type and the keys of the object the template
//! builds, or, when the body is not an object whose keys can be read, only
//! its size in lines. A key is a name the operator chose for a field of the
//! outgoing message, never a value.

/// The most keys the label lists; the rest are counted.
const MAX_KEYS: usize = 6;

/// A short, secret-free label for a template: `JSON to alert, status and 3 more`.
pub fn template_label(body: &str, content_type: &str) -> String {
    let kind = if content_type.to_ascii_lowercase().contains("json") {
        "JSON"
    } else {
        "Text"
    };
    let keys = top_level_keys(body);
    if keys.is_empty() {
        let lines = body.lines().count().max(1);
        return format!(
            "{kind} template, {lines} line{}",
            if lines == 1 { "" } else { "s" }
        );
    }
    let shown = keys.len().min(MAX_KEYS);
    let rest = keys.len() - shown;
    let list = keys[..shown].join(", ");
    if rest == 0 {
        format!("{kind} template with {list}")
    } else {
        format!("{kind} template with {list} and {rest} more")
    }
}

/// The keys of the outermost object a template writes, in order, without
/// repeats. Template tags (`{{ .. }}`, `{% .. %}`, `{# .. #}`) are skipped
/// whole, wherever they stand. A body that is not an object gives none.
fn top_level_keys(body: &str) -> Vec<String> {
    let chars: Vec<char> = body.chars().collect();
    let mut keys: Vec<String> = Vec::new();
    let mut depth = 0usize;
    let mut i = 0;
    let mut started = false;
    while i < chars.len() {
        let c = chars[i];
        if let Some(end) = tag_end(&chars, i) {
            i = end;
            continue;
        }
        match c {
            '{' | '[' => {
                if !started {
                    if c != '{' {
                        return Vec::new();
                    }
                    started = true;
                }
                depth += 1;
            }
            '}' | ']' => depth = depth.saturating_sub(1),
            '"' => {
                let (text, end) = read_string(&chars, i);
                i = end;
                if depth == 1 && next_is_colon(&chars, i) && !keys.contains(&text) {
                    keys.push(text);
                }
                continue;
            }
            c if c.is_whitespace() => {}
            _ if !started => return Vec::new(),
            _ => {}
        }
        i += 1;
    }
    keys
}

/// Where a template tag that starts at `at` ends (the index after it).
fn tag_end(chars: &[char], at: usize) -> Option<usize> {
    if chars.get(at) != Some(&'{') {
        return None;
    }
    let close = match chars.get(at + 1)? {
        '{' => '}',
        '%' => '%',
        '#' => '#',
        _ => return None,
    };
    let mut j = at + 2;
    while j + 1 < chars.len() {
        if chars[j] == close && chars[j + 1] == '}' {
            return Some(j + 2);
        }
        j += 1;
    }
    Some(chars.len())
}

/// The text of the string that opens at `at`, and the index after its
/// closing quote. A template tag inside the string is kept out of the text.
fn read_string(chars: &[char], at: usize) -> (String, usize) {
    let mut text = String::new();
    let mut j = at + 1;
    while j < chars.len() {
        if let Some(end) = tag_end(chars, j) {
            j = end;
            continue;
        }
        match chars[j] {
            '\\' => {
                if let Some(next) = chars.get(j + 1) {
                    text.push(*next);
                }
                j += 2;
                continue;
            }
            '"' => return (text, j + 1),
            other => text.push(other),
        }
        j += 1;
    }
    (text, chars.len())
}

fn next_is_colon(chars: &[char], from: usize) -> bool {
    chars[from.min(chars.len())..]
        .iter()
        .find(|c| !c.is_whitespace())
        == Some(&':')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn keys_of_the_outer_object_only() {
        let body = r#"{ "alert": "{{ labels.alertname }}", "nested": {"inner": 1}, "status": "{{ status }}" }"#;
        assert_eq!(
            template_label(body, "application/json"),
            "JSON template with alert, nested, status"
        );
    }

    #[test]
    fn a_long_list_is_counted_and_a_value_is_never_a_key() {
        let body = r#"{"a":"x","b":"y","c":"z","d":"1","e":"2","f":"3","g":"4","h":"5"}"#;
        assert_eq!(
            template_label(body, "application/json"),
            "JSON template with a, b, c, d, e, f and 2 more"
        );
    }

    #[test]
    fn a_body_without_readable_keys_is_only_its_size() {
        assert_eq!(
            template_label("hello {{ name }}\nbye", "text/plain"),
            "Text template, 2 lines"
        );
        assert_eq!(
            template_label("[1, 2]", "application/json"),
            "JSON template, 1 line"
        );
    }
}
