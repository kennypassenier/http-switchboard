//! Escaping for the HTML the sections build by hand.

pub(super) fn esc(text: &str) -> String {
    text.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

/// JSON that is safe inside a `<script type="application/json">`: a `<`
/// can never close the element, whatever a profile is called.
pub(crate) fn script_safe(json: &str) -> String {
    json.replace('<', "\\u003c")
}
