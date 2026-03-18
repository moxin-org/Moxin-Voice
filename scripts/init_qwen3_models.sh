#!/usr/bin/env bash
# Initialize Qwen3-TTS-MLX model files for development.
# Downloads both CustomVoice (inference) and Base (ICL voice cloning) models.
#
# Usage:
#   bash scripts/init_qwen3_models.sh
#
# Environment overrides:
#   QWEN3_TTS_MODEL_ROOT           - root directory (default: ~/.OminiX/models/qwen3-tts-mlx)
#   QWEN3_TTS_CUSTOMVOICE_REPO     - HuggingFace repo for CustomVoice model
#   QWEN3_TTS_BASE_REPO            - HuggingFace repo for Base model
#   HF_HUB_ENABLE_HF_TRANSFER      - set to 1 to use faster hf_transfer (default: 1)
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"

QWEN_ROOT="${QWEN3_TTS_MODEL_ROOT:-$HOME/.OminiX/models/qwen3-tts-mlx}"
CUSTOM_REPO="${QWEN3_TTS_CUSTOMVOICE_REPO:-mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit}"
BASE_REPO="${QWEN3_TTS_BASE_REPO:-Qwen/Qwen3-TTS-12Hz-1.7B-Base}"

echo "=== Qwen3-TTS-MLX Model Initialization ==="
echo "Model root: $QWEN_ROOT"
echo "CustomVoice repo: $CUSTOM_REPO"
echo "Base repo:        $BASE_REPO"
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
echo "Done. Models are at: $QWEN_ROOT"
echo "You can now run: cargo run -p moxin-voice-shell"
