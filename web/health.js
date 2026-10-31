// The top of the status page (kp-themes 11): the attention band, the problem alerts and the kyu hub's connection banner.
//
// It draws from the snapshot dashboard.js already polls: dashboard.js announces every refresh as `hsb:profiles` on the
// document, and `hsb:refresh` asks it for one more. There is no second poll. Words describe a profile from its own
// shape (kyu or HTTP source, URL or kyu destination); nothing here sees a header, a payload or a webhook path.
//
// Layout: health-words.js (a state in words), health-hub.js (the hub banner); this file draws the band and the alerts
// and does the rechecks.

import { setAttention, attachAttention } from '/static/kp/dist/kp-themes.min.js';
import { setProblems, attachProblems, PROBLEM_DISMISS_EVENT } from '/static/kp/dist/kp-themes.min.js';
import { attachConnection, CONNECTION_RETRY_EVENT } from '/static/kp/dist/kp-themes.min.js';
import { attachArt } from '/static/kp/dist/kp-themes.min.js';
import { setStrings } from '/static/kp/dist/kp-themes.min.js';
import { toast } from '/static/kp/dist/kp-themes.min.js';
import { list, wordsOf } from './health-words.js';
import { hubBanner } from './health-hub.js';

const UNREACHABLE = 'Could not reach HTTPSwitchboard. Check that the service is running, then try again.';
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
const band = byId('hsbh-band');
const pair = byId('hsbh-pair');
const clear = byId('hsbh-clear');
const clearBody = byId('hsbh-clear-body');
const problems = byId('hsbh-problems');
const banner = byId('hsbh-connection');

const embedded = JSON.parse(document.getElementById('hsb-data')?.textContent ?? '{}');
const routes = embedded.routes ?? { recheckAll: '/profiles/recheck', recheckOne: '/profiles/{name}/recheck' };

/** @type {import('./health-words.js').Profile[]} */
let profiles = embedded.profiles ?? [];
/** Profiles whose "has not tried yet" note the reader closed; it comes back if the profile turns unhealthy and starting again. @type {Set<string>} */
const closed = new Set();
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

// ── the band and the alerts ────────────────────────────────────────────────────────────────────────────────────────

function paintHealth() {
    for (const name of [...closed]) if (profiles.find((p) => p.name === name)?.state !== 'starting') closed.delete(name);
    const unwell = profiles.flatMap((p) => {
        const w = wordsOf(p);
        return w ? [{ p, w }] : [];
    });
    const shown = unwell.filter(({ p }) => !closed.has(p.name));
    const sig = JSON.stringify(shown.map(({ p, w }) => [p.name, p.state, w.title, w.text, w.why]));
    if (sig !== lastSig) {
        lastSig = sig;
        setAttention(
            band,
            unwell.map(({ p, w }) => ({
                key: p.name,
                severity: w.severity,
                title: w.title,
                text: w.text,
                action: button('See the fix', { 'data-hsbh-fix': p.name }),
            })),
        );
        setProblems(
            problems,
            shown.map(({ p, w }) => ({
                key: p.name,
                severity: w.severity,
                what: w.title,
                why: w.why,
                remedy: w.remedy,
                command: w.command,
                action: button(w.fix, { 'data-hsbh-recheck': p.name }),
                dismissible: p.state === 'starting',
            })),
        );
        problems.hidden = shown.length === 0;
        pair.style.setProperty('--hsbh-rows', String(Math.max(unwell.length, 1)));
    }
    const allWell = profiles.length > 0 && unwell.length === 0;
    clear.hidden = !allWell;
    if (allWell) {
        clearBody.textContent = profiles.length === 1 ? `${profiles[0]?.name} delivered its last message.` : profiles.length === 2 ? `${list(profiles.map((p) => p.name))} delivered their last message.` : `All ${profiles.length} profiles delivered their last message.`;
    }
}

// ── actions ────────────────────────────────────────────────────────────────────────────────────────────────────────

/** @param {string} url @returns {Promise<{ ok: true } | { ok: false, line: string }>} */
async function post(url) {
    try {
        const res = await fetch(url, { method: 'POST', headers: { accept: 'application/json' } });
        if (res.ok) return { ok: true };
        /** @type {any} */
        let body = null;
        try {
            body = await res.json();
        } catch {
            // Not JSON: the status decides.
        }
        const line = body?.error ? [body.error, body.remedy].filter(Boolean).join('. ') + (body.remedy ? '.' : '') : `HTTP ${res.status}. ${UNREACHABLE}`;
        return { ok: false, line };
    } catch {
        return { ok: false, line: UNREACHABLE };
    }
}

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

/** "See the fix": the profile's alert, in view and focused; a closed note has no alert, so the profiles are shown. @param {string} name */
function showFix(name) {
    const alert = [...problems.querySelectorAll('[data-kp-key]')].find((el) => el.getAttribute('data-kp-key') === name);
    const target = /** @type {HTMLElement | null} */ (alert ?? document.getElementById('hsb-grid'));
    if (!target) return;
    const reduced = window.matchMedia('(prefers-reduced-motion: reduce)').matches;
    target.scrollIntoView({ behavior: reduced ? 'auto' : 'smooth', block: 'center' });
    const focus = /** @type {HTMLElement | null} */ (alert?.querySelector('[data-hsbh-recheck]') ?? null);
    focus?.focus({ preventScroll: true });
}

const hub = hubBanner(banner, () => profiles, () => post(routes.recheckAll));

// ── wiring ─────────────────────────────────────────────────────────────────────────────────────────────────────────

attachAttention(frame);
attachProblems(frame);
attachConnection(frame);
attachArt(frame);

frame.addEventListener('click', (event) => {
    const target = /** @type {HTMLElement} */ (event.target);
    const fix = target.closest('[data-hsbh-fix]');
    if (fix) return showFix(fix.getAttribute('data-hsbh-fix') ?? '');
    const again = target.closest('[data-hsbh-recheck]');
    if (again) void recheck(again.getAttribute('data-hsbh-recheck') ?? '');
});
problems.addEventListener(PROBLEM_DISMISS_EVENT, (event) => {
    const key = /** @type {CustomEvent} */ (event).detail?.key;
    if (typeof key !== 'string') return;
    closed.add(key);
    lastSig = '';
    paintHealth();
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
