#!/usr/bin/env bash
# Initialize Qwen3-TTS-MLX and Qwen3-ASR-MLX model files for development.
# Downloads Qwen3-TTS CustomVoice + Base models and the Qwen3-ASR model.
#
# Usage:
#   bash scripts/init_qwen3_models.sh
#
# Environment overrides:
#   QWEN3_TTS_MODEL_ROOT           - TTS root directory (default: ~/.OminiX/models/qwen3-tts-mlx)
#   QWEN3_TTS_CUSTOMVOICE_REPO     - HuggingFace repo for CustomVoice model
#   QWEN3_TTS_BASE_REPO            - HuggingFace repo for Base model
#   QWEN3_ASR_MODEL_PATH           - ASR model directory (default: ~/.OminiX/models/qwen3-asr-1.7b)
#   QWEN3_ASR_REPO                 - HuggingFace repo for ASR model
#   HF_HUB_ENABLE_HF_TRANSFER      - set to 1 to use faster hf_transfer (default: 1)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

QWEN_ROOT="${QWEN3_TTS_MODEL_ROOT:-$HOME/.OminiX/models/qwen3-tts-mlx}"
CUSTOM_REPO="${QWEN3_TTS_CUSTOMVOICE_REPO:-mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit}"
BASE_REPO="${QWEN3_TTS_BASE_REPO:-Qwen/Qwen3-TTS-12Hz-1.7B-Base}"

QWEN_ASR_DIR="${QWEN3_ASR_MODEL_PATH:-$HOME/.OminiX/models/qwen3-asr-1.7b}"
QWEN_ASR_REPO="${QWEN3_ASR_REPO:-OminiX-ai/Qwen3-ASR-1.7B-MLX}"

echo "=== Qwen3 Model Initialization (TTS + ASR) ==="
echo "TTS model root:   $QWEN_ROOT"
echo "CustomVoice repo: $CUSTOM_REPO"
echo "Base repo:        $BASE_REPO"
echo "ASR model dir:    $QWEN_ASR_DIR"
echo "ASR repo:         $QWEN_ASR_REPO"
echo ""

# Check for Python
if ! command -v python3 >/dev/null 2>&1; then
  echo "ERROR: python3 not found. Install Python 3.8+ first."
  exit 1
fi

DOWNLOAD_SCRIPT="$ROOT_DIR/scripts/download_qwen3_tts_models.py"
if [[ ! -f "$DOWNLOAD_SCRIPT" ]]; then
  echo "ERROR: download script not found: $DOWNLOAD_SCRIPT"
  exit 1
fi

echo "Downloading models (this may take a while on first run)..."
HF_HUB_ENABLE_HF_TRANSFER="${HF_HUB_ENABLE_HF_TRANSFER:-1}" \
python3 "$DOWNLOAD_SCRIPT" \
  --root "$QWEN_ROOT" \
  --custom-repo "$CUSTOM_REPO" \
  --base-repo "$BASE_REPO" \
  --need-custom \
  --need-base

echo ""

# Download Qwen3-ASR model
echo "=== Qwen3-ASR Model ==="
if [[ ! -f "$QWEN_ASR_DIR/config.json" ]]; then
  if ! command -v huggingface-cli >/dev/null 2>&1; then
    echo "ERROR: huggingface-cli not found. Install with: pip install huggingface_hub"
    echo "Skipping ASR model download."
  else
    echo "Downloading Qwen3-ASR model to $QWEN_ASR_DIR ..."
    mkdir -p "$QWEN_ASR_DIR"
    HF_HUB_ENABLE_HF_TRANSFER="${HF_HUB_ENABLE_HF_TRANSFER:-1}" \
      huggingface-cli download "$QWEN_ASR_REPO" --local-dir "$QWEN_ASR_DIR"
  fi
else
  echo "Qwen3-ASR model already present at $QWEN_ASR_DIR"
fi

echo ""
echo "Done."
echo "  TTS models: $QWEN_ROOT"
echo "  ASR model:  $QWEN_ASR_DIR"
echo "You can now run: cargo run -p moxin-voice-shell"
