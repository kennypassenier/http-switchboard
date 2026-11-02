// What the profiles say about the kyu hub, with no drawing: the verdict (offline, refused, online, unknown, none), the
// profiles the hub holds up and the sentence that names them. health-hub.js draws the banner from these.

import { hubCaused, list, touchesHub } from './health-words.js';

/** @typedef {import('./health-words.js').Profile} Profile */

/**
 * What the profiles that read from the hub say about it: a verdict, or 'unknown' while they have not spoken since a recheck.
 * @param {Profile[]} profiles @returns {'offline' | 'refused' | 'online' | 'unknown' | 'none'}
 */
export function hubVerdict(profiles) {
    const kyu = profiles.filter(touchesHub);
    if (kyu.length === 0) return 'none';
    if (kyu.some((p) => p.state === 'hub-down')) return 'offline';
    if (kyu.some((p) => p.state === 'denied')) return 'refused';
    if (kyu.every((p) => p.state === 'working' || p.state === 'failing')) return 'online';
    return 'unknown';
}

/** The profiles the hub holds up, named once in the banner instead of one alert each. @param {Profile[]} profiles */
export const waitingNames = (profiles) => profiles.filter(hubCaused).map((p) => p.name);

/** "a and b wait for the hub." @param {string[]} names */
export const waitsSentence = (names) => `${list(names)} ${names.length === 1 ? 'waits' : 'wait'} for the hub.`;

/** The banner's why for a fault. @param {'offline' | 'refused'} verdict @param {string[]} names */
export function whyOf(verdict, names) {
    const who = waitsSentence(names);
    return verdict === 'offline' ? who : `The hub did not accept the switchboard’s token, so ${who}`;
}
