// The kyu hub's connection banner: offline when a profile that reads from the hub cannot reach it, refused when the hub
// turns our token down, online otherwise. Driven by the same snapshot as the band; a pressed "Retry now" rechecks the
// profiles first and then waits for the pump to speak again before it changes its verdict.

import { setConnection, setStrings } from '/static/kp/dist/kp-themes.min.js';
import { hubVerdict, waitingNames, waitsSentence, whyOf } from './hub-verdict.js';

/** The kit's neutral sentence while it tries to reach the hub; restored when no profile waits. */
const RECONNECTING_WHY = 'Trying to reach the hub now.';

/** How long the banner waits for the pump to speak again after a recheck, in ms. */
const SETTLE_FIRST_MS = 1500;
const SETTLE_STEP_MS = 2000;
const SETTLE_TRIES = 4;
/** How long one re-read of the snapshot may take before the banner stops waiting for it, in ms. */
const REPOLL_LIMIT_MS = 4000;

/** @param {number} ms */
const sleep = (ms) => new Promise((resolve) => window.setTimeout(resolve, ms));

/** Ask dashboard.js for a fresh snapshot and wait for it (a ready one; or give up after REPOLL_LIMIT_MS). */
function repoll() {
    return new Promise((resolve) => {
        const finish = () => {
            document.removeEventListener('hsb:profiles', onProfiles);
            window.clearTimeout(timer);
            resolve(undefined);
        };
        // A loading-state announcement is not an answer: wait for a ready one (a failed read ends at the limit).
        const onProfiles = (/** @type {Event} */ event) => {
            if (/** @type {CustomEvent} */ (event).detail?.status === 'ready') finish();
        };
        const timer = window.setTimeout(finish, REPOLL_LIMIT_MS);
        document.addEventListener('hsb:profiles', onProfiles);
        document.dispatchEvent(new CustomEvent('hsb:refresh'));
    });
}

/**
 * @param {HTMLElement} banner
 * @param {() => import('./health-words.js').Profile[]} profilesNow
 * @param {() => Promise<unknown>} recheckAll posts the recheck of every profile
 */
export function hubBanner(banner, profilesNow, recheckAll) {
    const verdict = () => hubVerdict(profilesNow());
    const waiting = () => waitingNames(profilesNow());

    /** What the banner was last told; the kit restarts its countdown on every call, so an unchanged verdict is not repeated. */
    let applied = 'online';
    /** The names the banner last listed, so a change of who waits is told again. */
    let told = '';
    let settling = false;
    /** True while a retry's settle loop runs: a second press waits for it instead of starting another. */
    let retrying = false;

    /** @param {'offline' | 'refused' | 'online'} v */
    function tell(v) {
        applied = v;
        told = waiting().join(',');
        if (v === 'online') setConnection(banner, v);
        else setConnection(banner, v, { why: whyOf(v, waiting()) });
    }

    /** The kit's own timed tries show "reconnecting" with a fixed sentence: it names who waits too, or is neutral. */
    function nameWaiting() {
        const names = waiting();
        setStrings({ connReconnectingWhy: names.length > 0 ? `${waitsSentence(names)} Trying to reach it now.` : RECONNECTING_WHY });
    }

    function paint() {
        nameWaiting();
        if (settling) return;
        const v = verdict();
        if (v === 'none' || v === 'unknown') return;
        if (v === applied && (v === 'online' || told === waiting().join(','))) return;
        tell(v);
    }

    /** Wait until the pump speaks again (or the tries run out) and give the verdict. @returns {Promise<string>} */
    async function settle() {
        await sleep(SETTLE_FIRST_MS);
        let v = 'unknown';
        for (let i = 0; i < SETTLE_TRIES; i++) {
            await repoll();
            v = verdict();
            if (v !== 'unknown') break;
            await sleep(SETTLE_STEP_MS);
        }
        return v;
    }

    /** A pressed Retry now rechecks the profiles first; the kit's own timed tries only look again. @param {boolean} auto */
    async function retry(auto) {
        if (retrying) return;
        retrying = true;
        settling = true;
        applied = 'reconnecting';
        try {
            if (!auto) await recheckAll();
            const v = await settle();
            // Still silent after a recheck: no fault is known any more, so the banner stands down; a real outage speaks again at once.
            tell(v === 'offline' || v === 'refused' ? v : 'online');
        } finally {
            settling = false;
            retrying = false;
        }
    }

    return { paint, retry };
}
