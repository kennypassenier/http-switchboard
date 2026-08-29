# Features — HTTPSwitchboard

Phase 2 output. Feature IDs are permanent: they appear in commits, test
names, docs and forms forever.

> **Rated and FROZEN by Kenny on 2026-08-29** in two rounds — round 1 the features
> that follow from the approved scope, round 2 Claude's own proposals
> plus the four mandatory items. The scale is the canonical one:
> Essential · Desired · Later · Don't do.

**Tally:** 21 Essential · 2 Desired · 2 Later · 0 Don't do (25 total).

Frozen at the Phase 2 gate on 2026-08-29: every item below, its rating
and its test bar. Changes go through a mini-round only
(`FORM_PROTOCOL.md` §5).

## Core (from the approved scope)

| ID | Rating | Feature |
|---|---|---|
| K1 | Essential | **HTTP source.** A profile listens on a configured path and accepts a POST. The classic webhook ingress, and the road for every source that cannot publish to kyu itself. |
| K2 | Essential | **kyu source, ack after delivery.** The profile subscribes to a topic and long-polls it. What arrives is translated, delivered, and only acknowledged once the destination accepted it; a failed delivery is left unacknowledged so the hub redelivers and eventually dead-letters visibly (scope G7). This ordering is what no off-the-shelf pipeline could express — see the build-vs-buy record in SCOPE.md. |
| K3 | Essential | **URL destination, inside or outside the house.** The translated message is POSTed to an address from the config. Outbound internet destinations are allowed (scope NG5). |
| K4 | Essential | **kyu destination.** The translated message is published to a topic, where hub-bridge picks it up. This is the road the first customer travels, and where the chain's durability comes from. |
| K5 | Essential | **Jinja translation of the body.** Build a new message from the incoming one in the same template language as Kenny's HA automations: field access, defaults, arithmetic, conditionals (scope G4). |
| K6 | Essential | **Headers and content-type per profile.** The half of "the whole envelope is translatable" that is needed immediately: a receiver wanting `Authorization: Bearer …` or a specific content-type the source does not send. Without it the translated message cannot even enter the hub. |
| K7 | Desired | **Method and templated target URL.** A receiver wanting PUT instead of POST, or an address with a fragment from the message in it (`/devices/{{ id }}/state`). None of today's receivers asks for it; it can move to v1.1 without harm. Interacts with K14: a templated address may never let the incoming message choose the destination. |
| K8 | Essential | **Secrets from the environment via latch.** The config holds only a reference (`token: ${KYU_TOKEN}`); the value comes from the environment latch starts the process in. The project never reads a `.env` file itself (scope C2). |
| K9 | Essential | **Several profiles in one config file.** One reviewable file in git. Several profiles may share a source — that is the fan-out model: the same alert to a phone and to a log topic as two independent lines with independent failure handling (scope G2). |
| K10 | Essential | **Fail-closed config validation.** A typo in a template, a profile without a destination, an unknown key: startup stops with a non-zero exit and a message naming the file, the profile, what is wrong and the remedy. Starting half-working is how a coupling silently does nothing for three weeks (scope S4). |
| K11 | Essential | **A remedy in every error, one log line per message.** Every error says what to do now, not only what broke. Every message that passes through produces exactly one log line: profile, outcome, duration — so an arrival is visible even when the delivery failed. |
| K12 | Essential | **The Alertmanager profile ships with the project**, together with a payload Alertmanager itself produced, pinned as a regression fixture with its expected output. Synthetic fixtures prove nothing about the real format (standing rule 9). |
| K13 | Essential | **The Home Assistant side is a deliverable.** A new webhook with its own id, `local_only: true`, POST only, and an automation that filters on `firing` before calling `script.notification_dispatch` — the shape approved at the Phase 0 gate (scope C4). Claude creates it through the HA API on Kenny's explicit go. |
| K14 | Essential | **A destination never comes from the incoming message.** Only the config decides where something goes. Otherwise a stranger can aim this service at any address, including ones inside the network that are unreachable from outside (scope C6). |

## Proposals (Claude's round-2 additions)

| ID | Rating | Feature |
|---|---|---|
| W1 | Essential | **Answer the sender honestly.** For an HTTP source, deliver first and answer afterwards: on failure the sender gets an error and retries with the message it still holds. Answering "accepted" up front and failing afterwards loses the message, because this service stores nothing (scope NG3). |
| W2 | Essential | **A timeout per destination.** A receiver that neither refuses nor answers would otherwise occupy a profile forever. Default around ten seconds, per profile adjustable; an expired timeout is an ordinary error with a remedy, not a stalled service. |
| W3 | Essential | **Retry with backoff.** Three attempts with growing pauses, for short hiccups. Complements the hub rather than replacing it: for a kyu source, giving up simply means not acknowledging (K2). Most valuable on the HTTP ingress, where no hub sits behind it. **Amended 2026-08-29 (mini-round MR1 at the Phase 4 gate): Desired → Essential.** The critic pass showed the original rationale was wrong: without in-process retry inside the same claim, every failure burns one of kyu's five attempts, so ~2.5 minutes of destination downtime dead-letters the whole backlog — shorter than a routine Home Assistant restart. Retry is not a bonus on top of the hub; it is what stops the hub from giving up. AR8 leans on it. No built work affected: no code existed yet. |
| W4 | Desired | **Dry-run a profile from the command line.** `http-switchboard test --profile alertmanager --input alert.json` prints what comes out and sends nothing. The difference between a config file you dare to edit and one you avoid. |
| W5 | Essential | **`/healthz` for Uptime Kuma.** Raised Desired → Essential by Kenny. Without it, a dead switchboard is noticed only by an alert that never arrives — and an alert that does not arrive does not draw attention. |
| W6 | Essential | **`/metrics` for Prometheus.** Raised Desired → Essential by Kenny. Counters per profile: received, delivered, failed, delivery duration. The loop closes: Alertmanager can alert on the switchboard dropping messages. Pull-based, so it still works when the hub is down — the fallback for W11. |
| W7 | Essential | **JSON log lines for Loki.** Raised Desired → Essential by Kenny. Fixed fields (time, profile, outcome, duration) so "every failed delivery of profile X today" is a query in Grafana instead of grep. The human-readable form stays available for direct inspection. |
| W8 | Essential | **A token on the inbound side.** Raised Desired → Essential by Kenny. A profile with an HTTP source may require a shared token. Not urgent on the LAN, but it is the door that must exist before M4 can even be discussed. |
| W9 | Later | **Size and rate limits.** A maximum body size and a per-profile rate cap, against a source that runs away. Becomes Essential the moment M4 happens. |
| W10 | Later | **Config reload without restart.** Small gain — the service holds nothing, so a restart costs half a second and loses nothing by construction — against a second path along which the running service can break. |
| W11 | Essential | **Self-reported failures onto a topic.** Chosen by Kenny in M2: a delivery that fails for good is published as an event on its own topic (`switchboard.events` or similar — **not** under `kyu.*`, which the hub reserves for its own events and refuses publishes to with a 403), which hub-bridge turns into a house warning in HA. Known blind spot, accepted: if kyu itself is down this warning does not arrive either — W6 is the pull-based fallback for exactly that case. |

## Mandatory items (procedure Phase 2)

| ID | Decision |
|---|---|
| M1 | **Update & distribution.** Redeploy through the homelab preset: it runs as a managed container, so a new version is a new image the orchestrator rolls out, inheriting the nightly image update and its rollback. **No self-update, by decision** — that is a lot of machinery, including a release signing key, for one service in one house. |
| M2 | **Ecosystem integration.** **kyu** as source and destination (K2/K4), **latch** for secrets (K8), the **homelab orchestrator** for deployment (scope C1), and — Kenny's choice — the project reports its own delivery failures as events so they surface in HA (W11), the pattern hub-bridge uses for the hub's dead letters. |
| M3 | **Backup & restore.** The full state is small by construction: no runtime state (scope NG3), so only the config file, the secrets and the deployment itself. Config in git, secrets encrypted in latch, container from the preset — **and, Kenny's choice, the container also rides the homelab's restic backup**, belt and braces at no extra cost since the orchestrator runs those jobs anyway. Backup is automatic on both roads. **The restore is drilled in Phase 6**: destroy the container and rebuild from repo, preset and latch until an alert flows again; the outcome becomes a numbered procedure in the operations runbook. |
| M4 | **Inbound traffic from the internet: after v1, as its own mini-round.** Outbound is allowed today (K3). The obstacle for inbound is measured: everything public arrives through one Cloudflare Tunnel and one Access application on `*.kp-soft.dev` demanding an interactive login, which no webhook sender can satisfy. No source outside the house is waiting today, and it is the only extension in this project that changes the risk profile of the house — so it gets its own design, its own security review, and its own conversation. |

## Test expectations (the concrete bar, fixed now)

| ID(s) | Bar |
|---|---|
| K1 | Integration: a POST on a configured path produces exactly one delivery attempt at a fake destination; an unknown path answers 404 and causes nothing. |
| K2 | E2E against a **real kyu** (scratch instance, not a mock): published → picked up → delivered → only then acked. Counter-proof: the fake destination answers 500, the message is NOT acked and is offered again on the next poll. |
| K3 | Integration: the fake receiver gets byte-identical bytes; an error answer is reported, never swallowed. |
| K4 | E2E against a real kyu: after delivery the message is readable back on the topic with the expected content. |
| K5 | Unit tests per operation class — field copy, `default()` on a missing field, arithmetic with rounding, conditional text — plus K12 as the end proof. |
| K6, K8 | Integration: the receiver sees exactly the profile's headers. Mandatory plaintext-scan assertion: no secret value appears in any log line or error message (standing rule 10). A missing environment variable stops startup naming that variable. |
| K7 | Unit + integration: a PUT profile uses PUT; a templated address resolves; a template that would produce a different host is refused (ties to K14). |
| K9 | Integration: two profiles on one path both deliver, and a failure in one does not touch the other. The example config from the documentation is loaded by a test, so the docs cannot rot unnoticed. |
| K10 | Unit test per validation rule, each asserting the remedy text is present and the exit code is non-zero. |
| K11 | A test walks every error variant and asserts a remedy is present; a successful and a failed message each produce exactly one log line with profile and outcome. |
| K12 | The recorded Alertmanager payload through the shipped profile equals the expected output byte for byte. |
| K13 | Not automatable — a live drill: the automation is created through the HA API, and a real alert arriving as a notification is the evidence. This is the project's flagship criterion S1. |
| K14 | Property test over deliberately hostile payloads (fields named `url`/`host`, `@`-tricks in a name): in no case does the destination change. |
| W1 | Integration: destination fails → the sender gets an error; destination succeeds → the sender gets 2xx and not a moment earlier. |
| W2 | Integration against a receiver that never answers: delivery fails after the configured time with a remedy, and the profile accepts the next message normally. |
| W3 | Integration: a receiver failing twice then succeeding yields exactly one delivered message and three attempts in the log; pauses are measured with a mocked clock, not by waiting. |
| W4 | The command on the K12 fixture produces exactly the same output as the real path, and demonstrably sends nothing. |
| W5 | The endpoint answers without a token, also while a profile is broken, and leaks no message content. |
| W6 | After one successful and one failed delivery the counters hold the expected values in Prometheus format; no message content in labels. |
| W7 | Every log line is valid JSON with the fixed fields; the secret-scan test runs over this form too. |
| W8 | Without a token 401 and no delivery; with the right token normal handling; the token appears in no log line. |
| W11 | E2E against a real kyu: a permanently failing delivery produces exactly one event on the configured topic, and it carries no secret material. |
| M3 | Phase 6 drill: destroy the container, rebuild from repo + preset + latch, and an alert flows again. The drill's steps become the runbook procedure. |
