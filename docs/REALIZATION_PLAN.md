# Realization plan — HTTPSwitchboard

Phase 5 output. Approved by Kenny on 2026-08-29: all ten milestones
agreed as drafted, and every enforcement question answered (S1-S7).

## Milestones

| ID | Contents | Features | Exit criterion | Status |
|---|---|---|---|---|
| L0 | Walking skeleton: layout, empty modules, pinned toolchain, licences, cargo-deny, CI green from day one | — | A push gives green CI; a commit without a feature ID is refused; a commit with a failing test cannot land | **done** |
| L1 | Config file and every check on it: schema, unknown keys, cross-field rules, topic-name validation, `${…}` resolution, fail-closed startup | K8, K9, K10 | One test per class of config error, each asserting startup stops and the message names file, profile and remedy | **done** |
| L2 | The translation core, with no network at all: JSON autoescape, strict undefined, per-value escaping, the recorded Alertmanager fixture | K5, K12, K14 | The fixture renders byte-identical; a missing field errors instead of rendering empty; no hostile payload changes the destination | **done** |
| L3 | Both sinks: URL and kyu topic, headers and content-type, timeout, retry inside the lease budget | K3, K4, K6, W2, W3 | A receiver failing twice then succeeding yields exactly one delivery; a receiver that never answers fails with a remedy; a secret header value appears in no log line | **done** |
| L4 | The hub client — the heart: long poll, topic-birth replay, nack, retry in-claim, ack ordering, "denied" as a state | K2 (AR8) | Against a real kyu (S5, never LXC 109): a message published before the first poll still arrives; a failing receiver does not ack and the message returns; `kill -9` mid-delivery costs at most a duplicate | **done** |
| L5 | The inbound side: one route per path dispatching to N profiles, POST only, body cap, inbound token, honest answer | K1, W1, W8 | Two profiles on one path both deliver without a startup panic; a failure gives the sender an error; no token means 401 and no delivery; a burst gives clean refusals, not growing memory | **done** |
| L6 | Visibility: `/healthz` with per-profile state, `/metrics` incl. duration, JSON logs | W5, W6, W7 | Counters correct after one success and one failure; a denied profile shows unhealthy while the process runs; every log line is valid JSON with no secrets | **done** |
| L6b | **Assembly**: load the config from a path, one pump task per kyu profile, serve the router, graceful shutdown, `--check-config` and `--healthcheck`. Added at the L1-L6 report gate (2026-08-30): every milestone described a *part*, and putting them together was in none of them — the binary still printed "no features built yet" while all its components were proven | — (assembly of L1-L6; AR13) | The binary loads a real config, a message travels the whole way through the running service, and a broken config stops it with a non-zero exit and a remedy | **done** |
| L7 | Self-reporting and the Home Assistant deliverable: transition events, the hub-bridge route, the new HA webhook + automation | W11, K13 | A failing profile produces exactly one event (not one per message), recovery one more, and a real test notification arrives in HA through the new automation | **done** |
| L8 | The two Desired features | K7, W4 | The dry-run command on the K12 fixture gives exactly the real path's output and demonstrably sends nothing | planned |
| L9 | Deployment and the restore drill: musl binary, container image, `--healthcheck`, homelab preset | M1, M3 (AR13) | The container returns by itself after a simulated power cut, and the restore drill is actually performed, written up as a numbered procedure | planned |

## Enforcement (decided at the Phase 5 gate)

- **S1 · Commit gates:** format check, clippy with warnings as errors, **and
  the full test suite**. Slow gates are a mini-round, never a shortcut.
- **S2 · No bypass.** `--no-verify` is not a sanctioned route; CI and branch
  protection catch whatever slips through anyway.
- **S3 · Every commit carries IDs**; non-feature commits carry `[meta]`.
- **S4 · Public GitHub repo with branch protection**, created by Claude on
  Kenny's explicit go, then read back and shown. Required status checks, no
  pull request requirement (single committer). CI runs on `branches: ["**"]`
  so protection cannot lock the branch shut (standing rule 6a).
- **S5 · Scratch resources, named:** a **kyu instance of our own** on the
  workstation, started from the published image by the test harness, plus a
  **throwaway LXC** on the Proxmox host for the L9 deployment drill. Kenny's
  kyu on **LXC 109 is the real hub and is never a test target**.
- **S6 · Work happens in a session opened in this directory** (moved
  2026-08-29), so the repo-scoped Phase 7 review can run.
- **S7 · The remaining standing rules** apply unchanged, as Kenny's rules.

## Gate log (from Phase 7 onward; standing rule 5)

| Gate | Date | Decision | Recorded |
|---|---|---|---|
| Phase 5 · plan | 2026-08-29 | All ten milestones agreed; S1-S7 answered (full suite per commit, no bypass, `[meta]` for non-features, public repo + branch protection by Claude, scratch = own kyu + throwaway LXC, session moved into the project) | this document |
| L7 · Home Assistant | 2026-08-30 | Kenny's explicit go for `automation.homelab_alert_webhook`; created through the HA API and proven live — a firing alert dispatched, a resolved alert stopped at the condition. Mini-round M1 shortened the chain: the switchboard delivers straight to HA, since it is already the kyu consumer that acks only after delivery, and hub-bridge is not deployed | SCOPE.md G6 amendment |
| L1-L6 · report | 2026-08-30 | All six milestones signed off with their evidence; coverage confirmed with no silent gaps; the five build-time decisions ratified (dead-letter on an unrenderable message, headers config-only, inbound payload must be JSON, 60 s default lease, KYU_IMAGE gating); **L6b added** because assembly was missing from the plan | this document + FEATURES.md |
| L1-L6 · AFK build | 2026-08-30 | built, tested and landed on `main` with green CI per milestone; combined report form pending | commits 368401f…8098d65 |
