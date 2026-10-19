// "Try a translation" (kp-themes 11): pick a profile, paste a sample message, see the body the profile would deliver and
// what changed. The components are the package's, served by the kit at /static/kp/js/ and attached by hand; the data is
// the server's: the profiles and their templates are embedded (#hsbt-data), and the translation is POSTed to
// /translate/{name}, which runs the same code the pump uses and delivers nothing. The answer never carries a header value
// or the destination's path, so nothing drawn here can leak one (see src/dashboard_translate.rs).

import { attachSegmented, setSegmentedValues } from '/static/kp/js/segmented.js';
import { attachJsonFields, jsonField } from '/static/kp/js/json-field.js';
import { attachCodes, setCode, prettyJson } from '/static/kp/js/code.js';
import { attachProblems, setProblems } from '/static/kp/js/problems.js';
import { attachCardStates, setCardState } from '/static/kp/js/card-state.js';

const SETTLE_MS = 350; // the box checks 300 ms after the last key; this waits for it
const SLOW_MS = 150; // a faster answer never flashes the busy card

const $ = (/** @type {string} */ id) => /** @type {HTMLElement} */ (document.getElementById(id));
const data = JSON.parse($('hsbt-data').textContent ?? '{}');
/** @type {{ name: string, takes: string, delivers: string, content_type: string, template: string, example: unknown }[]} */
const profiles = data.profiles ?? [];

const area = /** @type {HTMLTextAreaElement} */ ($('hsbt-message'));
const bar = $('hsbt-profile');
const takes = $('hsbt-takes');
const templateBlock = $('hsbt-template');
const resultCard = $('hsbt-result');
const diffCard = $('hsbt-diffcard');
const problems = $('hsbt-problems');
const body = $('hsbt-body');
const diff = $('hsbt-diff');
const diffCount = $('hsbt-diffcount');

const kib = (/** @type {number} */ n) => `${Math.round(n / 1024)} KiB`;
const bytes = (/** @type {string} */ text) => new TextEncoder().encode(text).length;
const pretty = (/** @type {unknown} */ value) => JSON.stringify(value, null, 2);
$('hsbt-limit').textContent = kib(data.maxBytes);

attachSegmented(document);
attachJsonFields(document);
attachCodes(document);
attachProblems(document);
attachCardStates(document);

// ── the diff (the package ships the style, not the lines) ──────────────────────────────────────────────────────────

/**
 * A line diff of two texts, indentation ignored when matching so a re-nested line still counts as the same line.
 * @param {string[]} a @param {string[]} b
 */
function lineOps(a, b) {
    const same = (/** @type {number} */ i, /** @type {number} */ j) => a[i].trim() === b[j].trim();
    const table = Array.from({ length: a.length + 1 }, () => new Array(b.length + 1).fill(0));
    for (let i = a.length - 1; i >= 0; i--) {
        for (let j = b.length - 1; j >= 0; j--) table[i][j] = same(i, j) ? table[i + 1][j + 1] + 1 : Math.max(table[i + 1][j], table[i][j + 1]);
    }
    /** @type {{ kind: 'same' | 'removed' | 'added', text: string, a: number, b: number }[]} */
    const ops = [];
    let i = 0;
    let j = 0;
    while (i < a.length || j < b.length) {
        if (i < a.length && j < b.length && same(i, j)) {
            ops.push({ kind: 'same', text: b[j], a: i + 1, b: j + 1 });
            i++;
            j++;
        } else if (j < b.length && (i === a.length || table[i][j + 1] >= table[i + 1][j])) {
            ops.push({ kind: 'added', text: b[j], a: i + 1, b: j + 1 });
            j++;
        } else {
            ops.push({ kind: 'removed', text: a[i], a: i + 1, b: j + 1 });
            i++;
        }
    }
    return ops;
}

/**
 * Draw `.kp-diff` with every line (the approved default context, "All lines": one block).
 * @param {string} before @param {string} after
 */
function drawDiff(before, after) {
    const ops = lineOps(before.split('\n'), after.split('\n'));
    const removed = ops.filter((o) => o.kind !== 'added');
    const added = ops.filter((o) => o.kind !== 'removed');
    diff.replaceChildren();
    const header = document.createElement('div');
    header.className = 'kp-diff__hunk';
    header.append(`@@ -${removed[0]?.a ?? 0},${removed.length} +${added[0]?.b ?? 0},${added.length} @@ `);
    const where = document.createElement('span');
    where.className = 'kp-diff__hunk-context';
    where.textContent = 'received → delivered';
    header.append(where);
    diff.append(header);
    for (const op of ops) {
        const row = document.createElement('span');
        row.className = 'kp-diff__line';
        row.dataset.kind = op.kind;
        const number = document.createElement('span');
        number.className = 'kp-diff__number';
        number.textContent = String(op.kind === 'added' ? op.b : op.a);
        const sign = document.createElement('span');
        sign.className = 'kp-diff__sign';
        sign.textContent = op.kind === 'added' ? '+' : op.kind === 'removed' ? '-' : ' ';
        const text = document.createElement('span');
        text.textContent = op.text;
        row.append(number, sign, text);
        diff.append(row);
    }
    const counts = { removed: ops.filter((o) => o.kind === 'removed').length, added: ops.filter((o) => o.kind === 'added').length };
    diff.setAttribute('aria-label', `Changes from the received message to the delivered body: ${counts.removed} lines removed, ${counts.added} lines added`);
    diffCount.textContent = `${counts.removed} lines removed · ${counts.added} lines added`;
}

// ── states ─────────────────────────────────────────────────────────────────────────────────────────────────────────

/** @param {string} title @param {string} text @param {string} [diffTitle] @param {string} [diffText] */
function empty(title, text, diffTitle = 'Nothing to compare yet', diffText = 'The comparison appears once a message has been translated.') {
    setCardState(resultCard, 'empty', { title, text });
    diff.replaceChildren();
    diffCount.textContent = '';
    setCardState(diffCard, 'empty', { title: diffTitle, text: diffText });
}

/** @param {string} title @param {string} text */
function unreachable(title, text) {
    setProblems(problems, []);
    setCardState(resultCard, 'error', { title, text });
    setCardState(diffCard, 'error', { title, text });
}

/** @param {{ error: string, why: string, remedy: string, kind: string, fixable?: boolean }} failure */
function showFailure(failure) {
    /** @type {HTMLButtonElement | undefined} */
    let action;
    if (failure.fixable) {
        action = document.createElement('button');
        action.type = 'button';
        action.className = 'kp-button kp-button--sm';
        action.textContent = 'Use the example message';
        action.addEventListener('click', useExample);
    }
    setCardState(resultCard, 'ready');
    setProblems(problems, [{ key: 'template', severity: 'warning', what: failure.error, why: failure.why, remedy: failure.remedy, ...(action ? { action } : {}) }]);
    setCode(body, { state: 'error', title: 'Translated body', message: failure.kind === 'size' ? 'Nothing was sent: the message is over the limit.' : 'There is no translated body: the message could not be translated.' });
    diff.replaceChildren();
    diffCount.textContent = '';
    setCardState(diffCard, 'empty', { title: 'Nothing to compare', text: 'No body was produced, so there is nothing to put next to the message.' });
}

// ── the profile and the box ────────────────────────────────────────────────────────────────────────────────────────

let current = profiles[0]?.name ?? '';
const wanted = new URLSearchParams(location.search).get('profile');
if (wanted && profiles.some((p) => p.name === wanted)) current = wanted;
/** What was in the box for each profile, so coming back to one finds it as it was. @type {Map<string, string>} */
const kept = new Map();

const profile = () => profiles.find((p) => p.name === current);

function paintProfile() {
    const p = profile();
    if (!p) return;
    setSegmentedValues(bar, [p.name]);
    takes.textContent = `Takes ${p.takes} · ${p.delivers}`;
    setCode(templateBlock, { text: p.template, title: `Template of ${p.name}`, language: 'minijinja', lines: true, wrap: true, maxLines: 8, state: 'ready' });
}

function useExample() {
    const p = profile();
    if (!p) return;
    area.value = pretty(p.example);
    jsonField(area)?.check();
    schedule(0);
}

// ── one translation ────────────────────────────────────────────────────────────────────────────────────────────────

let serial = 0;
/** @type {AbortController | null} */
let inFlight = null;
/** @type {number} */
let timer = 0;

/** @param {number} wait */
function schedule(wait) {
    window.clearTimeout(timer);
    timer = window.setTimeout(() => void run(), wait);
}

async function run() {
    const p = profile();
    if (!p) return;
    const mine = ++serial;
    inFlight?.abort();
    const state = jsonField(area)?.state() ?? 'empty';
    if (state === 'empty') {
        setProblems(problems, []);
        empty('Nothing to translate yet', 'Paste a message into the box, or use the example message.');
        return;
    }
    if (state !== 'valid') {
        setProblems(problems, []);
        empty('The message is not JSON yet', 'Fix the line the box points at; the translation follows by itself.', 'Nothing to compare', 'The message has to be valid JSON before it can be compared.');
        return;
    }
    const text = area.value;
    if (bytes(text) > data.maxBytes) {
        showFailure({
            error: 'The message is too large to try',
            why: `It is ${kib(bytes(text))} and the limit is ${kib(data.maxBytes)}.`,
            remedy: 'Paste one recorded message instead of a whole log; a template only reads the fields of a single message.',
            kind: 'size',
        });
        return;
    }
    const controller = new AbortController();
    inFlight = controller;
    const slow = window.setTimeout(() => {
        if (mine !== serial) return;
        setCardState(resultCard, 'loading', { words: 'Translating…' });
        setCardState(diffCard, 'loading', { words: 'Comparing…' });
    }, SLOW_MS);
    /** @type {Response} */
    let res;
    /** @type {any} */
    let answer = null;
    try {
        res = await fetch(data.routes.preview.replace('{name}', encodeURIComponent(p.name)), {
            method: 'POST',
            headers: { 'content-type': 'application/json', accept: 'application/json' },
            body: text,
            signal: controller.signal,
            credentials: 'same-origin',
        });
        try {
            answer = await res.json();
        } catch {
            // Not JSON: the status below decides.
        }
    } catch (error) {
        window.clearTimeout(slow);
        if (mine !== serial || /** @type {Error} */ (error).name === 'AbortError') return;
        unreachable('The service did not answer', 'Nothing could be translated and nothing was delivered. Check that HTTPSwitchboard is running, then try again.');
        return;
    }
    window.clearTimeout(slow);
    if (mine !== serial) return;
    if (res.ok && answer?.body !== undefined) {
        showResult(text, answer);
    } else if (answer?.error && answer?.why) {
        showFailure(answer);
    } else if (res.redirected || res.status === 401 || res.status === 403) {
        unreachable('You are signed out', 'Open the login page again, then come back to this page.');
    } else if (res.status === 413) {
        unreachable('The message is too large', `The limit is ${kib(data.maxBytes)}. Paste one recorded message instead of a whole log.`);
    } else {
        unreachable('The service could not translate', `HTTP ${res.status}. Nothing was delivered. Try again; if it persists, read the service's log.`);
    }
}

/** @param {string} text @param {{ body: string, content_type: string, notes: { key: string, what: string, why: string }[] }} answer */
function showResult(text, answer) {
    setCardState(resultCard, 'ready');
    setCardState(diffCard, 'ready');
    setProblems(
        problems,
        answer.notes.map((n) => ({ key: n.key, severity: 'info', what: n.what, why: n.why, dismissible: true })),
    );
    const json = /json/i.test(answer.content_type);
    setCode(body, {
        text: answer.body,
        title: 'Translated body',
        language: json ? 'JSON' : answer.content_type.split(';')[0],
        size: `${bytes(answer.body)} B`,
        lines: true,
        wrap: true,
        maxLines: 12,
        state: 'ready',
    });
    let before = text;
    try {
        before = pretty(JSON.parse(text));
    } catch {
        // The box said valid; the raw text stands in.
    }
    drawDiff(before, (json ? prettyJson(answer.body) : null) ?? answer.body);
}

// ── wiring ─────────────────────────────────────────────────────────────────────────────────────────────────────────

area.addEventListener('input', () => schedule(SETTLE_MS));
area.addEventListener('kp-json-state', () => schedule(SETTLE_MS));
for (const card of [resultCard, diffCard]) card.addEventListener('kp-card-retry', () => void run());
bar.addEventListener('kp-segmented-change', (event) => {
    const value = /** @type {CustomEvent} */ (event).detail?.value;
    if (!value || value === current) return;
    kept.set(current, area.value);
    current = value;
    paintProfile();
    area.value = kept.get(current) ?? pretty(profile()?.example ?? {});
    jsonField(area)?.check();
    schedule(0);
});

paintProfile();
area.value = pretty(profile()?.example ?? {});
jsonField(area)?.check();
schedule(0);
