#!/usr/bin/env bash
# generate_qwen_previews.sh
#
# Pre-generates preview WAV files for each Qwen3-TTS CustomVoice preset speaker.
# Output: ~/.OminiX/models/qwen3-tts-mlx/previews/<speaker_id>.wav
#
# Usage:
#   cd <repo-root>
#   bash scripts/generate_qwen_previews.sh
#
# The script builds the synthesize example from the local patch first,
# then generates one ~3s clip per speaker.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
PATCH_DIR="$REPO_ROOT/node-hub/dora-qwen3-tts-mlx/patches/qwen3-tts-mlx"
MODEL_DIR="$HOME/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit"
OUT_DIR="$HOME/.OminiX/models/qwen3-tts-mlx/previews"

# ── Validate model dir ────────────────────────────────────────────────────────
if [ ! -d "$MODEL_DIR" ]; then
    echo "[ERROR] CustomVoice-8bit model not found at: $MODEL_DIR"
    echo "        Run scripts/download_qwen3_tts_models.py first."
    exit 1
fi

# ── Build the synthesize example ──────────────────────────────────────────────
echo "[1/3] Building synthesize example..."
cd "$PATCH_DIR"
cargo build --example synthesize --release 2>&1 | tail -5
SYNTH_BIN="$PATCH_DIR/../../target/release/examples/synthesize"
if [ ! -f "$SYNTH_BIN" ]; then
    # Fallback: look in workspace target
    SYNTH_BIN="$REPO_ROOT/target/release/examples/synthesize"
fi
if [ ! -f "$SYNTH_BIN" ]; then
    echo "[ERROR] synthesize binary not found after build."
    exit 1
fi
echo "        Binary: $SYNTH_BIN"

# ── Create output dir ────────────────────────────────────────────────────────
mkdir -p "$OUT_DIR"

# ── Speaker definitions: (id, language, text) ────────────────────────────────
declare -a SPEAKERS=(
    "vivian     chinese  你好，欢迎使用 Moxin 语音助手，我是薇薇安。"
    "serena     chinese  你好，欢迎使用 Moxin 语音助手，我是赛琳娜。"
    "uncle_fu   chinese  你好，欢迎使用 Moxin 语音助手，我是傅叔。"
    "dylan      chinese  你好，欢迎使用 Moxin 语音助手，我是迪伦。"
    "eric       chinese  你好，欢迎使用 Moxin 语音助手，我是埃里克。"
    "ryan       english  Hello, welcome to Moxin Voice. I'm Ryan."
    "aiden      english  Hello, welcome to Moxin Voice. I'm Aiden."
    "ono_anna   japanese こんにちは。Moxin ボイスへようこそ。小野安奈です。"
    "sohee      korean   안녕하세요. Moxin 보이스에 오신 것을 환영합니다. 저는 소희입니다."
)

# ── Generate ─────────────────────────────────────────────────────────────────
echo "[2/3] Generating preview audio files → $OUT_DIR"
FAILED=()
for entry in "${SPEAKERS[@]}"; do
    # Parse: first word = id, second = language, rest = text
    SPEAKER_ID=$(echo "$entry" | awk '{print $1}')
    LANGUAGE=$(echo "$entry"   | awk '{print $2}')
    TEXT=$(echo "$entry"       | cut -d' ' -f3-)
    OUT_FILE="$OUT_DIR/${SPEAKER_ID}.wav"

    echo "  [$SPEAKER_ID] ($LANGUAGE) → $OUT_FILE"
    if "$SYNTH_BIN" \
        --model-dir "$MODEL_DIR" \
        --speaker  "$SPEAKER_ID" \
        --language "$LANGUAGE" \
        --output   "$OUT_FILE" \
        "$TEXT" 2>/dev/null; then
        echo "       OK"
    else
        echo "       FAILED"
        FAILED+=("$SPEAKER_ID")
    fi
done

# ── Summary ───────────────────────────────────────────────────────────────────
echo "[3/3] Done."
if [ ${#FAILED[@]} -gt 0 ]; then
    echo "      Failed speakers: ${FAILED[*]}"
    exit 1
fi
echo "      All ${#SPEAKERS[@]} preview files generated successfully."
ls -lh "$OUT_DIR"
