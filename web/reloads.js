// The "Reloads" card of the status page [feat-dash-kp11-42]: the history of the profile file being read again, newest
// first, as the approved kp-themes timeline. The data is GET /reloads.json (src/reload_log.rs: the last 50, in memory, no
// secrets); the card reads it from the first frame (loading until it answers) and then on the dashboard's own poll (the module
// dashboard.js fires `hsb-poll` after each refresh), so there is no second timer here. The words and shapes are in
// reloads-model.js; this file draws them.

import { attachCardStates, setCardState } from '/static/kp/dist/kp-themes.min.js';
import { setProblems, attachProblems } from '/static/kp/dist/kp-themes.min.js';
import { attachNewPills } from '/static/kp/dist/kp-themes.min.js';
import { countsLine, outcomeOf, problemsOf, viewOf, when, words } from './reloads-model.js';

const $ = (/** @type {string} */ id) => document.getElementById(id);
const card = /** @type {HTMLElement | null} */ ($('hsb-reloads'));
const list = /** @type {HTMLElement | null} */ ($('hsb-reloads-list'));
const alert = $('hsb-reloads-alert');
const counts = $('hsb-reloads-counts');
const LOADING = { words: 'Reading the reload history…' };

/** A span with text. @param {string} cls @param {string} text */
function span(cls, text) {
    const el = document.createElement('span');
    if (cls) el.className = cls;
    el.textContent = text;
    return el;
}

/** One timeline row. @param {import('./reloads-model.js').Reload} r */
function row(r) {
    const { headline, detail } = words(r);
    const li = document.createElement('li');
    li.className = 'kp-timeline__item';
    li.dataset.hsbReload = String(r.seq);
    li.dataset.hsbOutcome = r.outcome;
    const marker = span('kp-timeline__marker', '');
    marker.setAttribute('aria-hidden', 'true');
    const body = span('hsb-reload', '');
    const time = document.createElement('time');
    time.className = 'kp-timeline__when';
    time.dateTime = new Date(r.at_unix * 1000).toISOString();
    time.textContent = when(r.at_unix);
    const outcome = span('hsb-reload__outcome', '');
    const o = outcomeOf(r.outcome);
    outcome.append(span(o.badge, o.word));
    const what = span('hsb-reload__what', '');
    what.append(span('', headline), span('', detail));
    body.append(time, outcome, what);
    li.append(marker, body);
    return li;
}

if (card && list && alert) {
    const embedded = JSON.parse($('hsb-reloads-data')?.textContent ?? '{}');
    const route = embedded.route ?? '/reloads.json';
    /** @type {import('./reloads-model.js').View | null} */
    let view = null;
    /** @type {Map<number, HTMLElement>} */
    let rows = new Map();
    let shown = '';
    let inFlight = false;

    const setState = (/** @type {string} */ key, /** @type {() => void} */ apply) => {
        if (shown === key) return;
        shown = key;
        apply();
    };

    /** Bring the rows in step with the data: new ones on top, nothing redrawn. @param {import('./reloads-model.js').View} next */
    function paintRows(next) {
        if (view?.boot !== next.boot) {
            for (const el of rows.values()) el.remove();
            rows = new Map();
        }
        const keep = new Set(next.reloads.map((r) => r.seq));
        for (const [seq, el] of rows) {
            if (!keep.has(seq)) {
                el.remove();
                rows.delete(seq);
            }
        }
        for (const r of next.reloads.filter((x) => !rows.has(x.seq)).reverse()) {
            const el = row(r);
            rows.set(r.seq, el);
            list?.prepend(el);
        }
    }

    /** @param {import('./reloads-model.js').View} next */
    function show(next) {
        paintRows(next);
        view = next;
        if (alert) setProblems(alert, problemsOf(view));
        if (counts) counts.textContent = countsLine(view);
        if (next.reloads.length === 0) {
            setState('empty', () =>
                setCardState(card, 'empty', {
                    title: 'No reloads yet',
                    text: 'The profile file has not been read again since the service started. This history is kept in memory, so it starts empty after every restart.',
                }),
            );
        } else {
            setState('ready', () => setCardState(card, 'ready'));
        }
    }

    /** The error face; a background read that fails while rows are drawn keeps them. @param {string} text @param {boolean} quiet */
    function fail(text, quiet) {
        if (quiet && view) return;
        setState('error', () => setCardState(card, 'error', { title: 'The reload history could not be read', text }));
    }

    /** @param {boolean} quiet a background read (the page's poll) rather than the first read or Try again */
    async function refresh(quiet) {
        if (inFlight) return;
        inFlight = true;
        try {
            let raw;
            try {
                const res = await fetch(route, { headers: { accept: 'application/json' } });
                if (!res.ok || !(res.headers.get('content-type') ?? '').includes('json')) throw new Error(`HTTP ${res.status}`);
                raw = await res.json();
            } catch (error) {
                console.error('http-switchboard: the reload history could not be read', error);
                return fail('The service did not answer. Nothing was changed.', quiet);
            }
            try {
                show(viewOf(raw));
            } catch (error) {
                console.error('http-switchboard: the reload history could not be shown', error);
                fail('The service answered, but the answer could not be read. Nothing was changed.', quiet);
            }
        } finally {
            inFlight = false;
        }
    }

    attachCardStates(document);
    attachProblems(document);
    // The first answer is read like every later one: the card loads until it comes.
    setCardState(card, 'loading', LOADING);
    shown = 'loading';
    void refresh(false);
    attachNewPills(card);

    card.addEventListener('kp-card-retry', () => {
        shown = 'loading';
        setCardState(card, 'loading', LOADING);
        void refresh(false);
    });
    document.addEventListener('hsb-poll', () => void refresh(true));
}
