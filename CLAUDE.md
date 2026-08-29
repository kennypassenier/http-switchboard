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
| Current phase | 3 · Tech choice |
| Last completed gate | Phase 2 freeze form (2026-08-29) — features frozen |
| Next gate | Phase 3 tech-choice decision form |
| AFK mode | off |

<!-- Update this block after every completed gate. -->

## Project documents

| Doc | Purpose |
|---|---|
| docs/SCOPE.md | goals, non-goals, success criteria, constraints (Phase 0) |
| docs/FEATURES.md | rated feature list with permanent IDs (Phase 2) |
| docs/ARCHITECTURE_DECISIONS.md | frozen AR decisions incl. tech choice (Phases 3-4) |
| docs/REALIZATION_PLAN.md | milestones + status table (Phase 5) |
| docs/TEST_PLAN.md | what is proven where + accepted limitations (Phase 7) |

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
