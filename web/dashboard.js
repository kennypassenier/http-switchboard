// The status page's own views (kp-themes 11): profile cards or a map, the path of one profile, its settings and retry
// budget, and the recheck. The components are the package's, served by the kit at /static/kp/js/; the modules beside
// this one draw them from the snapshot in hsb-state.js. This file is the entry point: it reads the snapshot, draws every
// view from it, and wires the buttons and the poll.
//
// Freshness is a light poll (every 10 s while the tab is visible, and straight after a recheck); see src/dashboard.rs.
// Layout: hsb-format.js (pure helpers), hsb-state.js (the snapshot and what the page shows), view-profiles.js,
// view-trace.js, view-settings.js, recheck.js, test-command.js.

import { attachEntityCards } from '/static/kp/js/entity-card.js';
import { attachMatrices } from '/static/kp/js/heat.js';
import { attachTraces } from '/static/kp/js/path-trace.js';
import { attachFlashButtons } from '/static/kp/js/flash.js';
import { attachMenuButtons } from '/static/kp/js/menu-button.js';
import { attachDataTables } from '/static/kp/js/datatable.js';
import { attachFootprints } from '/static/kp/js/footprint.js';
import { attachFormgridStages } from '/static/kp/js/forms.js';
import { $, all } from './hsb-format.js';
import { VIEW_KEY, hooks, routes, store } from './hsb-state.js';
import { AREA_RELOAD_EVENT, attachArea, attachProfileHover, refreshHover, watchEntityNames } from './profiles.js';
import { drawProfiles } from './view-profiles.js';
import { drawTrace } from './view-trace.js';
import { drawSettings } from './view-settings.js';
import { done, drawRecheck, recheckAll, recheckOne } from './recheck.js';
import { copyTest } from './test-command.js';

/** The poll when history.js (and its refresh chooser) cannot load. */
const POLL_MS = 10_000;
/** The poll in seconds; history.js takes it over with the refresh chooser (0 = paused). */
let pollSeconds = POLL_MS / 1000;
/** @type {number | undefined} */
let pollTimer;

const area = /** @type {HTMLElement} */ ($('hsb-area'));
const grid = /** @type {HTMLElement} */ ($('hsb-grid'));
const map = /** @type {HTMLElement} */ ($('hsb-map'));
const traceEl = /** @type {HTMLElement} */ ($('hsb-trace'));
const settingsEl = /** @type {HTMLElement} */ ($('hsb-settings'));
const allButton = /** @type {HTMLButtonElement} */ ($('hsb-recheck-all'));

// ── data ───────────────────────────────────────────────────────────────────────────────────────────────────────────

function drawAll() {
    // The health section above (web/health.js) draws from the same snapshot: one poll for the page.
    document.dispatchEvent(new CustomEvent('hsb:profiles', { detail: { profiles: store.profiles, status: store.status } }));
    drawProfiles(area, grid, map);
    drawTrace(traceEl);
    drawSettings(settingsEl);
    drawRecheck();
    refreshHover();
}

let inFlight = false;
/** Read the snapshot again; a failed read turns the page to its failed faces. @returns {Promise<boolean | undefined>} */
async function refresh() {
    if (inFlight) return;
    inFlight = true;
    try {
        const res = await fetch(routes.snapshot, { headers: { accept: 'application/json' } });
        if (!res.ok || !(res.headers.get('content-type') ?? '').includes('json')) throw new Error(`HTTP ${res.status}`);
        store.profiles = (await res.json()).profiles ?? [];
        store.status = 'ready';
    } catch {
        store.status = 'error'; // Every view shows its failed face with a Try again; the next poll tries once more.
    } finally {
        inFlight = false;
    }
    drawAll();
    // Other cards of the page (the reload history) ride on this poll rather than run a timer of their own.
    document.dispatchEvent(new CustomEvent('hsb-poll'));
    return store.status === 'ready';
}

/** Load again: loading for a moment, then the data. */
async function reload() {
    store.status = 'loading';
    drawAll();
    await refresh();
}

/** @param {number} seconds how often to read again; 0 pauses the poll. */
function setPoll(seconds) {
    pollSeconds = seconds;
    window.clearInterval(pollTimer);
    pollTimer = seconds > 0 ? window.setInterval(() => !document.hidden && void refresh(), seconds * 1000) : undefined;
}

// ── wiring ─────────────────────────────────────────────────────────────────────────────────────────────────────────

/** @param {string} name */
const tryTranslation = (name) => location.assign(`/translate?profile=${encodeURIComponent(name)}`);

function attachComponents() {
    attachEntityCards(document);
    attachMatrices(document);
    attachTraces(document);
    attachMenuButtons(document);
    attachDataTables(document);
    attachFlashButtons(document);
    attachFootprints(document);
    attachFormgridStages(document);
    attachArea(area);
    attachProfileHover(
        [area, ...all('.hsb-trace'), settingsEl, ...all('.hsb-table')].filter((root, i, list) => list.indexOf(root) === i),
        (name) => store.profiles.find((p) => p.name === name),
    );
    watchEntityNames(grid);
}

function wireViews() {
    for (const b of all('[data-hsb-view]')) {
        b.addEventListener('click', () => {
            store.view = b.getAttribute('data-hsb-view') === 'map' ? 'map' : 'cards';
            try {
                localStorage.setItem(VIEW_KEY, store.view);
            } catch {
                // No storage: the choice holds for this visit only.
            }
            drawProfiles(area, grid, map);
        });
    }
    $('hsb-trace-pick')?.addEventListener('click', (event) => {
        const b = /** @type {HTMLElement} */ (event.target).closest('button[data-hsb-trace]');
        if (!b) return;
        store.traceName = /** @type {HTMLElement} */ (b).dataset.hsbTrace ?? store.traceName;
        drawTrace(traceEl);
    });
}

function wireActions() {
    grid.addEventListener('kp-entity-action', (event) => {
        const { key, id } = /** @type {CustomEvent} */ (event).detail;
        if (id === 'recheck') void recheckOne(key);
        else if (id === 'try') tryTranslation(key);
        else void copyTest(key, done);
    });
    allButton.addEventListener('click', () => void recheckAll(allButton));
    document.addEventListener('kp-menu-select', (event) => {
        const detail = /** @type {CustomEvent} */ (event).detail;
        const name = detail?.item?.closest('[data-hsb-row]')?.getAttribute('data-hsb-row');
        if (!name) return;
        if (detail.value === 'recheck') void recheckOne(name);
        else if (detail.value === 'copy') void copyTest(name, done);
        else if (detail.value === 'try') tryTranslation(name);
    });
}

function wireRetries() {
    area.addEventListener(AREA_RELOAD_EVENT, () => void reload());
    area.addEventListener('kp-card-retry', () => void reload());
    map.addEventListener('kp-uses-retry', () => void reload());
    traceEl.addEventListener('kp-trace-retry', () => void reload());
    document.querySelector('[data-kp-datatable][data-kp-key="hsb-profiles"]')?.addEventListener('kp-datatable-retry', () => void reload());
    document.addEventListener('hsb:refresh', () => void refresh());
}

hooks.drawAll = drawAll;
hooks.refresh = refresh;
attachComponents();
wireViews();
wireActions();
wireRetries();

// The first answer is read like every later one: the cards, the table and the strips load until it comes.
void reload();
setPoll(pollSeconds);
document.addEventListener('visibilitychange', () => {
    if (!document.hidden && pollSeconds > 0) void refresh();
});

// The last 24 hours (history.js): its refresh chooser sets the poll for the whole page. If the module cannot load, the
// ten-second poll above simply stays; the failure is logged so it is not invisible.
const historyUrl = new URL('history.js', import.meta.url);
historyUrl.search = new URL(import.meta.url).search;
import(historyUrl.href)
    .then((m) => m.start({ setPoll, refresh }))
    .catch((error) => console.warn('http-switchboard: the last-24-hours module did not load; the page polls every 10 s instead.', error));
