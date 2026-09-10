# Changelog

All notable changes to HTTPSwitchboard. The format is loosely
[Keep a Changelog](https://keepachangelog.com/); versions follow semver,
where the promise is about the **config file format**, the two HTTP
endpoints and the CLI verbs — not about the internals.

## [Unreleased]

### Changed

- **chassis-rs 1.8.0 → 2.0.0.** The major does not touch this project's
  code: its breaking change is the `Client` struct, which this service
  never constructs. What arrives through `chassis sync` is worth more than
  the version number suggests:
  - **The release asset is a static musl binary on a distroless image**
    (kit `feat-build-1`), and the release workflow refuses to publish one
    that links shared libraries. The previous asset needed `GLIBC_2.39`;
    CT 109 runs glibc 2.36, so it could not have started there. **The
    deployment no longer depends on the host's Debian version.**
  - **`update_cmd` passes the unit's own `Environment=` lines**, so a
    self-update's staged `--check` no longer runs without
    `HTTP_SWITCHBOARD_STATE_DIR` and fail with a message pointing at the
    wrong layer.
  - **`docs/KIT.md` describes only the features this binary carries**
    (kit K35): core, dashboard, assets, self-update.
  - kp-themes 5.1.0, which fixes the state badge that broke `active`
    across two lines on the Clients page.
- **The nineteen hand-written lines that split the shared config file are
  gone** (kit `feat-config-1`): `App::project_table()` removes the kit's
  knob keys and its table sections. One of those lines removed `notify` on
  knowledge that lived nowhere and would have gone silently wrong the day
  the kit gained a second section.

### Changed

- **The gates run in two tiers** (Kenny's test policy of 2026-09-09,
  applied here). An ordinary commit runs fmt, clippy and the in-process
  suite — about eleven seconds. The **release** commit additionally runs
  the end-to-end suite against a real kyu container, `cargo deny` and the
  container build, locally, before a tag exists. The tier is read from the
  repository: a commit that moves the package version is the release
  commit, so there is no flag to forget. CI keeps only the single check
  branch protection needs, on every branch, because that is the one
  genuinely GitHub-bound part; `tests/l12_gate_tiers.rs` compares what CI
  gave up against what the release tier runs.

### Fixed

- **`test --config <a real config>` refused a file the service starts from
  happily** (fix-3). The dry-run runs before the kit parses anything, so
  nothing stripped the kit's own knobs and tables; a config carrying
  `listen` or a `[[notify.webhook]]` table — which every deployed config
  does — was answered with "not valid TOML" on a file that is valid TOML.
  The operations runbook tells an operator to point this verb at exactly
  that file. It now does the same strip from the spec, which names both
  halves itself.
- **Two documented container commands used a path the image does not
  have** (fix-4): `/opt/http-switchboard/bin/…` is the native install;
  the image has always used `/usr/local/bin/…`. `tests/l10_docs.rs` now
  compares documented container commands against the Dockerfile.
- A paragraph in the operations runbook still presented `?strict=1` as the
  strict probe, two paragraphs below the correction that removed exactly
  that claim.

### Added

- `tests/l11_health_claims.rs` — the measure of correction C2. It measures
  what `/healthz`, `/healthz?strict=1` and `--healthcheck` answer with
  every profile working and with one failing, and then requires the README
  and the operations runbook to carry exactly that table. The binary writes
  the document's table, so a document cannot silently drift from it.

## [3.0.0] - 2026-09-09

Built on [chassis-rs](https://github.com/kennypassenier/chassis-rs) v1.8.0.
The inbound door is the kit's now. Everything that makes this service what
it is — profiles, translation, sinks, the hub pump, the deliver-then-ack
ordering — is unchanged.

### Migration

- **The door.** `inbound_token` in a profile is retired. A config that
  still carries one is refused at startup, with the command that replaces
  it in the message. Every inbound path is now behind a chassis client
  token:

  ```bash
  chassis clients issue alertmanager \
    --url http://127.0.0.1:8080 --token-env HTTP_SWITCHBOARD_TOKEN
  ```

  The sender keeps sending `authorization: Bearer <token>`; only who
  issues the token changed — and `chassis clients revoke` now shuts the
  door on the next request instead of at the next config edit and
  restart.
- **Two secrets became mandatory.** The kit's door brings its dashboard,
  and a dashboard never starts without a login: `HTTP_SWITCHBOARD_TOKEN`
  and `HTTP_SWITCHBOARD_SECRET_KEY`, both from `http-switchboard
  gen-secret` on a terminal, both in the environment file. The systemd
  unit's `ExecStartPre=--check` refuses without them, so put them in
  **before** swapping the binary. `docs/HANDOVER_HOMELAB.md` carries the
  order that works.
- **Scope note.** "It stores nothing" (NG3) is amended: no *message* is
  stored, and durability is still the hub's job, but the kit keeps two
  stores in the state directory — the senders that hold a token, and admin
  sessions. Losing the state directory costs tokens, never a message.

### Added

- A **status page** behind the admin login, with a **Profiles** section:
  one row per profile with its state, source, destination host and
  counters. A client is called a *sender* here.
- **Recheck profiles**, one button under that section. A profile's state
  only changes when a message goes through it, so an http-source profile
  that failed once stays red until somebody posts again — which, for a
  profile nobody uses yet, is never. Recheck puts them back to `starting`.
  It changes no message and no configuration.
- `docs/KIT.md` — generated by `chassis sync`, describing what this
  service gets from the kit at the pinned version. The project's own
  documents point at it instead of retelling it.
- `--knobs` prints every kit setting with its environment variable,
  default and meaning, before any configuration is opened.
- `tests/l10_docs.rs`: every documented command line is handed to the
  built binary (correction C1, `docs/CORRECTIONS.md`).

### Changed

- **chassis-rs 1.1.0 → 1.8.0** and the scaffold synced: the kit's CI,
  hooks, `deny.toml`, Dockerfile and deploy files come from `chassis sync
  --write`; the real-kyu end-to-end suite (`KYU_IMAGE`) is the project
  gate in `.claude/hooks/gates.project.sh`; `.chassis.toml` records CT
  109's measured config dir, token env file and vmid 109.
- The door tests run on the kit's `chassis::testing::TestApp` (1.8.0),
  which starts this service in-process with a temporary state directory
  and fresh secrets. The project's own fakes stay where the subject is
  this project's domain — the pump's ordering, the hub, the retry ladder.

### Fixed

- The README documented `http-switchboard <config.toml>`, which 2.0.0
  refuses; and claimed "not yet released" after 2.0.0 shipped. Both
  corrected, and a test now runs what the documents claim.
- **The health endpoints were described wrongly in three documents**
  (correction C2). Since 2.0.0 the kit owns `/healthz` and it answers
  **503 as soon as any profile is failing**, with or without
  `?strict=1`; the README, the operations runbook and the homelab
  handover all still described the 1.x split, and the handover went as
  far as telling the orchestrator to probe `/healthz` — which would
  restart this service every time Home Assistant is down. The shipped
  `Dockerfile` and `deploy/service.yml` were always right (they call
  `--healthcheck`, which exits 0 while a profile is failing); only the
  prose was wrong. Measured against a running 3.0.0 before the release.

## [2.0.0] - 2026-09-06

Built on [chassis-rs](https://github.com/kennypassenier/chassis-rs) v1.1.0.
The switchboard — profiles, translation, sinks, the hub pump, the per-path
`inbound_token` door, the in-flight bound — is unchanged; the kit now owns
the command line, configuration layers, logging of its own layers,
`/healthz`, `/metrics`, the graceful shutdown and signed self-update. This
breaks the CLI verbs and the `/healthz` body, hence 2.0.0.

### Migration

- **Command line.** `http-switchboard <config.toml> --listen …` becomes
  `http-switchboard --config <config.toml> --listen …` (or
  `HTTP_SWITCHBOARD_CONFIG` / `HTTP_SWITCHBOARD_LISTEN` in the environment
  file; the default config path is `<state_dir>/config.toml`);
  `--check-config <path>` becomes `--check --config <path>`; `--healthcheck`
  keeps its flag (503 now counts as alive: the process answers); `test …`
  is unchanged. An unknown argument exits 1.
- **A state directory is required** (`HTTP_SWITCHBOARD_STATE_DIR`, default
  `/var/lib/http-switchboard`): `--check` refuses a missing or unwritable
  one; it holds the self-update state only — the switchboard itself still
  stores nothing.
- **`/healthz`** answers the kit's shape, one subsystem per profile:
  `{"status","version","subsystems":{<profile>:{"ok","detail"}}}`, and
  **503 whenever a profile is failing, denied or cut off from the hub** —
  the old `?strict=1` semantics are now the only ones (Uptime Kuma already
  probes with `?strict=1`; the query is ignored). A plain liveness poll is
  `--healthcheck`. `/metrics` keeps every `switchboard_*` series and gains
  the kit's build-info, uptime and request counters.
- **The config file is shared** with the kit's knobs (`listen`, `log`, …);
  the switchboard strips them before its own `deny_unknown_fields` parse.
  `${VAR}` references, `[kyu]`, `[reporting]` and `[[profiles]]` are
  unchanged.
- **Deployment.** Install path `/opt/http-switchboard/bin/http-switchboard`,
  the hardened `Type=notify` unit in `deploy/http-switchboard.service`
  (fixed user, no `DynamicUser`), environment file
  `/etc/http-switchboard/http-switchboard.env`, homelab stack file
  `deploy/service.yml` with `update_cmd`; the never-adopted compose preset
  under `deploy/homelab-preset/` is gone. The container image is the kit's
  Dockerfile (Debian trixie, glibc).
- **Self-update is on.** Releases are glibc binaries named
  `http-switchboard` with `SHA256SUMS`, `SHA256SUMS.minisig` (trusted comment
  `kennypassenier/http-switchboard v<version>`) and `VERSION`; the release
  workflow is the kit's, signing is `scripts/sign-release.sh`. FEATURES M1
  ("no self-update, by decision") is amended.
- **Logging.** The switchboard's own JSON event lines still go to stdout as
  before; the kit's access lines and lifecycle lines go to stderr
  (`HTTP_SWITCHBOARD_LOG_FORMAT=json` for one object per line). Folding the
  switchboard's lines into the kit's logger is a follow-up decision.

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
