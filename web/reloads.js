// The "Reloads" card of the status page [feat-dash-kp11-42]: the history of the profile file being read again, newest
// first, as the approved kp-themes timeline. The data is GET /reloads.json (src/reload_log.rs: the last 50, in memory, no
// secrets); the card reads it from the first frame (loading until it answers) and then on the dashboard's own poll (the module
// dashboard.js fires `hsb-poll` after each refresh), so there is no second timer here.

import { attachCardStates } from '/static/kp/js/card-state.js';
import { holdCardState } from './profiles.js';
import { setProblems, attachProblems } from '/static/kp/js/problems.js';
import { attachNewPills } from '/static/kp/js/newpill.js';

const COMMAND = 'systemctl reload http-switchboard';
const OUTCOME = {
    applied: { badge: 'kp-badge kp-badge--success', word: 'Applied' },
    'needs-restart': { badge: 'kp-badge kp-badge--warning', word: 'Needs a restart' },
    invalid: { badge: 'kp-badge kp-badge--destructive', word: 'Invalid' },
};

/** @typedef {{ seq: number, at_unix: number, outcome: keyof typeof OUTCOME, changed: string[], reason: string | null }} Reload */
/** @typedef {{ boot: string, dropped: number, reloads: Reload[] }} View */

const $ = (/** @type {string} */ id) => document.getElementById(id);
const card = /** @type {HTMLElement | null} */ ($('hsb-reloads'));
const list = /** @type {HTMLElement | null} */ ($('hsb-reloads-list'));
const alert = $('hsb-reloads-alert');
const counts = $('hsb-reloads-counts');

if (card && list && alert) {
    const embedded = JSON.parse($('hsb-reloads-data')?.textContent ?? '{}');
    const route = embedded.route ?? '/reloads.json';
    const pad = (/** @type {number} */ n) => String(n).padStart(2, '0');
    const count = (/** @type {number} */ n) => n.toLocaleString('en-GB');

    /** @param {number} unix */
    const when = (unix) => {
        const d = new Date(unix * 1000);
        return `${pad(d.getDate())}/${pad(d.getMonth() + 1)}/${d.getFullYear()} ${pad(d.getHours())}:${pad(d.getMinutes())}`;
    };
    const sentence = (/** @type {string} */ text) => {
        const t = text.trim();
        return t ? (/[.!?]$/.test(t) ? t : `${t}.`) : '';
    };
    const capital = (/** @type {string} */ text) => text.charAt(0).toUpperCase() + text.slice(1);

    /** The two lines a row says. @param {Reload} r @returns {{ headline: string, detail: string }} */
    function words(r) {
        if (r.outcome === 'applied') {
            return r.changed.length
                ? { headline: `Changed: ${r.changed.join(', ')}`, detail: 'The next message already uses the new version.' }
                : { headline: 'No profile changed', detail: 'The file was read again and holds the same profiles as before.' };
        }
        const why = sentence(capital(r.reason ?? ''));
        if (r.outcome === 'needs-restart') {
            return { headline: 'Nothing changed', detail: `${why} The service keeps running as it started; restart it to apply the change.`.trim() };
        }
        return { headline: 'Refused: the profile file is not valid', detail: `${why} Nothing changed.`.trim() };
    }

    /** @param {Reload} r */
    function row(r) {
        const { headline, detail } = words(r);
        const li = document.createElement('li');
        li.className = 'kp-timeline__item';
        li.dataset.hsbReload = String(r.seq);
        li.dataset.hsbOutcome = r.outcome;
        const marker = document.createElement('span');
        marker.className = 'kp-timeline__marker';
        marker.setAttribute('aria-hidden', 'true');
        const body = document.createElement('span');
        body.className = 'hsb-reload';
        const time = document.createElement('time');
        time.className = 'kp-timeline__when';
        time.dateTime = new Date(r.at_unix * 1000).toISOString();
        time.textContent = when(r.at_unix);
        const outcome = document.createElement('span');
        outcome.className = 'hsb-reload__outcome';
        const badge = document.createElement('span');
        badge.className = (OUTCOME[r.outcome] ?? OUTCOME.invalid).badge;
        badge.textContent = (OUTCOME[r.outcome] ?? OUTCOME.invalid).word;
        outcome.append(badge);
        const what = document.createElement('span');
        what.className = 'hsb-reload__what';
        const head = document.createElement('span');
        head.textContent = headline;
        const more = document.createElement('span');
        more.textContent = detail;
        what.append(head, more);
        body.append(time, outcome, what);
        li.append(marker, body);
        return li;
    }

    /** @type {View | null} */
    let view = null;
    /** @type {Map<number, HTMLElement>} */
    let rows = new Map();
    let shown = '';

    const setState = (/** @type {string} */ key, /** @type {() => void} */ apply) => {
        if (shown === key) return;
        shown = key;
        apply();
    };

    function paintAlert() {
        const latest = view?.reloads[0];
        if (!latest || latest.outcome !== 'invalid') return setProblems(alert, []);
        const { detail } = words(latest);
        setProblems(alert, [
            {
                key: `reload-invalid-${latest.seq}`,
                severity: 'warning',
                what: `The latest reload (${when(latest.at_unix)}) was refused`,
                why: `${detail.replace(/ Nothing changed\.$/, '')} The service keeps running on the last good version, so nothing is lost.`,
                remedy: 'Correct the profile file, then reload again.',
                command: COMMAND,
            },
        ]);
    }

    function paintCounts() {
        if (!counts || !view) return;
        const n = (/** @type {string} */ o) => view.reloads.filter((r) => r.outcome === o).length;
        const total = view.reloads.length;
        let line = `${count(total)} ${total === 1 ? 'reload' : 'reloads'} · ${count(n('applied'))} applied · ${count(n('needs-restart'))} ${n('needs-restart') === 1 ? 'needs' : 'need'} a restart · ${count(n('invalid'))} invalid`;
        if (view.dropped > 0) line += ` · ${count(view.dropped)} older not kept`;
        counts.textContent = line;
    }

    /** Bring the rows in step with the data: new ones on top, nothing redrawn. @param {View} next */
    function paintRows(next) {
        const fresh = view?.boot !== next.boot;
        if (fresh) {
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
        const added = next.reloads.filter((r) => !rows.has(r.seq)).reverse();
        for (const r of added) {
            const el = row(r);
            rows.set(r.seq, el);
            list.prepend(el);
        }
    }

    /** @param {View} next */
    function show(next) {
        paintRows(next);
        view = next;
        paintAlert();
        paintCounts();
        if (next.reloads.length === 0) {
            setState('empty', () =>
                holdCardState(card, 'empty', {
                    title: 'No reloads yet',
                    text: 'The profile file has not been read again since the service started. This history is kept in memory, so it starts empty after every restart.',
                }),
            );
        } else {
            setState('ready', () => holdCardState(card, 'ready'));
        }
    }

    let inFlight = false;
    async function refresh() {
        if (inFlight) return;
        inFlight = true;
        try {
            const res = await fetch(route, { headers: { accept: 'application/json' } });
            if (!res.ok || !(res.headers.get('content-type') ?? '').includes('json')) throw new Error(`HTTP ${res.status}`);
            show(await res.json());
        } catch {
            setState('error', () =>
                holdCardState(card, 'error', {
                    title: 'The reload history could not be read',
                    text: 'The service did not answer. Nothing was changed.',
                }),
            );
        } finally {
            inFlight = false;
        }
    }

    attachCardStates(document);
    attachProblems(document);
    // The first answer is read like every later one: the card loads until it comes.
    holdCardState(card, 'loading', { words: 'Reading the reload history…' });
    shown = 'loading';
    void refresh();
    attachNewPills(card);

    card.addEventListener('kp-card-retry', () => {
        shown = 'loading';
        holdCardState(card, 'loading', { words: 'Reading the reload history…' });
        void refresh();
    });
    document.addEventListener('hsb-poll', () => void refresh());
}
