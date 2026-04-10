#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mode="${1:-all}"

run_check() {
  cargo check --workspace --all-targets
}

run_clippy() {
  local packages=(
    app
    summer-admin-macros
    summer-common
    summer-auth
    summer-system-model
    summer-ai-model
    summer-ai-hub
    summer-domain
    summer-system
  )

  local args=()
  for package in "${packages[@]}"; do
    args+=(-p "$package")
  done

  cargo clippy "${args[@]}" --all-targets --no-deps -- -D warnings
}

run_clippy_workspace() {
  cargo clippy --workspace --all-targets
}

run_test_compile() {
  cargo test --workspace --no-run
}

case "$mode" in
  check)
    run_check
    ;;
  clippy)
    run_clippy
    ;;
  clippy-workspace)
    run_clippy_workspace
    ;;
  test-compile)
    run_test_compile
    ;;
  all)
    run_check
    run_clippy
    run_test_compile
    ;;
  *)
    echo "usage: $0 [check|clippy|clippy-workspace|test-compile|all]" >&2
    exit 1
    ;;
esac
