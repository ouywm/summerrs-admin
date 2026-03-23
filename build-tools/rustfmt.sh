#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mode="${1:---check}"
case "$mode" in
  --check|--fix)
    ;;
  *)
    echo "usage: $0 [--check|--fix]" >&2
    exit 1
    ;;
esac

rustfmt_args=(
  --config-path "$repo_root/rustfmt.toml"
  --edition 2024
)

if [[ "$mode" == "--check" ]]; then
  rustfmt_args+=(--check)
fi

rust_files=()
while IFS= read -r -d '' file; do
  rust_files+=("$file")
done < <(find crates -type f -name '*.rs' -print0 | sort -z)

if [[ "${#rust_files[@]}" -eq 0 ]]; then
  echo "No Rust files found under crates/" >&2
  exit 0
fi

rustfmt "${rustfmt_args[@]}" "${rust_files[@]}"
