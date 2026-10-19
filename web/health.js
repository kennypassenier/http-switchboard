// The top of the status page (kp-themes 11): the attention band, the problem alerts and the kyu hub's connection banner.
//
// It draws from the snapshot dashboard.js already polls: dashboard.js announces every refresh as `hsb:profiles` on the
// document, and `hsb:refresh` asks it for one more. There is no second poll. Words describe a profile from its own
// shape (kyu or HTTP source, URL or kyu destination); nothing here sees a header, a payload or a webhook path.

import { setAttention, attachAttention } from '/static/kp/js/attention.js';
import { setProblems, attachProblems, PROBLEM_DISMISS_EVENT } from '/static/kp/js/problems.js';
import { setConnection, attachConnection, CONNECTION_RETRY_EVENT } from '/static/kp/js/connection.js';
import { attachArt } from '/static/kp/js/art.js';
import { setStrings } from '/static/kp/js/strings.js';
import { toast } from '/static/kp/js/overlays.js';

const UNREACHABLE = 'Could not reach HTTPSwitchboard. Check that the service is running, then try again.';
/** How long the banner waits for the pump to speak again after a recheck, in ms. */
const SETTLE_FIRST_MS = 1500;
const SETTLE_STEP_MS = 2000;
const SETTLE_TRIES = 4;

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

/** @typedef {{ name: string, state: 'working' | 'starting' | 'failing' | 'denied' | 'hub-down', received: number, failed: number,
 *   source: { kind: string, label: string, sub: string, path: string | null }, sink: { kind: string, label: string, path: string | null } }} Profile */
/** @typedef {{ severity: 'critical' | 'warning' | 'info', title: string, text: string, why: string, remedy: string, command?: string, fix: string }} Words */

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

/** @type {Profile[]} */
let profiles = embedded.profiles ?? [];
/** Profiles whose "has not tried yet" note the reader closed; it comes back if the profile turns unhealthy and starting again. @type {Set<string>} */
const closed = new Set();
/** @type {Set<string>} */
const rechecking = new Set();
let lastSig = '';

// ── what a profile's state means, in words ─────────────────────────────────────────────────────────────────────────

const hasHubSource = (/** @type {Profile} */ p) => p.source.kind === 'kyu';
const subscriptionOf = (/** @type {Profile} */ p) => p.source.sub.replace(/^subscription /, '');

/** @param {Profile} p @returns {Words | null} */
function wordsOf(p) {
    const kyu = hasHubSource(p);
    const topic = p.source.path ?? p.source.label;
    const sinkTopic = p.sink.path ?? p.sink.label;
    switch (p.state) {
        case 'denied':
            return {
                severity: 'critical',
                title: `${p.name}: the kyu hub refused our credentials`,
                text: kyu ? `No message can be fetched from ${topic} until the token is re-issued.` : `Nothing can be published to ${sinkTopic} until the token is re-issued.`,
                why: kyu
                    ? `The hub did not accept the switchboard’s token for the subscription “${subscriptionOf(p)}”, so it cannot read the topic ${topic}.`
                    : `The hub did not accept the switchboard’s token, so it cannot publish to the topic ${sinkTopic}.`,
                remedy: 'Re-issue the switchboard’s kyu token, then recheck the profiles.',
                fix: 'Recheck profiles',
            };
        case 'hub-down':
            return {
                severity: 'critical',
                title: `${p.name}: the kyu hub cannot be reached`,
                text: kyu ? `Nothing arrives on ${topic} while the hub does not answer.` : `Nothing can be published to ${sinkTopic} while the hub does not answer.`,
                why: kyu ? `The switchboard reads ${topic} from the hub, and the hub does not answer.` : `The switchboard publishes to ${sinkTopic} on the hub, and the hub does not answer.`,
                remedy: 'Check that kyu is running on CT 109.',
                command: 'systemctl status kyu',
                fix: 'Check again',
            };
        case 'failing': {
            const toKyu = p.sink.kind === 'kyu';
            const where = toKyu ? 'the kyu hub' : p.sink.label;
            const tally = `${p.failed.toLocaleString('en-GB')} of ${p.received.toLocaleString('en-GB')} received messages have failed so far.`;
            return {
                severity: 'warning',
                title: toKyu ? `${p.name}: publishing to the kyu hub is failing` : `${p.name}: deliveries to ${where} are failing`,
                text: kyu ? 'Messages wait on the hub and are tried again.' : `The last delivery from ${p.source.label} was not accepted.`,
                why: `${where === 'the kyu hub' ? 'The hub' : where} did not accept the last delivery. ${kyu ? 'The messages wait on the hub and are tried again. ' : ''}${tally}`,
                remedy: toKyu ? 'Check the kyu hub, then read the switchboard’s log.' : 'Check that Home Assistant is up, then read the switchboard’s log.',
                command: 'journalctl -u http-switchboard',
                fix: 'Check again',
            };
        }
        case 'starting':
            return {
                severity: 'info',
                title: `${p.name} has not tried yet`,
                text: 'Nothing was tried since the service started or since the last recheck.',
                why: 'Nothing was tried since the service started or since the last recheck.',
                remedy: 'Nothing to fix: it turns to working with the next message.',
                fix: 'Recheck profiles',
            };
        default:
            return null;
    }
}

/** A button of the kit's own look. @param {string} label @param {Record<string, string>} data */
function button(label, data) {
    const b = document.createElement('button');
    b.type = 'button';
    b.className = 'kp-button kp-button--secondary kp-button--sm';
    b.textContent = label;
    for (const [k, v] of Object.entries(data)) b.setAttribute(k, v);
    return b;
}

const list = (/** @type {string[]} */ names) =>
    names.length < 3 ? names.join(' and ') : `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`;

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

// ── the kyu hub banner ─────────────────────────────────────────────────────────────────────────────────────────────

/** What the profiles that read from the hub say about it: a verdict, or 'unknown' while they have not spoken since a recheck.
 *  @returns {'offline' | 'refused' | 'online' | 'unknown' | 'none'} */
function hubVerdict() {
    const kyu = profiles.filter(hasHubSource);
    if (kyu.length === 0) return 'none';
    if (kyu.some((p) => p.state === 'hub-down')) return 'offline';
    if (kyu.some((p) => p.state === 'denied')) return 'refused';
    if (kyu.every((p) => p.state === 'working' || p.state === 'failing')) return 'online';
    return 'unknown';
}

const refusedWhy = () => {
    const subs = [...new Set(profiles.filter((p) => hasHubSource(p) && p.state === 'denied').map(subscriptionOf))];
    return `The hub did not accept the switchboard’s token for the subscription ${subs.map((s) => `“${s}”`).join(', ')}. Nothing was lost.`;
};

/** What the banner was last told; the kit restarts its countdown on every call, so an unchanged verdict is not repeated. */
let applied = 'online';
let settling = false;

/** @param {'offline' | 'refused' | 'online'} verdict */
function tell(verdict) {
    applied = verdict;
    if (verdict === 'refused') setConnection(banner, 'refused', { why: refusedWhy() });
    else setConnection(banner, verdict);
}

function paintHub() {
    if (settling) return;
    const v = hubVerdict();
    if (v === 'none' || v === 'unknown' || v === applied) return;
    tell(v);
}

/** @param {number} ms */
const sleep = (ms) => new Promise((resolve) => window.setTimeout(resolve, ms));

/** Ask dashboard.js for a fresh snapshot and wait for it (or give up after 4 s). */
function repoll() {
    return new Promise((resolve) => {
        const done = () => {
            document.removeEventListener('hsb:profiles', done);
            window.clearTimeout(timer);
            resolve(undefined);
        };
        const timer = window.setTimeout(done, 4000);
        document.addEventListener('hsb:profiles', done);
        document.dispatchEvent(new CustomEvent('hsb:refresh'));
    });
}

/** A pressed Retry now rechecks the profiles first; the kit's own timed tries only look again. @param {boolean} auto */
async function retryHub(auto) {
    settling = true;
    applied = 'reconnecting';
    if (!auto) await post(routes.recheckAll);
    await sleep(SETTLE_FIRST_MS);
    /** @type {string} */
    let v = 'unknown';
    for (let i = 0; i < SETTLE_TRIES; i++) {
        await repoll();
        v = hubVerdict();
        if (v !== 'unknown') break;
        await sleep(SETTLE_STEP_MS);
    }
    settling = false;
    // Still silent after a recheck: no fault is known any more, so the banner stands down; a real outage speaks again at once.
    tell(v === 'offline' || v === 'refused' ? v : 'online');
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
        toast(`${name} back to starting`, { className: 'kp-toast kp-toast--success', ms: 6000, close: true });
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
    void retryHub(Boolean(/** @type {CustomEvent} */ (event).detail?.auto));
});
document.addEventListener('hsb:profiles', (event) => {
    const detail = /** @type {CustomEvent} */ (event).detail;
    // A failed poll says nothing about the profiles: what is shown stays until a poll answers.
    if (detail?.status !== 'ready') return;
    profiles = detail.profiles ?? [];
    paintHealth();
    paintHub();
});

paintHealth();
paintHub();
