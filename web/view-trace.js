// The path of one profile: sender, door, translation and destination as a path trace that stops at the first hop that
// refuses. Above it stands the page's one profile choice (two or more profiles): it drives this path and the last 24
// hours below it (history.js listens for the same PICK_EVENT).

import { setStrings, setTrace, trace } from '/static/kp/dist/kp-themes.min.js';
import { $ } from './hsb-format.js';
import { HEALTH_TONE, HEALTH_WORD, hasHub, setPick, store } from './hsb-state.js';
import { markName } from './profiles.js';
import { paintPick } from './profile-pick.js';
import { keepBreakable } from './break-points.js';

/** The hop's title where a profile meets the kyu hub: the page says "the hub", never the topic or the subscription (Settings has those). */
const HUB_HOP = 'Kyu hub';

/** Why a hop refuses, for the summary line "Stops at the kyu hub: it does not answer." Set by `drawTrace` before it draws. */
let blockedWhy = '';

setStrings({ traceBlocked: (/** @type {string} */ hop) => `Stops at ${hop === HUB_HOP ? 'the kyu hub' : hop}${blockedWhy ? `: ${blockedWhy}` : ''}.` });

/** The four hops of one profile, each allowed, unknown (nothing tried yet) or refused. @param {import('./hsb-state.js').Profile} p */
function hopsOf(p) {
    const st = p.state;
    /** @type {'allowed' | 'unknown'} */
    const base = st === 'starting' ? 'unknown' : 'allowed';
    const door = st === 'denied' || st === 'hub-down' ? 'refused' : base;
    const last = st === 'failing' ? 'refused' : base;
    blockedWhy = st === 'denied' ? 'it refused our credentials' : st === 'hub-down' ? 'it does not answer' : last === 'refused' ? 'it refused the delivery' : '';
    return [
        { title: 'Sender', note: hasHub(p) ? 'publishes to the hub' : 'holds a sender token', verdict: base },
        {
            title: hasHub(p) ? HUB_HOP : p.source.label,
            note: st === 'denied' ? 'the hub refused our credentials' : st === 'hub-down' ? 'the hub does not answer' : hasHub(p) ? 'the messages are read from it' : p.source.sub,
            verdict: door,
        },
        { title: 'Translation', note: `${p.content_type} from the profile's template`, verdict: base },
        {
            title: p.sink.kind === 'url' ? p.sink.label : HUB_HOP,
            note: p.sink.kind === 'url' ? 'destination' : 'published to the hub',
            verdict: last,
        },
    ];
}

/** The page's one profile choice, in step with the snapshot; hidden while there is only one profile. */
function drawPicker() {
    const pick = $('hsb-trace-pick');
    if (!pick) return;
    const { profiles } = store;
    const row = $('hsb-trace-pick-row');
    if (row) row.hidden = profiles.length < 2;
    paintPick(pick, profiles.map((p) => p.name), store.traceName, 'Profile shown in Path and Last 24 hours');
}

/** The name and state badge above the trace. @param {import('./hsb-state.js').Profile | undefined} p */
function drawHead(p) {
    const name = $('hsb-trace-name');
    const badge = $('hsb-trace-badge');
    if (name) {
        name.textContent = p?.name ?? '';
        if (p) markName(name, p.name);
    }
    if (badge) {
        badge.textContent = p ? HEALTH_WORD[p.state] : '';
        badge.className = p ? `kp-badge kp-badge--${HEALTH_TONE[p.state]} hsb-badge` : 'kp-badge hsb-badge';
    }
}

/** @param {HTMLElement} traceEl */
export function drawTrace(traceEl) {
    // A hop's title may be an address ("host:8123"): it wraps after its separators only, never inside the port.
    if (!traceEl.dataset.hsbBreakable) {
        traceEl.dataset.hsbBreakable = '1';
        keepBreakable(traceEl, '.kp-steplist__title');
    }
    drawPicker();
    const { profiles, status } = store;
    const p = profiles.find((x) => x.name === store.traceName) ?? profiles[0];
    // The chosen profile is gone (a reload removed it): fall back through the one setter, so the memory and Last 24 hours follow.
    if (p && p.name !== store.traceName) setPick(p.name);
    drawHead(p);
    if (status === 'ready' && p) {
        setTrace(traceEl, hopsOf(p));
        return;
    }
    if (!trace(traceEl)) setTrace(traceEl, p ? hopsOf(p) : []);
    const state = status === 'loading' ? 'running' : status === 'error' ? 'error' : 'empty';
    trace(traceEl)?.setState(state, { message: status === 'error' ? 'The profile could not be read.' : 'No profiles yet.' });
}
