#!/usr/bin/env bash
# http-switchboard's own gate. Run by the kit's gates.sh after fmt, clippy
# and the tests, on every commit. Project-owned; `chassis sync` never
# touches this file.
#
# ── The tiers (Kenny's test policy, 2026-09-09, applied here 2026-09-10)
#
# "Draai zoveel mogelijk lokaal, enkel testen wat er verandert, bij een
# release testen we de hele suite. Enkel wat via github actions moet gaat
# via ci."
#
# So this script has two tiers and picks between them by looking at the
# repository rather than at a flag somebody has to remember:
#
#   commit   fmt, clippy and the in-process suite — about eleven seconds.
#            Everything that needs docker or the network is skipped.
#
#   release  the commit tier PLUS the three infrastructure-bound checks:
#            the end-to-end suite against a REAL kyu container, cargo-deny,
#            and building the container image. All three run here, locally,
#            BEFORE the tag exists — which is the point. kyu-runner once
#            lost a tag because its image would not build, and the release
#            workflow is the worst place to find that out.
#
# ── How the release tier is detected
#
# `chassis release` bumps Cargo.toml and then commits through these gates
# (its own step 2). At that moment the package version is ahead of every
# tag in the repository, and at no other moment is it. That is the signal:
# no flag to pass, nothing to forget, and it cannot fire on an ordinary
# commit because an ordinary commit does not move the version.
#
# ── What CI still does, and why that is only one job
#
# Branch protection is a GitHub feature, so something GitHub can see has
# to say the commit is good: that is the single required check. Everything
# else it used to run now runs here, where it runs before the push instead
# of after it. The omissions are named on purpose (standing rule 38): CI
# does not run cargo-deny, the container build or coverage, because this
# script does — and `tests/l12_gate_tiers.rs` compares the two lists so
# they cannot drift apart silently.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

version=$(grep -m1 '^version' Cargo.toml | sed -E 's/.*"(.*)".*/\1/')
tier=commit
if [ -n "$version" ] && ! git rev-parse -q --verify "refs/tags/v$version" >/dev/null 2>&1; then
  # The version has no tag yet. On its own that is true of every commit
  # after a release too, so narrow it: only a commit that CHANGES the
  # version is the release commit.
  if git diff --cached --unified=0 -- Cargo.toml 2>/dev/null | grep -qE '^\+version = '; then
    tier=release
  fi
fi

echo "gates.project: tier=$tier (version $version)"

if [ "$tier" = commit ]; then
  echo "gates.project: the real-kyu suite, cargo-deny and the image build run at the release tier"
  exit 0
fi

# ── release tier ──────────────────────────────────────────────────────
# The end-to-end suite runs against a REAL kyu container and skips itself
# silently when KYU_IMAGE is unset — five tests, including the most
# important ones, reporting "ok" in 0.00 s. This sets it, so a green gate
# means what it says; if the image cannot be pulled the tests fail, which
# is the honest outcome.
export KYU_IMAGE="${KYU_IMAGE:-ghcr.io/kennypassenier/kyu:2.0.0}"
echo "gates.project: end-to-end against a real kyu container"
cargo test --test l4_pump

echo "gates.project: cargo-deny"
cargo deny check all

echo "gates.project: the container image builds"
docker build -q -t http-switchboard:release-gate . >/dev/null
docker run --rm http-switchboard:release-gate --version
if docker run --rm http-switchboard:release-gate --healthcheck http://127.0.0.1:1/healthz; then
  echo "the healthcheck must fail against a closed port" >&2
  exit 1
fi
