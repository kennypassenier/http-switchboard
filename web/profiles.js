// The profile cards' three finishing touches (kp-themes 11, approved demos app-hsb-profiles "hover" and app-hsb-health
// "states" and "freshness"): the hover card on a profile name, the states of the profiles card, and the "last delivery
// ... ago" line on each card. dashboard.js owns the data and the polling and calls these; nothing here fetches.
//
// Nothing here can leak a secret: it reads only the snapshot's fields (see src/dashboard.rs), and the template is a
// short label the server made, never the body.

import { attachHoverCards } from '/static/kp/js/hovercard.js';
import { attachCardStates, setCardState } from '/static/kp/js/card-state.js';
import { agoMoment, attachAgo, humanDuration, setAgo } from '/static/kp/js/freshness.js';
import { attachArt } from '/static/kp/js/art.js';
import { getStrings, setStrings } from '/static/kp/js/strings.js';

/**
 * How long a profile may go without a delivery before its line turns to a warning, in seconds. The configuration has no
 * expected quiet time per profile, and a profile that carries alerts is quiet exactly when nothing is wrong, so the
 * default is a day: long enough that a quiet night or weekend never cries wolf, short enough that a day of silence on a
 * profile that normally delivers is noticed. A profile never has to be tuned to be honest; this is the one knob.
 */
export const QUIET_AFTER_S = 86_400;

const count = (/** @type {number} */ n) => n.toLocaleString('en-GB');

/** @typedef {import('./dashboard.js').Profile} Profile */

// The app's own words over the package's dictionary: a profile with no delivery says "no delivery yet".
setStrings({ agoNever: (verb) => (verb === 'last delivery' ? 'no delivery yet' : `not ${verb} yet`) });

// ── the hover card on a profile name ───────────────────────────────────────────────────────────────────────────────

/** Native controls take the focus on their own; any other name gets a tab stop so the keyboard reaches the card. */
const FOCUSABLE = 'a[href], button, input, select, textarea, [tabindex]';

/**
 * Make an element the anchor of a profile's hover card.
 * @param {HTMLElement} el @param {string} name
 */
export function markName(el, name) {
    el.setAttribute('data-kp-hovercard', '');
    el.setAttribute('data-hsb-profile', name);
    if (!el.matches(FOCUSABLE)) el.tabIndex = 0;
}

/** @type {import('/static/kp/js/hovercard.js').HoverCardHandle[]} */
const cardsOpen = [];

/**
 * The card's rows: status, source, destination host only, content type, template, the three exact counts and the last
 * success. Built as nodes, so no value can inject markup. @param {Profile | undefined} p
 */
function rowsOf(p) {
    if (!p) return null;
    const kyu = p.source.kind === 'kyu';
    /** @type {[string, string][]} */
    const rows = [
        ['Status', p.state === 'hub-down' ? 'hub down' : p.state],
        ['Source', `${p.source.label}${kyu ? ` (${p.source.sub})` : ''}`],
        ['Destination host', p.sink.label],
        ['Content type', p.content_type],
        ['Template', p.template],
        ['Received', count(p.received)],
        ['Delivered', count(p.delivered)],
        ['Failed', count(p.failed)],
        ['Last success', p.last_success_unix === null ? 'none yet' : agoMoment(p.last_success_unix * 1000)],
    ];
    const dl = document.createElement('dl');
    dl.className = 'kp-kv hsb-card-kv';
    for (const [label, value] of rows) {
        const row = document.createElement('div');
        row.className = 'kp-kv__row';
        const dt = document.createElement('dt');
        dt.textContent = label;
        const dd = document.createElement('dd');
        dd.textContent = value;
        row.append(dt, dd);
        dl.append(row);
    }
    return dl;
}

/**
 * Put the hover card on the profile names under each root. A root of its own each, so this page's cards never take
 * another section's `fill` from it (a second attach on one root hands its options to the first card).
 * @param {HTMLElement[]} roots @param {(name: string) => Profile | undefined} find
 */
export function attachProfileHover(roots, find) {
    for (const root of roots) {
        const handle = attachHoverCards(root, {
            fill: (anchor) => rowsOf(find(anchor.getAttribute('data-hsb-profile') ?? '')),
        });
        cardsOpen.push(handle);
    }
}

/** A card that is open shows the newest numbers as soon as the poll brings them. */
export function refreshHover() {
    for (const h of cardsOpen) if (h.anchor()) h.refresh();
}

/**
 * The entity cards draw and redraw their names themselves, so the names are marked as they appear.
 * @param {HTMLElement} grid
 */
export function watchEntityNames(grid) {
    const mark = () => {
        for (const name of /** @type {HTMLElement[]} */ ([...grid.querySelectorAll('.kp-entity__name')])) {
            const key = name.closest('[data-kp-entity-key]')?.getAttribute('data-kp-entity-key');
            if (key && name.getAttribute('data-hsb-profile') !== key) markName(name, key);
        }
    };
    new MutationObserver(mark).observe(grid, { childList: true, subtree: true });
    mark();
}

// ── the states of the profiles card ────────────────────────────────────────────────────────────────────────────────

/** @param {string} text @param {string} className @param {string} [tag] */
const made = (text, className, tag = 'p') => {
    const node = document.createElement(tag);
    node.className = className;
    node.textContent = text;
    return node;
};

/**
 * The page's own note for a state: the package's empty plate with a drawing (CardStateOptions has no art).
 * @param {HTMLElement} card @param {HTMLElement | null} plate
 */
function ownNote(card, plate) {
    const stage = /** @type {HTMLElement} */ (card.querySelector(':scope > .kp-card__stage'));
    for (const old of stage.querySelectorAll(':scope > .kp-card__state')) old.remove();
    if (plate === null) return;
    const note = document.createElement('div');
    note.className = 'kp-card__state';
    note.setAttribute('role', 'status');
    note.append(plate);
    stage.append(note);
    // A note made after the boot is attached by the page that made it.
    attachArt(note);
}

/** @param {'none' | 'error'} art @param {string} title @param {string} body @param {{ label: string, attr: string }} action */
function plateOf(art, title, body, action) {
    const box = document.createElement('div');
    box.className = 'kp-empty kp-card__empty';
    box.setAttribute('data-kp-art', art);
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'kp-button';
    button.setAttribute(action.attr, '');
    button.textContent = action.label;
    const actions = document.createElement('div');
    actions.className = 'kp-card__state-actions';
    actions.append(button);
    box.append(made(title, 'kp-empty__title'), made(body, 'kp-empty__body'), actions);
    return box;
}

/** Fired on the card when its "Reload profiles" is pressed (the empty state's one action). */
export const AREA_RELOAD_EVENT = 'hsb-area-reload';

/** @type {WeakMap<HTMLElement, string>} */
const shown = new WeakMap();

/**
 * Put the profiles card in a state. A state that is already shown is left alone, so a poll does not rebuild the note
 * under a focused Try again.
 * @param {HTMLElement} card @param {'ready' | 'loading' | 'empty' | 'error'} state
 */
export function setArea(card, state) {
    if (shown.get(card) === state) return;
    shown.set(card, state);
    if (state === 'ready') {
        ownNote(card, null);
        setCardState(card, 'ready');
    } else if (state === 'loading') {
        setCardState(card, 'loading', { words: 'Reading the profiles…' });
    } else if (state === 'empty') {
        ownNote(
            card,
            plateOf('none', 'No profiles are configured', 'A profile says where messages come from and where they go. Add one to the configuration and reload.', {
                label: 'Reload profiles',
                attr: 'data-hsb-reload',
            }),
        );
        setCardState(card, 'empty');
    } else {
        ownNote(
            card,
            plateOf('error', 'The profiles could not be read', 'The status service did not answer in time. Nothing was changed.', {
                label: getStrings().cardRetry,
                attr: 'data-kp-card-retry',
            }),
        );
        setCardState(card, 'error');
    }
}

/** @param {HTMLElement} card */
export function attachArea(card) {
    attachCardStates(card);
    card.addEventListener('click', (event) => {
        if (event.target instanceof Element && event.target.closest('[data-hsb-reload]')) card.dispatchEvent(new CustomEvent(AREA_RELOAD_EVENT, { bubbles: true }));
    });
    attachAgo(card);
}

// ── last delivery, on each profile card ────────────────────────────────────────────────────────────────────────────

/**
 * The freshness line of one entity card: made once, then only its moment is set. The package ticks it, warns once to a
 * screen reader when it turns stale, and writes the moment (dd/mm/yyyy HH:mm) into its title.
 * @param {HTMLElement} card an entity card @param {Profile} p
 */
export function paintFresh(card, p) {
    let line = /** @type {HTMLElement | null} */ (card.querySelector(':scope > .hsb-fresh'));
    if (!line) {
        line = document.createElement('div');
        line.className = 'hsb-fresh';
        const time = document.createElement('time');
        time.className = 'kp-ago';
        time.setAttribute('data-kp-ago', '');
        time.setAttribute('data-kp-ago-verb', 'last delivery');
        time.setAttribute('data-kp-stale-after', String(QUIET_AFTER_S));
        time.setAttribute('data-kp-ago-announce', 'state');
        const quiet = document.createElement('span');
        quiet.className = 'hsb-quiet';
        quiet.textContent = `Quiet time allowed: ${humanDuration(QUIET_AFTER_S)}`;
        line.append(time, quiet);
        card.append(line);
    }
    const time = /** @type {HTMLElement} */ (line.querySelector('time'));
    setAgo(time, p.last_success_unix === null ? null : p.last_success_unix * 1000);
}
