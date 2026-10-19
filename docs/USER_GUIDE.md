# User guide — HTTPSwitchboard

Everything here is written from the code and the tests. Each section
names the feature ID it describes and, where it makes a claim, the test
that proves it — those names are checked mechanically against the suite
before this document is approved (standing rule 11a).

## The model: one profile, one coupling

A profile names a **source**, a **translation** and exactly one
**destination** (K1-K4, K9). Several profiles may share a source; that
is how one message reaches two places, each with its own failure
handling.

```toml
[[profiles]]
name = "alertmanager"          # also its label in logs and metrics
subscription = "switchboard"   # the kyu subscription; its own key on purpose
from = { kyu_topic = "alerts.raw" }
to   = { url = "http://homeassistant.lan:8123/api/webhook/ID" }
content_type = "application/json"
body = '''{"alert": {{ alerts.0.labels.alertname }}}'''
```

**Why `subscription` is its own key.** It defaults to the profile name,
but renaming a profile would otherwise create a *new* subscription on
the hub and abandon everything still unacknowledged. Set it once and
leave it.
*Proven by* `k10_the_subscription_is_its_own_key_and_is_validated_too`.

### Sources

| `from` | Meaning |
|---|---|
| `{ http_path = "/alertmanager" }` | An incoming POST on that path (K1). |
| `{ kyu_topic = "alerts.raw" }` | Long-poll that topic on the hub (K2). |

### Destinations

| `to` | Meaning |
|---|---|
| `{ url = "http://…" }` | POST there (K3). `https` works, inside or outside the house. |
| `{ kyu_topic = "alerts.homelab" }` | Publish to the hub (K4). |

## Writing the translation (K5)

The template language is Jinja — the same one Home Assistant uses, so
`{{ alerts.0.labels.alertname }}`, `| default("x")`, arithmetic and
conditionals all behave the way they do there.

**Do not wrap values in quotes.** For a JSON profile the engine emits a
complete JSON value and escapes it:

```
{"summary": {{ alerts.0.annotations.summary }}}     ✓
{"summary": "{{ alerts.0.annotations.summary }}"}   ✗ refused at startup
```

The wrong form is refused when the service starts, with a message
telling you which way round it goes.
*Proven by* `k10_a_quoted_interpolation_in_a_json_profile_is_refused_at_startup`.

Two consequences worth internalising:

- **A quote inside a value cannot break the document.** An alert summary
  containing `", "severity": "info` arrives as text; it cannot add a
  field or change the severity.
  *Proven by* `ar7_a_quote_in_an_alert_summary_cannot_rewrite_the_document`.
- **A field that disappears is an error, not an empty string.** If the
  source renames something, you get a per-message error with a remedy —
  not "Homelab-alarm: " with an empty body while every layer reports
  success. Write `| default(...)` when empty is what you want.
  *Proven by* `ar7_a_missing_field_is_an_error_with_a_remedy_not_an_empty_value`.

### Trying a template without sending anything (W4)

```bash
http-switchboard test --profile alertmanager --input recorded.json
```

Prints the destination, the content type, the header **names** (never
their values) and the rendered body. This is the fastest way to write a
profile: seconds instead of "edit, restart, provoke an alert, look at
your phone".
*Proven by* `w4_the_dry_run_shows_the_result_and_sends_nothing` and
`w4_a_header_value_is_never_printed`.

## The envelope (K6, K7)

```toml
headers = { authorization = "${HA_TOKEN}" }   # values from the environment
method = "PUT"                                # default POST
to = { url = "http://host/devices/{{ id }}/state" }
```

Headers are fixed values from the config, never templates — that is
deliberate: a templated `Host` header would let a message choose its own
receiver behind a reverse proxy.

A **path segment** may come from the message (K7). Scheme, host and port
are split off before rendering and put back after, and each value is
percent-encoded by the engine, so a slash or a whole URL in a value
becomes one harmless segment and a traversal is refused outright.
*Proven by* `k7_a_value_cannot_add_a_path_segment_or_leave_the_host`,
`k7_a_templated_host_is_refused` and
`k7_awkward_values_in_a_templated_path_stay_one_segment`.

## Secrets (K8)

The config holds a reference; the value comes from the environment the
service is started in.

```toml
token = "${KYU_TOKEN}"
```

A missing variable stops startup and names the variable. Secret values
never appear in a log line, an error message or the answer to a sender —
and that is asserted rather than assumed.
*Proven by* `k8_a_missing_environment_variable_stops_startup_and_names_the_variable`,
`k8_no_secret_value_appears_in_any_config_error`,
`k6_a_secret_header_value_appears_in_no_error_message` and
`k8_no_secret_reaches_the_log_of_a_running_service`.

## What happens when something fails

### From a kyu topic (K2, W3)

Poll → translate → deliver → **only then** acknowledge. A refused
delivery is handed straight back to the hub, which redelivers it; the
message is never lost because we acknowledged too early.
*Proven by* `k2_a_message_is_acknowledged_only_after_it_was_delivered`,
`k2_a_refused_delivery_is_handed_back_and_never_acknowledged` and, against
a real hub, `k2_e2e_a_refused_delivery_comes_back_and_a_delivered_one_does_not`.

Inside one claim the delivery is retried with growing pauses (1 s, 2 s,
4 s). The config refuses a profile whose timeout × attempts does not fit
inside the lease, because a delivery finishing after the lease expires
produces a duplicate by construction.
*Proven by* `w3_two_failures_then_success_is_one_delivery_and_three_attempts`
and `k10_a_retry_budget_that_does_not_fit_the_lease_is_refused`.

A message that can **never** work — not JSON, or missing a field the
template needs — is dead-lettered once so it becomes visible, instead of
cycling until its attempts run out.
*Proven by* `k2_e2e_a_message_that_can_never_work_is_settled_not_looped`.

### From an incoming webhook (W1, W12)

The sender is answered only after the delivery succeeded. If it failed,
the sender gets a 502 saying so — because this service stores nothing,
answering "accepted" and failing afterwards would lose the message while
the sender believed it arrived.
*Proven by* `w1_the_sender_is_told_the_truth_when_the_destination_refuses`.

The answer never names the destination: an address is deployment
configuration, and a Home Assistant webhook id is a credential.
*Proven by* `w1_the_answer_never_tells_the_sender_where_the_message_was_going`.

If you control both ends and want the receiver's own words back, set
`forward_error_body = true` on that profile (W12). It is bounded, stripped
of control characters, and refused on a kyu source where nobody is
waiting for an answer.
*Proven by* `w12_the_receivers_own_words_come_back_only_when_the_profile_asks`
and `k10_forwarding_is_refused_where_there_is_nobody_to_answer`.

## Watching it (W5, W6, W7, W11)

| Endpoint | Answer |
|---|---|
| `GET /healthz` | 200 while the process is alive, with each profile's state and how long ago it last succeeded. |
| `GET /healthz?strict=1` | 503 when any profile is failing, denied or cut off. This is the one a monitor should watch. |
| `GET /metrics` | Prometheus counters per profile: received, delivered, failed, and delivery duration. |

The split is deliberate: the container's own healthcheck uses plain
`/healthz`, so the orchestrator does not restart this service because
Home Assistant is down.
*Proven by* `w5_a_failing_profile_is_visible_without_restarting_the_container`.

Every message produces exactly one JSON log line (profile, outcome,
duration, attempts); state changes produce one line each, not one per
attempt. With `[reporting]` configured, a profile falling over publishes
**one** event to that topic and recovery publishes one more — never one
per message.
*Proven by* `w11_e2e_a_failing_profile_reports_once_and_recovery_reports_once`
and `w11_e2e_an_inbound_profile_reports_itself_too`.

## Guarding the door (W8, as of 3.0.0)

Every inbound path is behind a **sender token**. There is nothing to put
in the config: the door is the kit's, and a token is issued per sender
from the machine the service runs on.

```bash
http-switchboard gen-secret            # once, into the env file
chassis clients issue alertmanager \
  --url http://127.0.0.1:8080 \
  --token-env HTTP_SWITCHBOARD_TOKEN   # prints the sender's token, once
```

The sender then posts as it always did, with that token:

```
authorization: Bearer <the token that was printed>
```

Without it, or with a revoked one, the answer is 401 and nothing is
translated or delivered. `chassis clients revoke` takes effect on the
next request — no config edit, no restart. The same list is on the
service's own **Senders** page, behind the admin login.

Before 3.0.0 this was a per-path `inbound_token` in the profile. A
config that still carries one is refused, with the command above in the
message.
*Proven by* `w8_a_sender_without_a_token_never_reaches_the_translation`,
`w8_a_senders_token_opens_the_door_and_a_wrong_one_does_not`,
`w8_revoking_a_sender_shuts_the_door_on_the_next_request` and
`k10_a_two_x_inbound_token_is_refused_with_the_command_that_replaces_it`.

## The status page (3.0.0)

The door brought a dashboard with it. Behind the admin login (the
`HTTP_SWITCHBOARD_TOKEN` from `gen-secret`) the status page carries four
sections of its own (kp-themes 11 views; the page refreshes itself every
ten seconds):

- **Needs attention** (first of them): the profiles that are not well,
  worst first, each with *See the fix* (scrolls to its alert); a profile
  leaves the list when it works again, and "All profiles are working"
  takes its place. Under it, one alert per profile says what is wrong,
  why and what to do, with a command to copy where there is one (*hub
  down*: `systemctl status kyu`; *failing*: `journalctl -u
  http-switchboard`) and a button that rechecks that profile. Only the
  "has not tried yet" note can be closed. When a profile that reads from
  the kyu hub is hub down or denied, a banner above them says the hub is
  unreachable or refused our token; *Retry now* rechecks the profiles and
  looks again, and the banner says so when the hub is back.
- **Profiles**: one card per profile (the default) with its state in
  words, delivered, failed and average time, flags for the source and
  destination kind, a bar with the share delivered, and the buttons
  *Recheck this profile* and *Copy test command*. The **View** switch
  shows the same data as a **Map** instead: sources left, destinations
  right, one line per profile (dashed while `starting`, toned where it
  stops); the choice is remembered in the browser.

  The card that holds the profile cards has four faces and keeps one
  height in all of them: the cards, *Reading the profiles…* (the busy card
  over the dimmed cards), *No profiles are configured* (with *Reload
  profiles*) and *The profiles could not be read* (with *Try again*; a
  failed refresh shows it too). Each card ends with **last delivery 3 min
  ago** (or **no delivery yet**), ticking by itself; the exact moment
  (dd/mm/yyyy HH:mm, Brussels time) is in its tooltip. The line turns to a
  warning plate after **a day** without a delivery: the configuration has
  no expected quiet time per profile, and an alert profile is quiet exactly
  when nothing is wrong, so one day is the default for every profile
  (`QUIET_AFTER_S` in `web/profiles.js`). Rest the pointer on (or tab to) a
  **profile name**, on a card, in the path picker, in the settings and in
  the Recheck table, and a small card shows its status, source,
  destination host only, content type, a short template label (the field
  names, never the template), the three exact counts and the last success.
- **Path of a profile**: sender, door, translation, destination; it stops
  at the first refusing hop (hub refused our credentials, hub down,
  receiver failing) and says why.
- **Settings and retry budget**: source, destination host and port (never
  the path: it is the webhook credential), content type, timeout, retries,
  max attempts (hub hand-outs before a dead letter), lease, and a bar for
  the retry budget against the lease with a mark at 100 %. A profile over
  its lease shows a red piece past the end. An HTTP source holds no lease
  and has no hub, so it shows the budget in words only.
- **Recheck**: the **Recheck profiles** button (spinner and
  "Rechecking…", then a toast "N profiles back to starting"; a failure is
  a toast that stays, with *Try again*) and a table with an *Actions* menu
  per row: *Recheck this profile* (greyed with its reason when the
  profile is already starting) and *Copy test command* (a curl line for an
  HTTP source; the `test` dry-run command for a kyu source).
- **Reloads**: every time the profile file was read again
  (`systemctl reload http-switchboard`), newest first, as a timeline with
  its date (dd/mm/yyyy HH:MM) and the outcome in one column: *Applied*
  (names the profiles that changed), *Needs a restart* and *Invalid* (both
  say why). A count line ("8 reloads · 4 applied · 2 need a restart · 2
  invalid") follows every arrival; while you read older rows a pill counts
  the new ones ("1 new reload · back to the top"). When the latest reload
  was refused, an alert says so with the command to reload again (Copy); it
  goes away when a later reload is applied. The history is kept in memory,
  holds the last 50 and starts empty after every restart. Reasons never
  quote the file: values and the lines a parse error echoes are left out.

- **Last 24 hours** (feat-dash-kp11-30..39): received, delivered, failed
  and the average delivery time of one profile (pick it above the card),
  each with its trend by the hour; numbers count to their new value. The
  *Tiles* buttons switch to quiet sparklines where *Failed* is a filter
  that lists only the failed deliveries. Under it, *Uptime by the hour*:
  one strip of 24 cells per profile (green up, amber some deliveries
  failed, red every delivery failed or the hub refused or was away, grey
  before the service started); point at a cell for the hover card, press
  it for the hour's quick info (*Hover card only* turns that off). An hour
  without traffic keeps the health the profile had when it began.
  The history is **in memory and starts empty at every restart**; it keeps
  at most 24 hours (`GET /history.json`, admin login; counts, times and a
  cause word, never a payload, header, URL or error text).
- **Page refresh**: a chooser (Paused, 15 s, 30 s, 1 min, 5 min, any
  number of seconds; 30 s to start, remembered per browser) sets how often
  the profiles and the history are read again, with *Refresh now* and the
  R key. A failed refresh keeps what is drawn and retries. It replaces the
  earlier fixed ten-second poll.

That button exists for one situation. A profile's state only changes
when a message goes through it, so a profile whose source is an HTTP
path stays `failing` for as long as nobody posts to it — long after the
receiver that caused it came back, and `/healthz?strict=1` keeps
alarming. Recheck puts them back to `starting`, which means "nothing has
been tried since". It changes no message and no configuration.
*Proven by* `v1_the_status_page_names_the_profiles_and_calls_a_client_a_sender`
and `v1_recheck_clears_a_failure_a_profile_can_no_longer_clear_itself`.

### Try a translation

The **Try a translation** page (navigation, or *Try a translation* in a
profile's menu) shows what a profile makes of a message without sending
anything. Pick a profile, paste a sample message into the box (it checks
the JSON as you type, names the line that is wrong, and has line numbers
and Format/Minify), and the body that would be delivered appears next to
the profile's template, with a before/after comparison of the message and
the body. The box starts with an example built from the fields the
template reads. A default the template filled in (`| default(...)` on a
field the message lacks) is a quiet note; a template error says what,
why (the line, and which field the message lacks) and what to do, with a
button that puts the example message back. A message over 256 KiB is
refused with a sentence. Nothing is delivered: the page posts to
`POST /translate/{name}` (admin login), which runs the same
`translate::prepare` the pump uses; the answer never carries a header
value or the destination path. The shell equivalent is
`http-switchboard test`.

Everything else on those pages comes from the kit and is described in
`docs/KIT.md`, which `chassis sync` regenerates for the pinned kit
version — this guide deliberately does not retell it.

## Changing the config while it runs (feat-reload-1, 3.2.0)

`systemctl reload http-switchboard` (a `SIGHUP`) reads the config file
again. The next message uses the new version; a delivery already under
way finishes with the old one. A file that is not a complete, valid
config — a typo, or an editor caught mid-save — is refused as a whole
and nothing changes. A reload can change what lives inside a profile;
a change to the set of profiles, a source, a subscription, `[kyu]` or
`[reporting]` is refused with the advice to restart. Proven by
`tests/l14_reload.rs`.

## What this does not do

- **It does not filter.** Everything that arrives is forwarded;
  deciding what deserves attention is the receiver's job (NG1). That is
  why the Home Assistant automation carries the `firing` condition:
  Alertmanager also posts when an alert is over.
- **It does not collect, count or batch.** One message in, one message
  out (NG2).
- **It stores no message** (NG3, amended 2026-09-09): no queue, no
  database, no spool. Since 3.0.0 the kit keeps two stores of its own in
  the state directory — the senders that hold a token, and admin
  sessions. Losing that directory costs tokens, never a message.
- **It is not a poller** (NG4): it reacts, it does not go looking.
- **It accepts nothing from the internet yet** (NG5) — outbound is
  allowed, inbound is a decision of its own, deliberately postponed.
- **It knows nothing about Home Assistant** (NG6): the house lives in
  the config file, not in the code.
- **Only JSON destinations are supported.** Escaping is a mechanism only
  for JSON, so a profile declaring another content type is refused at
  startup rather than started with values interpolated raw.
