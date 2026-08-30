# HTTPSwitchboard

Accepts a message in one shape and delivers it in another to a
destination its own configuration chooses. First customer: Alertmanager
alerts on their way to Home Assistant.

This project follows the dev procedure in `~/Projects/dev-procedure/`
(`/project-flow`). Standing rules apply to every change:
`~/Projects/dev-procedure/STANDING_RULES.md`.
Enforcement is **git-native** (`.githooks/` via `core.hooksPath`), so
gates hold from any session or terminal. After a fresh clone, run:
`git config core.hooksPath .githooks`.

## Procedure status

| Field | Value |
|---|---|
| Current phase | 10 · Retrospective (after folding into Homelab Rust) |
| Last completed gate | Phase 9 release (2026-08-30): **v1.0.0 tagged and published** |
| Next gate | Phase 10 retrospective form (two-way, incl. ecosystem candidacy) |
| AFK mode | off (L1-L6 AFK run closed 2026-08-30) |

<!-- Update this block after every completed gate. -->

## Project documents

| Doc | Purpose |
|---|---|
| docs/SCOPE.md | goals, non-goals, success criteria, constraints (Phase 0) |
| docs/FEATURES.md | rated feature list with permanent IDs (Phase 2) |
| docs/ARCHITECTURE_DECISIONS.md | frozen AR decisions incl. tech choice (Phases 3-4) |
| docs/REALIZATION_PLAN.md | milestones + status table (Phase 5) |
| docs/TEST_PLAN.md | what is proven where + accepted limitations (Phase 7) |
| docs/USER_GUIDE.md | how to use it, per feature, with the test that proves each claim (Phase 8) |
| docs/DEBUGGING_GUIDE.md | the evidence trail and a symptom→cause table (Phase 8) |
| docs/OPERATIONS_RUNBOOK.md | numbered procedures, including what has NOT been drilled (Phase 8) |
| docs/ARCHITECTURE_REFERENCE.md | the system as built, module by module (Phase 8) |

## Repo

`https://github.com/kennypassenier/http-switchboard` — public, MIT/Apache-2.0.
Branch protection on `main` is ON: required check `fmt · clippy · tests`,
strict (branch must be up to date), admins included, no force pushes, no
deletions, **no pull request required** (single committer). The daily flow
is therefore: work on a branch, wait for green, fast-forward.

## Gates (enforced)

Commits are blocked by `.claude/hooks/check-commit.sh` unless
`.claude/hooks/gates.sh` passes and the message carries IDs in
brackets (`[W12]`, `[L4b]`, `[meta]`). CI re-runs the same gates on
every push; red blocks merge. Both are installed in Phase 5, before
the first line of feature code.

## Carried into later phases

- **Phase 2 mandatory items:** update/distribution mechanism,
  ecosystem integration (kyu, latch, homelab preset), backup &
  restore — plus the postponed **inbound-from-internet** round from
  NG5, which needs its own design (Cloudflare Access service token vs
  bypass with own token/signature checks).
- **Phase 3:** language and runtime are deliberately open (C5).
- **Phase 8:** the naming rationale opens `README.md`.
