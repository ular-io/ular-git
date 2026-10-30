#!/usr/bin/env bash
#
# Canonical routine verification for ular-git.
#
# Runs the automated contract that every change must pass, in order, failing
# fast and naming the stage that failed. Contains no local paths so it works
# from any clean checkout. Real-terminal checks (layout at 160 / 100 / 56
# columns, push / pull against a scratch remote, Open With) are manual steps
# documented in docs/testing.md.
#
# Usage: scripts/check.sh   (run from the repository root)

set -euo pipefail

cd "$(dirname "$0")/.."

stage() {
  printf '\n==> %s\n' "$1"
}

stage "cargo fmt --all -- --check"
cargo fmt --all -- --check

stage "cargo test -q"
cargo test -q

stage "cargo clippy --all-targets --all-features -- -D warnings"
cargo clippy --all-targets --all-features -- -D warnings

stage "cargo build --release"
cargo build --release

printf '\nAll checks passed.\n'
