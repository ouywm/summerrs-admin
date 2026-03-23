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

taplo_args=(
  format
  --config "$repo_root/.taplo.toml"
)

if [[ "$mode" == "--check" ]]; then
  taplo_args+=(--check)
fi

toml_files=()

if [[ -f "Cargo.toml" ]]; then
  toml_files+=("Cargo.toml")
fi

if [[ -f ".taplo.toml" ]]; then
  toml_files+=(".taplo.toml")
fi

if [[ -d "config" ]]; then
  while IFS= read -r -d '' file; do
    toml_files+=("$file")
  done < <(find config -type f -name '*.toml' -print0 | sort -z)
fi

if [[ -d "crates" ]]; then
  while IFS= read -r -d '' file; do
    toml_files+=("$file")
  done < <(find crates -type f -name 'Cargo.toml' -print0 | sort -z)
fi

if [[ "${#toml_files[@]}" -eq 0 ]]; then
  echo "No TOML files found in configured project scope" >&2
  exit 0
fi

taplo "${taplo_args[@]}" "${toml_files[@]}"
