# Changelog

All notable changes to HTTPSwitchboard. The format is loosely
[Keep a Changelog](https://keepachangelog.com/); versions follow semver,
where the promise is about the **config file format**, the two HTTP
endpoints and the CLI verbs — not about the internals.

## 1.0.0 — 2026-08-30

The first version. 1.0.0 is a promise about the **config file format**,
the two HTTP endpoints and the CLI verbs; breaking any of those means
2.0.0. The internals are not part of it.

Proven on real hardware before this release (the field test, 2026-08-30):
on a throwaway container on Proxmox the service refused to start without
its token — with the remedy — then ran under systemd, picked a message
off the **real kyu hub**, translated it and delivered it to Home
Assistant in 7 ms, where the automation ran to completion. The container
was then destroyed and rebuilt from nothing and a second message flowed
again.

**What is still not true, and is not claimed:** Alertmanager itself is
not deployed, so no *genuine* alert has travelled the chain — the
project's flagship criterion stays open until it is. Deployment through
the homelab preset is likewise unproven; the drill installed the binary
by hand. `docs/TEST_PLAN.md` lists both, and the operations runbook says
which of its steps have been executed.

### The service

- **Profiles** as the whole model (K1-K4, K9): a source, a translation
  and exactly one destination in one TOML file. Sources are an inbound
  HTTP path or a kyu topic; destinations are a URL or a kyu topic.
  Fan-out is several profiles on one source.
- **Jinja translation** (K5) with JSON autoescape and strict undefined:
  a quote in a value cannot rewrite the document, and a field that
  disappears is an error rather than an empty string.
- **The whole envelope** (K6, K7): headers and content type per profile;
  method and templated path segments, with scheme, host and port always
  from the config.
- **Secrets from the environment** (K8), never read from a file by this
  project, never printed.
- **Fail-closed startup** (K10): a config that does not hold up stops the
  process, naming the file, the profile, the fault and the remedy.
- **Ack only after delivery** (K2, G7 in the scope): a message from the
  hub is acknowledged only once the destination accepted it; a refused
  delivery is handed straight back.
- **Honest answers to a sender** (W1): answered only after delivery, and
  the answer never names the destination.
- **Retry inside the lease budget** (W3, W2) with a config check that
  refuses a profile whose attempts cannot fit.
- **Visibility** (W5, W6, W7): `/healthz` for liveness, `/healthz?strict=1`
  for "is it doing its job", Prometheus counters including delivery
  duration, and one JSON log line per message.
- **A door on the inbound side** (W8) and **self-reporting** (W11): one
  event when a profile falls over, one when it recovers.
- **`forward_error_body`** (W12): the receiver's own error text back to
  the sender, per profile, off by default.
- **CLI**: `--check-config`, `test` (dry run, sends nothing),
  `--healthcheck` for a container with no shell.
- **The Home Assistant side** (K13) delivered and proven live: a new
  webhook and automation filtering on `firing`.

### Deliberately not in this version

- No filtering, aggregation, batching or polling (NG1, NG2, NG4).
- No storage of its own (NG3).
- Inbound traffic from the internet (NG5) — outbound is allowed; inbound
  is a design of its own, postponed on purpose.
- Non-JSON destinations, which are refused at startup rather than
  supported with unescaped values.
