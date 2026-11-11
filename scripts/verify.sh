#!/usr/bin/env bash
# Run the project's checks. The same script backs the Stop hook, CI and the README.
#
#   scripts/verify.sh           everything: Rust, web, and the end-to-end check
#   scripts/verify.sh --quick   Rust only (format, clippy, tests)
set -euo pipefail

root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$root"

quick=0
[[ "${1:-}" == "--quick" ]] && quick=1

step() { printf '\n== %s\n' "$1"; }

step "rust: format"
cargo fmt --all --check

step "rust: clippy (warnings are errors)"
cargo clippy --all-targets -- -D warnings

step "rust: tests"
cargo test --quiet

if [[ $quick -eq 1 ]]; then
  printf '\nquick checks passed\n'
  exit 0
fi

step "web: install, typecheck, lint, build"
(
  cd apps/web
  pnpm install --frozen-lockfile --silent
  pnpm typecheck
  pnpm lint
  pnpm build >/dev/null
)

step "end to end: restore a page, then check the download like a skeptic"
scripts/e2e.sh

printf '\nall checks passed\n'
