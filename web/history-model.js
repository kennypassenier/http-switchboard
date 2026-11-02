// The last 24 hours, the pure part: the shape of /history.json, the four figures and their words, the Brussels clock,
// and what one hour cell says. No DOM here, so the figures, the strips and the page all read the same sentences.

import { uptimePercent } from '/static/kp/dist/kp-themes.min.js';

/** One hour, in milliseconds: the step of every trend and every strip cell. */
export const HOUR = 3_600_000;

/**
 * @typedef {{ start_unix: number, received: number, delivered: number, failed: number, duration_ms: number,
 *   state: 'up' | 'degraded' | 'down' | 'none', cause: string | null }} Hour
 * @typedef {{ at_unix: number, ok: boolean, ms: number | null }} Recent
 * @typedef {{ name: string, hours: Hour[], recent: Recent[], last_failure_unix: number | null }} ProfileHistory
 * @typedef {{ now_unix: number, since_unix: number, profiles: ProfileHistory[] }} History
 */

/** An exact count with thousands separators. @param {number} n */
export const count = (n) => n.toLocaleString('en-GB');

const zone = new Intl.DateTimeFormat('en-GB', {
    timeZone: 'Europe/Brussels',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
});

/** dd/mm/yyyy HH:MM on the Brussels clock, the page's one clock. @param {number} ms */
export const stamp = (ms) => {
    const p = Object.fromEntries(zone.formatToParts(ms).map((x) => [x.type, x.value]));
    return `${p.day}/${p.month}/${p.year} ${p.hour}:${p.minute}`;
};

/** HH:MM only. @param {number} ms */
const clockOf = (ms) => stamp(ms).slice(11);

/** Escape text for the tiles' HTML. @param {string} text */
export const esc = (text) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

export const METRICS = [
    { key: 'received', label: 'Received', series: (/** @type {Hour} */ h) => h.received },
    { key: 'delivered', label: 'Delivered', series: (/** @type {Hour} */ h) => h.delivered },
    { key: 'failed', label: 'Failed', series: (/** @type {Hour} */ h) => h.failed },
    { key: 'duration', label: 'Average duration', series: (/** @type {Hour} */ h) => (h.delivered ? Math.round(h.duration_ms / h.delivered) : NaN) },
];

/** The four figures of a profile over its recorded hours. @param {ProfileHistory} p @param {History} data */
export function totals(p, data) {
    const hours = p.hours.filter((h) => h.state !== 'none');
    const sum = (/** @type {(h: Hour) => number} */ f) => hours.reduce((n, h) => n + f(h), 0);
    const r = sum((h) => h.received);
    const d = sum((h) => h.delivered);
    const f = sum((h) => h.failed);
    const ms = sum((h) => h.duration_ms);
    const slow = hours.filter((h) => h.delivered > 0).sort((a, b) => b.duration_ms / b.delivered - a.duration_ms / a.delivered)[0];
    return { r, d, f, avg: d ? Math.round(ms / d) : 0, slow, lastFail: p.last_failure_unix, since: data.since_unix, windowStart: p.hours[0]?.start_unix ?? 0 };
}

/** The line under a figure. @param {string} key @param {ReturnType<typeof totals>} t */
export function words(key, t) {
    if (key === 'received') return t.since * 1000 > t.windowStart * 1000 ? `since ${stamp(t.since * 1000)}` : 'in the last 24 hours';
    if (key === 'delivered') return t.r ? `${uptimePercent(t.d / t.r)}% of received` : 'nothing received yet';
    if (key === 'failed') return t.f > 0 && t.lastFail ? `last ${stamp(t.lastFail * 1000)}` : 'none in 24 hours';
    return t.slow ? `slowest hour ${clockOf(t.slow.start_unix * 1000)}, ${Math.round(t.slow.duration_ms / t.slow.delivered)} ms` : 'no delivery yet';
}

/** The words after a number. @param {string} key @param {number} n */
export const unitOf = (key, n) => (key === 'duration' ? 'ms' : n === 1 ? 'message' : 'messages');

/** A value as a chart's readout says it. @param {string} key @param {number} v */
export const spoken = (key, v) => (key === 'duration' ? `${v} ms` : `${v} ${unitOf(key, v)}`);

/** The recorded hours of one figure as chart points. @param {ProfileHistory} p @param {(typeof METRICS)[number]} m */
export function points(p, m) {
    return p.hours.filter((h) => h.state !== 'none' && Number.isFinite(m.series(h))).map((h) => /** @type {[number, number]} */ ([h.start_unix * 1000, m.series(h)]));
}

/** The kit's failure-cause words, one per cause. @param {Hour} h */
function causeLines(h) {
    if (h.state === 'none') return ['History was not recorded yet: the service had not started.'];
    switch (h.cause) {
        case 'hub-down':
            return ['The kyu hub could not be reached during this hour.'];
        case 'denied':
            return ['The hub refused the credentials during this hour.'];
        case 'all-failed':
            return ['Every delivery in this hour failed.'];
        case 'failing':
            return [h.failed > 0 ? `${count(h.failed)} of ${count(h.received)} deliveries failed.` : 'The profile was failing during this hour.'];
        default:
            return h.received === 0 ? ['Nothing came in during this hour.'] : [];
    }
}

/** @param {Hour} h */
function stateWord(h) {
    if (h.state === 'none') return 'No data';
    if (h.cause === 'hub-down') return 'Hub down';
    if (h.cause === 'denied') return 'Denied';
    if (h.state === 'up') return 'Working';
    return 'Failing';
}

/** An hour that carries no verdict: not recorded yet, or nothing came in (an hour without traffic is neither up nor down). @param {Hour} h */
const silent = (h) => h.state === 'none' || (h.received === 0 && h.state === 'up');

/** One hour as an uptime strip cell: its tooltip and the quick-info lines. The kit's percentage counts only the cells that
 *  are not `none`, so a silent hour must not be `up` or an idle profile would read 100 %. @param {Hour} h */
export function periodOf(h) {
    const at = h.start_unix * 1000;
    const label = stamp(at).slice(0, 14) + '00';
    const avg = h.delivered ? `, ${count(Math.round(h.duration_ms / h.delivered))} ms on average` : '';
    if (silent(h)) {
        const none = h.state === 'none';
        return { key: new Date(at).toISOString(), label, state: /** @type {const} */ ('none'), tip: `${label} · ${none ? 'No data' : 'Nothing received'}`, details: [none ? causeLines(h)[0] : 'Nothing came in during this hour.'] };
    }
    return {
        key: new Date(at).toISOString(),
        label,
        state: h.state,
        tip: `${label} · ${stateWord(h)} · ${count(h.delivered)} of ${count(h.received)} delivered${avg}`,
        details: [`Received ${count(h.received)}, delivered ${count(h.delivered)}, failed ${count(h.failed)}.`, ...causeLines(h)],
    };
}

/** The history with every field defaulted, so a partial answer still draws; anything but an object is refused. @param {any} raw @returns {History} */
export function historyOf(raw) {
    if (!raw || typeof raw !== 'object' || !Array.isArray(raw.profiles)) throw new TypeError('the history has no profiles list');
    return {
        now_unix: Number(raw.now_unix ?? Date.now() / 1000),
        since_unix: Number(raw.since_unix ?? 0),
        profiles: raw.profiles.map((/** @type {any} */ p) => ({ ...p, hours: Array.isArray(p.hours) ? p.hours : [], recent: Array.isArray(p.recent) ? p.recent : [], last_failure_unix: p.last_failure_unix ?? null })),
    };
}
