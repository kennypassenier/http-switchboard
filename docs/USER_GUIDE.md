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

## Guarding the door (W8)

```toml
inbound_token = "${HOOK_TOKEN}"
```

Requires `authorization: Bearer <token>` on that path, checked before
the body is used and compared in constant time. Profiles sharing a path
must agree on the token — one path is one door.
*Proven by* `w8_without_the_token_the_door_stays_shut` and
`k10_profiles_sharing_a_path_must_agree_on_the_token`.

## What this does not do

- **It does not filter.** Everything that arrives is forwarded;
  deciding what deserves attention is the receiver's job (NG1). That is
  why the Home Assistant automation carries the `firing` condition:
  Alertmanager also posts when an alert is over.
- **It does not collect, count or batch.** One message in, one message
  out (NG2).
- **It stores nothing** (NG3): no queue, no database, no spool.
- **It is not a poller** (NG4): it reacts, it does not go looking.
- **It accepts nothing from the internet yet** (NG5) — outbound is
  allowed, inbound is a decision of its own, deliberately postponed.
- **It knows nothing about Home Assistant** (NG6): the house lives in
  the config file, not in the code.
- **Only JSON destinations are supported.** Escaping is a mechanism only
  for JSON, so a profile declaring another content type is refused at
  startup rather than started with values interpolated raw.
