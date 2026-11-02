// Where a long path or address may wrap: after a dot, colon or slash, never inside a word or a number. The kit and the page
// styles would otherwise break such text anywhere (`overflow-wrap: anywhere`: "…:81 / 23"). A <wbr> marks each allowed
// break, so the text itself, and what "Copy as text" reads from it, stays unchanged.

/** Positions after a colon or slash, and after a dot unless that dot sits between two digits (1.5 stays whole). */
const AFTER_SEPARATOR = /(?<=[:/])|(?<=\D\.)|(?<=\d\.)(?=\D)/;

/** Put the break marks into one element's text; safe to call again. @param {Element} el */
export function breakAfterSeparators(el) {
    if (el.querySelector(':scope > wbr')) return;
    for (const node of [...el.childNodes]) {
        if (node.nodeType !== Node.TEXT_NODE || !node.textContent) continue;
        const parts = node.textContent.split(AFTER_SEPARATOR);
        if (parts.length < 2) continue;
        const frag = document.createDocumentFragment();
        parts.forEach((part, i) => {
            if (i > 0) frag.append(document.createElement('wbr'));
            frag.append(part);
        });
        node.replaceWith(frag);
    }
}

/** A <code> element for `text` that may wrap after separators. @param {string} text */
export function breakableCode(text) {
    const el = document.createElement('code');
    el.className = 'hsb-breakable';
    el.textContent = text;
    breakAfterSeparators(el);
    return el;
}

/** Keep every `selector` match inside `root` breakable, also after the kit redraws it. @param {Element} root @param {string} selector */
export function keepBreakable(root, selector) {
    const apply = () => root.querySelectorAll(selector).forEach((el) => {
        el.classList.add('hsb-breakable');
        breakAfterSeparators(el);
    });
    apply();
    new MutationObserver(apply).observe(root, { childList: true, subtree: true, characterData: true });
}
