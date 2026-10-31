// The last 24 hours, the key figures: four package KPI tiles (trend tiles with their hourly chart by default, or plain
// sparkline tiles with Failed as a filter toggle), and the latest deliveries under them. The tiles are built once per
// profile and look, and on a later read only their numbers, words and points change, so a figure counts to its new
// value instead of being redrawn.

import { attachTrendCharts, setTrendData } from '/static/kp/js/chart.js';
import { attachSparklines, attachKpiToggles } from '/static/kp/js/kpi.js';
import { attachTicks } from '/static/kp/js/tick.js';
import { HOUR, METRICS, count, esc, points, spoken, stamp, totals, unitOf, words } from './history-model.js';

/** How many deliveries the list shows, normally and when filtered to the failed ones. */
const RECENT_SHOWN = 4;
const RECENT_FAILED_SHOWN = 6;

/** @typedef {import('./history.js').Context} Context */

/** @param {(typeof METRICS)[number]} m @param {number} n @param {ReturnType<typeof totals>} t @param {import('./history-model.js').ProfileHistory} p @param {Context} ctx */
function tileHtml(m, n, t, p, ctx) {
    const value = { received: t.r, delivered: t.d, failed: t.f, duration: t.avg }[/** @type {'received'} */ (m.key)];
    const head = `<span class="kp-kpi__label">${m.label}</span>`;
    const num = `<span class="kp-kpi__value" data-kp-tick data-hsb-value="${m.key}">${count(value)}<small>${unitOf(m.key, value)}</small></span>`;
    const sub = `<span class="kp-kpi__trend" data-hsb-words="${m.key}">${esc(words(m.key, t))}</span>`;
    if (ctx.look === 'trend') {
        return `<div class="kp-kpi kp-kpi--trend" data-hsb-metric="${m.key}">${head}
            <a class="kp-kpi__link" href="#hsb-h-strips" title="Open ${m.label} by the hour"><span class="kp-kpi__link-word">Hourly</span> ↗</a>
            ${num}${sub}
            <figure class="kp-kpi__chart" data-kp-chart="spark" data-kp-spark-head="none" data-kp-spark-axis="relative" data-kp-spark-readout="float" aria-label="${m.label}, last 24 hours" style="--kp-chart-series: var(--chart-${n})"></figure>
        </div>`;
    }
    if (m.key === 'failed') {
        return `<button type="button" class="kp-kpi kp-kpi--toggle" aria-pressed="${ctx.failedOnly}" data-kp-tone="destructive" data-hsb-metric="${m.key}">${head}
            <span class="kp-kpi__hint" aria-hidden="true"><span data-kp-when="off">Filter</span><span data-kp-when="on">Filtering ×</span></span>
            ${num}${sub}</button>`;
    }
    const spark = points(p, m).map((x) => x[1]).join(' ');
    return `<div class="kp-kpi" data-hsb-metric="${m.key}">${head}${num}${sub}<svg class="kp-kpi__spark" data-kp-spark="${spark}" aria-hidden="true"></svg></div>`;
}

/** One delivery as a row of the list. @param {import('./history-model.js').Recent} m */
function recentRow(m) {
    const li = document.createElement('li');
    li.className = 'hsb-h-row';
    li.innerHTML = `<time datetime="${new Date(m.at_unix * 1000).toISOString()}">${stamp(m.at_unix * 1000)}</time>
        <span class="kp-badge ${m.ok ? 'kp-badge--success' : 'kp-badge--destructive'}">${m.ok ? 'Delivered' : 'Failed'}</span>
        <span class="hsb-h-row__ms">${m.ms === null ? 'no answer' : `${count(m.ms)} ms`}</span>
        <span class="hsb-h-row__why">${m.ok ? 'Accepted by the destination' : 'Not accepted by the destination, retries used up'}</span>`;
    return li;
}

/** The latest deliveries, or only the failed ones while the Failed tile filters. @param {Context} ctx */
export function paintRecent(ctx) {
    const p = ctx.current();
    const list = (p?.recent ?? []).filter((m) => (ctx.failedOnly ? !m.ok : true)).slice(0, ctx.failedOnly ? RECENT_FAILED_SHOWN : RECENT_SHOWN);
    const title = document.getElementById('hsb-h-recent-title');
    if (title) title.textContent = ctx.failedOnly ? `Failed deliveries in the last 24 hours (${list.length})` : 'Latest deliveries';
    if (list.length === 0) {
        const li = document.createElement('li');
        li.className = 'hsb-h-row hsb-h-row--none';
        li.textContent = ctx.failedOnly ? 'No failed delivery in the last 24 hours.' : 'No message has gone through this profile since the last start.';
        ctx.el.recent.replaceChildren(li);
        return;
    }
    ctx.el.recent.replaceChildren(...list.map(recentRow));
}

/** Attach the trend charts and give each its hourly points. @param {Context} ctx @param {import('./history-model.js').ProfileHistory} p */
function attachTrends(ctx, p) {
    const strip = ctx.el.strip;
    ctx.figures.detachers.push(
        attachTrendCharts(strip, {
            now: () => Date.now() + ctx.skew,
            format: (/** @type {number} */ v, /** @type {any} */ where) => {
                const key = /** @type {Element | null} */ (where instanceof Element ? where : null)?.closest('[data-hsb-metric]')?.getAttribute('data-hsb-metric');
                return key ? spoken(key, Math.round(v)) : String(Math.round(v));
            },
        }),
    );
    for (const m of METRICS) {
        const figure = strip.querySelector(`[data-hsb-metric="${m.key}"] .kp-kpi__chart`);
        if (figure) setTrendData(figure, { points: points(p, m), step: HOUR });
    }
}

/** Build the tiles for the current profile and look. @param {Context} ctx */
function buildTiles(ctx) {
    for (const d of ctx.figures.detachers) d();
    const p = ctx.current();
    if (!p) return;
    const strip = ctx.el.strip;
    const t = totals(p, ctx.data);
    strip.innerHTML = METRICS.map((m, i) => tileHtml(m, i + 1, t, p, ctx)).join('');
    strip.setAttribute('aria-label', `Key figures of ${p.name}`);
    ctx.figures.detachers = [attachTicks(strip), attachSparklines(strip), attachKpiToggles(strip)];
    if (ctx.look === 'trend') attachTrends(ctx, p);
    ctx.figures.built = `${p.name}|${ctx.look}`;
    paintRecent(ctx);
}

/** New numbers for the tiles that stand: they count to the new value, the trend gets its new points. @param {Context} ctx */
function updateTiles(ctx) {
    const p = ctx.current();
    if (!p) return;
    const strip = ctx.el.strip;
    const t = totals(p, ctx.data);
    const values = { received: t.r, delivered: t.d, failed: t.f, duration: t.avg };
    for (const m of METRICS) {
        const v = values[/** @type {'received'} */ (m.key)];
        const el = strip.querySelector(`[data-hsb-value="${m.key}"]`);
        const node = [...(el?.childNodes ?? [])].find((n) => n.nodeType === Node.TEXT_NODE);
        if (node) node.nodeValue = count(v);
        const unit = el?.querySelector('small');
        if (unit) unit.textContent = unitOf(m.key, v);
        const w = strip.querySelector(`[data-hsb-words="${m.key}"]`);
        if (w) w.textContent = words(m.key, t);
        if (ctx.look === 'trend') {
            const fig = strip.querySelector(`[data-hsb-metric="${m.key}"] .kp-kpi__chart`);
            if (fig) setTrendData(fig, { points: points(p, m), step: HOUR });
        } else {
            strip.querySelector(`[data-hsb-metric="${m.key}"] .kp-kpi__spark`)?.setAttribute('data-kp-spark', points(p, m).map((x) => x[1]).join(' '));
        }
    }
    paintRecent(ctx);
}

/** Build the tiles when the profile or the look changed, else update the numbers. @param {Context} ctx */
export function paintFigures(ctx) {
    const p = ctx.current();
    if (!p) return;
    if (ctx.figures.built !== `${p.name}|${ctx.look}`) buildTiles(ctx);
    else updateTiles(ctx);
}
