#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

git config core.hooksPath scripts/git-hooks
chmod +x scripts/git-hooks/pre-push

echo "Git hooks installed: core.hooksPath=scripts/git-hooks"
