#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

chmod +x build-tools/pre-commit build-tools/install-git-hooks.sh
git config core.hooksPath build-tools

echo "Git hooks installed."
echo "core.hooksPath=$(git config --get core.hooksPath)"
