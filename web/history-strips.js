// The uptime strips of the last 24 hours: one strip of 24 hour cells per profile, made once and fed on every read.

import { attachUptimes, setStrings, setUptime } from '/static/kp/dist/kp-themes.min.js';
import { periodOf } from './history-model.js';

// The strip's figure when no hour has traffic: a dash and the plain reason, not a percentage.
setStrings({ uptimeEmpty: '— nothing received yet' });

/** The cells of one uptime strip: one per hour of the window. */
const STRIP_CELLS = 24;

/** @type {Map<string, HTMLElement>} */
const stripEls = new Map();

/** One uptime strip per profile, made once and fed on every read; a strip whose profile is gone is removed. @param {import('./history.js').Context} ctx */
export function paintStrips(ctx) {
    const names = new Set(ctx.data.profiles.map((p) => p.name));
    for (const [name, el] of stripEls) {
        if (names.has(name)) continue;
        el.remove();
        stripEls.delete(name);
    }
    for (const p of ctx.data.profiles) {
        let el = stripEls.get(p.name);
        if (!el) {
            el = document.createElement('div');
            el.className = 'kp-uptime';
            el.setAttribute('data-kp-uptime', '');
            el.setAttribute('data-kp-uptime-count', String(STRIP_CELLS));
            el.setAttribute('data-kp-uptime-label', p.name);
            ctx.el.strips.append(el);
            stripEls.set(p.name, el);
            attachUptimes(el); // once per strip: the kit marks an attached strip and skips it later
        }
        if (ctx.press === 'info') el.setAttribute('data-kp-uptime-open', 'info');
        else el.removeAttribute('data-kp-uptime-open');
        // An hour without traffic is a grey cell, and the kit's percentage counts only the other cells, so a profile that
        // received nothing reads as no figure instead of a green 100 %.
        setUptime(el, {
            label: p.name,
            periods: p.hours.map(periodOf),
            state: ctx.state === 'ready' ? 'ready' : ctx.state,
            count: STRIP_CELLS,
            from: '24 hours ago',
            to: 'Now',
            error: 'Could not read the hourly history of this profile.',
            open: ctx.press === 'info' ? 'info' : undefined,
        });
    }
}
