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
