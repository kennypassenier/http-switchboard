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
