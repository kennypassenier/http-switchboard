// "Try a translation" (kp-themes 12): pick a profile, paste a sample message, optionally edit a scratch copy of the
// profile's template, and see the body the profile would deliver and where each field of the message lands. The
// components are the package's, served by the kit as one bundle, /static/kp/dist/kp-themes.min.js and attached by hand;
// the data is the server's: the profiles and their templates are embedded (#hsbt-data), and the translation is POSTed to
// /translate/{name}, which runs the same code the pump uses and delivers nothing. The scratch template travels in the
// request and is never saved. The answer never carries a header value or the destination's path, so nothing drawn here
// can leak one (see src/dashboard_translate.rs). The modules: translate-dom (handles), translate-run (the request),
// translate-states (what the cards show), translate-fields (the table), translate-diff (the line diff),
// translate-toml (Copy as TOML), and profile-pick.js (the shared profile chooser).

import { attachJsonFields, jsonField, attachCodes, setCode, attachProblems, attachCardStates } from '/static/kp/dist/kp-themes.min.js';
import { paintPick, PICK_EVENT } from './profile-pick.js';
import { data, profiles, els, kib, pretty } from './translate-dom.js';
import { createRunner } from './translate-run.js';
import { tomlBody } from './translate-toml.js';

const SETTLE_MS = 350; // the box checks 300 ms after the last key; this waits for it
const COPIED_MS = 2500; // how long "Copied" stays on the button's status line

els.limit.textContent = kib(data.maxBytes);
attachJsonFields(document);
attachCodes(document);
attachProblems(document);
attachCardStates(document);

// ── the profile and the two boxes ──────────────────────────────────────────────────────────────────────────────────

let current = profiles[0]?.name ?? '';
const wanted = new URLSearchParams(location.search).get('profile');
if (wanted && profiles.some((p) => p.name === wanted)) current = wanted;
/** What was in the boxes for each profile, so coming back to one finds it as it was. @type {Map<string, { message: string, draft: string }>} */
const kept = new Map();

const profile = () => profiles.find((p) => p.name === current);
/** The draft when it differs from the profile's template, else null: an untouched draft runs the profile's own. */
const editedDraft = () => (profile() && els.draft.value !== profile()?.template ? els.draft.value : null);

const runner = createRunner({
    read: () => (profile() ? { name: current, draft: editedDraft() } : null),
    useExample,
    resetDraft,
});

function paintDraftState() {
    const edited = editedDraft() !== null;
    els.draftState.textContent = edited ? 'Edited here only. The profile keeps its own template.' : "Same as the profile's template.";
    els.reset.disabled = !edited;
    els.reset.title = edited ? "Drop the edits and use the profile's template again" : 'There is nothing to reset: the draft is the profile’s template';
}

function paintProfile() {
    const p = profile();
    if (!p) return;
    paintPick(els.pick, profiles.map((x) => x.name), current, 'Profile to try');
    els.takes.textContent = `Takes ${p.takes} · ${p.delivers}`;
    setCode(els.body, { state: 'loading', title: 'Translated body' });
}

function useExample() {
    const p = profile();
    if (!p) return;
    els.message.value = pretty(p.example);
    jsonField(els.message)?.check();
    runner.schedule(0);
}

function resetDraft() {
    els.draft.value = profile()?.template ?? '';
    paintDraftState();
    runner.schedule(0);
}

// ── Copy as TOML ───────────────────────────────────────────────────────────────────────────────────────────────────

let copiedTimer = 0;

/** @param {string} text */
async function writeClipboard(text) {
    if (navigator.clipboard?.writeText) return navigator.clipboard.writeText(text);
    // A page served over plain http has no clipboard API: select the text of a temporary field and copy that.
    const holder = document.createElement('textarea');
    holder.value = text;
    holder.setAttribute('readonly', '');
    holder.style.position = 'fixed';
    holder.style.insetBlockStart = '0';
    document.body.append(holder);
    holder.select();
    const done = document.execCommand('copy');
    holder.remove();
    if (!done) throw new Error('copy refused');
}

async function copyToml() {
    window.clearTimeout(copiedTimer);
    try {
        await writeClipboard(tomlBody(els.draft.value));
        els.copyState.textContent = 'Copied. Paste it over the body of the profile in the config file.';
    } catch {
        els.copyState.textContent = 'The browser did not allow copying. Select the template and copy it by hand.';
    }
    copiedTimer = window.setTimeout(() => (els.copyState.textContent = ''), COPIED_MS);
}

// ── wiring ─────────────────────────────────────────────────────────────────────────────────────────────────────────

els.message.addEventListener('input', () => runner.schedule(SETTLE_MS));
els.message.addEventListener('kp-json-state', () => runner.schedule(SETTLE_MS));
els.draft.addEventListener('input', () => {
    paintDraftState();
    runner.schedule(SETTLE_MS);
});
els.reset.addEventListener('click', resetDraft);
els.copy.addEventListener('click', () => void copyToml());
for (const card of [els.resultCard, els.fieldsCard]) card.addEventListener('kp-card-retry', () => void runner.run());
els.body.addEventListener('kp-code-retry', () => void runner.run());
els.pick.addEventListener(PICK_EVENT, (event) => {
    const name = /** @type {CustomEvent} */ (event).detail?.name;
    if (!name || name === current) return;
    kept.set(current, { message: els.message.value, draft: els.draft.value });
    current = name;
    paintProfile();
    const back = kept.get(current);
    els.message.value = back?.message ?? pretty(profile()?.example ?? {});
    els.draft.value = back?.draft ?? profile()?.template ?? '';
    paintDraftState();
    jsonField(els.message)?.check();
    runner.schedule(0);
});

paintProfile();
els.message.value = pretty(profile()?.example ?? {});
els.draft.value = profile()?.template ?? '';
paintDraftState();
jsonField(els.message)?.check();
runner.schedule(0);
