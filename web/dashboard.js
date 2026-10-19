// The status page's own views (kp-themes 11): profile cards or a map, the path of one profile, its settings and retry
// budget, and the recheck. The components are the package's, served by the kit at /static/kp/js/; this module feeds them
// the snapshot the server embeds (#hsb-data) and refreshes from /profiles.json. The snapshot carries no header value, no
// payload and no webhook path, so nothing drawn here can leak one.
//
// Freshness is a light poll (every 10 s while the tab is visible, and straight after a recheck); see src/dashboard.rs.

import { setEntityGrid, setEntityGridState, attachEntityCards } from '/static/kp/js/entity-card.js';
import { setMatrix, attachMatrices } from '/static/kp/js/heat.js';
import { setTrace, trace, attachTraces } from '/static/kp/js/path-trace.js';
import { setMeter } from '/static/kp/js/kpi.js';
import { runFlashButton, attachFlashButtons } from '/static/kp/js/flash.js';
import { toast } from '/static/kp/js/overlays.js';
import { setMenu, attachMenuButtons } from '/static/kp/js/menu-button.js';
import { attachDataTables } from '/static/kp/js/datatable.js';
import { attachFootprints } from '/static/kp/js/footprint.js';

const POLL_MS = 10_000;
const VIEW_KEY = 'hsb-profiles-view';
const UNREACHABLE = 'Could not reach HTTPSwitchboard. Check that the service is running, then try again.';

const HEALTH_WORD = { working: 'working', starting: 'starting', failing: 'failing', denied: 'denied', 'hub-down': 'hub down' };
const HEALTH_TONE = { working: 'success', starting: 'info', failing: 'destructive', denied: 'destructive', 'hub-down': 'warning' };

const $ = (/** @type {string} */ id) => document.getElementById(id);
const all = (/** @type {string} */ selector, root = document) => /** @type {HTMLElement[]} */ ([...root.querySelectorAll(selector)]);
const count = (/** @type {number} */ n) => n.toLocaleString('en-GB');
const plural = (/** @type {number} */ n, /** @type {string} */ one) => `${count(n)} ${one}${n === 1 ? '' : 's'}`;
const capital = (/** @type {string} */ text) => text.charAt(0).toUpperCase() + text.slice(1);

/** Whole seconds read "5 s", anything finer "1.5 s". @param {number} ms */
const seconds = (ms) => (ms % 1000 === 0 ? `${ms / 1000} s` : `${(ms / 1000).toFixed(1)} s`);
/** "1 s, 2 s and 4 s" @param {number[]} list */
const said = (list) => (list.length > 1 ? `${list.slice(0, -1).map(seconds).join(', ')} and ${seconds(/** @type {number} */ (list.at(-1)))}` : seconds(list[0] ?? 0));
/** The share delivered, cut and never rounded up. @param {number} v */
const percent = (v) => `${(Math.floor(v * 1000) / 10).toFixed(1)} %`;

/** @typedef {{ kind: string, label: string, sub: string, path: string | null }} End */
/** @typedef {{ name: string, state: keyof typeof HEALTH_WORD, received: number, delivered: number, failed: number, avg_ms: number | null, last_success_unix: number | null,
 *   source: End, sink: End, content_type: string, timeout_ms: number, retries: number, pauses_ms: number[], budget_ms: number, lease_ms: number | null,
 *   max_attempts: number | null }} Profile */

const embedded = JSON.parse($('hsb-data')?.textContent ?? '{}');
const routes = embedded.routes ?? { snapshot: '/profiles.json', recheckAll: '/profiles/recheck', recheckOne: '/profiles/{name}/recheck' };

/** @type {Profile[]} */
let profiles = embedded.profiles ?? [];
/** @type {'ready' | 'loading' | 'error'} */
let status = 'ready';
let view = 'cards';
try {
    view = localStorage.getItem(VIEW_KEY) === 'map' ? 'map' : 'cards';
} catch {
    // No storage: the default view stands.
}
let traceName = profiles[0]?.name ?? '';
/** @type {Set<string>} profiles with a recheck in flight */
const rechecking = new Set();

const grid = /** @type {HTMLElement} */ ($('hsb-grid'));
const map = /** @type {HTMLElement} */ ($('hsb-map'));
const traceEl = /** @type {HTMLElement} */ ($('hsb-trace'));
const settingsEl = /** @type {HTMLElement} */ ($('hsb-settings'));
const allButton = /** @type {HTMLButtonElement} */ ($('hsb-recheck-all'));

const hasHub = (/** @type {Profile} */ p) => p.source.kind === 'kyu';

// ── the profiles view ──────────────────────────────────────────────────────────────────────────────────────────────

/** @param {Profile} p */
function entityOf(p) {
    const settled = p.delivered + p.failed;
    const share = settled === 0 ? null : p.delivered / settled;
    /** @type {{ label: string, tone?: 'success' | 'warning' | 'destructive' | 'info' }[]} */
    const flags = [{ label: p.source.kind === 'kyu' ? 'kyu topic' : 'HTTP POST' }, { label: p.sink.kind === 'url' ? 'HTTP webhook' : 'kyu topic' }];
    if (p.state === 'failing') flags.push({ label: hasHub(p) ? 'messages waiting on the hub' : 'destination refusing', tone: 'destructive' });
    if (p.state === 'denied') flags.push({ label: 'credentials refused', tone: 'destructive' });
    if (p.state === 'hub-down') flags.push({ label: 'hub unreachable', tone: 'warning' });
    return {
        key: p.name,
        name: p.name,
        state: { label: HEALTH_WORD[p.state], tone: /** @type {'success' | 'warning' | 'destructive' | 'info'} */ (HEALTH_TONE[p.state]) },
        figures: [
            { value: count(p.delivered), label: 'Delivered' },
            { value: count(p.failed), label: 'Failed' },
            { value: p.avg_ms === null ? 'none yet' : `${count(p.avg_ms)} ms`, label: 'Average' },
        ],
        flags,
        meters: [
            {
                label: 'Delivered',
                value: share,
                ...(share !== null && share < 0.99 ? { tone: /** @type {'warning'} */ ('warning') } : {}),
                text: share === null ? 'Nothing tried yet' : `${percent(share)} delivered`,
            },
        ],
        actions: [
            { id: 'recheck', label: 'Recheck this profile' },
            { id: 'copy-test', label: 'Copy test command' },
            { id: 'try', label: 'Try a translation' },
        ],
    };
}

function mapSpec() {
    const toneOf = (/** @type {Profile} */ p, /** @type {'source' | 'dest'} */ side) => {
        if (p.state === 'starting') return 'muted';
        if (side === 'source') return p.state === 'denied' || p.state === 'hub-down' ? 'bad' : 'ok';
        return p.state === 'failing' ? 'bad' : p.state === 'denied' || p.state === 'hub-down' ? 'muted' : 'ok';
    };
    return {
        corner: 'Source to destination',
        rows: profiles.map((p) => ({
            key: `src-${p.name}`,
            label: p.source.label,
            sub: `profile ${p.name}, ${p.source.sub}`,
            tone: toneOf(p, 'source'),
            dashed: p.state === 'starting',
        })),
        columns: profiles.map((p) => ({ key: `dst-${p.name}`, label: p.sink.label, sub: `for ${p.name}`, tone: toneOf(p, 'dest') })),
        cell: (/** @type {string} */ row, /** @type {string} */ column) =>
            row.slice(4) === column.slice(4) ? { tone: 'ok', label: 'delivers' } : { tone: 'none' },
        countLabel: (/** @type {number} */ n) => plural(n, 'profile'),
        sourcesLabel: 'Sources',
        usersLabel: 'Destinations',
    };
}

function drawProfiles() {
    grid.hidden = view !== 'cards';
    map.hidden = view !== 'map';
    for (const b of all('[data-hsb-view]')) b.setAttribute('aria-pressed', String(b.getAttribute('data-hsb-view') === view));
    if (status === 'loading') {
        setEntityGridState(grid, 'loading', { count: Math.max(profiles.length, 2) });
        setMatrix(map, { ...mapSpec(), view: 'lines', state: 'loading', skeleton: Math.max(profiles.length, 2) });
    } else if (status === 'error') {
        setEntityGridState(grid, 'error', { title: 'The profiles could not be read', message: 'HTTPSwitchboard did not answer. Check that the service is running.' });
        setMatrix(map, { view: 'lines', rows: [], columns: [], state: 'error', message: 'The profiles could not be read' });
    } else if (profiles.length === 0) {
        setEntityGridState(grid, 'empty', {
            title: 'No profiles yet',
            message: 'Profiles are read from the configuration on disk. Add one and reload.',
            actionLabel: 'Reload profiles',
        });
        setMatrix(map, { view: 'lines', rows: [], columns: [], state: 'empty', message: 'No profiles yet' });
    } else {
        setEntityGrid(grid, profiles.map(entityOf));
        setMatrix(map, { ...mapSpec(), view: 'lines' });
    }
}

// ── the path of one profile ────────────────────────────────────────────────────────────────────────────────────────

/** @param {Profile} p */
function hopsOf(p) {
    const st = p.state;
    /** @type {'allowed' | 'unknown'} */
    const base = st === 'starting' ? 'unknown' : 'allowed';
    const door = st === 'denied' || st === 'hub-down' ? 'refused' : base;
    const last = st === 'failing' ? 'refused' : base;
    return [
        { title: 'Sender', note: hasHub(p) ? 'publishes to the hub' : 'holds a sender token', verdict: base },
        {
            title: p.source.label,
            note: st === 'denied' ? 'the hub refused our credentials' : st === 'hub-down' ? 'the hub cannot be reached' : p.source.sub,
            verdict: door,
            ...(door === 'refused' ? { rule: st === 'denied' ? 'Credentials refused' : 'Hub unreachable' } : {}),
        },
        { title: 'Translation', note: `${p.content_type} from the profile's template`, verdict: base },
        {
            title: p.sink.label,
            note: p.sink.kind === 'url' ? 'destination' : 'published to the hub',
            verdict: last,
            ...(last === 'refused' ? { rule: 'Receiver failing' } : {}),
        },
    ];
}

function drawTrace() {
    const pick = $('hsb-trace-pick');
    if (pick) {
        pick.hidden = profiles.length < 2;
        if (!pick.querySelector('b')) {
            const label = document.createElement('b');
            label.textContent = 'Profile';
            pick.append(label);
        }
        for (const old of all('button', pick)) if (!profiles.some((p) => p.name === old.dataset.hsbTrace)) old.remove();
        for (const p of profiles) {
            if (all('button', pick).some((b) => b.dataset.hsbTrace === p.name)) continue;
            const b = document.createElement('button');
            b.type = 'button';
            b.className = 'kp-button kp-button--sm';
            b.dataset.hsbTrace = p.name;
            b.textContent = p.name;
            pick.append(b);
        }
        for (const b of all('button', pick)) b.setAttribute('aria-pressed', String(b.dataset.hsbTrace === traceName));
    }
    const p = profiles.find((x) => x.name === traceName) ?? profiles[0];
    if (p) traceName = p.name;
    const name = $('hsb-trace-name');
    const badge = $('hsb-trace-badge');
    if (name) name.textContent = p?.name ?? '';
    if (badge) {
        badge.textContent = p ? HEALTH_WORD[p.state] : '';
        badge.className = p ? `kp-badge kp-badge--${HEALTH_TONE[p.state]}` : 'kp-badge';
    }
    if (status === 'ready' && p) {
        setTrace(traceEl, hopsOf(p));
        return;
    }
    if (!trace(traceEl)) setTrace(traceEl, p ? hopsOf(p) : []);
    const state = status === 'loading' ? 'running' : status === 'error' ? 'error' : 'empty';
    trace(traceEl)?.setState(state, { message: status === 'error' ? 'The profile could not be read.' : 'No profiles yet.' });
}

// ── settings and the retry budget ──────────────────────────────────────────────────────────────────────────────────

/** @type {Map<string, { card: HTMLElement, sig: string, rows: Record<string, HTMLElement>, meter: HTMLElement, inline: HTMLElement, badge: HTMLElement, words: HTMLElement, sum: HTMLElement }>} */
const cards = new Map();

/** @param {string} tag @param {string} [className] @param {string} [text] */
function el(tag, className = '', text = '') {
    const node = document.createElement(tag);
    if (className) node.className = className;
    if (text) node.textContent = text;
    return node;
}
/** @param {string} text */
const code = (text) => el('code', 'kp-id', text);

/** @param {string} name */
function makeCard(name) {
    const card = el('article', 'kp-card hsb-profile');
    const head = el('header', 'hsb-profile__head');
    const title = el('h3', 'kp-card__title', name);
    const inline = el('span', 'kp-meter kp-meter--inline kp-meter--over-spill');
    const badge = el('span', 'kp-badge');
    head.append(title, inline, badge);
    const list = el('dl', 'kp-kv kp-kv--lines');
    /** @type {Record<string, HTMLElement>} */
    const rows = {};
    for (const label of ['Source', 'Destination host', 'Content type', 'Timeout', 'Retries', 'Max attempts', 'Lease', 'Retry budget']) {
        const row = el('div', 'kp-kv__row');
        const dt = el('dt', '', label);
        const dd = el('dd');
        row.append(dt, dd);
        list.append(row);
        rows[label] = dd;
    }
    const budget = /** @type {HTMLElement} */ (rows['Retry budget']);
    budget.className = 'hsb-budget';
    const words = el('span', 'hsb-budget__words');
    const meter = el('span', 'kp-meter kp-meter--over-spill');
    const sum = el('span', 'hsb-quiet hsb-budget__sum');
    budget.append(words, meter, sum);
    const titleId = `hsb-p-${cards.size}`;
    title.id = titleId;
    card.setAttribute('aria-labelledby', titleId);
    card.append(head, list);
    return { card, sig: '', rows, meter, inline, badge, words, sum };
}

/** @param {HTMLElement} dd @param {...(string | Node)} parts */
const put = (dd, ...parts) => dd.replaceChildren(...parts);

/** @param {ReturnType<typeof makeCard>} c @param {Profile} p */
function fillCard(c, p) {
    const r = c.rows;
    const wait = () => {
        const w = el('span', 'kp-kv__wait');
        w.setAttribute('aria-hidden', 'true');
        w.append(el('span', 'kp-skeleton'));
        return [w, el('span', 'kp-sr-only', 'Loading')];
    };
    const loading = status === 'loading';
    for (const dd of Object.values(r)) {
        if (loading) {
            dd.setAttribute('aria-busy', 'true');
            if (dd.classList.contains('hsb-budget')) {
                for (const part of all('.hsb-budget__words, .hsb-budget__sum', dd)) part.replaceChildren(...wait());
            } else dd.replaceChildren(...wait());
        } else dd.removeAttribute('aria-busy');
    }
    if (loading) {
        setMeter(c.meter, { loading: true });
        setMeter(c.inline, { loading: true });
        c.sig = '';
        return;
    }
    const sig = JSON.stringify(p, ['name', 'source', 'sink', 'kind', 'label', 'sub', 'content_type', 'timeout_ms', 'retries', 'pauses_ms', 'budget_ms', 'lease_ms', 'max_attempts']);
    if (sig === c.sig) return;
    c.sig = sig;
    const kyu = p.source.kind === 'kyu';
    put(/** @type {HTMLElement} */ (r['Source']), kyu ? 'kyu topic ' : 'HTTP ', code(kyu ? (p.source.path ?? p.source.label) : p.source.label));
    put(
        /** @type {HTMLElement} */ (r['Destination host']),
        ...(p.sink.kind === 'url' ? [code(p.sink.label), ' ', el('span', 'hsb-quiet', '(path hidden)')] : ['kyu topic ', code(p.sink.path ?? p.sink.label)]),
    );
    put(/** @type {HTMLElement} */ (r['Content type']), code(p.content_type));
    put(/** @type {HTMLElement} */ (r['Timeout']), `${seconds(p.timeout_ms)} per try`);
    put(/** @type {HTMLElement} */ (r['Retries']), String(p.retries));
    put(
        /** @type {HTMLElement} */ (r['Max attempts']),
        p.max_attempts === null ? 'Not used: an HTTP source has no hub' : `${p.max_attempts} hand-outs by the hub, then a dead letter`,
    );
    put(/** @type {HTMLElement} */ (r['Lease']), p.lease_ms === null ? 'None: an HTTP source holds no lease' : seconds(p.lease_ms));
    const tries = p.timeout_ms * (p.retries + 1);
    const pauses = p.pauses_ms.reduce((a, b) => a + b, 0);
    c.sum.textContent = p.retries
        ? `${p.retries + 1} tries of ${seconds(p.timeout_ms)} is ${seconds(tries)}, plus pauses of ${said(p.pauses_ms)} is ${seconds(pauses)}.`
        : `1 try of ${seconds(p.timeout_ms)} is ${seconds(tries)}, with no pauses.`;
    const hasLease = p.lease_ms !== null;
    c.meter.hidden = !hasLease;
    c.inline.hidden = !hasLease;
    c.badge.hidden = !hasLease;
    if (!hasLease || p.lease_ms === null) {
        c.words.textContent = `${seconds(p.budget_ms)} in the worst case, with no lease to fit in`;
        return;
    }
    const share = p.budget_ms / p.lease_ms;
    const over = p.budget_ms > p.lease_ms;
    setMeter(c.meter, { value: share, mark: 1, tone: over ? 'destructive' : null, label: 'of the lease', markLabel: 'the lease' });
    setMeter(c.inline, { value: share, tone: over ? 'destructive' : null, label: 'of the lease' });
    const pct = Math.round(share * 100);
    c.words.textContent = over
        ? `${seconds(p.budget_ms)} of ${seconds(p.lease_ms)} lease (${pct} %): ${seconds(p.budget_ms - p.lease_ms)} over`
        : `${seconds(p.budget_ms)} of ${seconds(p.lease_ms)} lease (${pct} %)`;
    c.badge.textContent = over ? 'Over budget' : 'Within the lease';
    c.badge.className = `kp-badge ${over ? 'kp-badge--destructive' : 'kp-badge--success'}`;
}

function drawSettings() {
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

// ── recheck: the button, the table and the toasts ──────────────────────────────────────────────────────────────────

/** @param {string} name */
const faceOf = (name) => {
    const p = profiles.find((x) => x.name === name);
    if (rechecking.has(name)) return ['Rechecking…', 'kp-badge kp-badge--warning'];
    if (!p || status === 'error') return ['Unknown', 'kp-badge'];
    return [capital(HEALTH_WORD[p.state]), `kp-badge kp-badge--${HEALTH_TONE[p.state]}`];
};

/** @param {Profile | undefined} p */
const entriesOf = (p) => {
    const name = p?.name ?? '';
    const copyHint = p && hasHub(p) ? 'A command that renders one sample message without sending it' : 'A curl line that sends one sample message';
    return [
        {
            group: name,
            items: [
                {
                    label: 'Recheck this profile',
                    hint: 'Puts it back to starting',
                    value: 'recheck',
                    disabled: rechecking.has(name) ? 'It is being rechecked now.' : p?.state === 'starting' ? 'It is already starting.' : null,
                },
                { label: 'Copy test command', hint: copyHint, value: 'copy' },
                { label: 'Try a translation', hint: 'Renders one sample message here, without sending it', value: 'try' },
            ],
        },
    ];
};

function drawRecheck() {
    for (const row of all('[data-hsb-row]')) {
        const name = row.getAttribute('data-hsb-row') ?? '';
        const p = profiles.find((x) => x.name === name);
        const badge = row.querySelector('[data-hsb-state]');
        if (badge) {
            const [text, cls] = /** @type {[string, string]} */ (faceOf(name));
            badge.textContent = text;
            badge.className = cls;
        }
        const cell = /** @type {HTMLElement | null} */ (row.querySelector('[data-hsb-budget]'));
        if (cell && p) {
            if (p.lease_ms === null) {
                cell.setAttribute('data-kp-sort-value', '0');
                cell.replaceChildren(`No lease (${seconds(p.budget_ms)} worst case)`);
            } else {
                const share = p.budget_ms / p.lease_ms;
                cell.setAttribute('data-kp-sort-value', String(Math.round(share * 100)));
                let meter = /** @type {HTMLElement | null} */ (cell.querySelector('.kp-meter'));
                if (!meter) {
                    meter = el('span', 'kp-meter kp-meter--inline kp-meter--over-spill');
                    cell.replaceChildren(meter, el('span'));
                }
                setMeter(meter, { value: share, tone: share > 1 ? 'destructive' : null, label: 'of the lease' });
                const text = /** @type {HTMLElement} */ (cell.lastElementChild);
                text.textContent = `${Math.round(share * 100)} % of the lease`;
            }
        }
        const wrapper = row.querySelector('[data-kp-menu-button]');
        if (wrapper) setMenu(wrapper, entriesOf(p));
    }
}

/** The success toast, with its close button; it waits while it is read. @param {string} text */
const done = (text) => toast(text, { className: 'kp-toast kp-toast--success', ms: 6000, close: true });
/** The error toast stays until it is closed and offers to try again. @param {string} text @param {() => void} again */
const failed = (text, again) => {
    const t = toast(text, { className: 'kp-toast kp-toast--error', sticky: true, action: { label: 'Try again', onClick: () => { t.dismiss(); again(); } } });
};

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
            // Not JSON: the status below decides.
        }
        if (res.ok && body) return { ok: true, body };
        if (body?.error) return { ok: false, line: [body.error, body.remedy].filter(Boolean).join('. ') + (body.remedy ? '.' : '') };
        return { ok: false, line: res.ok ? UNREACHABLE : `HTTP ${res.status}. ${UNREACHABLE}` };
    } catch {
        return { ok: false, line: UNREACHABLE };
    }
}

async function recheckAll() {
    /** @type {number} */
    let n = 0;
    const result = await runFlashButton(allButton, async () => {
        for (const p of profiles) rechecking.add(p.name);
        drawAll();
        const r = await post(routes.recheckAll);
        rechecking.clear();
        if (!r.ok) {
            drawAll();
            return { error: r.line };
        }
        n = Number(r.body.rechecked ?? 0);
        await refresh();
        return 'Done';
    });
    if (!result) return;
    if (result.ok) done(`${plural(n, 'profile')} back to starting`);
    else failed(result.line, () => void recheckAll());
}

/** @param {string} name */
async function recheckOne(name) {
    rechecking.add(name);
    drawAll();
    const r = await post(routes.recheckOne.replace('{name}', encodeURIComponent(name)));
    rechecking.delete(name);
    if (r.ok) {
        await refresh();
        done(`${name} back to starting`);
    } else {
        drawAll();
        failed(r.line, () => void recheckOne(name));
    }
}

/** The command that exercises one profile. An HTTP source takes a real curl; a kyu source has no door to post to. @param {Profile} p */
function testCommand(p) {
    if (hasHub(p)) return `http-switchboard test --config config.toml --profile ${p.name} --input sample.json`;
    return `curl -X POST '${location.origin}${p.source.path ?? ''}' -H "Authorization: Bearer $SENDER_TOKEN" -H 'Content-Type: ${p.content_type}' -d @sample.json`;
}

/** The clipboard API needs a secure context; a LAN address over plain http falls back to a selected field. @param {string} text */
async function copyText(text) {
    try {
        if (navigator.clipboard?.writeText) {
            await navigator.clipboard.writeText(text);
            return true;
        }
    } catch {
        // Fall through to the legacy path.
    }
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
        copied = false;
    }
    field.remove();
    return copied;
}

/** @param {string} name */
async function copyTest(name) {
    const p = profiles.find((x) => x.name === name);
    if (!p) return;
    const command = testCommand(p);
    if (await copyText(command)) done('Test command copied');
    else toast(`Could not copy. Select it here: ${command}`, { className: 'kp-toast kp-toast--error', sticky: true });
}

// ── data ───────────────────────────────────────────────────────────────────────────────────────────────────────────

function drawAll() {
    drawProfiles();
    drawTrace();
    drawSettings();
    drawRecheck();
}

let inFlight = false;
async function refresh() {
    if (inFlight) return;
    inFlight = true;
    try {
        const res = await fetch(routes.snapshot, { headers: { accept: 'application/json' } });
        if (!res.ok || !(res.headers.get('content-type') ?? '').includes('json')) throw new Error(`HTTP ${res.status}`);
        profiles = (await res.json()).profiles ?? [];
        status = 'ready';
    } catch {
        status = 'error';
    } finally {
        inFlight = false;
    }
    drawAll();
}

/** Load again: loading for a moment, then the data. */
async function reload() {
    status = 'loading';
    drawAll();
    await refresh();
}

// ── wiring ─────────────────────────────────────────────────────────────────────────────────────────────────────────

attachEntityCards(document);
attachMatrices(document);
attachTraces(document);
attachMenuButtons(document);
attachDataTables(document);
attachFlashButtons(document);
attachFootprints(document);

for (const b of all('[data-hsb-view]')) {
    b.addEventListener('click', () => {
        view = b.getAttribute('data-hsb-view') === 'map' ? 'map' : 'cards';
        try {
            localStorage.setItem(VIEW_KEY, view);
        } catch {
            // The choice holds for this visit only.
        }
        drawProfiles();
    });
}
$('hsb-trace-pick')?.addEventListener('click', (event) => {
    const b = /** @type {HTMLElement} */ (event.target).closest('button[data-hsb-trace]');
    if (!b) return;
    traceName = /** @type {HTMLElement} */ (b).dataset.hsbTrace ?? traceName;
    drawTrace();
});
grid.addEventListener('kp-entity-action', (event) => {
    const { key, id } = /** @type {CustomEvent} */ (event).detail;
    if (id === 'recheck') void recheckOne(key);
    else if (id === 'try') location.assign(`/translate?profile=${encodeURIComponent(key)}`);
    else void copyTest(key);
});
grid.addEventListener('kp-entity-new', () => void reload());
grid.addEventListener('kp-entity-grid-retry', () => void reload());
map.addEventListener('kp-uses-retry', () => void reload());
traceEl.addEventListener('kp-trace-retry', () => void reload());
allButton.addEventListener('click', () => void recheckAll());
document.addEventListener('kp-menu-select', (event) => {
    const detail = /** @type {CustomEvent} */ (event).detail;
    const name = detail?.item?.closest('[data-hsb-row]')?.getAttribute('data-hsb-row');
    if (!name) return;
    if (detail.value === 'recheck') void recheckOne(name);
    else if (detail.value === 'copy') void copyTest(name);
    else if (detail.value === 'try') location.assign(`/translate?profile=${encodeURIComponent(name)}`);
});

drawAll();
window.setInterval(() => {
    if (!document.hidden) void refresh();
}, POLL_MS);
document.addEventListener('visibilitychange', () => {
    if (!document.hidden) void refresh();
});
