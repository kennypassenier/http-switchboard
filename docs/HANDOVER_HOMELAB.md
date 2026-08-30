# Handover — folding HTTPSwitchboard into Homelab Rust

Written 2026-08-30 for a session opened in `~/Projects/homelab`. Kenny's
sequence: the deployment drill first (done), then the rest of the
procedure (done), and **just before the retrospective, fold this project
into Homelab Rust so the orchestrator owns the deployment**.

This is a change to a project whose feature list is frozen, so it starts
with a **mini-round in that project**, not with an edit.

## What lands there

`deploy/homelab-preset/` in this repository holds the proposal:

- `compose.yml` — the service as the orchestrator would run it. Image:
  `ghcr.io/kennypassenier/http-switchboard:1.0.0` (published, and pullable
  anonymously — verified 2026-08-30). Environment: `KYU_TOKEN`. Config
  mounted read-only at `/etc/http-switchboard/config.toml`.
- `preset.yml` — 256 MB, 1 core, 4 GB. A **new** managed guest: A1
  forbids the orchestrator managing a pre-existing one, so it cannot be
  placed on CT 113 (Prometheus) or LXC 109 (kyu).

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
3. **The deployed config must not go into a public repository.** The
   profile that matters names the Home Assistant webhook id, and that id
   is the credential for the notification dispatcher. This project's own
   repo is public, so its example config carries a placeholder. Check
   where the homelab repo actually stores per-stack config before writing
   the real one.

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
