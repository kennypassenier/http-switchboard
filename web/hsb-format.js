// Small, pure helpers the status page's modules share: DOM lookups, number and duration wording, and element builders.
// Nothing here reads the page's state, so every view module can import it without an import cycle.

/** @param {string} id */
export const $ = (id) => document.getElementById(id);

/** @param {string} selector @param {ParentNode} [root] */
export const all = (selector, root = document) => /** @type {HTMLElement[]} */ ([...root.querySelectorAll(selector)]);

/** An exact count with thousands separators, never "9+" or "1.5k". @param {number} n */
export const count = (n) => n.toLocaleString('en-GB');

/** "1 profile", "3 profiles". @param {number} n @param {string} one */
export const plural = (n, one) => `${count(n)} ${one}${n === 1 ? '' : 's'}`;

/** @param {string} text */
export const capital = (text) => text.charAt(0).toUpperCase() + text.slice(1);

/** Whole seconds read "5 s", anything finer "1.5 s". @param {number} ms */
export const seconds = (ms) => (ms % 1000 === 0 ? `${ms / 1000} s` : `${(ms / 1000).toFixed(1)} s`);

/** "1 s, 2 s and 4 s" @param {number[]} list */
export const said = (list) =>
    list.length > 1 ? `${list.slice(0, -1).map(seconds).join(', ')} and ${seconds(/** @type {number} */ (list.at(-1)))}` : seconds(list[0] ?? 0);

/** The share delivered, cut and never rounded up. @param {number} v */
export const percent = (v) => `${(Math.floor(v * 1000) / 10).toFixed(1)} %`;

/** @param {string} tag @param {string} [className] @param {string} [text] */
export function el(tag, className = '', text = '') {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text) node.textContent = text;
    return node;
}

/** An identifier set as code. @param {string} text */
export const code = (text) => el('code', 'kp-id', text);
