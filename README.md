# HTTPSwitchboard

A switchboard is the panel where an incoming call is connected to the
line it actually needs to reach: the operator decides where it goes, not
the caller. That is this project's job. A system sends a message in the
shape it happens to speak; HTTPSwitchboard translates it and delivers it
where *our* configuration says — the sender never knows the receiver, and
the receiver never knows the sender.

> **Status, 2026-09-09: released, running, one criterion short.**
> 2.0.0 moved the service onto [chassis](https://github.com/kennypassenier/chassis-rs),
> the shared foundation that owns the command line, the configuration
> layers, `/healthz`, `/metrics` and the signed self-update;
> `docs/KIT.md` describes everything that comes from there. On 2026-08-30
> the whole chain ran on real machines: a message published on the
> production hub was translated by this service on a throwaway container
> and arrived in Home Assistant, and the restore-from-zero procedure was
> drilled. What is still missing is **Alertmanager itself**, which is not
> deployed, so the flagship criterion — a *genuine* alert travelling the
> chain — is not claimed. `docs/TEST_PLAN.md` says which claims rest on
> what.

The first customer is Prometheus Alertmanager, whose webhook format is
fixed and does not fit Home Assistant's receiver:

```
Alertmanager      →  kyu topic  alerts.raw
HTTPSwitchboard   →  subscribes, translates
                  →  Home Assistant webhook  →  notification dispatcher
```

## What it does

A **profile** is the whole model: a source, a translation and exactly
one destination, in one TOML file.

```toml
[kyu]
base_url = "http://10.10.10.9:8080"
token = "${KYU_TOKEN}"

[[profiles]]
name = "alertmanager"
subscription = "switchboard"
from = { kyu_topic = "alerts.raw" }
to = { url = "http://homeassistant.lan:8123/api/webhook/YOUR-WEBHOOK-ID" }
content_type = "application/json"
body = '''
{"alert": {{ alerts.0.labels.alertname }},
 "status": {{ alerts.0.status }},
 "severity": {{ alerts.0.labels.severity | default("warning") }},
 "instance": {{ alerts.0.labels.instance | default("unknown") }},
 "summary": {{ alerts.0.annotations.summary }}}
'''
```

A source is an incoming HTTP path or a topic on the kyu hub; a
destination is a URL or a kyu topic. The translation is Jinja — the same
template language as Home Assistant's, so nothing new has to be learned.
Values are **not** wrapped in quotes: the engine emits complete JSON
values and escapes them, which is what stops a quote inside an alert
summary from rewriting the document.

Three habits worth knowing before writing a profile:

- **A missing field is an error, not an empty string.** Write
  `| default(...)` when an empty value is what you mean.
- **A message can never change where it goes.** Scheme, host and port
  come only from the config.
- **No message is stored.** Durability is the hub's job: a message from a
  kyu topic is acknowledged only after the destination accepted it. Since
  3.0.0 the kit keeps two stores of its own in the state directory — the
  senders that hold a token, and admin sessions — so losing that
  directory costs tokens, never a message.

## Running it

```bash
http-switchboard --config /etc/http-switchboard/config.toml
http-switchboard --check --config /etc/http-switchboard/config.toml
http-switchboard test --profile alertmanager --input recorded.json
http-switchboard --healthcheck http://127.0.0.1:8080/healthz
http-switchboard --knobs
```

The config path is a flag, not a positional argument — it has been since
2.0.0, and `tests/l10_docs.rs` now hands every command line on this page
to the binary so this block cannot drift away from it again.
`--knobs` prints every setting the kit reads, with its environment
variable, its default and what it means, before any configuration is
opened.

`/healthz` carries the state of every profile and answers **503 as soon
as one is failing** — that is the endpoint a monitor watches. It is *not*
the one a container healthcheck should call: `--healthcheck` is the
liveness answer, and it exits 0 while a profile is failing, so the
orchestrator never restarts this service because the receiver is down.

<!-- health-table:start -->
| Probe | Every profile working | One profile failing |
|---|---|---|
| `GET /healthz` | 200 | 503 |
| `GET /healthz?strict=1` | 200 | 503 |
| `--healthcheck` | exit 0 | exit 0 |
<!-- health-table:end -->
This table is not written by hand. `tests/l11_health_claims.rs` measures
every cell against a running service and then refuses to pass unless this
document carries exactly what it measured — the fix for correction C2,
where three documents described a split that had been gone for two
releases.

`/metrics` is Prometheus text. Neither endpoint ever echoes message
content.

## Development

This project follows the development procedure in
`~/Projects/dev-procedure`. Every phase gate is recorded in `docs/`.

**One-time setup after cloning** — the gates are git-native, but
`core.hooksPath` is local config a clone cannot carry:

```bash
git config core.hooksPath .githooks
```

From then on a commit is refused unless `cargo fmt --check`, `cargo
clippy -D warnings` and the full test suite pass, and unless the message
carries the feature IDs it implements (`[K3, W2]`, or `[meta]`). The
gate sets `KYU_IMAGE` itself, so the end-to-end suites really run rather
than skipping themselves. CI repeats all of it on every branch; red
blocks `main`.

## Licence

MIT or Apache-2.0, at your option.
