// The last 24 hours on the status page (feat-dash-kp11-30..39): four key figures with their trend by the hour, one uptime
// strip of 24 hour cells per profile, and the View options popover (how the tiles look, what pressing an hour opens).
//
// Started by dashboard.js (`start`). It reads again on every poll of the page (the hsb-poll event, which rides on the
// kit's refresh menu in the page header), and shows the profile chosen in Path (PICK_EVENT, profile-pick.js). The components are the package's, served by the kit as one bundle, /static/kp/dist/kp-themes.min.js. The data is
// /history.json (counts, times and a cause word per hour; no payload, header or URL), first embedded as #hsb-history-data.
// The history is in memory on the server and starts empty at every service start.
//
// Layout: history-model.js (the data's shape and its words), history-figures.js (the key figures and the deliveries
// list); this file holds the page's context, the card, the strips, the reads and the wiring.

import { attachCardStates, setCardState } from '/static/kp/dist/kp-themes.min.js';
import { attachKpiStrips, KPI_TOGGLE_EVENT } from '/static/kp/dist/kp-themes.min.js';
import { attachUptimes, setUptime } from '/static/kp/dist/kp-themes.min.js';
import { attachSegmented, setSegmentedValues } from '/static/kp/dist/kp-themes.min.js';
import { historyOf, periodOf, stamp, totals } from './history-model.js';
import { paintFigures, paintRecent } from './history-figures.js';
import { clock, store } from './hsb-state.js';
import { PICK_EVENT } from './profile-pick.js';

const TILES_KEY = 'hsb-history-tiles';
const PRESS_KEY = 'hsb-history-press';
/** The cells of one uptime strip: one per hour of the window. */
const STRIP_CELLS = 24;

const $ = (/** @type {string} */ id) => document.getElementById(id);

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
 *   failedOnly: boolean, look: string, press: string, el: { card: HTMLElement, strip: HTMLElement, recent: HTMLElement, strips: HTMLElement },
 *   errorText: string | undefined, figures: { built: string, detachers: (() => void)[] }, current: () => import('./history-model.js').ProfileHistory | undefined }} Context
 */

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
        return setCardState(ctx.el.card, 'error', { title: 'Could not read the last 24 hours', text: ctx.errorText ?? 'The service did not answer. The counters keep running; nothing is lost.' });
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

/** One uptime strip per profile, made once and fed on every read; a strip whose profile is gone is removed. @param {Context} ctx */
function paintStrips(ctx) {
    const names = new Set(ctx.data.profiles.map((p) => p.name));
    for (const [name, el] of stripEls) {
        if (names.has(name)) continue;
        el.remove();
        stripEls.delete(name);
    }
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
            attachUptimes(el); // once per strip: the kit marks an attached strip and skips it later
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
    paintCard(ctx);
    paintStrips(ctx);
    $('hsb-h-options-open')?.removeAttribute('hidden');
}

/** The newest read started; an older answer that arrives late is dropped. */
let readSerial = 0;

/** Take a fresh history into the context. @param {Context} ctx @param {import('./history-model.js').History} data */
function adopt(ctx, data) {
    ctx.data = data;
    ctx.skew = data.now_unix * 1000 - Date.now();
    clock.skewMs = ctx.skew;
    if (!data.profiles.some((p) => p.name === ctx.profile)) ctx.profile = data.profiles[0]?.name ?? '';
    ctx.state = 'ready';
}

/**
 * Read the history. A background read that fails keeps what is drawn and the next poll of the page tries
 * again; only a first read or "Try again" turns the card into the error. Reads overlap (a poll while Try again is
 * running): only the newest one may paint.
 * @param {Context} ctx @param {string} route @param {boolean} quiet
 */
async function load(ctx, route, quiet) {
    const mine = ++readSerial;
    let ok = false;
    let why = 'The service did not answer. The counters keep running; nothing is lost.';
    try {
        const res = await fetch(route, { headers: { accept: 'application/json' } });
        if (!res.ok || !(res.headers.get('content-type') ?? '').includes('json')) throw new Error(`HTTP ${res.status}`);
        const raw = await res.json();
        if (mine !== readSerial) return false;
        try {
            adopt(ctx, historyOf(raw));
        } catch (error) {
            why = 'The service answered, but the answer could not be read. The counters keep running; nothing is lost.';
            throw error;
        }
        ok = true;
    } catch (error) {
        console.error('http-switchboard: the last 24 hours could not be read', error);
    }
    if (mine !== readSerial) return false;
    if (ok) paintAll(ctx);
    else if (!quiet || ctx.state !== 'ready') {
        ctx.state = 'error';
        ctx.errorText = why;
        paintAll(ctx);
    }
    return ok;
}

/** One View options choice: its segmented group, painted with the current value, telling `choose` what was picked. @param {string} id @param {string} value @param {(v: string) => void} choose */
function wireOption(id, value, choose) {
    const group = $(id);
    if (!group) return;
    attachSegmented(group);
    setSegmentedValues(group, [value]);
    group.addEventListener('kp-segmented-change', (event) => {
        const v = /** @type {CustomEvent} */ (event).detail?.value;
        if (typeof v === 'string') choose(v);
    });
}

/** The profile choice in Path, the View options, the Failed filter and Try again. @param {Context} ctx @param {() => void} retry */
function wireChoices(ctx, retry) {
    document.addEventListener(PICK_EVENT, (event) => {
        const name = /** @type {CustomEvent} */ (event).detail?.name;
        if (typeof name !== 'string' || name === ctx.profile) return;
        ctx.profile = name;
        ctx.failedOnly = false;
        paintAll(ctx);
    });
    wireOption('hsb-h-tiles', ctx.look, (v) => {
        ctx.look = v === 'spark' ? 'spark' : 'trend';
        remember(TILES_KEY, ctx.look);
        ctx.failedOnly = false;
        paintAll(ctx);
    });
    wireOption('hsb-h-press', ctx.press, (v) => {
        ctx.press = v === 'hover' ? 'hover' : 'info';
        remember(PRESS_KEY, ctx.press);
        paintAll(ctx);
    });
    ctx.el.strip.addEventListener(KPI_TOGGLE_EVENT, (event) => {
        ctx.failedOnly = Boolean(/** @type {CustomEvent} */ (event).detail?.pressed);
        paintRecent(ctx);
    });
    ctx.el.card.addEventListener('kp-card-retry', retry);
}

/** The embedded history, or null (logged) when the page holds none or it cannot be read. */
function embeddedHistory() {
    try {
        const embedded = JSON.parse($('hsb-history-data')?.textContent ?? 'null');
        if (embedded?.history) return embedded;
        console.warn('http-switchboard: the page embeds no history; the last-24-hours card cannot start');
    } catch (error) {
        console.error('http-switchboard: the embedded history is not valid JSON', error);
    }
    return null;
}

/** @returns {boolean} whether the card started; false (logged) leaves it to the caller to say so */
export function start() {
    const embedded = embeddedHistory();
    if (!embedded) return false;
    const route = embedded.route ?? '/history.json';
    const el = { card: $('hsb-h-card'), strip: $('hsb-h-strip'), recent: $('hsb-h-recent'), strips: $('hsb-h-strips') };
    if (!el.card || !el.strip || !el.recent || !el.strips) {
        console.warn('http-switchboard: the last-24-hours card is missing part of its markup');
        return false;
    }
    /** @type {import('./history-model.js').History} */
    const data = embedded.history;
    /** @type {Context} */
    const ctx = {
        data,
        skew: data.now_unix * 1000 - Date.now(),
        state: 'loading', // the first read is in flight from the first frame; the embedded history only shapes the strips
        profile: data.profiles.some((p) => p.name === store.traceName) ? store.traceName : (data.profiles[0]?.name ?? ''),
        failedOnly: false,
        look: remembered(TILES_KEY, ['trend', 'spark']),
        press: remembered(PRESS_KEY, ['info', 'hover']),
        el: /** @type {Context['el']} */ (el),
        figures: { built: '', detachers: [] },
        errorText: undefined,
        current: () => ctx.data.profiles.find((p) => p.name === ctx.profile) ?? ctx.data.profiles[0],
    };
    attachCardStates(ctx.el.card);
    attachKpiStrips(ctx.el.strip);
    wireChoices(ctx, () => {
        ctx.state = 'loading';
        paintAll(ctx);
        void load(ctx, route, false);
    });

    // Every poll of the page (dashboard.js) reads the history too; a failed background read keeps what is drawn.
    document.addEventListener('hsb-poll', () => void load(ctx, route, true));
    paintAll(ctx);
    void load(ctx, route, false);
    return true;
}
