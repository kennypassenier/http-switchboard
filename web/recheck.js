// Recheck: the "Recheck profiles" flash button, the per-profile table with its row menus, and the toasts. A recheck puts
// a profile back to "starting" (nothing tried since), the state that means "the next message decides".

import { setMeter } from '/static/kp/js/kpi.js';
import { runFlashButton } from '/static/kp/js/flash.js';
import { toast } from '/static/kp/js/overlays.js';
import { setMenu } from '/static/kp/js/menu-button.js';
import { dataTable } from '/static/kp/js/datatable.js';
import { all, capital, el, plural, seconds } from './hsb-format.js';
import { HEALTH_TONE, HEALTH_WORD, UNREACHABLE, hasHub, hooks, profileNamed, routes, store } from './hsb-state.js';

/** How long the success toast stays, in milliseconds. */
const DONE_TOAST_MS = 6000;

/** The success toast, with its close button; it waits while it is read. @param {string} text */
export const done = (text) => toast(text, { className: 'kp-toast kp-toast--success', ms: DONE_TOAST_MS, close: true });

/** The error toast stays until it is closed and offers to try again. @param {string} text @param {() => void} again */
const failed = (text, again) => {
    const t = toast(text, { className: 'kp-toast kp-toast--error', sticky: true, action: { label: 'Try again', onClick: () => { t.dismiss(); again(); } } });
};

/** The state badge of one row. @param {string} name */
function faceOf(name) {
    const p = profileNamed(name);
    if (store.rechecking.has(name)) return ['Rechecking…', 'kp-badge kp-badge--warning'];
    if (!p || store.status === 'error') return ['Unknown', 'kp-badge'];
    return [capital(HEALTH_WORD[p.state]), `kp-badge kp-badge--${HEALTH_TONE[p.state]}`];
}

/** The row menu of one profile. @param {import('./hsb-state.js').Profile | undefined} p */
function entriesOf(p) {
    const name = p?.name ?? '';
    const copyHint = p && hasHub(p) ? 'A command that renders one sample message without sending it' : 'A curl line that sends one sample message';
    const busy = store.rechecking.has(name) ? 'It is being rechecked now.' : p?.state === 'starting' ? 'It is already starting.' : null;
    return [
        {
            group: name,
            items: [
                { label: 'Recheck this profile', hint: 'Puts it back to starting', value: 'recheck', disabled: busy },
                { label: 'Copy test command', hint: copyHint, value: 'copy' },
                { label: 'Try a translation', hint: 'Renders one sample message here, without sending it', value: 'try' },
            ],
        },
    ];
}

/** The retry budget cell: the meter and the words are two tracks of the cell, the same in every row. @param {HTMLElement | null} meter @param {HTMLElement} words */
function budgetCell(meter, words) {
    const box = el('div', 'hsb-bcell');
    box.append(...(meter ? [meter, words] : [words]));
    return box;
}

/** @param {HTMLElement} cell @param {import('./hsb-state.js').Profile} p */
function drawBudget(cell, p) {
    if (p.lease_ms === null) {
        cell.setAttribute('data-kp-sort-value', '0');
        cell.replaceChildren(budgetCell(null, el('span', 'hsb-budget__words', `No lease (${seconds(p.budget_ms)} worst case)`)));
        return;
    }
    const share = p.budget_ms / p.lease_ms;
    cell.setAttribute('data-kp-sort-value', String(Math.round(share * 100)));
    let meter = /** @type {HTMLElement | null} */ (cell.querySelector('.kp-meter'));
    if (!meter) {
        meter = el('span', 'kp-meter kp-meter--inline kp-meter--over-spill');
        cell.replaceChildren(budgetCell(meter, el('span', 'hsb-budget__words')));
    }
    setMeter(meter, { value: share, tone: share > 1 ? 'destructive' : null, label: 'of the lease' });
    const text = /** @type {HTMLElement} */ (cell.querySelector('.hsb-budget__words'));
    text.textContent = `${Math.round(share * 100)} % of the lease`;
}

/** The recheck table is a data table: it has its own loading, failed and ready faces, like the cards above it. */
let tableShown = '';
function drawTableState() {
    const table = document.querySelector('[data-kp-datatable][data-kp-key="hsb-profiles"]');
    const handle = table instanceof HTMLElement ? dataTable(table) : null;
    if (!handle || tableShown === store.status) return;
    tableShown = store.status;
    // The table's busy card (its own spinner and words) sits over the rows while it loads, also on the first read.
    if (store.status === 'loading') {
        handle.busy({ text: 'Reading the profiles…', overlay: true });
        handle.state('loading');
    } else if (store.status === 'error') handle.fail('The profiles could not be read. Nothing was changed.');
    else handle.state('ready');
}

export function drawRecheck() {
    drawTableState();
    for (const row of all('[data-hsb-row]')) {
        const name = row.getAttribute('data-hsb-row') ?? '';
        const p = profileNamed(name);
        const badge = row.querySelector('[data-hsb-state]');
        if (badge) {
            const [text, cls] = /** @type {[string, string]} */ (faceOf(name));
            badge.textContent = text;
            badge.className = cls;
        }
        const cell = /** @type {HTMLElement | null} */ (row.querySelector('[data-hsb-budget]'));
        if (cell && p) drawBudget(cell, p);
        const wrapper = row.querySelector('[data-kp-menu-button]');
        if (wrapper) setMenu(wrapper, entriesOf(p));
    }
}

/** The kit's refusal shape `{ error, remedy }` as one sentence. @param {any} body */
const refusalLine = (body) => [body.error, body.remedy].filter(Boolean).join('. ') + (body.remedy ? '.' : '');

/**
 * POST to a recheck route and read the answer. A refusal comes back in the kit's `{ error, remedy }` shape.
 * @param {string} url
 * @returns {Promise<{ ok: true, body: any } | { ok: false, line: string }>}
 */
async function post(url) {
    try {
        const res = await fetch(url, { method: 'POST', headers: { accept: 'application/json' } });
        /** @type {any} */
        let body = null;
        try {
            body = await res.json();
        } catch {
            // Not JSON (a proxy's error page, an empty answer): the status below decides what the person reads.
        }
        if (res.ok && body) return { ok: true, body };
        if (body?.error) return { ok: false, line: refusalLine(body) };
        return { ok: false, line: res.ok ? UNREACHABLE : `HTTP ${res.status}. ${UNREACHABLE}` };
    } catch {
        return { ok: false, line: UNREACHABLE }; // The request never got an answer: the service is not reachable.
    }
}

/** "Recheck profiles": every profile back to starting, behind the package's flash button. @param {HTMLButtonElement} button */
export async function recheckAll(button) {
    let n = 0;
    const result = await runFlashButton(button, async () => {
        for (const p of store.profiles) store.rechecking.add(p.name);
        hooks.drawAll();
        const r = await post(routes.recheckAll);
        store.rechecking.clear();
        if (!r.ok) {
            hooks.drawAll();
            return { error: r.line };
        }
        n = Number(r.body.rechecked ?? 0);
        await hooks.refresh();
        return 'Done';
    });
    if (!result) return;
    if (result.ok) done(`${plural(n, 'profile')} back to starting`);
    else failed(result.line, () => void recheckAll(button));
}

/** "Recheck this profile", from a card or a row menu. @param {string} name */
export async function recheckOne(name) {
    store.rechecking.add(name);
    hooks.drawAll();
    const r = await post(routes.recheckOne.replace('{name}', encodeURIComponent(name)));
    store.rechecking.delete(name);
    if (r.ok) {
        await hooks.refresh();
        done(`${name} back to starting`);
    } else {
        hooks.drawAll();
        failed(r.line, () => void recheckOne(name));
    }
}
