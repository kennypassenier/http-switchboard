// The field table of the "Try a translation" page: each field of the message the template reads, the key of the result
// it lands under and what this message gives it. The rows come from the server (src/dashboard_translate/fields.rs); this
// module only draws them, as the package's plain table with its card layout on a narrow container.

import { els } from './translate-dom.js';

const STATES = {
    read: { label: 'Read', badge: 'kp-badge--success' },
    default: { label: 'Default used', badge: 'kp-badge--info' },
    missing: { label: 'Missing', badge: 'kp-badge--warning' },
};

/** @param {string} label @param {Node[]} content @param {string} [cls] */
function cell(label, content, cls = '') {
    const td = document.createElement('td');
    td.dataset.label = label;
    if (cls) td.className = cls;
    td.append(...content);
    return td;
}

/** @param {string} text */
function code(text) {
    const el = document.createElement('code');
    el.textContent = text;
    return el;
}

/** @param {{ field: string, key: string, value: string, state: keyof typeof STATES }} row */
function rowOf(row) {
    const tr = document.createElement('tr');
    const state = STATES[row.state] ?? STATES.read;
    const key = row.key ? code(row.key) : document.createTextNode('the whole body');
    const value = row.value ? [code(row.value)] : [document.createTextNode('nothing in this message')];
    const badge = document.createElement('span');
    badge.className = `kp-badge ${state.badge}`;
    badge.textContent = state.label;
    // A value is at most 80 characters (fields.rs), so it breaks onto a second line rather than being cut.
    tr.append(cell('Field in the message', [code(row.field)], 'kp-cell-break'), cell('Key in the result', [key], 'kp-cell-break'), cell('Value', value, 'kp-cell-break'), cell('Used as', [badge]));
    return tr;
}

/** "3 read · 1 default used · 1 missing", exact numbers. @param {{ state: string }[]} rows */
function countsOf(rows) {
    const n = (/** @type {string} */ state) => rows.filter((r) => r.state === state).length;
    return `${n('read')} read · ${n('default')} default used · ${n('missing')} missing`;
}

/**
 * Draw the table, or clear it. @param {{ field: string, key: string, value: string, state: keyof typeof STATES }[]} rows
 */
export function drawFields(rows) {
    els.fieldsBody.replaceChildren(...rows.map(rowOf));
    els.fieldsCount.textContent = rows.length ? countsOf(rows) : '';
    els.fieldsWrap.hidden = rows.length === 0;
}
