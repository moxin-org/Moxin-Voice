#!/usr/bin/env bash
# generate_qwen_previews.sh
#
# Pre-generates preview WAV files for each Qwen3-TTS CustomVoice preset speaker.
# Output: ~/.OminiX/models/qwen3-tts-mlx/previews/<speaker_id>.wav
#
# Usage:
#   cd <repo-root>
#   bash scripts/generate_qwen_previews.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
MODEL_DIR="$HOME/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit"
OUT_DIR="$HOME/.OminiX/models/qwen3-tts-mlx/previews"

# ── Validate model dir ────────────────────────────────────────────────────────
if [ ! -d "$MODEL_DIR" ]; then
    echo "[ERROR] CustomVoice-8bit model not found at: $MODEL_DIR"
    echo "        Run scripts/download_qwen3_tts_models.py first."
    exit 1
fi

# ── Build gen-qwen-previews binary ───────────────────────────────────────────
echo "[1/2] Building gen-qwen-previews binary..."
cd "$REPO_ROOT"
cargo build -p dora-qwen3-tts-mlx --bin gen-qwen-previews --release 2>&1 | tail -5
GEN_BIN="$REPO_ROOT/target/release/gen-qwen-previews"
if [ ! -f "$GEN_BIN" ]; then
    echo "[ERROR] gen-qwen-previews binary not found after build."
    exit 1
fi
echo "        Binary: $GEN_BIN"

# ── Generate preview files ────────────────────────────────────────────────────
echo "[2/2] Generating preview audio files → $OUT_DIR"
QWEN3_TTS_CUSTOMVOICE_MODEL_DIR="$MODEL_DIR" "$GEN_BIN"
