// What the "Try a translation" page's two cards show in each state: nothing to translate yet, the service not
// answering, a refusal in words, a result. Both cards keep their footprint in every state (their stage has a least
// height), so nothing jumps when the state changes.

import { setCode, prettyJson, setProblems, setCardState } from '/static/kp/dist/kp-themes.min.js';
import { clearDiff, queueDiff } from './translate-diff.js';
import { drawFields } from './translate-fields.js';
import { els, bytes, pretty } from './translate-dom.js';

/** Clear what the fields card draws. */
function clearFields() {
    drawFields([]);
    clearDiff();
    els.diff.replaceChildren();
    els.diffCount.textContent = '';
    els.diffBox.hidden = true;
}

/**
 * Both cards empty.
 * @param {string} title @param {string} text @param {string} [sideTitle] @param {string} [sideText]
 */
export function empty(title, text, sideTitle = 'No fields to show yet', sideText = 'The fields appear once a message has been translated.') {
    setCardState(els.resultCard, 'empty', { title, text });
    clearFields();
    setCardState(els.fieldsCard, 'empty', { title: sideTitle, text: sideText });
}

/** The service did not answer: the result card's code view draws the failed plate itself (Retry fires kp-code-retry). @param {string} title @param {string} text */
export function unreachable(title, text) {
    setCardState(els.resultCard, 'ready');
    setProblems(els.problems, [{ key: 'service', severity: 'warning', what: title, why: text }]);
    setCode(els.body, { state: 'error', title: 'Translated body', message: title });
    clearFields();
    setCardState(els.fieldsCard, 'error', { title, text });
}

/**
 * A refusal in words (what, why, what now), the fields the template reads when the server could say them, and the one
 * action that can mend it, if there is one.
 * @param {{ error: string, why: string, remedy: string, kind: string, fixable?: boolean, fields?: any[] }} failure
 * @param {HTMLButtonElement | undefined} action
 */
export function showFailure(failure, action) {
    setCardState(els.resultCard, 'ready');
    setProblems(els.problems, [{ key: 'template', severity: 'warning', what: failure.error, why: failure.why, remedy: failure.remedy, ...(action ? { action } : {}) }]);
    const message = failure.kind === 'size' ? 'Nothing was sent: it is over the limit.' : 'There is no translated body: it could not be translated.';
    setCode(els.body, { state: 'error', title: 'Translated body', message });
    clearFields();
    if (failure.fields?.length) {
        setCardState(els.fieldsCard, 'ready');
        drawFields(failure.fields);
    } else {
        setCardState(els.fieldsCard, 'empty', { title: 'No fields to show', text: 'No body was produced, so there is nothing to put next to the message.' });
    }
}

/** @param {string} text @param {{ body: string, content_type: string, notes: { key: string, what: string, why: string }[], fields: any[] }} answer */
export function showResult(text, answer) {
    setCardState(els.resultCard, 'ready');
    setCardState(els.fieldsCard, 'ready');
    setProblems(
        els.problems,
        (answer.notes ?? []).map((n) => ({ key: n.key, severity: 'info', what: n.what, why: n.why, dismissible: true })),
    );
    // A field the server left out is defaulted here, so a partial answer still draws what it has.
    const contentType = answer.content_type ?? '';
    const json = /json/i.test(contentType);
    setCode(els.body, {
        text: answer.body ?? '',
        title: 'Translated body',
        language: json ? 'JSON' : contentType.split(';')[0] || 'text',
        size: `${bytes(answer.body ?? '')} B`,
        lines: true,
        wrap: true,
        maxLines: 12,
        state: 'ready',
    });
    drawFields(answer.fields ?? []);
    let before = text;
    try {
        before = pretty(JSON.parse(text));
    } catch {
        // The box said valid, so this only fails on a race with a new keystroke: the raw text stands in.
    }
    queueDiff(els.diffBox, els.diff, els.diffCount, before, (json ? prettyJson(answer.body ?? '') : null) ?? answer.body ?? '');
    els.diffBox.hidden = false;
}
