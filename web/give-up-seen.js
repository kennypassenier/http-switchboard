// Which given-up messages a person has already dealt with. The server counts them since the service started and a recheck
// does not reset that count, so "given up since the last recheck" is the count now minus the count at the last recheck of
// that profile, kept in this browser. A restart of the service empties the server's count: reconcileGivenUpSeen resets a memory above it.

const KEY = 'hsb-given-up-seen';

/** @returns {Record<string, number>} */
function read() {
    try {
        const raw = JSON.parse(localStorage.getItem(KEY) ?? '{}');
        return raw && typeof raw === 'object' ? raw : {};
    } catch {
        return {}; // No storage, or not JSON: nothing is known to be seen, so everything is shown.
    }
}

/** @param {Record<string, number>} seen */
function write(seen) {
    try {
        localStorage.setItem(KEY, JSON.stringify(seen));
    } catch {
        // No storage: the notice comes back after a reload, which is the safe side.
    }
}

/** How many given-up messages of a profile are new since its last recheck. @param {string} name @param {number} total */
export function unseenGivenUp(name, total) {
    const seen = read()[name] ?? 0;
    return total >= seen ? total - seen : total; // A smaller count is only seen before the next reconcile; it shows all.
}

/**
 * Called on every ready snapshot: forget profiles that are gone, and reset a memory above the server's count (a restart
 * of the service, or a profile removed and added again, starts its count from zero; the old memory would hide new ones).
 * @param {{ name: string, given_up?: number | null }[]} profiles
 */
export function reconcileGivenUpSeen(profiles) {
    const seen = read();
    const kept = /** @type {Record<string, number>} */ ({});
    for (const p of profiles) {
        const was = seen[p.name];
        if (was === undefined) continue;
        kept[p.name] = was > (p.given_up ?? 0) ? 0 : was;
    }
    const same = Object.keys(seen).length === Object.keys(kept).length && Object.entries(kept).every(([k, v]) => seen[k] === v);
    if (!same) write(kept);
}

/** A recheck was done: everything given up so far counts as seen. @param {string} name @param {number} total */
export function markGivenUpSeen(name, total) {
    write({ ...read(), [name]: total });
}
