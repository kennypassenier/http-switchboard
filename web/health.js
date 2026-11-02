// The top of the status page: one alert per real problem (what, why, what now and the fix, worst first) and the kyu
// hub's connection banner. A profile that is only `starting` is not a problem: its card says so, not this list.
//
// It draws from the snapshot dashboard.js already polls: dashboard.js announces every refresh as `hsb:profiles` on the
// document, and `hsb:refresh` asks it for one more. There is no second poll. Words describe a profile from its own
// shape (kyu or HTTP source, URL or kyu destination); nothing here sees a header, a payload or a webhook path.
//
// Layout: health-words.js (a state in words), health-hub.js (the hub banner); this file draws the alerts and does the
// rechecks.

import { setProblems, attachProblems } from '/static/kp/dist/kp-themes.min.js';
import { attachConnection, CONNECTION_RETRY_EVENT } from '/static/kp/dist/kp-themes.min.js';
import { attachArt } from '/static/kp/dist/kp-themes.min.js';
import { setStrings } from '/static/kp/dist/kp-themes.min.js';
import { toast } from '/static/kp/dist/kp-themes.min.js';
import { list, wordsOf } from './health-words.js';
import { hubBanner } from './health-hub.js';
import { post } from './hsb-post.js';

/** How long the success toast stays, in milliseconds. */
const DONE_TOAST_MS = 6000;

setStrings({
    connOffline: 'The kyu hub is unreachable',
    connOfflineWhy: 'Messages cannot be fetched until the hub answers. The switchboard keeps trying by itself.',
    connReconnecting: 'Reaching the kyu hub…',
    connReconnectingWhy: 'Trying to reach the hub now.',
    connPending: (/** @type {number} */ n) => `${n} ${n === 1 ? 'message' : 'messages'} waiting on the hub`,
    connBack: 'Connected to the kyu hub again',
    connBackWhy: 'Messages are being fetched.',
    connBackSending: (/** @type {number} */ n) => `Delivering ${n} waiting ${n === 1 ? 'message' : 'messages'}`,
    connRefused: 'The kyu hub refused our credentials',
    connRefusedWhy: 'The switchboard’s token was not accepted. Re-issue it.',
});

const byId = (/** @type {string} */ id) => /** @type {HTMLElement} */ (document.getElementById(id));
const frame = /** @type {HTMLElement} */ (document.querySelector('.hsbh-frame'));
const clear = byId('hsbh-clear');
const clearBody = byId('hsbh-clear-body');
const problems = byId('hsbh-problems');
const banner = byId('hsbh-connection');

const embedded = JSON.parse(document.getElementById('hsb-data')?.textContent ?? '{}');
const routes = embedded.routes ?? { recheckAll: '/profiles/recheck', recheckOne: '/profiles/{name}/recheck' };

/** @type {import('./health-words.js').Profile[]} */
let profiles = embedded.profiles ?? [];
/** @type {Set<string>} */
const rechecking = new Set();
let lastSig = '';

/** A button of the kit's own look. @param {string} label @param {Record<string, string>} data */
function button(label, data) {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'kp-button kp-button--secondary kp-button--sm';
    b.textContent = label;
    for (const [k, v] of Object.entries(data)) b.setAttribute(k, v);
    return b;
}

// ── the alerts ─────────────────────────────────────────────────────────────────────────────────────────────────────

/** The quiet line when nothing is wrong: who delivered, and who has not tried yet since the start. @param {import('./health-words.js').Profile[]} all */
function clearLine(all) {
    const working = all.filter((p) => p.state === 'working').map((p) => p.name);
    const waiting = all.filter((p) => p.state === 'starting').map((p) => p.name);
    const did = working.length === 0 ? '' : `${list(working)} delivered ${working.length === 1 ? 'its' : 'their'} last message.`;
    const not = waiting.length === 0 ? '' : `${list(waiting)} ${waiting.length === 1 ? 'has' : 'have'} not tried yet since the start; nothing is wrong.`;
    return [did, not].filter(Boolean).join(' ');
}

function paintHealth() {
    const unwell = profiles.flatMap((p) => {
        const w = wordsOf(p);
        return w ? [{ p, w }] : [];
    });
    const sig = JSON.stringify(unwell.map(({ p, w }) => [p.name, p.state, w.title, w.why]));
    if (sig !== lastSig) {
        lastSig = sig;
        setProblems(
            problems,
            unwell.map(({ p, w }) => ({
                key: p.name,
                severity: w.severity,
                what: w.title,
                why: w.why,
                remedy: w.remedy,
                command: w.command,
                action: button(w.fix, { 'data-hsbh-recheck': p.name }),
            })),
        );
        problems.hidden = unwell.length === 0;
    }
    const allWell = profiles.length > 0 && unwell.length === 0;
    clear.hidden = !allWell;
    if (allWell) clearBody.textContent = clearLine(profiles);
}

// ── actions ────────────────────────────────────────────────────────────────────────────────────────────────────────

/** @param {string} name */
async function recheck(name) {
    if (rechecking.has(name)) return;
    rechecking.add(name);
    const r = await post(routes.recheckOne.replace('{name}', encodeURIComponent(name)));
    rechecking.delete(name);
    if (r.ok) {
        document.dispatchEvent(new CustomEvent('hsb:refresh'));
        toast(`${name} back to starting`, { className: 'kp-toast kp-toast--success', ms: DONE_TOAST_MS, close: true });
    } else {
        const t = toast(r.line, { className: 'kp-toast kp-toast--error', sticky: true, action: { label: 'Try again', onClick: () => { t.dismiss(); void recheck(name); } } });
    }
}

const hub = hubBanner(banner, () => profiles, () => post(routes.recheckAll));

// ── wiring ─────────────────────────────────────────────────────────────────────────────────────────────────────────

attachProblems(frame);
attachConnection(frame);
attachArt(frame);

frame.addEventListener('click', (event) => {
    const again = /** @type {HTMLElement} */ (event.target).closest('[data-hsbh-recheck]');
    if (again) void recheck(again.getAttribute('data-hsbh-recheck') ?? '');
});
banner.addEventListener(CONNECTION_RETRY_EVENT, (event) => {
    void hub.retry(Boolean(/** @type {CustomEvent} */ (event).detail?.auto));
});
document.addEventListener('hsb:profiles', (event) => {
    const detail = /** @type {CustomEvent} */ (event).detail;
    // A failed poll says nothing about the profiles: what is shown stays until a poll answers.
    if (detail?.status !== 'ready') return;
    profiles = detail.profiles ?? [];
    paintHealth();
    hub.paint();
});

paintHealth();
hub.paint();
