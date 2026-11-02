// The "Try a translation" page's shared handles: the embedded data, the elements the other translate-*.js modules draw
// into, and the few formatters they all use. One place, so a renamed id is changed once.

const $ = (/** @type {string} */ id) => /** @type {HTMLElement} */ (document.getElementById(id));

export const data = JSON.parse($('hsbt-data').textContent ?? '{}');
/** @type {{ name: string, takes: string, delivers: string, content_type: string, template: string, example: unknown }[]} */
export const profiles = data.profiles ?? [];

export const els = {
    message: /** @type {HTMLTextAreaElement} */ ($('hsbt-message')),
    draft: /** @type {HTMLTextAreaElement} */ ($('hsbt-draft')),
    pick: $('hsbt-profile'),
    takes: $('hsbt-takes'),
    unknown: $('hsbt-unknown'),
    resultCard: $('hsbt-result'),
    fieldsCard: $('hsbt-fieldscard'),
    problems: $('hsbt-problems'),
    body: $('hsbt-body'),
    fieldsWrap: $('hsbt-fields-wrap'),
    fieldsBody: $('hsbt-fields'),
    fieldsCount: $('hsbt-fieldcount'),
    diffBox: /** @type {HTMLDetailsElement} */ ($('hsbt-diffbox')),
    diff: $('hsbt-diff'),
    diffCount: $('hsbt-diffcount'),
    draftState: $('hsbt-draft-state'),
    reset: /** @type {HTMLButtonElement} */ ($('hsbt-reset')),
    copy: /** @type {HTMLButtonElement} */ ($('hsbt-copy')),
    copyState: $('hsbt-copy-state'),
    limit: $('hsbt-limit'),
};

export const kib = (/** @type {number} */ n) => `${Math.round(n / 1024)} KiB`;
export const bytes = (/** @type {string} */ text) => new TextEncoder().encode(text).length;
export const pretty = (/** @type {unknown} */ value) => JSON.stringify(value, null, 2);
