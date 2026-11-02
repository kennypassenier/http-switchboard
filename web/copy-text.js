// One copy-to-clipboard helper for the pages that copy text: the clipboard API first, then a selected, off-screen field
// and execCommand (a LAN address over plain http has no clipboard API). It answers true or false; the caller says what to
// show when the browser refuses both (test-command.js carries the same approach for the status page).

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
    } catch (error) {
        console.warn('http-switchboard: the legacy copy was refused', error);
    }
    field.remove();
    return copied;
}

/** @param {string} text @returns {Promise<boolean>} whether the text reached the clipboard */
export async function copyText(text) {
    try {
        if (navigator.clipboard?.writeText) {
            await navigator.clipboard.writeText(text);
            return true;
        }
    } catch (error) {
        console.warn('http-switchboard: the clipboard API refused; trying the legacy copy', error);
    }
    return copyWithField(text);
}
