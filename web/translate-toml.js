// "Copy as TOML": the draft template as the line the config stores it in, `body = …`, ready to paste over the profile's.
// The config is TOML and a template is usually several lines of JSON with quotes, so the string form is chosen to keep
// the text exactly as written: a literal string (no escapes at all) whenever the text allows one, a basic string with
// escapes only for what a literal string cannot hold.

/** Control characters other than tab and newline: not allowed raw in a TOML string. */
const CONTROL = /[\u0000-\u0008\u000b-\u001f\u007f]/;

/** @param {string} char */
const unicodeEscape = (char) => `\\u${char.charCodeAt(0).toString(16).padStart(4, '0')}`;

/** A basic multi-line string: backslash and quote escaped, the control characters spelled out. @param {string} text */
function basicMultiline(text) {
    const escaped = text
        .replace(/\\/g, '\\\\')
        .replace(/"/g, '\\"')
        .replace(/\r/g, '\\r')
        .replace(/[\u0000-\u0008\u000b\u000c\u000e-\u001f\u007f]/g, unicodeEscape);
    return `body = """\n${escaped}"""`;
}

/**
 * The `body = …` assignment for `template`.
 * @param {string} template
 */
export function tomlBody(template) {
    const plain = !CONTROL.test(template) && !template.includes('\r');
    if (plain && !template.includes('\n') && !template.includes("'")) return `body = '${template}'`;
    // TOML drops the newline right after the opening ''' , so one is always written and the text starts on the next line.
    if (plain && !template.includes("'''")) return `body = '''\n${template}'''`;
    return basicMultiline(template);
}
