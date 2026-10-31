// Settings and the retry budget: one card per profile on the shared form-grid stage, so Source stands beside Source in
// every card. The retry budget is the worst-case time of all tries plus the pauses between them, shown against the lease
// the hub grants (a delivery that outlives the lease is handed to someone else and arrives twice).

import { setMeter } from '/static/kp/dist/kp-themes.min.js';
import { all, code, el, said, seconds } from './hsb-format.js';
import { store } from './hsb-state.js';
import { markName } from './profiles.js';

const ROWS = ['Source', 'Destination host', 'Content type', 'Timeout', 'Retries', 'Max attempts', 'Lease', 'Retry budget'];

/** The fields that change a card; a poll that changes none of them leaves the card alone. */
const SIGNATURE_KEYS = ['name', 'source', 'sink', 'kind', 'label', 'sub', 'content_type', 'timeout_ms', 'retries', 'pauses_ms', 'budget_ms', 'lease_ms', 'max_attempts'];

/** @typedef {{ card: HTMLElement, sig: string, rows: Record<string, HTMLElement>, meter: HTMLElement, inline: HTMLElement, badge: HTMLElement, words: HTMLElement, sum: HTMLElement }} Card */

/** @type {Map<string, Card>} */
const cards = new Map();

/** The card's head: the name (with its hover card), an inline meter and the budget badge. @param {string} name */
function makeHead(name) {
    const head = el('header', 'hsb-head');
    head.setAttribute('data-kp-formgrid-span', 'all');
    const title = el('h3', 'kp-card__title');
    const label = el('span', '', name);
    markName(label, name);
    title.append(label);
    title.id = `hsb-p-${cards.size}`;
    const inline = el('span', 'kp-meter kp-meter--inline kp-meter--over-spill');
    const badge = el('span', 'kp-badge');
    head.append(title, inline, badge);
    return { head, title, inline, badge };
}

/** @param {string} name @returns {Card} */
function makeCard(name) {
    const card = el('article', 'kp-card kp-formgrid hsb-profile');
    // The card takes its rows from the stage (data-kp-formgrid-stage): Source stands beside Source in every card.
    card.setAttribute('data-kp-formgrid-rows', 'subgrid');
    card.setAttribute('data-kp-formgrid-lines', 'lines');
    const { head, title, inline, badge } = makeHead(name);
    const list = el('dl', 'kp-kv');
    /** @type {Record<string, HTMLElement>} */
    const rows = {};
    for (const label of ROWS) {
        const row = el('div', 'kp-kv__row');
        const dd = el('dd');
        row.append(el('dt', '', label), dd);
        list.append(row);
        rows[label] = dd;
    }
    const budget = /** @type {HTMLElement} */ (rows['Retry budget']);
    budget.className = 'hsb-budget';
    const words = el('span', 'hsb-budget__words');
    const meter = el('span', 'kp-meter kp-meter--over-spill');
    const sum = el('span', 'hsb-quiet hsb-budget__sum');
    budget.append(words, meter, sum);
    card.setAttribute('aria-labelledby', title.id);
    card.append(head, list);
    return { card, sig: '', rows, meter, inline, badge, words, sum };
}

/** @param {HTMLElement} dd @param {...(string | Node)} parts */
const put = (dd, ...parts) => dd.replaceChildren(...parts);

/** The package's skeleton line in every value while the snapshot loads. @param {Card} c */
function showLoading(c) {
    const wait = () => {
        const w = el('span', 'kp-kv__wait');
        w.setAttribute('aria-hidden', 'true');
        w.append(el('span', 'kp-skeleton'));
        return [w, el('span', 'kp-sr-only', 'Loading')];
    };
    for (const dd of Object.values(c.rows)) {
        dd.setAttribute('aria-busy', 'true');
        if (dd.classList.contains('hsb-budget')) {
            for (const part of all('.hsb-budget__words, .hsb-budget__sum', dd)) part.replaceChildren(...wait());
        } else dd.replaceChildren(...wait());
    }
    setMeter(c.meter, { loading: true });
    setMeter(c.inline, { loading: true });
    c.sig = '';
}

/** The plain rows: where messages come from and go, and the delivery settings. @param {Card} c @param {import('./hsb-state.js').Profile} p */
function fillRows(c, p) {
    const r = /** @type {Record<string, HTMLElement>} */ (c.rows);
    const kyu = p.source.kind === 'kyu';
    put(r['Source'], kyu ? 'kyu topic ' : 'HTTP ', code(kyu ? (p.source.path ?? p.source.label) : p.source.label));
    put(
        r['Destination host'],
        ...(p.sink.kind === 'url' ? [code(p.sink.label), ' ', el('span', 'hsb-quiet', '(path hidden)')] : ['kyu topic ', code(p.sink.path ?? p.sink.label)]),
    );
    put(r['Content type'], code(p.content_type));
    put(r['Timeout'], `${seconds(p.timeout_ms)} per try`);
    put(r['Retries'], String(p.retries));
    put(r['Max attempts'], p.max_attempts === null ? 'Not used: an HTTP source has no hub' : `${p.max_attempts} hand-outs by the hub, then a dead letter`);
    put(r['Lease'], p.lease_ms === null ? 'None: an HTTP source holds no lease' : seconds(p.lease_ms));
}

/** The retry budget: the sum in words, and against the lease a meter, the head's inline meter and the badge. @param {Card} c @param {import('./hsb-state.js').Profile} p */
function fillBudget(c, p) {
    const tries = p.timeout_ms * (p.retries + 1);
    const pauses = p.pauses_ms.reduce((a, b) => a + b, 0);
    c.sum.textContent = p.retries
        ? `${p.retries + 1} tries of ${seconds(p.timeout_ms)} is ${seconds(tries)}, plus pauses of ${said(p.pauses_ms)} is ${seconds(pauses)}.`
        : `1 try of ${seconds(p.timeout_ms)} is ${seconds(tries)}, with no pauses.`;
    const lease = p.lease_ms;
    c.meter.hidden = lease === null;
    c.inline.hidden = lease === null;
    c.badge.hidden = lease === null;
    if (lease === null) {
        c.words.textContent = `${seconds(p.budget_ms)} in the worst case, with no lease to fit in`;
        return;
    }
    const share = p.budget_ms / lease;
    const over = p.budget_ms > lease;
    setMeter(c.meter, { value: share, mark: 1, tone: over ? 'destructive' : null, label: 'of the lease', markLabel: 'the lease' });
    setMeter(c.inline, { value: share, tone: over ? 'destructive' : null, label: 'of the lease' });
    const pct = Math.round(share * 100);
    const base = `${seconds(p.budget_ms)} of ${seconds(lease)} lease (${pct} %)`;
    c.words.textContent = over ? `${base}: ${seconds(p.budget_ms - lease)} over` : base;
    c.badge.textContent = over ? 'Over budget' : 'Within the lease';
    c.badge.className = `kp-badge ${over ? 'kp-badge--destructive' : 'kp-badge--success'}`;
}

/** @param {Card} c @param {import('./hsb-state.js').Profile} p */
function fillCard(c, p) {
    if (store.status === 'loading') {
        showLoading(c);
        return;
    }
    for (const dd of Object.values(c.rows)) dd.removeAttribute('aria-busy');
    const sig = JSON.stringify(p, SIGNATURE_KEYS);
    if (sig === c.sig) return;
    c.sig = sig;
    fillRows(c, p);
    fillBudget(c, p);
}

/** One card per profile, added and removed in step with the snapshot. @param {HTMLElement} settingsEl */
export function drawSettings(settingsEl) {
    const { profiles } = store;
    for (const p of profiles) {
        let c = cards.get(p.name);
        if (!c) {
            c = makeCard(p.name);
            cards.set(p.name, c);
            settingsEl.append(c.card);
        }
        fillCard(c, p);
    }
    for (const [name, c] of cards) {
        if (profiles.some((p) => p.name === name)) continue;
        c.card.remove();
        cards.delete(name);
    }
}
