# Test plan — HTTPSwitchboard

Phase 7 output. What each suite proves, what is deliberately not covered,
and why. Written after the hardening pass on 2026-08-30, when the suite
stood at **96 tests**.

Run everything with:

```bash
KYU_IMAGE=ghcr.io/kennypassenier/kyu:2.0.0 cargo test --all
```

The commit gate sets `KYU_IMAGE` itself. Without it the end-to-end
suites skip themselves and the run over-reports — that was a real gap
(Phase 7, G14) and is now closed at the gate rather than trusted to
memory.

## The suites

| Suite | What it proves | Runs against |
|---|---|---|
| `src/secret.rs`, `src/obs.rs` (unit) | A secret cannot print itself; log lines are valid JSON with fixed fields and survive awkward profile names; a warning is not dressed up as a state change; health and metrics carry names and counters only. | pure |
| `src/adapters.rs` (unit) | Every delivery-status remedy exists, says something specific, and 401/413/404/4xx/5xx do not share text; only the errors worth retrying are retried. | pure |
| `tests/l1_config.rs` (26) | One test per class of config error: startup stops, and the message names file, profile, fault and remedy. Includes a walk over seventeen configs covering fifteen distinct variants, so a new variant without a remedy fails the suite. | pure |
| `tests/l2_translate.rs` (8) | The recorded Alertmanager payload renders byte-for-byte through the shipped profile; a quote in an alert summary cannot add a field or change the severity; a missing field errors instead of rendering empty; a profile we cannot escape safely is refused at startup. | pure |
| `tests/l3_sinks.rs` (9) | Headers and body reach the receiver unchanged; a receiver that never answers times out with a remedy and frees the profile; two failures then success is one delivery and three attempts, with the pauses measured on a fake clock; no secret in any error. | a real TCP listener |
| `tests/l4_pump.rs` (12) | The ordering: deliver, then ack — never the other way. A refused delivery is handed back and never acked. A 404 makes the next poll ask for the history. "Denied" is its own state and carries the remedy. Against a **real kyu**: a message published before the first poll still arrives; a refused delivery comes back and the retry succeeds exactly once; the translation is publishable back onto a topic; the subscription policy is in force after the first poll; a message that can never work is dead-lettered once and does not return. | real kyu container |
| `tests/l5_inbound.rs` (14) | One route per path serving N profiles without a startup panic; the sender is answered only after delivery and told plainly when it failed; the destination never leaks back to the sender; no token means 401; a body over the cap is refused; a burst past the bound gets refusals carrying a remedy. | real TCP listeners |
| `tests/l6_observability.rs` (4) | Liveness stays green while a receiver is down, `?strict=1` goes 503 for the monitor; counters move; neither endpoint echoes message content. | real listeners |
| `tests/l6b_assembly.rs` (4) | The binary refuses a broken config and accepts the shipped one; a message travels the whole way through the running service; against a real kyu the service pumps a published message by itself and counts it. | binary + real kyu |
| `tests/l7_selfreport.rs` (2) | A failing profile produces exactly one event and recovery one more — for a kyu source and for an inbound one — and no event carries any part of a payload. | real kyu |
| `tests/l7_resilience.rs` (1) | `kill -9` while a delivery is in flight loses nothing: after a restart the message comes back and is delivered, unchanged. | binary + real kyu |
| `tests/l8_desired.rs` (7) | A path segment may come from the message while scheme, host and port cannot; awkward values (empty, structured, CR/LF, `?`, `#`) stay one segment; the dry run shows exactly what the real path produces and sends nothing, printing header names but never their values. | binary |
| `tests/l9_deployment.rs` (3) | `--healthcheck` answers correctly for a live and a dead service — the container's only probe, since the image has no shell; no secret reaches the log of the running binary; the CLI fails closed with a remedy on every wrong invocation. | binary |
| CI job `container image` | The image builds, starts with the shipped config path, answers `/healthz`, passes its own `--healthcheck` from inside a distroless image, and can actually reach an **https** destination — the CA claim in the Dockerfile, executed rather than argued. | docker |
| CI job `coverage` | A coverage number, informational and deliberately not a gate. | — |

## What the doubles cannot express

Written down because a test double does not merely stand in for a
dependency, it silently deletes classes of behaviour (standing rule 9).

- **`FakeClock`** does not pass time; it records what it was asked to
  wait. Anything depending on real elapsed time — a lease actually
  expiring — needs the real hub, and does have it (`l7_resilience.rs`).
- **`TestServer`** speaks just enough HTTP to answer with a status and
  record what arrived: no chunked encoding, no keep-alive, no redirects,
  no TLS. TLS is covered instead by the CI image job against a real
  https host.
- **`FakeHub`** has no leases, no redelivery and no dead letters. Each of
  those three is covered against a real kyu container.

## Not covered, by decision

- **S1, the flagship criterion, is still not met — but the gap is now one
  hop, not five.** The deployment drill of 2026-08-30 ran the whole chain
  on real machines: a message published on the **real kyu hub** was picked
  up by the service on a scratch container, translated, delivered to the
  Home Assistant webhook, and the automation ran to completion (traces at
  09:30:06 and, after the restore, 09:31:29). What is still missing is
  **Alertmanager itself**, which is not deployed — that is the homelab
  project's metrics round, deliberately on hold. So: the chain is proven;
  the *genuine Alertmanager alert* S1 asks for is not, and is not claimed.
- **The restore drill (M3) HAS now been run** (2026-08-30) and the runbook
  records it as a proven procedure. One step within it remains untested:
  deploying through the homelab preset rather than by hand, which belongs
  to the homelab project.
- **Deployment through the homelab orchestrator is unproven.** The drill
  installed the static binary under systemd; the container image is
  proven by the CI job, and the preset is a proposal until the homelab
  project adopts it.
- **A non-JSON destination is refused rather than supported.** Escaping
  is a mechanism only for JSON; supporting another content type needs a
  safe escaping rule first, and that is a mini-round rather than a quiet
  default (Phase 7, G8).
