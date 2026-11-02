// The kyu hub's connection banner: offline when a profile that reads from the hub cannot reach it, refused when the hub
// turns our token down, online otherwise. Driven by the same snapshot as the band; a pressed "Retry now" rechecks the
// profiles first and then waits for the pump to speak again before it changes its verdict.

import { setConnection } from '/static/kp/dist/kp-themes.min.js';
import { hasHubSource, subscriptionOf } from './health-words.js';

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
    /** What the profiles that read from the hub say about it: a verdict, or 'unknown' while they have not spoken since a recheck.
     *  @returns {'offline' | 'refused' | 'online' | 'unknown' | 'none'} */
    function verdict() {
        const kyu = profilesNow().filter(hasHubSource);
        if (kyu.length === 0) return 'none';
        if (kyu.some((p) => p.state === 'hub-down')) return 'offline';
        if (kyu.some((p) => p.state === 'denied')) return 'refused';
        if (kyu.every((p) => p.state === 'working' || p.state === 'failing')) return 'online';
        return 'unknown';
    }

    const refusedWhy = () => {
        const subs = [...new Set(profilesNow().filter((p) => hasHubSource(p) && p.state === 'denied').map(subscriptionOf))];
        return `The hub did not accept the switchboard’s token for the subscription ${subs.map((s) => `“${s}”`).join(', ')}. Nothing was lost.`;
    };

    /** What the banner was last told; the kit restarts its countdown on every call, so an unchanged verdict is not repeated. */
    let applied = 'online';
    let settling = false;

    /** @param {'offline' | 'refused' | 'online'} v */
    function tell(v) {
        applied = v;
        if (v === 'refused') setConnection(banner, 'refused', { why: refusedWhy() });
        else setConnection(banner, v);
    }

    function paint() {
        if (settling) return;
        const v = verdict();
        if (v === 'none' || v === 'unknown' || v === applied) return;
        tell(v);
    }

    /** A pressed Retry now rechecks the profiles first; the kit's own timed tries only look again. @param {boolean} auto */
    async function retry(auto) {
        settling = true;
        applied = 'reconnecting';
        if (!auto) await recheckAll();
        await sleep(SETTLE_FIRST_MS);
        /** @type {string} */
        let v = 'unknown';
        for (let i = 0; i < SETTLE_TRIES; i++) {
            await repoll();
            v = verdict();
            if (v !== 'unknown') break;
            await sleep(SETTLE_STEP_MS);
        }
        settling = false;
        // Still silent after a recheck: no fault is known any more, so the banner stands down; a real outage speaks again at once.
        tell(v === 'offline' || v === 'refused' ? v : 'online');
    }

    return { paint, retry };
}
