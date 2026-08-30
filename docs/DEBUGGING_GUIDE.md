# Debugging guide — HTTPSwitchboard

Start here when something is not arriving. The order below is the order
the evidence appears in, so you can stop as soon as it points somewhere.

## The evidence trail

A message leaves four traces, and each one tells you a different thing:

1. **The answer to the sender** (inbound profiles only). A 2xx means the
   destination accepted it. A 502 means it did not, and the body says
   which profile and why.
2. **One JSON log line per message**: `{"ts":…,"profile":…,"source":…,
   "outcome":"delivered|failed|handed-back|dead-lettered","duration_ms":…,
   "attempts":…}`. This is what to query in Loki.
3. **State-change lines**, one per transition, never one per attempt:
   `{"event":"state_change","from":…,"to":…,"detail":…}`. An hour of hub
   downtime is two lines, not thousands.
4. **`/healthz`** for the state right now, and **`/metrics`** for the
   counts since start.

If a message left *no* trace at all, it never reached this service — look
at the sender, or at the hub.

## Symptom → cause

| Symptom | Most likely cause | How to confirm | What to do |
|---|---|---|---|
| The service will not start | A config error. It refuses to start half-working. | `http-switchboard --check-config <file>` — the message names the file, the profile, the fault and the remedy. | Fix what it names. Every startup error carries a "What now". |
| `state: "denied"` on a profile | The hub token was rotated and this service was not restarted. It reads secrets once, at startup. | `/healthz` shows `denied`; the log line carries the remedy. | Mint an app token on kyu's Apps page, put it in the environment, redeploy. |
| `state: "hub-down"` | kyu is unreachable from this container. | The log has one transition line with the connection error. | Check the hub and the address in `[kyu] base_url`. Nothing is lost meanwhile: unacknowledged messages stay on the hub. |
| `state: "failing"`, messages piling up on the hub | The destination is refusing or not answering. | The log's `delivery_failed` line carries the receiver's status and a remedy. | Fix the receiver. The messages are waiting, not lost. |
| Alerts arrive twice | An acknowledgement the hub refused, usually because the delivery outlasted the lease. | Look for `"event":"ack_failed"`. | Lower `timeout_ms` or `retries`, or raise `lease_ms`. Duplicates are safe by design — HA's `ack_id` is per alert name, so a repeat replaces its own notification. |
| A notification arrives with an empty title or body | Not possible here by construction: a missing field is a per-message error. If the *text* is empty, the source sent it empty. | `http-switchboard test --profile … --input …` with a recorded message. | Look at the source's payload, not at the template. |
| One message never arrives, the rest do | It could not be rendered or was not JSON, so it was dead-lettered once rather than cycled. | `"outcome":"dead-lettered"` in the log; the message is visible on kyu's dead-letter list. | Read the reason in the log line; fix the template or the sender, then requeue from the hub's dashboard. |
| The sender gets 401 | The profile has an `inbound_token` and the request did not carry it (or carried the wrong one). | The 401 body says how to send it. | `authorization: Bearer <token>`. |
| The sender gets 503 with `retry-after` | More deliveries in flight than the bound allows. | It is a refusal, not a loss — the body says so. | Retry. If it keeps happening, the destination is too slow: look at the duration metric. |
| Uptime Kuma is green while nothing arrives | It is watching plain `/healthz`, which is liveness. | `curl /healthz?strict=1` — that is the one that goes 503. | Point the monitor at `?strict=1`. |
| The container restarts in a loop | Its healthcheck is failing. It uses plain `/healthz`, so this means the process itself is not answering — not that a receiver is down. | `docker logs`; the startup error is on stderr. | Almost always a config error the container cannot start with. |
| The end-to-end tests "pass" suspiciously fast | `KYU_IMAGE` is not set, so they skipped themselves. | They print a skip line. | The commit gate sets it; set it by hand for a bare `cargo test`. |

## Reading a delivery failure

```
profile 'alertmanager': the destination refused the message with status 404.
What now: the receiver does not know this address — check the URL, or, for a
Home Assistant webhook, that the automation with that webhook id still exists.
```

The status is always there. The receiver's *own* words are not, unless
that profile sets `forward_error_body = true` — an error page is not ours
and can name internal addresses.

## Things that look like bugs and are not

- **A duplicate after a restart.** Delivery is at-least-once by contract.
  A `kill -9` mid-delivery costs at most one duplicate, and the drill in
  `tests/l7_resilience.rs` exists to keep it that way.
- **A `resolved` alert that produces nothing.** This service forwards
  everything; the Home Assistant automation filters on `firing`. That is
  the design, not a dropped message.
- **`last_success_age_s: null`.** That profile has not delivered anything
  yet since start. It is not an error.
