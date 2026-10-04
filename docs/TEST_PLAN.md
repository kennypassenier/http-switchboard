# Test plan — HTTPSwitchboard

Phase 7 output. What each suite proves, what is deliberately not covered,
and why. Written after the hardening pass on 2026-08-30 when the suite
stood at **96 tests**; at 3.0.0 (2026-09-09) it stands at **116**.

Run everything with:

```bash
cargo test --all
```

The end-to-end suites run against the newest signed kyu release, which
`~/Projects/workstation/bin/kyu-latest` downloads, verifies against its
SHA256SUMS and the ecosystem minisign key, and caches (Kenny, 2026-10-04:
always the latest kyu, never a pinned image; the ghcr images are gone since
kyu runs as a native service). `KYU_BIN` points them at another binary.
They no longer skip themselves: a missing hub fails the run. The release
tier of the gate (`CHASSIS_RELEASE_GATE=1`, set by `chassis release`) runs
`cargo test --test l4_pump` once more on the bumped tree.

## The suites

| Suite | What it proves | Runs against |
|---|---|---|
| `src/secret.rs`, `src/obs.rs` (unit) | A secret cannot print itself; log lines are valid JSON with fixed fields and survive awkward profile names; a warning is not dressed up as a state change; health and metrics carry names and counters only. | pure |
| `src/adapters.rs` (unit) | Every delivery-status remedy exists, says something specific, and 401/413/404/4xx/5xx do not share text; only the errors worth retrying are retried. | pure |
| `tests/l1_config.rs` (26) | One test per class of config error: startup stops, and the message names file, profile, fault and remedy. Includes a walk over seventeen configs covering fifteen distinct variants, so a new variant without a remedy fails the suite. | pure |
| `tests/l2_translate.rs` (8) | The recorded Alertmanager payload renders byte-for-byte through the shipped profile; a quote in an alert summary cannot add a field or change the severity; a missing field errors instead of rendering empty; a profile we cannot escape safely is refused at startup. | pure |
| `tests/l3_sinks.rs` (9) | Headers and body reach the receiver unchanged; a receiver that never answers times out with a remedy and frees the profile; two failures then success is one delivery and three attempts, with the pauses measured on a fake clock; no secret in any error. | a real TCP listener |
| `tests/l4_pump.rs` (12) | The ordering: deliver, then ack — never the other way. A refused delivery is handed back and never acked. A 404 makes the next poll ask for the history. "Denied" is its own state and carries the remedy. Against a **real kyu**: a message published before the first poll still arrives; a refused delivery comes back and the retry succeeds exactly once; the translation is publishable back onto a topic; the subscription policy is in force after the first poll; a message that can never work is dead-lettered once and does not return. | real kyu container |
| `tests/l5_inbound.rs` (14) | One route per path serving N profiles without a startup panic; the sender is answered only after delivery and told plainly when it failed; the destination never leaks back to the sender; a body over the cap is refused; a burst past the bound gets refusals carrying a remedy; a 2.x `inbound_token` is refused with the command that replaces it. | real TCP listeners |
| `tests/l5b_door.rs` (6) | The door, as the kit holds it since 3.0.0 (H1): no token is 401 and nothing is translated; an issued sender token gets in and a wrong one does not; **a revoked token is out on the next request**; no token appears in a refusal or on a page; the status page carries the Profiles section, the Recheck button and this project's word for a client; Recheck clears a failure an http-source profile cannot clear itself. | the kit's `TestApp`, in-process |
| `tests/l11_health_claims.rs` (1) | Correction C2: the health table in README.md and docs/OPERATIONS_RUNBOOK.md is **measured** against a running service — `/healthz`, `/healthz?strict=1` and `--healthcheck`, with every profile working and with one failing — and the documents must carry exactly what was measured. Also pins the two facts C2 was about: the two paths answer alike, and `--healthcheck` exits 0 while a profile is failing. It cannot see a health claim written outside the marked block. | the kit's `TestApp` + the built binary |
| `tests/l10_docs.rs` (1) | Every `http-switchboard …` command line in README.md and USER_GUIDE.md is one the built binary parses (correction C1). It checks the *shape* of a command, never its outcome — a documented path that does not exist here is meant to fail. It cannot see a command that is wrong but still parses, nor prose around the block that has gone stale. | the built binary |
| `tests/l6_observability.rs` (4) | Liveness stays green while a receiver is down, `?strict=1` goes 503 for the monitor; counters move; neither endpoint echoes message content. | real listeners |
| `tests/l6b_assembly.rs` (4) | The binary refuses a broken config and accepts the shipped one; a message travels the whole way through the running service; against a real kyu the service pumps a published message by itself and counts it. | binary + real kyu |
| `tests/l7_selfreport.rs` (2) | A failing profile produces exactly one event and recovery one more — for a kyu source and for an inbound one — and no event carries any part of a payload. | real kyu |
| `tests/l7_resilience.rs` (1) | `kill -9` while a delivery is in flight loses nothing: after a restart the message comes back and is delivered, unchanged. | binary + real kyu |
| `tests/l8_desired.rs` (7) | A path segment may come from the message while scheme, host and port cannot; awkward values (empty, structured, CR/LF, `?`, `#`) stay one segment; the dry run shows exactly what the real path produces and sends nothing, printing header names but never their values. | binary |
| `tests/l9_deployment.rs` (3) | `--healthcheck` answers correctly for a live and a dead service — the container's only probe, since the image has no shell; no secret reaches the log of the running binary; the CLI fails closed with a remedy on every wrong invocation. | binary |
| Container image + smoke (`chassis release` gate and the release tier of `.claude/hooks/gates.project.sh`) | The image builds, answers `--version`, and its `--healthcheck` fails against a closed port. The old CI image job also started the image with the shipped config, probed `/healthz` and reached an **https** destination (the Dockerfile's CA claim); that job went on 2026-09-10 and those three steps are not reproduced locally. | docker |
| Coverage (`chassis release` gate, `cargo llvm-cov --summary-only`) | A coverage number, informational and deliberately not a gate; skipped when `cargo-llvm-cov` is absent. | — |

## What the doubles cannot express

Written down because a test double does not merely stand in for a
dependency, it silently deletes classes of behaviour (standing rule 9).

- **`FakeClock`** does not pass time; it records what it was asked to
  wait. Anything depending on real elapsed time — a lease actually
  expiring — needs the real hub, and does have it (`l7_resilience.rs`).
- **`TestServer`** speaks just enough HTTP to answer with a status and
  record what arrived: no chunked encoding, no keep-alive, no redirects,
  no TLS. TLS was covered by the CI image job against a real https host until
  2026-09-10; no local check has replaced that step.
- **`FakeHub`** has no leases, no redelivery and no dead letters. Each of
  those three is covered against a real kyu container.

## Not covered, by decision

- **scope-flagship-1, the flagship criterion, is MET — by Kenny's
  decision of 2026-09-26** ("Telt mee"). On 2026-09-19 22:17 UTC an alert
  travelled the whole chain: Alertmanager (10.10.10.13:9093, receiver
  `kyu-hub`) → kyu → this service (profile `alertmanager`) →
  `automation.homelab_alert_webhook` → `script.notification_dispatch`
  with `push_targets: [kenny]`, trace `545d5214…` finished; the resolved
  message at 22:32 stopped on the `firing` filter, as designed. Measured
  2026-09-26 on the running 3.1.1: `switchboard_messages_delivered_total
  {profile="alertmanager"} 2`, `…failed_total 0`. The alert was
  `HomelabTestAlarm`, injected into Alertmanager by hand; Kenny counted it
  because no hop this project built was skipped — the criterion's "no
  test curl" was aimed at a shortcut past the chain, and this was none.
  The only link not exercised is the Prometheus rule that fires an alert,
  which is the homelab project's.
- **The restore drill (M3) HAS now been run** (2026-08-30) and the runbook
  records it as a proven procedure. One step within it remains untested:
  deploying through the homelab preset rather than by hand, which belongs
  to the homelab project.
- **Deployment through the homelab orchestrator is unproven.** The drill
  installed the static binary under systemd; the container image is
  proven by the image check of the release gate, and the preset is a proposal until the homelab
  project adopts it.
- **A non-JSON destination is refused rather than supported.** Escaping
  is a mechanism only for JSON; supporting another content type needs a
  safe escaping rule first, and that is a mini-round rather than a quiet
  default (Phase 7, G8).
