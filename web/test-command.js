// "Copy test command": the one command that exercises a profile, copied to the clipboard. An HTTP source gets a real
// curl to this service's door; a kyu source has no door to post to, so it gets the `test` subcommand, which renders one
// sample message on the host without sending it.

import { toast } from '/static/kp/dist/kp-themes.min.js';
import { hasHub, profileNamed } from './hsb-state.js';

/** Single-quote a value for a POSIX shell; a quote inside it closes, escapes and reopens the quoting. @param {string} value */
function shellQuote(value) {
    return `'${value.replaceAll("'", "'\\''")}'`;
}

/** @param {import('./hsb-state.js').Profile} p */
function testCommand(p) {
    if (hasHub(p)) return `http-switchboard test --config config.toml --profile ${shellQuote(p.name)} --input sample.json`;
    const url = shellQuote(`${location.origin}${p.source.path ?? ''}`);
    return `curl -X POST ${url} -H "Authorization: Bearer $SENDER_TOKEN" -H ${shellQuote(`Content-Type: ${p.content_type}`)} -d @sample.json`;
}

/** The legacy path: a selected, off-screen field and execCommand. @param {string} text */
function copyWithField(text) {
    const field = document.createElement('textarea');
    field.value = text;
    field.setAttribute('readonly', '');
    field.style.position = 'fixed';
    field.style.insetInlineStart = '-9999px';
    document.body.append(field);
    field.select();
    let copied = false;
    try {
        copied = field.selectionEnd - field.selectionStart === text.length && document.execCommand('copy');
    } catch {
        copied = false; // The browser refused the legacy copy; the caller shows the command instead.
    }
    field.remove();
    return copied;
}

/** The clipboard API needs a secure context; a LAN address over plain http falls back to a selected field. @param {string} text */
async function copyText(text) {
    try {
        if (navigator.clipboard?.writeText) {
            await navigator.clipboard.writeText(text);
            return true;
        }
    } catch {
        // Refused (not a secure context, or no permission): fall through to the legacy path.
    }
    return copyWithField(text);
}

/** @param {string} name @param {(text: string) => void} done the success toast */
export async function copyTest(name, done) {
    const p = profileNamed(name);
    if (!p) return;
    const command = testCommand(p);
    if (await copyText(command)) done('Test command copied');
    else toast(`Could not copy. Select it here: ${command}`, { className: 'kp-toast kp-toast--error', sticky: true });
}
