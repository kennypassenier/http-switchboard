# Handover — folding HTTPSwitchboard into Homelab Rust

Written 2026-08-30 for a session opened in `~/Projects/homelab`. Kenny's
sequence: the deployment drill first (done), then the rest of the
procedure (done), and **just before the retrospective, fold this project
into Homelab Rust so the orchestrator owns the deployment**.

This is a change to a project whose feature list is frozen, so it starts
with a **mini-round in that project**, not with an edit.

## What lands there

`deploy/homelab-preset/` in this repository holds the proposal, and it
was **rewritten on 2026-08-30 to match how that project's presets are
actually shaped** — read against `presets/kyu/`, which is the closest
real example, and against `client/src/scaffold.rs::PresetMeta`. The first
draft was written from the outside and got five things wrong; they are
listed below so the same guesses do not come back.

- `preset.yml` — `description`, `ram_mb: 256`, `cores: 1`, `disk_gb: 4`.
  A **new** managed guest: A1 forbids the orchestrator managing a
  pre-existing one, so it cannot be placed on CT 113 (Prometheus) or
  LXC 109 (kyu).
- `http-switchboard/docker-compose.yml` — the app directory inside the
  preset, which is the layout every real preset uses.
  Image `ghcr.io/kennypassenier/http-switchboard:latest` (1.0.0 is
  published and pullable anonymously — verified 2026-08-30).

**What the first draft had wrong**, all five found by reading the real
presets rather than by reasoning:

1. A flat `compose.yml` instead of `<app>/docker-compose.yml`.
2. `preset.yml` keys `name:` and `env:`, which are not in `PresetMeta`.
   Unknown keys are dropped silently, so the file would have loaded and
   simply not meant what it said.
3. No `__STACK___net` external network block, which every preset has.
4. A relative `./config.toml` bind instead of the `/appdata/__STACK__/…`
   host bind the scaffolder creates and restic backs up.
5. `environment: [KYU_TOKEN]` instead of `env_file: .env`. This one
   matters beyond formatting: it means my claim in point 2 below is only
   half right — the secret arrives as an app `.env` pushed from the
   client's vault, and `/var/lib/homelab/secrets/` is not the thing the
   compose file reads.

It also answers a question this handover previously left open: **the real
config lives in `/appdata/<stack>/http-switchboard-config/`**, not in
either repository. That is also where restic finds it.

## Three things that session needs to know

1. **The healthcheck must use plain `/healthz`, not `?strict=1`.**
   Liveness answers "is this process alive"; `?strict=1` answers "is it
   doing its job" and goes 503 when *Home Assistant* is down. Wiring the
   container healthcheck to the strict one makes the orchestrator restart
   this service because someone else is broken — and each restart resets
   the pump state, turning "exactly one failure event" into one per
   restart. Uptime Kuma watches the strict one; the container does not.
2. **The secret path is D12**, not `latch run`: the client resolves
   `KYU_TOKEN` with `latch cat` at deploy time and ships it into the host
   vault at `/var/lib/homelab/secrets/`, which composes it into the
   container's environment. The scope document carries a dated correction
   saying exactly this — an earlier version claimed `latch run`, which
   cannot work for a distroless container started by compose.
3. **The deployed config must not go into a public repository — and
   `kennypassenier/homelab` is public too** (checked, 2026-08-30). The
   profile that matters names the Home Assistant webhook id, and that id
   is the credential for the notification dispatcher. This project's own
   repo is public as well, which is why its example config carries a
   placeholder. So the real config needs somewhere that is neither: the
   host vault beside the secrets, or latch, or a private path — that
   choice belongs to the mini-round, and it should be made deliberately
   rather than by whoever writes the file first. This exact class of
   mistake already happened once in this project: the webhook id was
   written into a public SCOPE.md and only the pre-gate scan caught it.

## What the drill proved, and what it did not

**Proved on real hardware (2026-08-30, scratch container 192, deleted
afterwards):** the binary runs under systemd on a Debian 13 LXC; the
config check fails closed without its token, with the remedy; a message
published on the real kyu hub was translated and delivered to Home
Assistant in 7 ms; the subscription policy is in force on the real hub
(`lease_ms` 60000); and restore-from-zero works — destroy, rebuild, and a
second message flowed again.

**Not proved:** deployment *through the preset*. The drill installed the
binary by hand. The container image itself is exercised by this project's
CI on every push (it starts, answers `/healthz`, passes its own
`--healthcheck`, and reaches an https destination), but the orchestrator
path has never run. That is precisely what this handover is for.

## And then, finally

With the preset adopted, one thing still stands between this and the
project's flagship criterion: **Alertmanager is not deployed**. That is
the homelab project's own metrics round (`node_exporter`, `alertmanager`,
`smartctl_exporter`), on hold since 2026-08-29 waiting for exactly this
service to exist. It exists now.

---

## Request from the homelab project — 2026-09-02: it only speaks when something is wrong

Measured on the running service (CT 109, `109-app-kyu`), not inferred:

```
journalctl -u http-switchboard --since "24 hours ago" | wc -l   →  9
```

Nine lines in a day, and all nine are `level=warn`. They are good lines — the
hub going down at 00:41 and coming back thirty seconds later is exactly what
somebody would want to know, and the messages carry a "What now:" sentence,
which is better than most. The problem is what is missing between them.

For comparison on the same container: `kyu` writes 454 lines in 24 h,
`kyu-runner` 30.

**What the homelab cannot answer today:** did the switchboard translate
anything at all today, and how much? A day where it forwards 200 messages and
a day where it is wedged and forwards none look identical from outside —
both produce silence. The 00:41 hub outage was noticed by nobody at the time,
and would have been noticed by nobody later either, because there is no
baseline of normal activity to compare it against.

**The request, and it is a request:** something at INFO on the successful
path. One line per delivered message would be plenty, or a periodic summary
(`translated 37 messages in the last hour, 0 failures`) if per-message is too
noisy for a service that may burst. You know the traffic shape and we do not.

**Why now.** Kenny looked at the Grafana dashboards on 2026-09-02 and found
"no data" everywhere for this container. Two causes underneath: CT 109 runs no
promtail at all, so nothing reaches Loki — that half is ours and we are fixing
it. The other half is that even with promtail, nine warning lines a day is not
a picture of a working service. His words: *"kijk is of die daadwerkelijk logs
schrijven, als dit gewenst is kan de software daarop aangepast worden."*

**If the answer is no,** that is a real answer: a translator that is silent by
design when nothing needs saying is a defensible choice, and we will write it
down as deliberate quiet rather than as a gap. Say so and we will record it
that way.

No session for this project was running when this was written, which is why it
is a note in the repository rather than a message.
