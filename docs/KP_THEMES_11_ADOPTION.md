# kp-themes 11.0.0 adoption plan

Kenny, 19/10/2026 01:11: every kp-themes project uses as many of the new
components as make sense, every option to the full; usability and
cool-factor decide; new use cases are reported. Chain: kp-themes 11.0.0,
then chassis-rs, then this app.

## What this service shows today

| Surface | Owner | Today |
|---|---|---|
| Login, senders (clients), passkeys, status page shell | chassis-rs kit | kit markup |
| Profiles section on the status page | this app (`src/dashboard.rs`) | text rows `state · shape · counts`, one button "Recheck profiles" that answers raw JSON |

The kit lets this app add raw markup to a section (`Section::html`), its
own pages (`App::page` + `dashboard_routes`) and live pushes (`Live`,
Server-Sent Events). So the app is not limited to label/value rows.

## Mapping: component to place

Every row below is a proposal; each visual choice goes to Kenny as a demo
on the Agenda (`kp-themes/research/app-http-switchboard/demo.html`).

| # | Place | kp-themes 11.0.0 component and options | Data it needs |
|---|---|---|---|
| 1 | Profiles section | **"Used by" map** (`.kp-uses`): sources left (kyu topic `alerts.raw`, `POST /uptime-kuma`), destinations right (HA webhook), one line per profile; `hot` on a failing line, `dashed` on a profile still `starting`, `lit` on hover, `remember` | config + health (exists) |
| 2 | Profiles section | **Entity grid** (`.kp-entity-grid`): one card per profile, figures delivered/failed, flags source kind/sink kind, meter success share, actions menu (Recheck this profile, Copy test command) | counters (exist) |
| 3 | Per profile | **Path trace** (`data-kp-trace`): hops sender → door/kyu topic → translation → destination; stops at the first refusal (Denied = hub refused credentials, HubDown, receiver failing) with `trace-verdict` | health (exists) |
| 4 | Above the section | **Attention band** (`.kp-attention`) worst-first for failing/denied/hub-down profiles, plus **problem alert** what/why/remedy (`.kp-problem`, `copy-value` = the journalctl line) | health (exists) |
| 5 | Per profile | **Freshness** (`data-kp-ago`) "last delivery 3 min ago" with `stale-after` = the profile's expected quiet time | `last_success_unix` (exists) |
| 6 | Profiles section head | **Key figures** (`.kp-kpis`, `--trend`) received / delivered / failed / avg duration, ticking (`data-kp-tick`) via Live | counters (exist); trend needs history (new) |
| 7 | Per profile | **Uptime strip** (`.kp-uptime`) last 24 h in hour cells up/degraded/down, `hovercard` | **new**: per-hour outcome ring buffer |
| 8 | Per profile detail | **Key-value list** + **meter** (`.kp-meter`): timeout, retries, lease; retry budget vs lease as a meter with a mark at 100 % (the AR rule made visible) | config (exists) |
| 9 | New page "Try a translation" | **JSON field** (`data-kp-json`, `json-check`, `-format`) to paste a sample message, **code block** (`.kp-code`, `prettyJson`, copy) for the translated body, **diff** of before/after shape | **new** route: render the template without delivering |
| 10 | Recheck button | **Flash button** + **toast** (`toast()` success "3 profiles back to starting") instead of a JSON answer | exists |
| 11 | Reload of profiles from disk | **Timeline** (`.kp-timeline`) of reloads with outcome applied / needs-restart / invalid, **unsaved/alert** for invalid with the reason | **new**: keep the last N reload outcomes |
| 12 | Hub link | **Connection banner** (`data-kp-connection`) online/reconnecting/refused for the kyu hub | health (exists) |
| 13 | Live updates | **Auto-refresh chooser** (`data-kp-refresh`) or Live push; **"N new" pill** on the reload timeline | Live channel (kit) |
| 14 | Empty/failed states | **Empty state** with art `none` (no profiles), `error`; **card data states** + busy card while loading | — |
| 15 | Hover on a profile name | **Hover card** (`data-kp-hovercard`) with the full shape: source, destination host, content type | config (exists) |

Kit-owned pages (login, senders, passkeys, version chip on the status
page) are adopted in the chassis-rs thread; ideas for them go there.

## New use cases found

- **Translation preview** (row 9): today the only way to see what a
  template makes of a message is to send one through to Home Assistant.
- **Per-profile recheck** (row 2): the button resets every profile; the
  entity card can reset one.
- **History of outcomes** (rows 6, 7): the service keeps counters only,
  so "was it down last night" cannot be answered on the page.
- **Reload history** (row 11): a reload that was refused is only in the
  journal and in a metric.

## Not adopted, and why

Calendar, week agenda, gantt, kitchen timers, wheel, rating, claim, QR,
thread, quote card, people components: this service has no dates,
people or media to show.
