// The one POST of the status page: the rechecks (recheck.js, health.js) send it and read the answer the same way, so the
// sentence for an unreachable service and the reading of a refusal live here once.

export const UNREACHABLE = 'Could not reach HTTPSwitchboard. Check that the service is running, then try again.';

/** The kit's refusal shape `{ error, remedy }` as one sentence. @param {any} body */
const refusalLine = (body) => [body.error, body.remedy].filter(Boolean).join('. ') + (body.remedy ? '.' : '');

/**
 * POST to a route and read the answer. A refusal comes back in the kit's `{ error, remedy }` shape.
 * @param {string} url
 * @returns {Promise<{ ok: true, body: any } | { ok: false, line: string }>}
 */
export async function post(url) {
    try {
        const res = await fetch(url, { method: 'POST', headers: { accept: 'application/json' } });
        /** @type {any} */
        let body = null;
        try {
            body = await res.json();
        } catch (error) {
            console.warn(`http-switchboard: ${url} answered HTTP ${res.status} without JSON`, error); // The status below decides what the person reads.
        }
        if (res.ok && body) return { ok: true, body };
        if (body?.error) return { ok: false, line: refusalLine(body) };
        return { ok: false, line: res.ok ? UNREACHABLE : `HTTP ${res.status}. ${UNREACHABLE}` };
    } catch (error) {
        console.error(`http-switchboard: ${url} could not be reached`, error);
        return { ok: false, line: UNREACHABLE }; // The request never got an answer: the service is not reachable.
    }
}
