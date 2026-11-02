// One translation of the "Try a translation" page: check the box, POST the message (and the draft template, when it
// differs from the profile's) to /translate/{name}, show what came back. Nothing is delivered and nothing is stored: the
// draft lives in the request. A newer run cancels an older one; a fast answer never flashes the busy card.

import { setProblems, setCardState } from '/static/kp/dist/kp-themes.min.js';
import { jsonField } from '/static/kp/dist/kp-themes.min.js';
import { data, els, bytes, kib } from './translate-dom.js';
import { empty, unreachable, showFailure, showResult } from './translate-states.js';

const SLOW_MS = 150; // a faster answer never flashes the busy card
/** How long the page waits for the preview route before it says the service did not answer. */
const TIMEOUT_MS = 15_000;
/** The content type that tells the route the body is `{ message, template }`, not a bare message. */
const ENVELOPE_TYPE = 'application/vnd.hsb.try+json';

let serial = 0;
/** @type {AbortController | null} */
let inFlight = null;
let timer = 0;

/** @param {() => void} act @param {string} label @param {string} text */
function actionButton(act, label, text) {
    const button = document.createElement('button');
    button.type = 'button';
    button.className = 'kp-button kp-button--sm';
    button.textContent = label;
    button.title = text;
    button.addEventListener('click', act);
    return button;
}

/** Check the box before asking the server; false when the page already says why nothing is translated. @param {string} state @param {string} text */
function boxReady(state, text) {
    if (state === 'empty') {
        setProblems(els.problems, []);
        empty('Nothing to translate yet', 'Paste a message into the box, or use the example message.');
        return false;
    }
    if (state !== 'valid') {
        setProblems(els.problems, []);
        empty('The message is not JSON yet', 'Fix the line the box points at; the translation follows by itself.', 'No fields to show', 'The message has to be valid JSON before its fields can be read.');
        return false;
    }
    return true;
}

/** The client-side size refusals, in the server's words. @param {string} text @param {string | null} draft @returns {{ error: string, why: string, remedy: string, kind: string } | null} */
function tooLarge(text, draft) {
    if (bytes(text) > data.maxBytes) {
        return {
            error: 'The message is too large to try',
            why: `It is ${kib(bytes(text))} and the limit is ${kib(data.maxBytes)}.`,
            remedy: 'Paste one recorded message instead of a whole log; a template only reads the fields of a single message.',
            kind: 'size',
        };
    }
    if (draft !== null && bytes(draft) > data.maxTemplateBytes) {
        return {
            error: 'The template is too large to try',
            why: `It is ${kib(bytes(draft))} and the limit is ${kib(data.maxTemplateBytes)}.`,
            remedy: "Try a smaller template; a profile's template is one document with a few fields.",
            kind: 'size',
        };
    }
    return null;
}

/** POST to the preview route. @param {string} name @param {string} message @param {string | null} template @param {AbortSignal} signal */
async function preview(name, message, template, signal) {
    const res = await fetch(data.routes.preview.replace('{name}', encodeURIComponent(name)), {
        method: 'POST',
        headers: { 'content-type': ENVELOPE_TYPE, accept: 'application/json' },
        body: JSON.stringify(template === null ? { message } : { message, template }),
        signal,
        credentials: 'same-origin',
    });
    /** @type {any} */
    let answer = null;
    try {
        answer = await res.json();
    } catch {
        // Not JSON (a login redirect, a proxy page): the status decides in settle().
    }
    return { res, answer };
}

/**
 * Show what the server answered.
 * @param {Response} res @param {any} answer @param {string} text @param {(failure: any, status: number) => HTMLButtonElement | undefined} actionFor
 */
function settle(res, answer, text, actionFor) {
    if (res.ok && answer?.body !== undefined) showResult(text, answer);
    // Any non-2xx answer that carries the what/why/what-now body (413, 429, 503, 504, ...) is shown in the server's words.
    else if (answer?.error && answer?.why) showFailure(answer, actionFor(answer, res.status));
    else if (res.redirected || res.status === 401 || res.status === 403) unreachable('You are signed out', 'Open the login page again, then come back to this page.');
    else if (res.status === 413) unreachable('The message is too large', `The limit is ${kib(data.maxBytes)}. Paste one recorded message instead of a whole log.`);
    else unreachable('The service could not translate', `HTTP ${res.status}. Nothing was delivered. Try again; if it persists, read the service's log.`);
}

/**
 * Build the runner. `read` says what to translate; `actions` supply the one button a refusal may carry.
 * @param {{ read: () => ({ name: string, draft: string | null } | null), useExample: () => void, resetDraft: () => void }} source
 */
export function createRunner(source) {
    /** The action that can mend a refusal: try again after "too many at once" (429), reset a draft that broke the template, or fill in the example message. */
    const actionFor = (/** @type {any} */ failure, /** @type {number} */ status) => {
        const current = source.read();
        if (status === 429) return actionButton(() => void run(), 'Try again', 'Send the same message again');
        if (failure.kind === 'template' && current?.draft !== null) return actionButton(source.resetDraft, "Reset to the profile's template", 'Drop the edits and use the template the profile has');
        if (failure.fixable) return actionButton(source.useExample, 'Use the example message', 'Put the example message in the box');
        return undefined;
    };

    /** POST with the slow-card and timeout timers; null when a newer run took over or the failure is already shown. @param {{ name: string, draft: string | null }} current @param {string} text @param {number} mine */
    async function send(current, text, mine) {
        const controller = new AbortController();
        inFlight = controller;
        let timedOut = false;
        const limit = window.setTimeout(() => {
            timedOut = true;
            controller.abort();
        }, TIMEOUT_MS);
        const slow = window.setTimeout(() => {
            if (mine !== serial) return;
            setCardState(els.resultCard, 'loading', { words: 'Translating…' });
            setCardState(els.fieldsCard, 'loading', { words: 'Reading the fields…' });
        }, SLOW_MS);
        try {
            return await preview(current.name, text, current.draft, controller.signal);
        } catch (error) {
            if (mine !== serial) return null;
            if (timedOut) {
                unreachable('The service did not answer', `Nothing came back within ${TIMEOUT_MS / 1000} s, so nothing could be translated and nothing was delivered. Try again.`);
            } else if (/** @type {Error} */ (error).name !== 'AbortError') {
                console.error('http-switchboard: the translate request failed', error);
                unreachable('The service did not answer', 'Nothing could be translated and nothing was delivered. Check that HTTPSwitchboard is running, then try again.');
            }
            return null;
        } finally {
            window.clearTimeout(slow);
            window.clearTimeout(limit);
        }
    }

    async function run() {
        const current = source.read();
        if (!current) return;
        const mine = ++serial;
        inFlight?.abort();
        const text = els.message.value;
        if (!boxReady(jsonField(els.message)?.state() ?? 'empty', text)) return;
        const refusal = tooLarge(text, current.draft);
        if (refusal) {
            showFailure(refusal, undefined);
            return;
        }
        const reply = await send(current, text, mine);
        if (!reply || mine !== serial) return;
        try {
            settle(reply.res, reply.answer, text, actionFor);
        } catch (error) {
            console.error('http-switchboard: the translate answer could not be shown', error);
            unreachable('The answer could not be shown', 'The service answered, but the page could not read the answer. Nothing was delivered. Try again; if it persists, read the browser console.');
        }
    }

    /** Drop the translation in flight and the one scheduled: what was typed or picked since is what counts. */
    function cancel() {
        window.clearTimeout(timer);
        serial++;
        inFlight?.abort();
    }

    /** @param {number} wait */
    const schedule = (wait) => {
        cancel();
        timer = window.setTimeout(() => void run(), wait);
    };
    return { run, schedule, cancel };
}
