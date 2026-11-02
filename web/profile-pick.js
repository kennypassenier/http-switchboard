// One profile choice, drawn the same way wherever a page asks "which profile?": a segmented control while the names fit
// (up to MAX_SEGMENTS), a select beyond, so a long list never runs off a phone screen. The host gets the control and
// fires PICK_EVENT with the chosen name; the caller decides what the choice drives. The segmented control and the field
// are the package's (kp-segmented, kp-field); this file only picks between them and keeps the chosen name in step.

import { attachSegmented, setSegmentedValues } from '/static/kp/dist/kp-themes.min.js';

/** The most profiles shown as segments; from one more on the choice is a select. */
const MAX_SEGMENTS = 5;
/** Fired on the host, bubbling: `detail.name` is the profile just chosen. */
export const PICK_EVENT = 'hsb-profile-pick';

const esc = (/** @type {string} */ text) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/** The segmented markup, the chosen name checked. @param {string[]} names @param {string} current @param {string} label */
function segmentsHtml(names, current, label) {
    const items = names
        .map((n) => `<button type="button" class="kp-segmented__item" data-value="${esc(n)}" role="radio" aria-checked="${n === current}"><span class="kp-segmented__label">${esc(n)}</span></button>`)
        .join('');
    return `<div class="kp-segmented" data-kp-segmented="single" role="radiogroup" aria-label="${esc(label)}">${items}</div>`;
}

/** The select markup, the chosen name selected. @param {string[]} names @param {string} current @param {string} label */
function selectHtml(names, current, label) {
    const options = names.map((n) => `<option value="${esc(n)}"${n === current ? ' selected' : ''}>${esc(n)}</option>`).join('');
    return `<select class="kp-field__input" aria-label="${esc(label)}">${options}</select>`;
}

/** @param {HTMLElement} host @param {string} name */
function announce(host, name) {
    host.dispatchEvent(new CustomEvent(PICK_EVENT, { bubbles: true, detail: { name } }));
}

/**
 * Draw the choice into `host`, or only move the mark when the same names are already drawn, so a live refresh keeps the
 * focus. Wired once per host: later calls only repaint.
 * @param {HTMLElement} host @param {string[]} names @param {string} current @param {string} label what the choice is for
 */
export function paintPick(host, names, current, label) {
    const shape = `${names.length > MAX_SEGMENTS ? 'select' : 'seg'}|${names.join('\u0000')}`;
    if (host.dataset.hsbPickShape !== shape) {
        host.dataset.hsbPickShape = shape;
        host.innerHTML = names.length > MAX_SEGMENTS ? selectHtml(names, current, label) : segmentsHtml(names, current, label);
    }
    const select = host.querySelector('select');
    if (select) select.value = current;
    const group = host.querySelector('.kp-segmented');
    if (group) setSegmentedValues(group, [current]);
    if (host.dataset.hsbPickWired) return;
    host.dataset.hsbPickWired = '1';
    attachSegmented(host);
    host.addEventListener('kp-segmented-change', (event) => {
        const value = /** @type {CustomEvent} */ (event).detail?.value;
        if (typeof value === 'string') announce(host, value);
    });
    host.addEventListener('change', (event) => {
        const target = event.target;
        if (target instanceof HTMLSelectElement) announce(host, target.value);
    });
}
