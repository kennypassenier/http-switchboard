// The profiles view: one entity card per profile, or the map of sources to destinations (the choice is remembered).
// Both live in one card whose loading, empty and failed faces are the package's card states (web/profiles.js setArea).

import { setEntityGrid } from '/static/kp/dist/kp-themes.min.js';
import { setMatrix } from '/static/kp/dist/kp-themes.min.js';
import { all, count, percent, plural } from './hsb-format.js';
import { HEALTH_TONE, HEALTH_WORD, hasHub, store } from './hsb-state.js';
import { paintFresh, setArea } from './profiles.js';

/** Below this share delivered the card's meter turns to a warning. */
const WARN_BELOW_SHARE = 0.99;

/** @param {import('./hsb-state.js').Profile} p */
function flagsOf(p) {
    /** @type {{ label: string, tone?: 'success' | 'warning' | 'destructive' | 'info' }[]} */
    const flags = [{ label: p.source.kind === 'kyu' ? 'kyu topic' : 'HTTP POST' }, { label: p.sink.kind === 'url' ? 'HTTP webhook' : 'kyu topic' }];
    if (p.state === 'failing') flags.push({ label: hasHub(p) ? 'messages waiting on the hub' : 'destination refusing', tone: 'destructive' });
    if (p.state === 'denied') flags.push({ label: 'credentials refused', tone: 'destructive' });
    if (p.state === 'hub-down') flags.push({ label: 'hub unreachable', tone: 'warning' });
    return flags;
}

/** One profile as an entity card. @param {import('./hsb-state.js').Profile} p */
function entityOf(p) {
    const settled = p.delivered + p.failed;
    const share = settled === 0 ? null : p.delivered / settled;
    return {
        key: p.name,
        name: p.name,
        state: { label: HEALTH_WORD[p.state], tone: /** @type {'success' | 'warning' | 'destructive' | 'info'} */ (HEALTH_TONE[p.state]) },
        figures: [
            { value: count(p.delivered), label: 'Delivered' },
            { value: count(p.failed), label: 'Failed' },
            { value: p.avg_ms === null ? 'none yet' : `${count(p.avg_ms)} ms`, label: 'Average' },
        ],
        flags: flagsOf(p),
        meters: [
            {
                label: 'Delivered',
                value: share,
                ...(share !== null && share < WARN_BELOW_SHARE ? { tone: /** @type {'warning'} */ ('warning') } : {}),
                text: share === null ? 'Nothing tried yet' : `${percent(share)} delivered`,
            },
        ],
        actions: [
            { id: 'recheck', label: 'Recheck this profile' },
            { id: 'copy-test', label: 'Copy test command' },
            { id: 'try', label: 'Try a translation' },
        ],
    };
}

/** @param {import('./hsb-state.js').Profile} p @param {'source' | 'dest'} side */
function toneOf(p, side) {
    if (p.state === 'starting') return 'muted';
    if (side === 'source') return p.state === 'denied' || p.state === 'hub-down' ? 'bad' : 'ok';
    return p.state === 'failing' ? 'bad' : p.state === 'denied' || p.state === 'hub-down' ? 'muted' : 'ok';
}

/** The map: sources down the side, destinations across, a line where a profile joins them. */
function mapSpec() {
    const { profiles } = store;
    return {
        corner: 'Source to destination',
        rows: profiles.map((p) => ({
            key: `src-${p.name}`,
            label: p.source.label,
            sub: `profile ${p.name}, ${p.source.sub}`,
            tone: toneOf(p, 'source'),
            dashed: p.state === 'starting',
        })),
        columns: profiles.map((p) => ({ key: `dst-${p.name}`, label: p.sink.label, sub: `for ${p.name}`, tone: toneOf(p, 'dest') })),
        cell: (/** @type {string} */ row, /** @type {string} */ column) =>
            row.slice(4) === column.slice(4) ? { tone: 'ok', label: 'delivers' } : { tone: 'none' },
        countLabel: (/** @type {number} */ n) => plural(n, 'profile'),
        sourcesLabel: 'Sources',
        usersLabel: 'Destinations',
    };
}

/** @param {HTMLElement} area @param {HTMLElement} grid @param {HTMLElement} map */
export function drawProfiles(area, grid, map) {
    const { profiles, status, view } = store;
    grid.hidden = view !== 'cards';
    map.hidden = view !== 'map';
    for (const b of all('[data-hsb-view]')) b.setAttribute('aria-pressed', String(b.getAttribute('data-hsb-view') === view));
    if (status !== 'ready') {
        setArea(area, status);
        return;
    }
    if (profiles.length === 0) {
        grid.replaceChildren();
        grid.removeAttribute('role');
        setMatrix(map, { view: 'lines', rows: [], columns: [], state: 'empty', message: 'No profiles yet' });
        setArea(area, 'empty');
        return;
    }
    setEntityGrid(grid, profiles.map(entityOf));
    for (const p of profiles) {
        const card = grid.querySelector(`[data-kp-entity-key="${CSS.escape(p.name)}"]`);
        if (card instanceof HTMLElement) paintFresh(card, p);
    }
    setMatrix(map, { ...mapSpec(), view: 'lines' });
    setArea(area, 'ready');
}
