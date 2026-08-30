# HTTPSwitchboard

A switchboard is the panel where an incoming call is connected to the
line it actually needs to reach: the operator decides where it goes, not
the caller. That is this project's job. A system sends a message in the
shape it happens to speak; HTTPSwitchboard translates it and delivers it
where *our* configuration says — the sender never knows the receiver, and
the receiver never knows the sender.

> **Status, 2026-08-30: built and tested, not yet deployed.** Every
> feature on the frozen list is implemented, with 99 tests green
> including six end-to-end suites against a real kyu message hub in a
> container. The service has **not** been deployed to a real machine, no
> genuine Alertmanager alert has travelled the whole chain, and the
> restore procedure has not been drilled. `docs/TEST_PLAN.md` says which
> claims rest on what.

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
- **Nothing is stored.** Durability is the hub's job: a message from a
  kyu topic is acknowledged only after the destination accepted it.

## Running it

```bash
http-switchboard /etc/http-switchboard/config.toml
http-switchboard --check-config /etc/http-switchboard/config.toml
http-switchboard test --profile alertmanager --input recorded.json
http-switchboard --healthcheck http://127.0.0.1:8080/healthz
```

`/healthz` answers 200 while the process is alive and carries the state
of every profile; `/healthz?strict=1` answers 503 when a profile is
failing, which is what a monitor should watch. `/metrics` is Prometheus
text. Neither ever echoes message content.

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
