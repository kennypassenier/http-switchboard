// Which given-up messages a person has already dealt with. The server counts them since the service started and a recheck
// does not reset that count, so "given up since the last recheck" is the count now minus the count at the last recheck of
// that profile, kept in this browser. A restart of the service empties the server's count: a smaller count resets the memory.

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
    return total >= seen ? total - seen : total;
}

/** A recheck was done: everything given up so far counts as seen. @param {string} name @param {number} total */
export function markGivenUpSeen(name, total) {
    write({ ...read(), [name]: total });
}
