#!/usr/bin/env bash
# http-switchboard's own gate (chassis 1.6.0, M1): run by the kit's gates.sh
# and CI after fmt, clippy and the tests. Project-owned; `chassis sync`
# never touches it.
set -euo pipefail
cd "$(git rev-parse --show-toplevel)"

# The end-to-end suite runs against a REAL kyu container and skips itself
# silently when KYU_IMAGE is unset — five tests, including the most
# important ones, reporting "ok" in 0.00 s (Phase 7 audit, G14). This gate
# sets it and runs that suite, so a green gate means what a green CI means;
# if the image cannot be pulled the tests fail, which is the honest outcome.
export KYU_IMAGE="${KYU_IMAGE:-ghcr.io/kennypassenier/kyu:2.0.0}"
cargo test --test l4_pump
