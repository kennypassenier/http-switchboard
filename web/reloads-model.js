// The "Reloads" card, the pure part: the shape of /reloads.json, the words of a row, the counts line and the alert about a
// refused reload. No DOM here, so the view (reloads.js) only draws what these say. Times use the page's one clock, the
// Brussels clock of history-model.js.

import { count, stamp } from './history-model.js';

const COMMAND = 'systemctl reload http-switchboard';
const OUTCOME = {
    applied: { badge: 'kp-badge kp-badge--success', word: 'Applied' },
    'needs-restart': { badge: 'kp-badge kp-badge--warning', word: 'Needs a restart' },
    invalid: { badge: 'kp-badge kp-badge--destructive', word: 'Invalid' },
};

/** @typedef {{ seq: number, at_unix: number, outcome: keyof typeof OUTCOME, changed: string[], reason: string | null }} Reload */
/** @typedef {{ boot: string, dropped: number, reloads: Reload[] }} View */

/** The outcome's badge, the refusal's for a word this page does not know. @param {string} outcome */
export const outcomeOf = (outcome) => OUTCOME[/** @type {keyof typeof OUTCOME} */ (outcome)] ?? OUTCOME.invalid;

/** dd/mm/yyyy HH:MM on the Brussels clock. @param {number} unix */
export const when = (unix) => stamp(unix * 1000);

const sentence = (/** @type {string} */ text) => {
    const t = text.trim();
    return t ? (/[.!?]$/.test(t) ? t : `${t}.`) : '';
};
const capital = (/** @type {string} */ text) => text.charAt(0).toUpperCase() + text.slice(1);

/**
 * A view with every field defaulted, so a partial answer still draws what it has. @param {any} raw
 * @returns {View}
 */
export function viewOf(raw) {
    if (!raw || typeof raw !== 'object') throw new TypeError('the reload history is not an object');
    const reloads = Array.isArray(raw.reloads) ? raw.reloads : [];
    return {
        boot: String(raw.boot ?? ''),
        dropped: Number(raw.dropped ?? 0),
        reloads: reloads.map((/** @type {any} */ r) => ({ ...r, changed: Array.isArray(r.changed) ? r.changed : [], reason: r.reason ?? null })),
    };
}

/** The two lines a row says. @param {Reload} r @returns {{ headline: string, detail: string }} */
export function words(r) {
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

/** "3 reloads · 2 applied · 1 needs a restart · 0 invalid". @param {View} view */
export function countsLine(view) {
    const n = (/** @type {string} */ o) => view.reloads.filter((r) => r.outcome === o).length;
    const total = view.reloads.length;
    let line = `${count(total)} ${total === 1 ? 'reload' : 'reloads'} · ${count(n('applied'))} applied · ${count(n('needs-restart'))} ${n('needs-restart') === 1 ? 'needs' : 'need'} a restart · ${count(n('invalid'))} invalid`;
    if (view.dropped > 0) line += ` · ${count(view.dropped)} older not kept`;
    return line;
}

/** The problems list: one warning when the latest reload was refused, else none. @param {View | null} view */
export function problemsOf(view) {
    const latest = view?.reloads[0];
    if (!latest || latest.outcome !== 'invalid') return [];
    const { detail } = words(latest);
    return [
        {
            key: `reload-invalid-${latest.seq}`,
            severity: 'warning',
            what: `The latest reload (${when(latest.at_unix)}) was refused`,
            why: `${detail.replace(/ Nothing changed\.$/, '')} The service keeps running on the last good version, so nothing is lost.`,
            remedy: 'Correct the profile file, then reload again.',
            command: COMMAND,
        },
    ];
}
