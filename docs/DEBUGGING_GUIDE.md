# Debugging guide — HTTPSwitchboard

Start here when something is not arriving. The order below is the order
the evidence appears in, so you can stop as soon as it points somewhere.

## The evidence trail

A message leaves four traces, and each one tells you a different thing:

1. **The answer to the sender** (inbound profiles only). A 2xx means the
   destination accepted it. A 502 means it did not (or refused it for good
   with 400, 413 or 422, or the template could not render it), and the body
   says which profile and why. A 400 means the body was not valid JSON; the
   body says what is wrong and does not repeat it.
2. **One JSON log line per message**, in the kit's shape: the message
   fields are nested under `fields`, e.g. `fields.profile`, `fields.source`,
   `fields.outcome`, `fields.duration_ms`, `fields.attempts`, with target
   `http_switchboard::events` (`src/obs/events.rs`). `outcome` is one of
   `delivered`, `failed` (inbound, destination did not take it),
   `handed-back` (kyu, temporary failure), `dead-lettered` (kyu, can never
   work), `given-up` (inbound, can never work) or `bad-json` (inbound, the
   body was not JSON). This is what to query in Loki.
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
| The service will not start | A config error. It refuses to start half-working. | `http-switchboard --check --config <file>` — the message names the file, the profile, the fault and the remedy. | Fix what it names. Every startup error carries a "What now". |
| `state: "denied"` on a profile | The hub token was rotated and this service was not restarted. It reads secrets once, at startup. | `/healthz` shows `denied`; the log line carries the remedy. | Mint an app token on kyu's Apps page, put it in the environment, redeploy. |
| `state: "hub-down"` | kyu is unreachable from this container. | The log has one transition line with the connection error. | Check the hub and the address in `[kyu] base_url`. Nothing is lost meanwhile: unacknowledged messages stay on the hub. |
| `state: "failing"`, messages piling up on the hub | The destination is refusing or not answering. | The log's `delivery_failed` line carries the receiver's status and a remedy. | Fix the receiver. The messages are waiting, not lost. |
| Alerts arrive twice | An acknowledgement the hub refused, usually because the delivery outlasted the lease. | Look for `"event":"ack_failed"`. | Lower `timeout_ms` or `retries`, or raise `lease_ms`. Duplicates are safe by design — HA's `ack_id` is per alert name, so a repeat replaces its own notification. |
| A notification arrives with an empty title or body | Not possible here by construction: a missing field is a per-message error. If the *text* is empty, the source sent it empty. | `http-switchboard test --profile … --input …` with a recorded message. | Look at the source's payload, not at the template. |
| One message never arrives, the rest do | It could not be rendered, was not JSON, or the destination refused it with 400, 413 or 422, so it was dead-lettered once rather than cycled. Health does not change. | `"outcome":"dead-lettered"` (kyu) or `given-up` / `bad-json` (inbound) in the log; the dashboard counts it as "N given up" with a one-line reason; a kyu message is visible on kyu's dead-letter list. | Read the reason in the log line; fix the template or the sender, then requeue from the hub's dashboard. |
| The sender gets 400 | The body is not valid JSON. The profile's state is unchanged. | The 400 body names the parser's position; the log line says `"outcome":"bad-json"`. | Send a well-formed JSON body with `content-type: application/json`; sending it again is refused the same way. |
| The sender gets 401 | Since 3.0.0 every inbound path is behind the kit's door: the request carried no sender token, a wrong one, or one that has been revoked. | The 401 body is the kit's `{error, remedy}` and names the header to send. `chassis clients list --url … --token-env HTTP_SWITCHBOARD_TOKEN` shows which senders exist and which are revoked. | Issue or re-issue: `chassis clients issue <sender> --url … --token-env HTTP_SWITCHBOARD_TOKEN`. |
| The service will not start, or `--check` refuses | 3.0.0 compiled the dashboard in, so `HTTP_SWITCHBOARD_TOKEN` and `HTTP_SWITCHBOARD_SECRET_KEY` are mandatory. The systemd unit runs `--check` as `ExecStartPre`, so a missing pair stops the unit before it binds. | The refusal names both variables and the command. | `http-switchboard gen-secret` on a terminal, both lines into the env file, then `systemctl start`. |
| The sender gets 503 with `retry-after` | More deliveries in flight than the bound allows. | It is a refusal, not a loss — the body says so. | Retry. If it keeps happening, the destination is too slow: look at the duration metric. |
| Uptime Kuma is green while nothing arrives | `/healthz` answers 503 as soon as a profile is failing, denied or cut off, so a monitor on it is red then. Green with nothing arriving means no profile is in one of those states: a profile that has not tried yet reads `starting`, and a message given up leaves the state alone. | `curl -i /healthz` and read each profile's state; the dashboard's *Needs attention* and the "given up" count. | Point the monitor at `/healthz` (`?strict=1` answers the same). |
| The container restarts in a loop | Its healthcheck is failing. It runs `/usr/local/bin/http-switchboard --healthcheck`, which is liveness and exits 0 while a profile fails, so this means the process itself is not answering — not that a receiver is down. | `docker logs`; the startup error is on stderr. | Almost always a config error the container cannot start with. |
| The end-to-end tests cannot start a hub | `kyu-latest` could not reach GitHub, or the download failed its signature check. | The panic names kyu-latest and its stderr. | Check `gh auth status`, or set `KYU_BIN` to a kyu binary. |

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
