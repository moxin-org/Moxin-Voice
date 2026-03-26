#!/usr/bin/env bash
# macos_bootstrap.sh — Qwen3-only, no conda/Python required.
#
# Model download is handled by the bundled `moxin-init` Rust binary,
# which downloads Qwen3 TTS and ASR models directly from HuggingFace
# via HTTP (with resume support and HF_ENDPOINT mirror support).
#
# This script is intentionally minimal: locate moxin-init, pass
# environment variables, and exec it. All progress reporting is done
# by moxin-init itself (writes bootstrap_state.txt directly).
set -euo pipefail

APP_RESOURCES="${MOXIN_APP_RESOURCES:-$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)}"
STATE_PATH="${MOXIN_BOOTSTRAP_STATE_PATH:-$HOME/Library/Logs/MoxinVoice/bootstrap_state.txt}"

QWEN_ROOT="${QWEN3_TTS_MODEL_ROOT:-$HOME/.OminiX/models/qwen3-tts-mlx}"
QWEN_CUSTOM_DIR="${QWEN3_TTS_CUSTOMVOICE_MODEL_DIR:-$QWEN_ROOT/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit}"
QWEN_BASE_DIR="${QWEN3_TTS_BASE_MODEL_DIR:-$QWEN_ROOT/Qwen3-TTS-12Hz-1.7B-Base-8bit}"
QWEN_ASR_DIR="${QWEN3_ASR_MODEL_PATH:-$HOME/.OminiX/models/qwen3-asr-1.7b}"

QWEN_CUSTOM_REPO="${QWEN3_TTS_CUSTOMVOICE_REPO:-mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit}"
QWEN_BASE_REPO="${QWEN3_TTS_BASE_REPO:-mlx-community/Qwen3-TTS-12Hz-1.7B-Base-8bit}"
QWEN_ASR_REPO="${QWEN3_ASR_REPO:-mlx-community/Qwen3-ASR-1.7B-8bit}"

# Locate the moxin-init binary: app bundle first, then dev build trees.
resolve_moxin_init() {
  if [[ -x "$APP_RESOURCES/../MacOS/moxin-init" ]]; then
    echo "$APP_RESOURCES/../MacOS/moxin-init"; return 0
  fi
  for profile in release debug; do
    if [[ -x "$APP_RESOURCES/target/$profile/moxin-init" ]]; then
      echo "$APP_RESOURCES/target/$profile/moxin-init"; return 0
    fi
  done
  return 1
}

if ! MOXIN_INIT="$(resolve_moxin_init)"; then
  echo "ERROR: moxin-init binary not found." >&2
  echo "  Checked: $APP_RESOURCES/../MacOS/moxin-init" >&2
  echo "  Checked: $APP_RESOURCES/target/{release,debug}/moxin-init" >&2
  echo "  Run: cargo build -p moxin-init --release" >&2
  exit 1
fi

echo "=== Moxin Voice Bootstrap (moxin-init) ==="
echo "moxin-init: $MOXIN_INIT"
echo "Qwen TTS root: $QWEN_ROOT"
echo "ASR model dir: $QWEN_ASR_DIR"
echo ""

exec env \
  MOXIN_BOOTSTRAP_STATE_PATH="$STATE_PATH" \
  QWEN3_TTS_MODEL_ROOT="$QWEN_ROOT" \
  QWEN3_TTS_CUSTOMVOICE_MODEL_DIR="$QWEN_CUSTOM_DIR" \
  QWEN3_TTS_CUSTOMVOICE_REPO="$QWEN_CUSTOM_REPO" \
  QWEN3_TTS_BASE_MODEL_DIR="$QWEN_BASE_DIR" \
  QWEN3_TTS_BASE_REPO="$QWEN_BASE_REPO" \
  QWEN3_ASR_MODEL_PATH="$QWEN_ASR_DIR" \
  QWEN3_ASR_REPO="$QWEN_ASR_REPO" \
  HF_ENDPOINT="${HF_ENDPOINT:-}" \
  "$MOXIN_INIT"
