// What a profile's state means, in words: the problem alert's what / why / remedy and its fix button, from the profile's
// own shape (kyu or HTTP source, URL or kyu destination). Pure: no DOM, no state. A profile that is only `starting` has
// nothing wrong with it, so it has no words here: its card says so quietly.

/** @typedef {{ name: string, state: 'working' | 'starting' | 'failing' | 'denied' | 'hub-down', received: number, failed: number,
 *   source: { kind: string, label: string, sub: string, path: string | null }, sink: { kind: string, label: string, path: string | null } }} Profile */
/** @typedef {{ severity: 'critical' | 'warning', title: string, why: string, remedy: string, command?: string, fix: string }} Words */

export const hasHubSource = (/** @type {Profile} */ p) => p.source.kind === 'kyu';
export const subscriptionOf = (/** @type {Profile} */ p) => p.source.sub.replace(/^subscription /, '');

/** @param {Profile} p @returns {Words | null} */
export function wordsOf(p) {
    const kyu = hasHubSource(p);
    const topic = p.source.path ?? p.source.label;
    const sinkTopic = p.sink.path ?? p.sink.label;
    switch (p.state) {
        case 'denied':
            return {
                severity: 'critical',
                title: `${p.name}: the kyu hub refused our credentials`,
                why: kyu
                    ? `The hub did not accept the switchboard’s token for the subscription “${subscriptionOf(p)}”, so it cannot read the topic ${topic}.`
                    : `The hub did not accept the switchboard’s token, so it cannot publish to the topic ${sinkTopic}.`,
                remedy: 'Re-issue the switchboard’s kyu token, then recheck the profile.',
                fix: `Recheck ${p.name}`,
            };
        case 'hub-down':
            return {
                severity: 'critical',
                title: `${p.name}: the kyu hub cannot be reached`,
                why: kyu ? `The switchboard reads ${topic} from the hub, and the hub does not answer.` : `The switchboard publishes to ${sinkTopic} on the hub, and the hub does not answer.`,
                remedy: 'Check that the kyu hub is running.',
                command: 'systemctl status kyu',
                fix: `Recheck ${p.name}`,
            };
        case 'failing': {
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
        default:
            return null;
    }
}

/** "a and b", "a, b and c". @param {string[]} names */
export const list = (names) =>
    names.length < 3 ? names.join(' and ') : `${names.slice(0, -1).join(', ')} and ${names.at(-1)}`;
