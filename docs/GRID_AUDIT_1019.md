# Grid and vertical alignment audit (19/10/2026): done

Kenny approved the bundled demo "HTTPSwitchboard: everything on the grid"
(kp-themes `research/app-hsb-align/`, 12 of 12). Every status-page view and
`/translate` now stand on `.kp-formgrid`, `data-kp-subgrid` and subgrids as
that demo lays them out; the two early commits' script (`alignRows()`) and
flex pickers are gone.

## Measured (kp-themes `measure-align.mjs`, per section, Firefox)

Before (3.5.0 plus the two early commits): `/` 11 lines at 1280 px and 12 at
390 px; `/translate` 1 at 1280 and 0 at 390 (measured per section of the page).
After: every section of `/` and `/translate` 0 at 1280 and 390 px, except
the chassis "Status" card (Service / Health / Updates), which the shell
renders and this app cannot lay out: 3 lines at each width
(state badge and word of its rows). A kp-themes `measure-states.mjs` run
(`/` 0 lines at both widths; `/translate` 5 lines, all the code view, see
below).

## Views

| View | Before | After |
| --- | --- | --- |
| Needs attention | flex band, measured heights | two subgrid lists on shared rows, three tracks each |
| Profiles card, Path | flex pickers, badge off the grid | form grid rows, equal choice tracks; name and badge in the label and control columns |
| Settings | two cards, own rows | nine shared subgrid rows, head a grid |
| Recheck | meter and words off one edge | meter track and words track |
| Last 24 hours, Uptime | flex bar, flex pickers, flex rows | refresh bar and pickers on the form grid, deliveries on four tracks |
| Reloads | title and count off one line | two-track head, outcome column |
| /translate | unequal columns | 12-column grid, profile row a subgrid, shared tool tracks, diff card head |

Not built, because the real app has no such view: the demo's sections
"Profiles: the figures and the last delivery in rows" and "Profile names with
their hover card" (a mock list of names); the hover card and the cards'
freshness line exist and are laid out.

## Standard states

Profiles card: empty and failed notes now come from `setCardState` (the
hand-made `.kp-card__state` slot and plate are gone). Recheck table: the
`data-kp-datatable-empty` slot was missing; loading and failed now follow the
poll (`state('loading')`, `fail()`, Try again reloads).

## Open (package)

Code view (`js/code.js`, empty and error drawn as its own text, lines 382 to 391)
shows no `.kp-stateplate`: 4 driven lines and 1 wiring line on `/translate`.
The chassis Status card is not ours. Needs chassis 3.9.0 (kp-themes with
`.kp-formgrid`) to confirm in production; verified here with the new css and
js served by interception.

## At the bump to chassis-rs 3.9.0 (kp-themes 11.1.0)

kp-themes 11.1.0 fixes gaps 23 (tool-row heights), 26 (trace state word
column), 27 (`.kp-diff__line` minmax) and 33 (entity action tracks), and
35 (code view state plate). Remove the app's workarounds for them, as
kp-themes eb9d9825 did in research/app-hsb-align and app-hsb-translate,
then measure align and states again on `/` and `/translate` at 1280 and
390 against the real 3.9.0 (no interception); wire the translate result's load to `kp-code-retry` (the code view's plate Retry, kp-themes 2b2bed2a). 11.1.0 also makes buttons
and toggles at least 44 x 44 px on a coarse pointer: check the phone
layouts still hold.

Also in 11.1.0 (kp-themes fd795018): gaps 28 and 29 (`.kp-attention`
with `data-kp-subgrid` gets icon | words | actions tracks and keeps the
dismiss cross in its own column) and 31 (`data-kp-formgrid-stage` +
`data-kp-formgrid-rows="subgrid"` lets cards share rows). Gaps 30
(`.kp-refresh` label slot and fold) and 32 (`.kp-field` cap) were already
fixed. Drop the app's workarounds for all five; this app does not load
auto.js, so call `attachFormgridStages` from web/dashboard.js itself
(see kp-themes research/app-hsb-align for the use).

## States, ab7a749b (19/10/2026)

kp-themes `measure-states.mjs` at main ab7a749b (one loading look per kind), formal and dark, against the real
app with `/static/kp/**` served from that checkout. Before: `/` 0 lines at 1280 and 390; `/translate` 1 line at each
width (result code view had no failed plate when the preview request failed). After: 0 and 0 everywhere. Fixed:
the unreachable-service answer now leaves the result card ready and draws the code view's own failed plate
(`setCode` error, Retry wired to `kp-code-retry`), the reason going into the problems list. `measure-align` against the
same main found the needs-attention band broken (our `grid-template-columns` override removed the package's
named icon | words | actions lines, alerts 59.7 px off the band at 1280): health.css now only shares the rows, as
research/app-hsb-align. Align is 0 for all our sections at 1280 and 390; the three chassis "Status" card lines remain.
The app has no graph or network-graph component, so there are no graph lines.

Measuring after the bump: no hand-written wrapper any more. kp-themes
43d46255 gives measure-gaps, measure-states and measure-align one option
set: `--url`, `--login-form <url>,<userField>,<passField>,<submitSelector>`
(credentials from `KP_LOGIN_USER` / `KP_LOGIN_PASS`, never on argv),
`--cookie`, `--storage-state`, `--assets-from /static/kp=<kp-themes
checkout>` and `--ignore-https-errors`. Against the real 3.9.0 build no
`--assets-from` is needed.
