// The status page's shared state: the snapshot the server embeds (#hsb-data), the routes it names, and what the page
// currently shows. One object, so every view module reads the same answer and the refresh in dashboard.js replaces it in
// one place. The snapshot carries no header value, no payload and no webhook path, so nothing drawn from it can leak one.

import { $ } from './hsb-format.js';
import { PICK_EVENT } from './profile-pick.js';

/** The choice between the cards and the map, remembered per browser (synced by the kit's preferences). */
export const VIEW_KEY = 'hsb-profiles-view';
/** The profile the per-profile sections (Path, Last 24 hours) show, remembered per browser. */
export const PICK_KEY = 'hsb-profile';

export const HEALTH_WORD = { working: 'working', starting: 'starting', failing: 'failing', denied: 'hub refused', 'hub-down': 'hub offline' };
export const HEALTH_TONE = { working: 'success', starting: 'info', failing: 'destructive', denied: 'destructive', 'hub-down': 'warning' };

/** @typedef {{ kind: string, label: string, sub: string, path: string | null }} End */
/** @typedef {{ name: string, template: string, state: keyof typeof HEALTH_WORD, received: number, delivered: number, failed: number, avg_ms: number | null, last_success_unix: number | null,
 *   source: End, sink: End, content_type: string, timeout_ms: number, retries: number, pauses_ms: number[], budget_ms: number, lease_ms: number | null,
 *   max_attempts: number | null, attempts?: number, paused?: boolean, given_up?: number, last_given_up_unix?: number | null, last_given_up_reason?: string | null }} Profile */

const embedded = JSON.parse($('hsb-data')?.textContent ?? '{}');

/** The JSON routes this page reads and posts to; the server names them, these are the fallbacks. */
export const routes = embedded.routes ?? { snapshot: '/profiles.json', recheckAll: '/profiles/recheck', recheckOne: '/profiles/{name}/recheck' };

/** @returns {'cards' | 'map'} */
function rememberedView() {
    try {
        return localStorage.getItem(VIEW_KEY) === 'map' ? 'map' : 'cards';
    } catch {
        return 'cards'; // No storage: the default view stands.
    }
}

/** @type {Profile[]} */
const first = embedded.profiles ?? [];

/** The remembered profile while it still exists, else the first. */
function rememberedPick() {
    try {
        const name = localStorage.getItem(PICK_KEY);
        if (name && first.some((p) => p.name === name)) return name;
    } catch {
        // No storage: the first profile stands.
    }
    return first[0]?.name ?? '';
}

export const store = {
    /** @type {Profile[]} */
    profiles: first,
    /** The first read is in flight from the first frame: the page holds no answer until it comes. @type {'ready' | 'loading' | 'error'} */
    status: /** @type {'ready' | 'loading' | 'error'} */ ('loading'),
    view: rememberedView(),
    /** The profile Path and Last 24 hours show (one choice for both). */
    traceName: rememberedPick(),
    /** Profiles with a recheck in flight. @type {Set<string>} */
    rechecking: new Set(),
};

/** What the action modules call back into: dashboard.js fills these in at start. */
export const hooks = {
    drawAll: () => {},
    /** @returns {Promise<boolean | undefined>} */
    refresh: async () => undefined,
};

/** @param {Profile} p */
export const hasHub = (p) => p.source.kind === 'kyu';

/** @param {string} name */
export const profileNamed = (name) => store.profiles.find((p) => p.name === name);

/**
 * Choose the profile Path and Last 24 hours show: the one setter, so the store, the browser's memory and the listeners
 * (history.js follows PICK_EVENT) always move together. @param {string} name
 */
export function setPick(name) {
    store.traceName = name;
    try {
        localStorage.setItem(PICK_KEY, name);
    } catch (error) {
        console.warn('http-switchboard: the profile choice could not be remembered; it holds for this visit only', error);
    }
    document.dispatchEvent(new CustomEvent(PICK_EVENT, { detail: { name } }));
}

/**
 * The server's clock against this browser's, set from the server's `now_unix` on every snapshot (dashboard.js) and
 * history read (history.js). Every "N ago" line compares server timestamps with Date.now(), so they are moved by this skew.
 */
export const clock = { skewMs: 0 };

/** A server timestamp (unix seconds) as a moment on this browser's clock. @param {number} unix */
export const localMs = (unix) => unix * 1000 - clock.skewMs;
