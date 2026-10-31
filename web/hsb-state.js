// The status page's shared state: the snapshot the server embeds (#hsb-data), the routes it names, and what the page
// currently shows. One object, so every view module reads the same answer and the refresh in dashboard.js replaces it in
// one place. The snapshot carries no header value, no payload and no webhook path, so nothing drawn from it can leak one.

import { $ } from './hsb-format.js';

/** The choice between the cards and the map, remembered per browser (synced by the kit's preferences). */
export const VIEW_KEY = 'hsb-profiles-view';
export const UNREACHABLE = 'Could not reach HTTPSwitchboard. Check that the service is running, then try again.';

export const HEALTH_WORD = { working: 'working', starting: 'starting', failing: 'failing', denied: 'denied', 'hub-down': 'hub down' };
export const HEALTH_TONE = { working: 'success', starting: 'info', failing: 'destructive', denied: 'destructive', 'hub-down': 'warning' };

/** @typedef {{ kind: string, label: string, sub: string, path: string | null }} End */
/** @typedef {{ name: string, template: string, state: keyof typeof HEALTH_WORD, received: number, delivered: number, failed: number, avg_ms: number | null, last_success_unix: number | null,
 *   source: End, sink: End, content_type: string, timeout_ms: number, retries: number, pauses_ms: number[], budget_ms: number, lease_ms: number | null,
 *   max_attempts: number | null }} Profile */

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

export const store = {
    /** @type {Profile[]} */
    profiles: first,
    /** The first read is in flight from the first frame: the page holds no answer until it comes. @type {'ready' | 'loading' | 'error'} */
    status: /** @type {'ready' | 'loading' | 'error'} */ ('loading'),
    view: rememberedView(),
    traceName: first[0]?.name ?? '',
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
