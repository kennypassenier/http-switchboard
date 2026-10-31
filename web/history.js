// The last 24 hours on the status page (feat-dash-kp11-30..39): four key figures with their trend by the hour, one uptime
// strip of 24 hour cells per profile, and the refresh chooser that sets how often the whole page reads again.
//
// Started by dashboard.js (`start`), which hands over its poll: the chooser drives both the profiles snapshot and the
// history from here on. The components are the package's, served by the kit as one bundle, /static/kp/dist/kp-themes.min.js. The data is
// /history.json (counts, times and a cause word per hour; no payload, header or URL), first embedded as #hsb-history-data.
// The history is in memory on the server and starts empty at every service start.
//
// Layout: history-model.js (the data's shape and its words), history-figures.js (the key figures and the deliveries
// list); this file holds the page's context, the card, the strips, the reads and the wiring.

import { attachCardStates, setCardState } from '/static/kp/dist/kp-themes.min.js';
import { attachKpiStrips, KPI_TOGGLE_EVENT } from '/static/kp/dist/kp-themes.min.js';
import { attachUptimes, setUptime } from '/static/kp/dist/kp-themes.min.js';
import { attachRefresh, REFRESH_EVENT } from '/static/kp/dist/kp-themes.min.js';
import { esc, periodOf, stamp, totals } from './history-model.js';
import { paintFigures, paintRecent } from './history-figures.js';

const TILES_KEY = 'hsb-history-tiles';
const PRESS_KEY = 'hsb-history-press';
/** The cells of one uptime strip: one per hour of the window. */
const STRIP_CELLS = 24;

const $ = (/** @type {string} */ id) => document.getElementById(id);
const all = (/** @type {string} */ selector, root = document) => /** @type {HTMLElement[]} */ ([...root.querySelectorAll(selector)]);

/** A remembered choice, one of `allowed`; storage that refuses keeps the first. @param {string} key @param {string[]} allowed */
function remembered(key, allowed) {
    try {
        const v = localStorage.getItem(key);
        return v && allowed.includes(v) ? v : allowed[0];
    } catch {
        return allowed[0]; // No storage: the default stands.
    }
}

/** @param {string} key @param {string} value */
function remember(key, value) {
    try {
        localStorage.setItem(key, value);
    } catch {
        // No storage: the choice holds for this visit only.
    }
}

/**
 * Everything the page's parts share: the data, what is chosen, and the elements.
 * @typedef {{ data: import('./history-model.js').History, skew: number, state: 'ready' | 'loading' | 'error', profile: string,
 *   failedOnly: boolean, look: string, press: string, el: { card: HTMLElement, strip: HTMLElement, recent: HTMLElement, strips: HTMLElement, pick: HTMLElement },
 *   figures: { built: string, detachers: (() => void)[] }, current: () => import('./history-model.js').ProfileHistory | undefined }} Context
 */

function paintPicker(/** @type {Context} */ ctx) {
    const row = $('hsb-h-pick-row');
    if (row) row.hidden = ctx.data.profiles.length < 2;
    ctx.el.pick.innerHTML = ctx.data.profiles
        .map((p) => `<button type="button" class="kp-button kp-button--sm" aria-pressed="${p.name === ctx.profile}" data-hsb-profile="${esc(p.name)}">${esc(p.name)}</button>`)
        .join('');
}

/** The card's head lines and its state: loading, failed, nothing received yet, or the figures. @param {Context} ctx */
function paintCard(ctx) {
    const { data } = ctx;
    const p = ctx.current();
    const title = $('hsb-h-title');
    if (title) title.textContent = p ? `Last 24 hours of ${p.name}` : 'Last 24 hours';
    const read = $('hsb-h-read');
    if (read) read.textContent = `read ${stamp(data.now_unix * 1000)}`;
    const body = $('hsb-h-body');
    if (body) body.textContent = `Counted per hour since the service started recording on ${stamp(data.since_unix * 1000)}.`;
    if (ctx.state === 'loading') return setCardState(ctx.el.card, 'loading');
    if (ctx.state === 'error') {
        return setCardState(ctx.el.card, 'error', { title: 'Could not read the last 24 hours', text: 'The service did not answer. The counters keep running; nothing is lost.' });
    }
    if (!p || totals(p, data).r === 0) {
        const since = stamp(data.since_unix * 1000);
        return setCardState(ctx.el.card, 'empty', {
            title: 'Nothing received yet',
            text: `Since the service started at ${since} no message has gone through ${p ? p.name : 'a profile'}. The figures appear with the first one.`,
        });
    }
    setCardState(ctx.el.card, 'ready');
    paintFigures(ctx);
}

/** @type {Map<string, HTMLElement>} */
const stripEls = new Map();

/** One uptime strip per profile, made once and fed on every read. @param {Context} ctx */
function paintStrips(ctx) {
    for (const p of ctx.data.profiles) {
        let el = stripEls.get(p.name);
        if (!el) {
            el = document.createElement('div');
            el.className = 'kp-uptime';
            el.setAttribute('data-kp-uptime', '');
            el.setAttribute('data-kp-uptime-count', String(STRIP_CELLS));
            el.setAttribute('data-kp-uptime-label', p.name);
            ctx.el.strips.append(el);
            stripEls.set(p.name, el);
            attachUptimes(ctx.el.strips);
        }
        if (ctx.press === 'info') el.setAttribute('data-kp-uptime-open', 'info');
        else el.removeAttribute('data-kp-uptime-open');
        setUptime(el, {
            label: p.name,
            periods: p.hours.map(periodOf),
            state: ctx.state === 'ready' ? 'ready' : ctx.state,
            count: STRIP_CELLS,
            from: '24 hours ago',
            to: 'Now',
            error: 'Could not read the hourly history of this profile.',
            open: ctx.press === 'info' ? 'info' : undefined,
        });
    }
}

function paintAll(/** @type {Context} */ ctx) {
    paintPicker(ctx);
    paintCard(ctx);
    paintStrips(ctx);
    for (const b of all('[data-hsb-tiles]')) b.setAttribute('aria-pressed', String(b.dataset.hsbTiles === ctx.look));
    for (const b of all('[data-hsb-press]')) b.setAttribute('aria-pressed', String(b.dataset.hsbPress === ctx.press));
    $('hsb-h-tiles')?.removeAttribute('hidden');
    $('hsb-h-press')?.removeAttribute('hidden');
}

/**
 * Read the history. A background read that fails keeps what is drawn (the chooser says the refresh failed and
 * retries); only a first read or "Try again" turns the card into the error.
 * @param {Context} ctx @param {string} route @param {boolean} quiet
 */
async function load(ctx, route, quiet) {
    try {
        const res = await fetch(route, { headers: { accept: 'application/json' } });
        if (!res.ok || !(res.headers.get('content-type') ?? '').includes('json')) throw new Error(`HTTP ${res.status}`);
        ctx.data = await res.json();
        ctx.skew = ctx.data.now_unix * 1000 - Date.now();
        if (!ctx.data.profiles.some((p) => p.name === ctx.profile)) ctx.profile = ctx.data.profiles[0]?.name ?? '';
        ctx.state = 'ready';
        paintAll(ctx);
        return true;
    } catch {
        if (!quiet) {
            ctx.state = 'error';
            paintAll(ctx);
        }
        return false;
    }
}

/** The pickers, the tile and press choices, the Failed filter and Try again. @param {Context} ctx @param {() => void} retry */
function wireChoices(ctx, retry) {
    ctx.el.pick.addEventListener('click', (event) => {
        const b = /** @type {HTMLElement} */ (event.target).closest('button[data-hsb-profile]');
        if (!b) return;
        ctx.profile = /** @type {HTMLElement} */ (b).dataset.hsbProfile ?? ctx.profile;
        ctx.failedOnly = false;
        paintAll(ctx);
    });
    for (const b of all('[data-hsb-tiles]')) {
        b.addEventListener('click', () => {
            ctx.look = b.dataset.hsbTiles === 'spark' ? 'spark' : 'trend';
            remember(TILES_KEY, ctx.look);
            ctx.failedOnly = false;
            paintAll(ctx);
        });
    }
    for (const b of all('[data-hsb-press]')) {
        b.addEventListener('click', () => {
            ctx.press = b.dataset.hsbPress === 'hover' ? 'hover' : 'info';
            remember(PRESS_KEY, ctx.press);
            paintAll(ctx);
        });
    }
    ctx.el.strip.addEventListener(KPI_TOGGLE_EVENT, (event) => {
        ctx.failedOnly = Boolean(/** @type {CustomEvent} */ (event).detail?.pressed);
        paintRecent(ctx);
    });
    ctx.el.card.addEventListener('kp-card-retry', retry);
}

/** @param {{ setPoll: (seconds: number) => void, refresh: () => Promise<boolean | void> }} dashboard */
export function start(dashboard) {
    const embedded = JSON.parse($('hsb-history-data')?.textContent ?? 'null');
    if (!embedded?.history) return;
    const route = embedded.route ?? '/history.json';
    const el = { card: $('hsb-h-card'), strip: $('hsb-h-strip'), recent: $('hsb-h-recent'), strips: $('hsb-h-strips'), pick: $('hsb-h-pick') };
    if (!el.card || !el.strip || !el.recent || !el.strips || !el.pick) return;
    /** @type {import('./history-model.js').History} */
    const data = embedded.history;
    /** @type {Context} */
    const ctx = {
        data,
        skew: data.now_unix * 1000 - Date.now(),
        state: 'loading', // the first read is in flight from the first frame; the embedded history only shapes the strips
        profile: data.profiles[0]?.name ?? '',
        failedOnly: false,
        look: remembered(TILES_KEY, ['trend', 'spark']),
        press: remembered(PRESS_KEY, ['info', 'hover']),
        el: /** @type {Context['el']} */ (el),
        figures: { built: '', detachers: [] },
        current: () => ctx.data.profiles.find((p) => p.name === ctx.profile) ?? ctx.data.profiles[0],
    };
    attachCardStates(ctx.el.card);
    attachKpiStrips(ctx.el.strip);
    wireChoices(ctx, () => {
        ctx.state = 'loading';
        paintAll(ctx);
        void load(ctx, route, false);
    });

    // The refresh chooser is the form grid's own refresh bar: it reads the profiles snapshot and the history together,
    // and takes the page's poll over from dashboard.js.
    const chooser = $('hsb-h-refresh');
    if (chooser) {
        attachRefresh($('hsb-h-grid') ?? chooser);
        dashboard.setPoll(0);
        chooser.addEventListener(REFRESH_EVENT, async (event) => {
            const detail = /** @type {CustomEvent} */ (event).detail;
            const [profiles, history] = await Promise.all([dashboard.refresh(), load(ctx, route, true)]);
            detail.done(profiles !== false && history);
        });
    }
    paintAll(ctx);
    void load(ctx, route, false);
}
