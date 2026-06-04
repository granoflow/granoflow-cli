#!/usr/bin/env bash
set -euo pipefail

ROOT_DIR="$(cd "$(dirname "$0")/.." && pwd)"
TARGET="${GRANOFLOW_CLI_TARGET:-}"
PROFILE="release"
PARENT_DEST="${GRANOFLOW_CLI_SYNC_PATH:-$(cd "$ROOT_DIR/.." && pwd)/granoflow/scripts/granoflow-cli}"

if [[ -n "$TARGET" ]]; then
  cargo build --release --target "$TARGET" --manifest-path "$ROOT_DIR/Cargo.toml"
  BUILT_BIN="$ROOT_DIR/target/$TARGET/$PROFILE/granoflow"
else
  cargo build --release --manifest-path "$ROOT_DIR/Cargo.toml"
  BUILT_BIN="$ROOT_DIR/target/$PROFILE/granoflow"
fi

if [[ ! -x "$BUILT_BIN" ]]; then
  echo "Built binary not found: $BUILT_BIN" >&2
  exit 1
fi

mkdir -p "$(dirname "$PARENT_DEST")"
cp "$BUILT_BIN" "$PARENT_DEST"
chmod 0755 "$PARENT_DEST"

echo "Built: $BUILT_BIN"
echo "Synced: $PARENT_DEST"
