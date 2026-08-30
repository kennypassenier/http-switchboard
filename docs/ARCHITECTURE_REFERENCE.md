# Architecture reference — the system as built

`docs/ARCHITECTURE_DECISIONS.md` records what was decided and why, with
the counter-arguments that shaped it. This describes what actually
exists, module by module, for someone reading the code for the first
time.

## The shape

```
                       config.toml  ──►  config::load  ──► Config
                                              │  (pure: text + env lookup)
   kyu topic ──► pump::pump_once ──┐          │
                                   ├──► translate::prepare ──► Delivery
   HTTP POST ──► inbound::handle ──┘                              │
                                                     adapters::deliver_with_retry
                                                                  │
                                                     HttpSink ──► URL or kyu topic
```

Two halves, on purpose (AR1):

- **Pure**: `config`, `translate`, `secret`. No network, no clock, no
  environment. Given text and a lookup, they answer the same way every
  time — which is why the hostile-payload suite and the recorded-payload
  fixture are ordinary unit tests.
- **Shell**: `adapters`, `pump`, `inbound`, `obs`, `app`. Everything that
  touches the outside world sits behind a trait (`Sink`, `Hub`, `Clock`)
  so the ordering that actually loses messages can be tested with fakes,
  and then again against a real hub.

## Module by module

| Module | What lives there |
|---|---|
| `config` | The TOML schema, every validation rule, and `${VAR}` resolution. Unknown keys are errors. Loading is pure: the caller passes the text and an `EnvLookup`. |
| `secret` | `Secret`, whose `Debug` and `Display` print `***`. The only way to the plaintext is `expose()`, named to be conspicuous at call sites. |
| `translate` | The render: minijinja with JSON autoescape and strict undefined, the post-render JSON check, and the destination — copied from the config, never rendered, except for path segments in K7 which are percent-encoded per value. |
| `adapters` | `HttpSink` (one client for both URL and hub destinations), `KyuHub` (the three verbs plus the policy write), `TokioClock`, and `deliver_with_retry`. |
| `pump` | The state machine for a kyu source: poll, translate, deliver, settle. Returns a `Step` rather than logging, so the caller decides what to record and tests can assert on it. |
| `inbound` | The axum router: one route per path serving the profiles behind it, the token check, the body cap, the in-flight bound, and `/healthz` + `/metrics`. |
| `obs` | The registry behind both endpoints, and the log-line builders. Counters and names only — never payloads. |
| `app` | Assembly: one pump task per kyu profile, the listener, graceful shutdown, and the shared transition handling both sides use. |

## The orderings that matter

**A message from the hub.** Poll → render → deliver (with in-claim
retries) → acknowledge. Never acknowledge first. On failure, hand the
message straight back rather than sitting on the claim: the hub's own
backoff waits better than we do, and waiting burns one of its five
attempts per failure.

**A message from a webhook.** Read → check the token → take a permit →
render → deliver → *then* answer. The sender is the only party that
still has the message, so it must not be told "accepted" until it is.

**A profile's state.** `Starting → Working ⇄ Failing / Denied / HubDown`.
One fact, modelled once in `obs::Registry`, read by `/healthz`, written
by both drivers through `app::note_transition`, and published as at most
one event per transition.

## The three things that are structurally impossible

Worth stating, because they are the point of several decisions:

1. **A message cannot choose its destination.** Scheme, host, port and
   topic come from the config. A templated path segment is percent-encoded
   per value, so it stays one segment.
2. **A value cannot break out of the JSON document.** Escaping is done by
   the engine for every interpolation, not by the template author
   remembering a filter.
3. **A secret cannot be printed by accident.** It exists only inside
   `Secret`, and the tests scan the logs, the errors and the sender's
   answer for the real values.

## What it depends on

| Dependency | Why |
|---|---|
| `tokio`, `axum`, `reqwest` | The async runtime, the inbound server, the outbound client. The same set as kyu and hub-bridge, so patterns transfer. |
| `minijinja` (json feature) | The template engine, and the JSON autoescape mode that makes escaping a mechanism. |
| `serde`, `serde_json`, `toml` | Config parsing with unknown-key rejection, and JSON in and out. |
| `thiserror` | Error enums whose `Display` carries the remedy. |

Externally it needs **kyu** (the message hub) for any profile using a
topic, and nothing else. Secrets come from the environment; on the
homelab that environment is composed by the orchestrator from the host
vault, which resolves them from latch at deploy time.

## Where the numbers come from

| Value | Default | Why |
|---|---|---|
| `timeout_ms` | 10 000 | One attempt's ceiling. |
| `retries` | 2 | Three attempts, with 1 s and 2 s pauses. |
| `lease_ms` | 60 000 | kyu's own default is 30 s, which the retry budget does not fit into — 38 s of attempts and pauses plus the margin. The service pushes this policy to the subscription after its first poll. |
| `max_attempts` | 5 | kyu's own default; how often the hub redelivers before dead-lettering. |
| Body cap | 1 MiB | Shipped from day one although configurable limits are rated Later. |
| In-flight bound | 32 | Past it, an immediate 503 rather than a queue. |
| Forwarded error text | 512 chars | Bounded because it is text we did not write. |
