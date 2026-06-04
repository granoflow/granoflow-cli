#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
cd "$ROOT_DIR"

echo "[quality] cargo fmt --check"
cargo fmt --check

echo "[quality] cargo clippy --all-targets --all-features -- -D warnings"
cargo clippy --all-targets --all-features -- -D warnings

echo "[quality] cargo test --all-targets --all-features"
cargo test --all-targets --all-features
