// The status page's own views (kp-themes 11): profile cards or a map, the path of one profile, its settings and retry
// budget, and the recheck. The components are the package's, served by the kit as one bundle, /static/kp/dist/kp-themes.min.js; the modules beside
// this one draw them from the snapshot in hsb-state.js. This file is the entry point: it reads the snapshot, draws every
// view from it, and wires the buttons and the poll.
//
// Freshness rides on the kit's refresh menu in the page header (its kp-refresh event reads the snapshot again, as often
// as that menu says), and on a recheck. Without that menu (an older kit) a 10 s poll runs instead; see src/dashboard.rs.
// Layout: hsb-format.js (pure helpers), hsb-state.js (the snapshot and what the page shows), view-profiles.js,
// view-trace.js, view-settings.js, recheck.js, test-command.js.

import { attachEntityCards } from '/static/kp/dist/kp-themes.min.js';
import { attachMatrices } from '/static/kp/dist/kp-themes.min.js';
import { attachTraces } from '/static/kp/dist/kp-themes.min.js';
import { attachFlashButtons } from '/static/kp/dist/kp-themes.min.js';
import { attachMenuButtons } from '/static/kp/dist/kp-themes.min.js';
import { attachDataTables } from '/static/kp/dist/kp-themes.min.js';
import { attachFootprints } from '/static/kp/dist/kp-themes.min.js';
import { attachFormgridStages } from '/static/kp/dist/kp-themes.min.js';
import { attachSegmented, setSegmentedValues } from '/static/kp/dist/kp-themes.min.js';
import { $, all } from './hsb-format.js';
import { PICK_KEY, VIEW_KEY, hooks, routes, store } from './hsb-state.js';
import { PICK_EVENT } from './profile-pick.js';
import { AREA_RELOAD_EVENT, attachArea, attachProfileHover, refreshHover, watchEntityNames } from './profiles.js';
import { drawProfiles } from './view-profiles.js';
import { drawTrace } from './view-trace.js';
import { drawSettings } from './view-settings.js';
import { done, drawRecheck, recheckAll, recheckOne } from './recheck.js';
import { copyTest } from './test-command.js';

/** The poll when the page header has no refresh menu to ride on. */
const POLL_MS = 10_000;
/** The kit's refresh menu in the page header: one setting for the whole page. */
const headerRefresh = document.querySelector('.chassis-status__fresh .kp-refresh');

const area = /** @type {HTMLElement} */ ($('hsb-area'));
const grid = /** @type {HTMLElement} */ ($('hsb-grid'));
const map = /** @type {HTMLElement} */ ($('hsb-map'));
const traceEl = /** @type {HTMLElement} */ ($('hsb-trace'));
const settingsEl = /** @type {HTMLElement} */ ($('hsb-settings'));
const allButton = /** @type {HTMLButtonElement} */ ($('hsb-recheck-all'));
const viewSwitch = /** @type {HTMLElement} */ ($('hsb-view'));

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

/** Read again whenever the header's refresh menu does (its interval, its Refresh now), or every 10 s without it. */
function startPolling() {
    if (headerRefresh) {
        headerRefresh.addEventListener('kp-refresh', () => void refresh());
        return;
    }
    window.setInterval(() => !document.hidden && void refresh(), POLL_MS);
    document.addEventListener('visibilitychange', () => {
        if (!document.hidden) void refresh();
    });
}

/** Remember a choice in this browser; without storage it holds for this visit only. @param {string} key @param {string} value */
function remember(key, value) {
    try {
        localStorage.setItem(key, value);
    } catch {
        // No storage: the choice holds for this visit only.
    }
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
    attachSegmented(viewSwitch);
    setSegmentedValues(viewSwitch, [store.view]);
    viewSwitch.addEventListener('kp-segmented-change', (event) => {
        store.view = /** @type {CustomEvent} */ (event).detail?.value === 'map' ? 'map' : 'cards';
        remember(VIEW_KEY, store.view);
        drawProfiles(area, grid, map);
    });
    // The one profile choice (in Path) drives the path here; history.js follows the same event for the last 24 hours.
    document.addEventListener(PICK_EVENT, (event) => {
        const name = /** @type {CustomEvent} */ (event).detail?.name;
        if (typeof name !== 'string' || name === store.traceName) return;
        store.traceName = name;
        remember(PICK_KEY, name);
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
startPolling();

// The last 24 hours (history.js) reads again on every poll of this page (the hsb-poll event). If the module cannot load,
// the rest of the page carries on; the failure is logged so it is not invisible.
const historyUrl = new URL('history.js', import.meta.url);
historyUrl.search = new URL(import.meta.url).search;
import(historyUrl.href)
    .then((m) => m.start())
    .catch((error) => console.warn('http-switchboard: the last-24-hours module did not load; the rest of the page still refreshes.', error));
