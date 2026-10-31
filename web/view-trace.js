// The path of one profile: sender, door, translation and destination as a path trace that stops at the first hop that
// refuses. A picker chooses the profile when there are two or more.

import { setTrace, trace } from '/static/kp/dist/kp-themes.min.js';
import { $, all } from './hsb-format.js';
import { HEALTH_TONE, HEALTH_WORD, hasHub, store } from './hsb-state.js';
import { markName } from './profiles.js';

/** The four hops of one profile, each allowed, unknown (nothing tried yet) or refused. @param {import('./hsb-state.js').Profile} p */
function hopsOf(p) {
    const st = p.state;
    /** @type {'allowed' | 'unknown'} */
    const base = st === 'starting' ? 'unknown' : 'allowed';
    const door = st === 'denied' || st === 'hub-down' ? 'refused' : base;
    const last = st === 'failing' ? 'refused' : base;
    return [
        { title: 'Sender', note: hasHub(p) ? 'publishes to the hub' : 'holds a sender token', verdict: base },
        {
            title: p.source.label,
            note: st === 'denied' ? 'the hub refused our credentials' : st === 'hub-down' ? 'the hub cannot be reached' : p.source.sub,
            verdict: door,
            ...(door === 'refused' ? { rule: st === 'denied' ? 'Credentials refused' : 'Hub unreachable' } : {}),
        },
        { title: 'Translation', note: `${p.content_type} from the profile's template`, verdict: base },
        {
            title: p.sink.label,
            note: p.sink.kind === 'url' ? 'destination' : 'published to the hub',
            verdict: last,
            ...(last === 'refused' ? { rule: 'Receiver failing' } : {}),
        },
    ];
}

/** Keep one picker button per profile, in step with the snapshot, and mark the chosen one. */
function drawPicker() {
    const pick = $('hsb-trace-pick');
    if (!pick) return;
    const { profiles } = store;
    const row = $('hsb-trace-pick-row');
    if (row) row.hidden = profiles.length < 2;
    for (const old of all('button', pick)) if (!profiles.some((p) => p.name === old.dataset.hsbTrace)) old.remove();
    for (const p of profiles) {
        if (all('button', pick).some((b) => b.dataset.hsbTrace === p.name)) continue;
        const b = document.createElement('button');
        b.type = 'button';
        b.className = 'kp-button kp-button--sm';
        b.dataset.hsbTrace = p.name;
        b.textContent = p.name;
        markName(b, p.name);
        pick.append(b);
    }
    for (const b of all('button', pick)) b.setAttribute('aria-pressed', String(b.dataset.hsbTrace === store.traceName));
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
    drawPicker();
    const { profiles, status } = store;
    const p = profiles.find((x) => x.name === store.traceName) ?? profiles[0];
    if (p) store.traceName = p.name;
    drawHead(p);
    if (status === 'ready' && p) {
        setTrace(traceEl, hopsOf(p));
        return;
    }
    if (!trace(traceEl)) setTrace(traceEl, p ? hopsOf(p) : []);
    const state = status === 'loading' ? 'running' : status === 'error' ? 'error' : 'empty';
    trace(traceEl)?.setState(state, { message: status === 'error' ? 'The profile could not be read.' : 'No profiles yet.' });
}
