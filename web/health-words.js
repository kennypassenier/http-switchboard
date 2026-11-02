// What a profile's state means, in words: the problem alert's what / why / remedy and its fix button, from the profile's
// own shape (kyu or HTTP source, URL or kyu destination). Pure: no DOM, no state. A profile that is only `starting` has
// nothing wrong with it, and one the kyu hub holds up is named by the hub banner (health-hub.js), so neither has words here.

import { stamp } from './history-model.js';
import { unseenGivenUp } from './give-up-seen.js';

/** @typedef {{ name: string, state: 'working' | 'starting' | 'failing' | 'denied' | 'hub-down', received: number, failed: number,
 *   source: { kind: string, label: string, sub: string, path: string | null }, sink: { kind: string, label: string, path: string | null },
 *   attempts?: number, given_up?: number, last_given_up_unix?: number | null, last_given_up_reason?: string | null }} Profile */
/** @typedef {{ severity: 'critical' | 'warning', title: string, why: string, remedy: string, command?: string, fix: string }} Words */

export const hasHubSource = (/** @type {Profile} */ p) => p.source.kind === 'kyu';
/** Whether the profile reads from or publishes to the kyu hub. */
export const touchesHub = (/** @type {Profile} */ p) => p.source.kind === 'kyu' || p.sink.kind === 'kyu';
/** A problem whose cause is the hub itself (it does not answer, or refuses our token). The hub banner says it once, for all the
 *  profiles it holds up; they get no alert of their own. */
export const hubCaused = (/** @type {Profile} */ p) => touchesHub(p) && (p.state === 'hub-down' || p.state === 'denied');

/** The alert of a profile whose own destination is failing; null for everything else (the hub banner and the cards say the rest).
 *  @param {Profile} p @returns {Words | null} */
export function wordsOf(p) {
    if (p.state !== 'failing') return null;
    const kyu = hasHubSource(p);
    const toKyu = p.sink.kind === 'kyu';
    const where = toKyu ? 'the kyu hub' : p.sink.label;
    const tally = `${p.failed.toLocaleString('en-GB')} of ${p.received.toLocaleString('en-GB')} received messages have failed so far.`;
    return {
        severity: 'warning',
        title: toKyu ? `${p.name}: publishing to the kyu hub is failing` : `${p.name}: deliveries to ${where} are failing`,
        why: `${where === 'the kyu hub' ? 'The hub' : where} did not accept the last delivery. ${kyu ? 'The messages wait on the hub and are tried again. ' : ''}${tally}`,
        remedy: toKyu ? 'Check the kyu hub, then read the switchboard’s log.' : 'Check that Home Assistant is up, then read the switchboard’s log.',
        command: 'journalctl -u http-switchboard',
        fix: `Recheck ${p.name}`,
    };
}

/** The notice of a profile that gave up messages (it stopped trying to deliver them) since its last recheck; null when none.
 *  @param {Profile} p @returns {Words | null} */
export function giveUpWordsOf(p) {
    const n = unseenGivenUp(p.name, p.given_up ?? 0);
    if (n === 0) return null;
    const when = p.last_given_up_unix ? ` The last one was on ${stamp(p.last_given_up_unix * 1000)}.` : '';
    const reason = p.last_given_up_reason ? `The reason: ${p.last_given_up_reason}.` : 'No reason was recorded.';
    return {
        severity: 'warning',
        title: `${p.name}: ${n.toLocaleString('en-GB')} ${n === 1 ? 'message was' : 'messages were'} given up`,
        why: `${reason}${when}`,
        remedy: 'Check the destination, then recheck the profile; messages that were given up are not delivered later.',
        command: 'journalctl -u http-switchboard',
        fix: `Recheck ${p.name}`,
    };
}

/** "a and b", "a, b and c". @param {string[]} names */
export const list = (names) =>
    names.length < 3 ? names.join(' and ') : `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`;
