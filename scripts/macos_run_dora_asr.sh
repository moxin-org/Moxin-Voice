#!/usr/bin/env bash
# Resolve and exec the dora-qwen3-asr Rust binary.
# Replaces the old Python dora-asr conda entry point.
#
# Search order:
#   1) App bundle: Contents/MacOS/dora-qwen3-asr  (distribution)
#   2) Dev release build: <repo>/target/release/dora-qwen3-asr
#   3) Dev debug build:   <repo>/target/debug/dora-qwen3-asr
#   4) PATH fallback
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"

# Bundle layout: Contents/Resources/scripts/ -> Contents/MacOS/
BUNDLE_BIN="$SCRIPT_DIR/../MacOS/dora-qwen3-asr"
if [[ -x "$BUNDLE_BIN" ]]; then
  exec "$BUNDLE_BIN" "$@"
fi

# Dev repo layout: scripts/ is at <repo>/scripts/
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
if [[ -x "$REPO_ROOT/target/release/dora-qwen3-asr" ]]; then
  exec "$REPO_ROOT/target/release/dora-qwen3-asr" "$@"
fi
if [[ -x "$REPO_ROOT/target/debug/dora-qwen3-asr" ]]; then
  exec "$REPO_ROOT/target/debug/dora-qwen3-asr" "$@"
fi

if command -v dora-qwen3-asr >/dev/null 2>&1; then
  exec "$(command -v dora-qwen3-asr)" "$@"
fi

echo "ERROR: dora-qwen3-asr not found. Build with: cargo build -p dora-qwen3-asr" >&2
exit 127
