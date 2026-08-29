#!/usr/bin/env bash
# Project quality gates (Phase 5, S1: format + lint + the FULL test suite).
# Called by .githooks/pre-commit and by .claude/hooks/check-commit.sh;
# a non-zero exit blocks the commit.
set -euo pipefail

# Standing rule 7: the checks themselves rewrite files (cargo refreshes
# Cargo.lock, the formatter rewrites sources). Anything rewritten after
# `git add` is green here and absent from the commit. Fingerprint before
# and after, and refuse rather than report a green run over a moved tree.
gate_tree_fingerprint() {
  { git status --porcelain; git diff; } | sha256sum | cut -d' ' -f1
}
gate_tree_before=$(gate_tree_fingerprint)

cargo fmt --all -- --check
cargo clippy --all-targets -- -D warnings

# The end-to-end suite runs against a REAL kyu container and skips itself
# silently when KYU_IMAGE is unset — five tests, including the most
# important ones, reporting "ok" in 0.00 s. That made a locally green gate
# mean something weaker than a green CI run (Phase 7 audit, G14). The gate
# sets it, so the two agree; if the image cannot be pulled the tests fail,
# which is the honest outcome.
export KYU_IMAGE="${KYU_IMAGE:-ghcr.io/kennypassenier/kyu:2.0.0}"
cargo test --all

if [ "$(gate_tree_fingerprint)" != "$gate_tree_before" ]; then
  {
    echo "gates: the checks rewrote the working tree while they ran."
    echo "A file changed after it was staged, so what this commit carries is"
    echo "NOT what was just tested. The changed paths:"
    echo
    git status --porcelain
    echo
    echo "What now: run 'git add -A' and commit again."
  } >&2
  exit 1
fi
