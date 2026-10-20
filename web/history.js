// The last 24 hours on the status page (feat-dash-kp11-30..39): four key figures with their trend by the hour, one uptime
// strip of 24 hour cells per profile, and the refresh chooser that sets how often the whole page reads again.
//
// Started by dashboard.js (`start`), which hands over its poll: the chooser drives both the profiles snapshot and the
// history from here on. The components are the package's, served by the kit at /static/kp/js/. The data is
// /history.json (counts, times and a cause word per hour; no payload, header or URL), first embedded as #hsb-history-data.
// The history is in memory on the server and starts empty at every service start.

import { setCardState, attachCardStates } from '/static/kp/js/card-state.js';
import { attachTrendCharts, setTrendData } from '/static/kp/js/chart.js';
import { attachSparklines, attachKpiToggles, attachKpiStrips, KPI_TOGGLE_EVENT } from '/static/kp/js/kpi.js';
import { attachTicks } from '/static/kp/js/tick.js';
import { attachUptimes, setUptime, uptimePercent } from '/static/kp/js/uptime.js';
import { attachRefresh, REFRESH_EVENT } from '/static/kp/js/auto-refresh.js';

const TILES_KEY = 'hsb-history-tiles';
const PRESS_KEY = 'hsb-history-press';
const HOUR = 3_600_000;
/** The seconds the page opens reading again at; the kit's own default, adjustable by the reader and remembered. */
const REFRESH_START = 30;

const $ = (/** @type {string} */ id) => document.getElementById(id);
const all = (/** @type {string} */ selector, root = document) => /** @type {HTMLElement[]} */ ([...root.querySelectorAll(selector)]);
const count = (/** @type {number} */ n) => n.toLocaleString('en-GB');

const zone = new Intl.DateTimeFormat('en-GB', {
    timeZone: 'Europe/Brussels',
    year: 'numeric',
    month: '2-digit',
    day: '2-digit',
    hour: '2-digit',
    minute: '2-digit',
    hourCycle: 'h23',
});
/** dd/mm/yyyy HH:MM on the Brussels clock, the page's one clock. @param {number} ms */
const stamp = (ms) => {
    const p = Object.fromEntries(zone.formatToParts(ms).map((x) => [x.type, x.value]));
    return `${p.day}/${p.month}/${p.year} ${p.hour}:${p.minute}`;
};
/** HH:MM only. @param {number} ms */
const clockOf = (ms) => stamp(ms).slice(11);
const esc = (/** @type {string} */ text) => text.replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/**
 * @typedef {{ start_unix: number, received: number, delivered: number, failed: number, duration_ms: number,
 *   state: 'up' | 'degraded' | 'down' | 'none', cause: string | null }} Hour
 * @typedef {{ at_unix: number, ok: boolean, ms: number | null }} Recent
 * @typedef {{ name: string, hours: Hour[], recent: Recent[], last_failure_unix: number | null }} ProfileHistory
 * @typedef {{ now_unix: number, since_unix: number, profiles: ProfileHistory[] }} History
 */

const METRICS = [
    { key: 'received', label: 'Received', series: (/** @type {Hour} */ h) => h.received },
    { key: 'delivered', label: 'Delivered', series: (/** @type {Hour} */ h) => h.delivered },
    { key: 'failed', label: 'Failed', series: (/** @type {Hour} */ h) => h.failed },
    { key: 'duration', label: 'Average duration', series: (/** @type {Hour} */ h) => (h.delivered ? Math.round(h.duration_ms / h.delivered) : NaN) },
];

/** The four figures of a profile over its recorded hours. @param {ProfileHistory} p @param {History} data */
function totals(p, data) {
    const hours = p.hours.filter((h) => h.state !== 'none');
    const sum = (/** @type {(h: Hour) => number} */ f) => hours.reduce((n, h) => n + f(h), 0);
    const r = sum((h) => h.received);
    const d = sum((h) => h.delivered);
    const f = sum((h) => h.failed);
    const ms = sum((h) => h.duration_ms);
    const slow = hours.filter((h) => h.delivered > 0).sort((a, b) => b.duration_ms / b.delivered - a.duration_ms / a.delivered)[0];
    return { r, d, f, avg: d ? Math.round(ms / d) : 0, slow, lastFail: p.last_failure_unix, since: data.since_unix, windowStart: p.hours[0]?.start_unix ?? 0 };
}

/** @param {string} key @param {ReturnType<typeof totals>} t */
function words(key, t) {
    if (key === 'received') return t.since * 1000 > t.windowStart * 1000 ? `since ${stamp(t.since * 1000)}` : 'in the last 24 hours';
    if (key === 'delivered') return t.r ? `${uptimePercent(t.d / t.r)}% of received` : 'nothing received yet';
    if (key === 'failed') return t.f > 0 && t.lastFail ? `last ${stamp(t.lastFail * 1000)}` : 'none in 24 hours';
    return t.slow ? `slowest hour ${clockOf(t.slow.start_unix * 1000)}, ${Math.round(t.slow.duration_ms / t.slow.delivered)} ms` : 'no delivery yet';
}

/** The words after a number. @param {string} key @param {number} n */
const unitOf = (key, n) => (key === 'duration' ? 'ms' : n === 1 ? 'message' : 'messages');

/** @param {string} name @param {string} key @param {number} v */
const spoken = (name, key, v) => (key === 'duration' ? `${v} ms` : `${v} ${unitOf(key, v)}`);

/** @param {ProfileHistory} p @param {(typeof METRICS)[number]} m */
function points(p, m) {
    return p.hours.filter((h) => h.state !== 'none' && Number.isFinite(m.series(h))).map((h) => /** @type {[number, number]} */ ([h.start_unix * 1000, m.series(h)]));
}

/** The kit's failure-cause words, one per cause. @param {Hour} h */
function causeLines(h) {
    if (h.state === 'none') return ['History was not recorded yet: the service had not started.'];
    switch (h.cause) {
        case 'hub-down':
            return ['The kyu hub could not be reached during this hour.'];
        case 'denied':
            return ['The hub refused the credentials during this hour.'];
        case 'all-failed':
            return ['Every delivery in this hour failed.'];
        case 'failing':
            return [h.failed > 0 ? `${count(h.failed)} of ${count(h.received)} deliveries failed.` : 'The profile was failing during this hour.'];
        default:
            return h.received === 0 ? ['Nothing came in during this hour.'] : [];
    }
}

/** @param {Hour} h */
function stateWord(h) {
    if (h.state === 'none') return 'No data';
    if (h.cause === 'hub-down') return 'Hub down';
    if (h.cause === 'denied') return 'Denied';
    if (h.state === 'up') return 'Working';
    return 'Failing';
}

// ── the page ──────────────────────────────────────────────────────────────────────────────────────────────────────────

/** @param {{ setPoll: (seconds: number) => void, refresh: () => Promise<boolean | void> }} dashboard */
export function start(dashboard) {
    const embedded = JSON.parse($('hsb-history-data')?.textContent ?? 'null');
    if (!embedded?.history) return;
    const route = embedded.route ?? '/history.json';

    /** @type {History} */
    let data = embedded.history;
    let skew = data.now_unix * 1000 - Date.now();
    /** @type {'ready' | 'loading' | 'error'} */
    let state = 'loading'; // the first read is in flight from the first frame; the embedded history only shapes the strips
    let profile = data.profiles[0]?.name ?? '';
    let failedOnly = false;
    const store = {
        get: (/** @type {string} */ key, /** @type {string[]} */ allowed) => {
            try {
                const v = localStorage.getItem(key);
                return v && allowed.includes(v) ? v : allowed[0];
            } catch {
                return allowed[0];
            }
        },
        set: (/** @type {string} */ key, /** @type {string} */ value) => {
            try {
                localStorage.setItem(key, value);
            } catch {
                // The choice holds for this visit only.
            }
        },
    };
    let look = store.get(TILES_KEY, ['trend', 'spark']);
    let press = store.get(PRESS_KEY, ['info', 'hover']);

    const card = /** @type {HTMLElement} */ ($('hsb-h-card'));
    const strip = /** @type {HTMLElement} */ ($('hsb-h-strip'));
    const recent = /** @type {HTMLElement} */ ($('hsb-h-recent'));
    const strips = /** @type {HTMLElement} */ ($('hsb-h-strips'));
    const pick = /** @type {HTMLElement} */ ($('hsb-h-pick'));
    if (!card || !strip || !recent || !strips || !pick) return;

    const current = () => data.profiles.find((p) => p.name === profile) ?? data.profiles[0];

    // ── key figures ───────────────────────────────────────────────────────────────────────────────────────────────

    /** @type {(() => void)[]} */
    let detachers = [];
    let built = '';

    /** @param {(typeof METRICS)[number]} m @param {number} n @param {ReturnType<typeof totals>} t @param {ProfileHistory} p */
    function tileHtml(m, n, t, p) {
        const value = { received: t.r, delivered: t.d, failed: t.f, duration: t.avg }[/** @type {'received'} */ (m.key)];
        const head = `<span class="kp-kpi__label">${m.label}</span>`;
        const num = `<span class="kp-kpi__value" data-kp-tick data-hsb-value="${m.key}">${count(value)}<small>${unitOf(m.key, value)}</small></span>`;
        const sub = `<span class="kp-kpi__trend" data-hsb-words="${m.key}">${esc(words(m.key, t))}</span>`;
        if (look === 'trend') {
            return `<div class="kp-kpi kp-kpi--trend" data-hsb-metric="${m.key}">${head}
                <a class="kp-kpi__link" href="#hsb-h-strips" title="Open ${m.label} by the hour"><span class="kp-kpi__link-word">Hourly</span> ↗</a>
                ${num}${sub}
                <figure class="kp-kpi__chart" data-kp-chart="spark" data-kp-spark-head="none" data-kp-spark-axis="relative" data-kp-spark-readout="float" aria-label="${m.label}, last 24 hours" style="--kp-chart-series: var(--chart-${n})"></figure>
            </div>`;
        }
        if (m.key === 'failed') {
            return `<button type="button" class="kp-kpi kp-kpi--toggle" aria-pressed="${failedOnly}" data-kp-tone="destructive" data-hsb-metric="${m.key}">${head}
                <span class="kp-kpi__hint" aria-hidden="true"><span data-kp-when="off">Filter</span><span data-kp-when="on">Filtering ×</span></span>
                ${num}${sub}</button>`;
        }
        const spark = points(p, m).map((x) => x[1]).join(' ');
        return `<div class="kp-kpi" data-hsb-metric="${m.key}">${head}${num}${sub}<svg class="kp-kpi__spark" data-kp-spark="${spark}" aria-hidden="true"></svg></div>`;
    }

    function paintRecent() {
        const p = current();
        const list = (p?.recent ?? []).filter((m) => (failedOnly ? !m.ok : true)).slice(0, failedOnly ? 6 : 4);
        const title = $('hsb-h-recent-title');
        if (title) title.textContent = failedOnly ? `Failed deliveries in the last 24 hours (${list.length})` : 'Latest deliveries';
        if (list.length === 0) {
            const li = document.createElement('li');
            li.className = 'hsb-h-row hsb-h-row--none';
            li.textContent = failedOnly ? 'No failed delivery in the last 24 hours.' : 'No message has gone through this profile since the last start.';
            recent.replaceChildren(li);
            return;
        }
        recent.replaceChildren(
            ...list.map((m) => {
                const li = document.createElement('li');
                li.className = 'hsb-h-row';
                li.innerHTML = `<time datetime="${new Date(m.at_unix * 1000).toISOString()}">${stamp(m.at_unix * 1000)}</time>
                    <span class="kp-badge ${m.ok ? 'kp-badge--success' : 'kp-badge--destructive'}">${m.ok ? 'Delivered' : 'Failed'}</span>
                    <span class="hsb-h-row__ms">${m.ms === null ? 'no answer' : `${count(m.ms)} ms`}</span>
                    <span class="hsb-h-row__why">${m.ok ? 'Accepted by the destination' : 'Not accepted by the destination, retries used up'}</span>`;
                return li;
            }),
        );
    }

    function buildTiles() {
        for (const d of detachers) d();
        const p = current();
        if (!p) return;
        const t = totals(p, data);
        strip.innerHTML = METRICS.map((m, i) => tileHtml(m, i + 1, t, p)).join('');
        strip.setAttribute('aria-label', `Key figures of ${p.name}`);
        detachers = [attachTicks(strip), attachSparklines(strip), attachKpiToggles(strip)];
        if (look === 'trend') {
            detachers.push(
                attachTrendCharts(strip, {
                    now: () => Date.now() + skew,
                    format: (/** @type {number} */ v, /** @type {any} */ where) => {
                        const key = /** @type {Element | null} */ (where instanceof Element ? where : null)?.closest('[data-hsb-metric]')?.getAttribute('data-hsb-metric');
                        return key ? spoken(p.name, key, Math.round(v)) : String(Math.round(v));
                    },
                }),
            );
            for (const tile of all('[data-hsb-metric]', strip)) {
                const m = /** @type {(typeof METRICS)[number]} */ (METRICS.find((x) => x.key === tile.dataset.hsbMetric));
                const figure = tile.querySelector('.kp-kpi__chart');
                if (figure) setTrendData(figure, { points: points(p, m), step: HOUR });
            }
        }
        built = `${p.name}|${look}`;
        paintRecent();
    }

    /** New numbers for the tiles that stand: they count to the new value, the trend gets its new points. */
    function updateTiles() {
        const p = current();
        if (!p) return;
        const t = totals(p, data);
        const values = { received: t.r, delivered: t.d, failed: t.f, duration: t.avg };
        for (const m of METRICS) {
            const v = values[/** @type {'received'} */ (m.key)];
            const el = strip.querySelector(`[data-hsb-value="${m.key}"]`);
            const node = [...(el?.childNodes ?? [])].find((n) => n.nodeType === 3);
            if (node) node.nodeValue = count(v);
            const unit = el?.querySelector('small');
            if (unit) unit.textContent = unitOf(m.key, v);
            const w = strip.querySelector(`[data-hsb-words="${m.key}"]`);
            if (w) w.textContent = words(m.key, t);
            if (look === 'trend') {
                const fig = strip.querySelector(`[data-hsb-metric="${m.key}"] .kp-kpi__chart`);
                if (fig) setTrendData(fig, { points: points(p, m), step: HOUR });
            } else {
                strip.querySelector(`[data-hsb-metric="${m.key}"] .kp-kpi__spark`)?.setAttribute('data-kp-spark', points(p, m).map((x) => x[1]).join(' '));
            }
        }
        paintRecent();
    }

    function paintPicker() {
        const row = $('hsb-h-pick-row');
        if (row) row.hidden = data.profiles.length < 2;
        pick.innerHTML = `${data.profiles
            .map((p) => `<button type="button" class="kp-button kp-button--sm" aria-pressed="${p.name === profile}" data-hsb-profile="${esc(p.name)}">${esc(p.name)}</button>`)
            .join('')}`;
    }

    function paintCard() {
        const p = current();
        const t = p ? totals(p, data) : null;
        const title = $('hsb-h-title');
        if (title) title.textContent = p ? `Last 24 hours of ${p.name}` : 'Last 24 hours';
        const read = $('hsb-h-read');
        if (read) read.textContent = `read ${stamp(data.now_unix * 1000)}`;
        const body = $('hsb-h-body');
        if (body) body.textContent = `Counted per hour since the service started recording on ${stamp(data.since_unix * 1000)}.`;
        if (state === 'loading') {
            setCardState(card, 'loading');
        } else if (state === 'error') {
            setCardState(card, 'error', { title: 'Could not read the last 24 hours', text: 'The service did not answer. The counters keep running; nothing is lost.' });
        } else if (!p || !t || t.r === 0) {
            setCardState(card, 'empty', {
                title: 'Nothing received yet',
                text: `Since the service started at ${stamp(data.since_unix * 1000)} no message has gone through ${p ? p.name : 'a profile'}. The figures appear with the first one.`,
            });
        } else {
            setCardState(card, 'ready');
            if (built !== `${p.name}|${look}`) buildTiles();
            else updateTiles();
        }
    }

    // ── uptime strips ─────────────────────────────────────────────────────────────────────────────────────────────

    /** @type {Map<string, HTMLElement>} */
    const stripEls = new Map();

    function paintStrips() {
        for (const p of data.profiles) {
            let el = stripEls.get(p.name);
            if (!el) {
                el = document.createElement('div');
                el.className = 'kp-uptime';
                el.setAttribute('data-kp-uptime', '');
                el.setAttribute('data-kp-uptime-count', '24');
                el.setAttribute('data-kp-uptime-label', p.name);
                strips.append(el);
                stripEls.set(p.name, el);
                attachUptimes(strips);
            }
            const periods = p.hours.map((h) => {
                const at = h.start_unix * 1000;
                const label = stamp(at).slice(0, 14) + '00';
                const figures =
                    h.state === 'none'
                        ? 'No data: the service was not recording yet'
                        : h.received
                          ? `${count(h.delivered)} of ${count(h.received)} delivered${h.delivered ? `, ${count(Math.round(h.duration_ms / h.delivered))} ms on average` : ''}`
                          : 'nothing to deliver';
                return {
                    key: new Date(at).toISOString(),
                    label,
                    state: h.state,
                    tip: h.state === 'none' ? `${label} · No data` : `${label} · ${stateWord(h)} · ${figures}`,
                    details: h.state === 'none' ? causeLines(h) : [`Received ${count(h.received)}, delivered ${count(h.delivered)}, failed ${count(h.failed)}.`, ...causeLines(h)],
                };
            });
            if (press === 'info') el.setAttribute('data-kp-uptime-open', 'info');
            else el.removeAttribute('data-kp-uptime-open');
            setUptime(el, {
                label: p.name,
                periods,
                state: state === 'loading' ? 'loading' : state === 'error' ? 'error' : 'ready',
                count: 24,
                from: '24 hours ago',
                to: 'Now',
                error: 'Could not read the hourly history of this profile.',
                open: press === 'info' ? 'info' : undefined,
            });
        }
    }

    function paintAll() {
        paintPicker();
        paintCard();
        paintStrips();
        for (const b of all('[data-hsb-tiles]')) b.setAttribute('aria-pressed', String(b.dataset.hsbTiles === look));
        for (const b of all('[data-hsb-press]')) b.setAttribute('aria-pressed', String(b.dataset.hsbPress === press));
        $('hsb-h-tiles')?.removeAttribute('hidden');
        $('hsb-h-press')?.removeAttribute('hidden');
    }

    // ── reading ───────────────────────────────────────────────────────────────────────────────────────────────────

    /**
     * Read the history. A background read that fails keeps what is drawn (the chooser says the refresh failed and
     * retries); only a first read or "Try again" turns the card into the error.
     * @param {boolean} quiet
     */
    async function load(quiet) {
        try {
            const res = await fetch(route, { headers: { accept: 'application/json' } });
            if (!res.ok || !(res.headers.get('content-type') ?? '').includes('json')) throw new Error(`HTTP ${res.status}`);
            data = await res.json();
            skew = data.now_unix * 1000 - Date.now();
            if (!data.profiles.some((p) => p.name === profile)) profile = data.profiles[0]?.name ?? '';
            state = 'ready';
            paintAll();
            return true;
        } catch {
            if (!quiet) {
                state = 'error';
                paintAll();
            }
            return false;
        }
    }

    // ── wiring ────────────────────────────────────────────────────────────────────────────────────────────────────

    attachCardStates(card);
    attachKpiStrips(strip);
    pick.addEventListener('click', (event) => {
        const b = /** @type {HTMLElement} */ (event.target).closest('button[data-hsb-profile]');
        if (!b) return;
        profile = /** @type {HTMLElement} */ (b).dataset.hsbProfile ?? profile;
        failedOnly = false;
        paintAll();
    });
    for (const b of all('[data-hsb-tiles]')) {
        b.addEventListener('click', () => {
            look = b.dataset.hsbTiles === 'spark' ? 'spark' : 'trend';
            store.set(TILES_KEY, look);
            failedOnly = false;
            paintAll();
        });
    }
    for (const b of all('[data-hsb-press]')) {
        b.addEventListener('click', () => {
            press = b.dataset.hsbPress === 'hover' ? 'hover' : 'info';
            store.set(PRESS_KEY, press);
            paintAll();
        });
    }
    strip.addEventListener(KPI_TOGGLE_EVENT, (event) => {
        failedOnly = Boolean(/** @type {CustomEvent} */ (event).detail?.pressed);
        paintRecent();
    });
    card.addEventListener('kp-card-retry', () => {
        state = 'loading';
        paintAll();
        void load(false);
    });

    // The refresh chooser (default mode; Segments): it reads the profiles snapshot and the history together, and
    // takes the page's poll over from dashboard.js.
    // The chooser is the form grid's own refresh bar: its choice, its freshness line and Refresh now stand on the tracks of
    // the rows around it, and the page's label (data-kp-refresh-label) in the first track.
    const chooser = $('hsb-h-refresh');
    if (chooser) {
        attachRefresh($('hsb-h-grid') ?? chooser);
        dashboard.setPoll(0);
        chooser.addEventListener(REFRESH_EVENT, async (event) => {
            const detail = /** @type {CustomEvent} */ (event).detail;
            const [profiles, history] = await Promise.all([dashboard.refresh(), load(true)]);
            detail.done(profiles !== false && history);
        });
    }

    paintAll();
    void load(false);
}
