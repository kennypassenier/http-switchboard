# Architecture decisions — HTTPSwitchboard

Phases 3 and 4. Every entry is a decision Kenny approved in a gate form;
changes go through a mini-round (`FORM_PROTOCOL.md` §5).

## Phase 3 · Tech choice (approved 2026-08-29)

- **T1 · Platform, and only this one.** The single supported runtime
  target is **a Linux LXC on Kenny's Proxmox host** (x86_64). Not his
  Garuda PC, not Windows, not the Proxmox host itself. Development and
  the test suite run on the Garuda workstation, but that is a
  *development* environment, never a supported deployment target — the
  distinction is recorded because the reverse mistake (a target that
  compiles but was never run) cost latch a whole cross-platform
  retrofit. Build target: `x86_64-unknown-linux-musl`, statically
  linked, so the container carries no runtime.

- **T2 · Rust.** Everything comparable in the house is Rust — kyu,
  hub-bridge, latch, homelab — pinned to the same toolchain, so the
  hooks, CI and gate scripts are copied rather than reinvented, and the
  binary runs with no runtime to keep updated (kyu, for scale, sits at
  ~5 MB resident). Recorded honestly: **Python was the serious rival**,
  because Home Assistant is Python and uses real Jinja2, so templates
  would have been identical to HA's down to the corners. Rejected
  because minijinja covers everything K5 needs (field access,
  `default()`, arithmetic, conditionals), HA's own template extras
  (`states()`, `is_state()`) are meaningless in a project that knows
  nothing about the house (NG6), and Python would add a runtime, a
  second dependency-management style, and the only non-Rust project in
  the toolbox.

- **T3 · Libraries: axum, reqwest, tokio, serde, minijinja.** The same
  set as kyu and hub-bridge, so their structure, test patterns and
  error handling transfer directly. The alternative (a small
  synchronous server, no async) was rejected: long-polling several kyu
  topics at once would become running threads instead of waiting tasks,
  diverging from everything already in the house.

- **T4 · Config format: TOML**, with unknown keys treated as an error
  (no silent typo tolerance — K10 demands it). Decisive argument: the
  profiles' bodies are Jinja templates that almost always start with
  `{`, and in YAML a value starting with a brace is parsed as a
  structure rather than text unless a block scalar is remembered. TOML
  literal strings (`'''…'''`) never reinterpret their content.
  Counter-argument recorded: the homelab presets, compose files and
  Prometheus are all YAML, so YAML is the shape Kenny reads most often.

- **T5 · Dependency policy: frugal, with justification.** The T3 set
  plus what is genuinely needed; every addition gets one line of
  reasoning in this document. `cargo-deny` runs in CI for advisories
  and licences, as in kyu and hub-bridge.

- **T6 · Public repository, dual MIT / Apache-2.0**, like kyu and
  hub-bridge. Consequence deliberately chosen: a public repo can carry
  branch protection, so "red CI blocks `main`" is a lock rather than a
  promise — a private repo on the free plan cannot (that is docgen's
  recorded limitation). Nothing house-specific lives in the code; the
  house lives in the config file (NG6).

- **T7 · Toolchain pinned to 1.97** in `rust-toolchain.toml`, the same
  version as kyu, hub-bridge, latch and binforge, and the version
  installed on the workstation (1.97.1). CI asks for that exact
  version, never "stable" — a gate that does not predict the build is
  not a gate (kyu shipped a commit CI rejected over a lint that did not
  exist locally). Raising it is a deliberate act across the projects.

## Phase 4 · Architecture (approved and FROZEN 2026-08-29)

Drafted, then attacked by the `architecture-critic` in a fresh context —
mandatory here because this project touches network, secrets and auth.
The critic raised seven BLOCKING objections; all seven were verified
against the actual files of kyu, hub-bridge and homelab, all seven held,
and the decisions below are the revised versions. Each entry records the
surviving objection, because a decision without its counter-argument is
half a decision.

- **AR1 · Core/shell split, and a pump that is testable without a hub.**
  The translation is a pure function (profile + incoming message →
  outgoing message or error): no network, no clock, no environment.
  ⚔ The critic: that purity protects the wrong half — messages are lost
  in the poll/deliver/ack ordering, which lives in the shell, and
  hub-bridge's two worst defects were both state-machine bugs a pure
  template core could never catch. Also, W3's frozen test bar demands a
  mocked clock, and nothing in the draft could inject one. **Revised:**
  the pump is a state machine over traits (hub client, sink, clock) with
  fakes in tests; real-kyu E2E stays on top for the bars that demand it.

- **AR2 · Profile schema with cross-field validation.** One TOML file,
  unknown keys are errors. Added after the critic: the kyu subscription
  name is **its own key** (defaulting to the profile name), destination
  topic names are checked at startup (charset + the reserved `kyu.`
  prefix), cross-field combinations are validated (an inbound token on a
  kyu-source profile is a config error, not a silently ignored key), and
  a profile delivering JSON **must** declare its content-type.
  ⚔ Scenarios that forced this: a profile named `alertmanager-HA` starts
  fine and then fails every poll because kyu refuses the name, while the
  service reports itself healthy; renaming a profile six months later
  silently creates a *new* subscription and abandons everything still
  unacked; and a missing content-type is forwarded byte-for-byte by
  hub-bridge, so HA's `trigger.json` is empty and every layer still
  reports success.

- **AR3 · Concurrency differs per source kind.** kyu sources: one message
  in flight per profile, strictly sequential, so a backlog drains in
  publish order. HTTP sources: concurrent, with a bounded number in
  flight and an immediate 503 + `Retry-After` when the bound is reached.
  ⚔ The critic: serialising an HTTP source buys no ordering (the caller
  is the serialiser) and is dangerous — 20 POSTs on one profile with W1
  (answer after delivery) and a 10 s timeout makes the twentieth caller
  wait ~200 s while an unbounded queue of bodies sits in memory.

- **AR4 · Delivery semantics.** Delivered = the destination answered 2xx;
  only then the kyu ack (G7), only then the caller's answer (W1). No
  dedup store, ever — and this is safe on the flagship path *because*
  C4's `ack_id` is per alert name, so a duplicate replaces its own
  notification rather than stacking. **HTTP-source fan-out** (two
  profiles on one path, which K9's bar requires): all branches must
  succeed or the caller gets a 502, with the documented consequence that
  the sender's retry re-delivers the branch that had succeeded.
  ⚔ The critic: the draft treated the ack as infallible. An ack that
  times out or returns 409 after a lease expiry is the routine path, not
  the exotic one; it gets a short timeout, one retry, then a log line —
  never a counted delivery failure.

- **AR5 · Error model.** Every error carries a remedy naming the file,
  the profile, what is wrong and what to do — as owned text, not a fixed
  string, because K10's bar demands specifics ("unknown key `bodyy` in
  profile `alertmanager`; did you mean `body`?"). Config errors are fatal
  at startup; per-message errors are logged and returned.
  ⚔ The critic: the draft allowed a bounded payload excerpt in errors,
  but G5 explicitly permits secrets in the envelope, and a rendered body
  is no longer inside the `Secret` wrapper. **Revised:** excerpts come
  from the *incoming* payload only, never the rendered output, and every
  resolved secret value is scrubbed by value from all log and error
  output.

- **AR6 · Secret handling.** Values live only in a `Secret<String>`
  newtype printing `***`; they come from the environment at startup; the
  config holds only `${VAR}` references; a missing variable stops startup
  naming the variable; a plaintext-scan test asserts no secret reaches a
  log line or error string. ⚔ Survived the attack unchanged. Noted
  consequence: rotating the kyu token needs a restart, and forgetting it
  means every poll is denied — which is why AR8 makes "denied" a state
  and AR11 surfaces it.

- **AR7 · JSON safety: escaping by mechanism, not by habit.** minijinja
  runs with **JSON autoescape on** for JSON profiles, and with **strict
  undefined**: a missing field is a per-message error, and an empty value
  must be written explicitly with `default()`.
  ⚔ The sharpest objection of the round. The draft relied on the author
  remembering `tojson` in every template — and the C4 example Kenny
  approved does not use it. A summary containing `disk full", "severity":
  "info` renders *valid* JSON in which the sender picks the severity, and
  the post-render parse check sees nothing wrong; the same trick on the
  alert name poisons C4's `ack_id`. Alertmanager annotations are free
  text from exporters. Second half: with default undefined behaviour a
  renamed field renders empty, so "Homelab-alarm: " with an empty body
  arrives while every layer reports success — the exact failure mode the
  build-vs-buy record rejected the throwaway script for.
  The post-render JSON parse stays as a second net, and is
  non-mutating, because K3's bar is byte-identical bytes.

- **AR8 · The kyu client.** Long-poll with `?envelope=json`; the template
  input is the *parsed payload*, and a payload that will not parse for a
  JSON profile is a per-message error with a remedy, never an empty
  render. Four revisions, three of them from BLOCKING objections:
  1. **Topic-birth replay.** A kyu subscription only sees what is
     published after its first poll. After a 404 unknown-topic, the next
     successful poll carries `from=beginning`, so the *first* alert —
     the one S1 is about — cannot fall into the gap. Pre-existing topics
     start from now, on purpose.
  2. **Nack, do not let the lease expire.** A failed delivery is handed
     back actively. Waiting out the 30 s lease burns one of kyu's five
     attempts each time, so ~2.5 minutes of destination downtime
     dead-letters the whole backlog — shorter than a routine HA restart,
     in a house with one admin who is asleep.
  3. **A lease budget.** Retries happen inside the same claim, bounded
     well under the lease; config validation *refuses* a profile whose
     timeout × attempts does not fit, because 3 × 10 s does not fit in
     30 s and the late ack then returns 409 while kyu has already
     redelivered. Per-profile `lease_ms` / `max_attempts` live in the
     config and are pushed to kyu's subscription policy.
  4. **"Denied" is its own state.** A 401 is not "hub unreachable": it
     gets its own visible state and a remedy naming kyu's Apps page.
     Hub-down/up transitions are logged once, not per attempt.

- **AR9 · The HTTP server.** One route per path dispatching to the N
  profiles behind it, POST only, with a body cap from day one (default
  1 MiB) even though W9 is rated Later. Reserved paths (`/healthz`,
  `/metrics`) are validated at startup with an ordinary config error.
  ⚔ The critic: "one route per profile path" would make axum panic at
  registration when K9's two-profiles-on-one-path bar is exercised — a
  panic in `main` inside a distroless container is a restart loop whose
  only documentation is a backtrace.

- **AR10 · Destination safety, including the back door.** Scheme, host
  and port come only from config. Added: **routing-relevant headers
  (`Host` and anything that would steer a proxied destination) are
  config-only**, and the **destination topic is never templated**.
  ⚔ The critic: the draft honoured its own rule while leaving the door
  open — the receivers sit behind Traefik, which routes on `Host`, so a
  templated header lets the message pick the receiver; and nothing
  forbade `topic = "alerts.{{ severity }}"`, a message choosing its own
  mailbox. Per-value escaping (JSON, percent-encoding) is one mechanism
  shared with AR7, not two conventions that can each be forgotten.

- **AR11 · Observability that shows work, not just life.** `/healthz`
  reports per profile: state and how long ago the last success was —
  names only, never payloads. One summary log line per message (K11's
  contract, what Loki queries) plus per-attempt lines at debug level
  (what W3's bar asserts). Counters per profile: received, delivered,
  failed, **and delivery duration**.
  ⚔ The critic: a liveness-only `/healthz` answers 200 while every
  profile has been denied for six hours — exactly the silent death W5
  was raised to Essential against; and K11's "exactly one log line"
  cannot coexist with W3's "three attempts in the log" without saying
  which level each lives at.

- **AR12 · Self-reporting on state transitions, not per message.** One
  event when a profile transitions to failing, one when it recovers,
  onto a configured topic (default `switchboard.events`, never under the
  reserved `kyu.`). The consumer — a hub-bridge route plus its HA
  webhook — is a **deliverable of this project**, like K13's alert
  webhook. No profile may take the self-report topic as its source.
  ⚔ The critic: "definitively failed" is undefined for a kyu source, and
  both readings fail W11's bar — per attempt gives 150 events after 20
  minutes of HA downtime, arriving in one burst in a house whose
  dispatcher exists to prevent that; per dead-letter never fires,
  because the switchboard does not observe dead-lettering. And a topic
  nobody consumes is worse than no feature: kyu subscriptions do not see
  what was published before they existed, so those events are gone.

- **AR13 · Deployment: the homelab vault, not `latch run`.** The draft
  said both, and both cannot be true — a distroless image contains no
  latch, and the orchestrator starts containers itself. The real
  mechanism, from the homelab's own D12: at deploy time the client runs
  `latch cat <stack>/<app>/.env --env $HOMELAB_LATCH_ENV --expand` and
  ships the result into the **host vault** at `/var/lib/homelab/secrets/`
  (root-only), which composes them into the container's environment.
  **Consequence stated plainly: the secrets do land on the host's disk,
  deliberately, and the vault's permissions are the actual control.**
  This is also the only reading under which the container comes back by
  itself after a power cut. The binary carries a `--healthcheck` flag
  (kyu's T9 pattern) because a distroless image has no shell or curl.
  W4's dry-run is a workstation tool; the runbook documents the exact
  invocation for running it inside the container.
  ↳ *Amends scope C2: "secrets come from `latch run`" is true of the
  chain, not of this process — see the dated note in SCOPE.md.*

**Frozen at the Phase 4 gate on 2026-08-29.** Changes go through a
mini-round only (`FORM_PROTOCOL.md` §5).
