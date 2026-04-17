#!/usr/bin/env bash
# Preflight checks for Moxin Voice (Qwen3-only, no conda/Python).
# Conda checks removed — bootstrap now uses the bundled moxin-init Rust binary.
set -euo pipefail

MODE="${1:-}"
APP_RESOURCES="${MOXIN_APP_RESOURCES:-}"
DATAFLOW_PATH="${MOXIN_DATAFLOW_PATH:-}"
APP_BIN_PATH=""

QWEN_ROOT="${QWEN3_TTS_MODEL_ROOT:-$HOME/.OminiX/models/qwen3-tts-mlx}"
QWEN_CUSTOM_DIR="${QWEN3_TTS_CUSTOMVOICE_MODEL_DIR:-$QWEN_ROOT/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit}"
QWEN_BASE_DIR="${QWEN3_TTS_BASE_MODEL_DIR:-$QWEN_ROOT/Qwen3-TTS-12Hz-1.7B-Base-8bit}"
QWEN_ASR_MODEL_DIR="${QWEN3_ASR_MODEL_PATH:-$HOME/.OminiX/models/qwen3-asr-1.7b}"
QWEN35_TRANSLATOR_MODEL_DIR="${QWEN35_TRANSLATOR_MODEL_PATH:-$HOME/.OminiX/models/Qwen3.5-2B-MLX-4bit}"
QWEN_CUSTOM_REPO="${QWEN3_TTS_CUSTOMVOICE_REPO:-mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit}"
QWEN_BASE_REPO="${QWEN3_TTS_BASE_REPO:-mlx-community/Qwen3-TTS-12Hz-1.7B-Base-8bit}"
QWEN_ASR_REPO="${QWEN3_ASR_REPO:-mlx-community/Qwen3-ASR-1.7B-8bit}"
QWEN35_TRANSLATOR_REPO="${QWEN35_TRANSLATOR_REPO:-mlx-community/Qwen3.5-2B-MLX-4bit}"
MODEL_COMPLETION_MARKER=".moxin-model-complete.json"
BOOTSTRAP_VERSION=1

if [[ -z "$APP_RESOURCES" ]]; then
  APP_RESOURCES="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fi

if [[ -z "$DATAFLOW_PATH" ]]; then
  if [[ -f "$APP_RESOURCES/dataflow/tts.yml" ]]; then
    DATAFLOW_PATH="$APP_RESOURCES/dataflow/tts.yml"
  elif [[ -f "$APP_RESOURCES/apps/moxin-voice/dataflow/tts.yml" ]]; then
    DATAFLOW_PATH="$APP_RESOURCES/apps/moxin-voice/dataflow/tts.yml"
  else
    DATAFLOW_PATH="$APP_RESOURCES/dataflow/tts.runtime.yml"
  fi
fi

TRANSLATION_DATAFLOW_PATH=""
if [[ -f "$APP_RESOURCES/dataflow/translation_qwen35.yml" ]]; then
  TRANSLATION_DATAFLOW_PATH="$APP_RESOURCES/dataflow/translation_qwen35.yml"
elif [[ -f "$APP_RESOURCES/apps/moxin-voice/dataflow/translation_qwen35.yml" ]]; then
  TRANSLATION_DATAFLOW_PATH="$APP_RESOURCES/apps/moxin-voice/dataflow/translation_qwen35.yml"
fi

if [[ -f "$APP_RESOURCES/../MacOS/moxin-voice-shell-bin" ]]; then
  APP_BIN_PATH="$APP_RESOURCES/../MacOS/moxin-voice-shell-bin"
else
  APP_BIN_PATH="$APP_RESOURCES/target/debug/moxin-voice-shell"
  if [[ ! -f "$APP_BIN_PATH" ]]; then
    APP_BIN_PATH="$APP_RESOURCES/target/release/moxin-voice-shell"
  fi
fi

errors=()
warnings=()

check_file() {
  local path="$1"
  local label="$2"
  if [[ ! -f "$path" ]]; then
    errors+=("$label missing: $path")
  fi
}

check_cmd() {
  local cmd="$1"
  local label="$2"
  if ! command -v "$cmd" >/dev/null 2>&1; then
    errors+=("$label command not found: $cmd")
  fi
}

qwen_model_ready() {
  local model_dir="$1"
  [[ -f "$model_dir/config.json" ]] &&
  [[ -f "$model_dir/generation_config.json" ]] &&
  [[ -f "$model_dir/vocab.json" ]] &&
  [[ -f "$model_dir/merges.txt" ]] &&
  ([[ -f "$model_dir/model.safetensors" ]] || [[ -f "$model_dir/model.safetensors.index.json" ]]) &&
  [[ -f "$model_dir/speech_tokenizer/config.json" ]] &&
  [[ -f "$model_dir/speech_tokenizer/model.safetensors" ]]
}

qwen35_translation_model_ready() {
  local model_dir="$1"
  [[ -f "$model_dir/config.json" ]] &&
  [[ -f "$model_dir/tokenizer.json" ]] &&
  [[ -f "$model_dir/tokenizer_config.json" ]] &&
  ([[ -f "$model_dir/model.safetensors" ]] || [[ -f "$model_dir/model.safetensors.index.json" ]])
}

asr_model_ready() {
  local model_dir="$1"
  [[ -f "$model_dir/config.json" ]]
}

model_completion_marker_valid() {
  local model_dir="$1"
  local repo_id="$2"
  local marker="$model_dir/$MODEL_COMPLETION_MARKER"
  [[ -s "$marker" ]] && grep -q "\"repo_id\"[[:space:]]*:[[:space:]]*\"$repo_id\"" "$marker"
}

write_model_completion_marker() {
  local model_dir="$1"
  local repo_id="$2"
  mkdir -p "$model_dir"
  cat > "$model_dir/$MODEL_COMPLETION_MARKER" <<EOF
{
  "repo_id": "$repo_id",
  "bootstrap_version": $BOOTSTRAP_VERSION
}
EOF
}

ensure_model_complete() {
  local model_dir="$1"
  local repo_id="$2"
  local checker="$3"

  if "$checker" "$model_dir"; then
    if ! model_completion_marker_valid "$model_dir" "$repo_id"; then
      write_model_completion_marker "$model_dir" "$repo_id"
    fi
    return 0
  fi
  return 1
}

# Locate qwen-tts-node binary
qwen_node_resolved=0
resolve_qwen_node() {
  if [[ -f "$APP_RESOURCES/../MacOS/qwen-tts-node" ]]; then
    qwen_node_resolved=1; return
  fi
  if [[ -x "$APP_RESOURCES/target/debug/qwen-tts-node" ]]; then
    qwen_node_resolved=1; return
  fi
  if [[ -x "$APP_RESOURCES/target/release/qwen-tts-node" ]]; then
    qwen_node_resolved=1; return
  fi
  if [[ -n "${MOXIN_QWEN3_TTS_NODE_BIN:-}" && -x "${MOXIN_QWEN3_TTS_NODE_BIN}" ]]; then
    qwen_node_resolved=1; return
  fi
  if command -v qwen3-tts-node >/dev/null 2>&1; then
    qwen_node_resolved=1; return
  fi
  if command -v qwen-tts-node >/dev/null 2>&1; then
    qwen_node_resolved=1; return
  fi
}

# Locate moxin-init binary
moxin_init_resolved=0
resolve_moxin_init() {
  if [[ -x "$APP_RESOURCES/../MacOS/moxin-init" ]]; then
    moxin_init_resolved=1; return
  fi
  if [[ -x "$APP_RESOURCES/target/debug/moxin-init" ]]; then
    moxin_init_resolved=1; return
  fi
  if [[ -x "$APP_RESOURCES/target/release/moxin-init" ]]; then
    moxin_init_resolved=1; return
  fi
}

check_cmd dora "Dora CLI"
check_file "$DATAFLOW_PATH" "Dataflow file"
check_file "$APP_BIN_PATH" "App runtime binary"
check_file "$TRANSLATION_DATAFLOW_PATH" "Translation dataflow file"

# Check dora-qwen3-asr binary
if [[ ! -x "${APP_RESOURCES}/../MacOS/dora-qwen3-asr" ]] && \
   [[ ! -x "${APP_RESOURCES}/target/debug/dora-qwen3-asr" ]] && \
   [[ ! -x "${APP_RESOURCES}/target/release/dora-qwen3-asr" ]]; then
  warnings+=("dora-qwen3-asr binary not found — voice cloning transcription unavailable (run: cargo build -p dora-qwen3-asr)")
fi

# Check dora-qwen35-translator binary
if [[ ! -x "${APP_RESOURCES}/../MacOS/dora-qwen35-translator" ]] && \
   [[ ! -x "${APP_RESOURCES}/target/debug/dora-qwen35-translator" ]] && \
   [[ ! -x "${APP_RESOURCES}/target/release/dora-qwen35-translator" ]]; then
  warnings+=("dora-qwen35-translator binary not found — Qwen3.5 translation backend unavailable (run: cargo build -p dora-qwen35-translator)")
fi

if [[ -n "$TRANSLATION_DATAFLOW_PATH" && -f "$TRANSLATION_DATAFLOW_PATH" ]]; then
  if grep -q 'TRANSLATION_MERGE_ENABLED' "$TRANSLATION_DATAFLOW_PATH"; then
    errors+=("translation_qwen35.yml still references removed TRANSLATION_MERGE_ENABLED placeholder: $TRANSLATION_DATAFLOW_PATH")
  fi
  if ! grep -q 'path: __ASR_BIN_PATH__' "$TRANSLATION_DATAFLOW_PATH"; then
    errors+=("translation_qwen35.yml missing __ASR_BIN_PATH__ placeholder: $TRANSLATION_DATAFLOW_PATH")
  fi
  if ! grep -q 'path: __TRANSLATOR_BIN_PATH__' "$TRANSLATION_DATAFLOW_PATH"; then
    errors+=("translation_qwen35.yml missing __TRANSLATOR_BIN_PATH__ placeholder: $TRANSLATION_DATAFLOW_PATH")
  fi
  if ! grep -q 'question_ended: moxin-mic-input/question_ended' "$TRANSLATION_DATAFLOW_PATH"; then
    warnings+=("translation_qwen35.yml no longer wires question_ended into ASR: $TRANSLATION_DATAFLOW_PATH")
  fi
fi

# Check qwen-tts-node binary
resolve_qwen_node
if [[ "$qwen_node_resolved" != "1" ]]; then
  if [[ -f "$APP_RESOURCES/../MacOS/moxin-voice-shell-bin" ]]; then
    errors+=("qwen-tts-node binary missing from app bundle. Run build_macos_app.sh.")
  else
    warnings+=("qwen-tts-node not found yet in dev tree; it will be built on-demand when dataflow starts.")
  fi
fi

# Check moxin-init binary (required for first-run bootstrap)
resolve_moxin_init
if [[ "$moxin_init_resolved" != "1" ]]; then
  if [[ -f "$APP_RESOURCES/../MacOS/moxin-voice-shell-bin" ]]; then
    errors+=("moxin-init binary missing from app bundle. Run build_macos_app.sh.")
  else
    warnings+=("moxin-init not found in dev tree (run: cargo build -p moxin-init --release)")
  fi
fi

# Check Qwen3 ASR model (required)
if ! ensure_model_complete "$QWEN_ASR_MODEL_DIR" "$QWEN_ASR_REPO" asr_model_ready; then
  errors+=("Qwen3-ASR model not found: $QWEN_ASR_MODEL_DIR — run moxin-init or launch the app")
fi

# Check Qwen3 TTS models (required)
if ! ensure_model_complete "$QWEN_CUSTOM_DIR" "$QWEN_CUSTOM_REPO" qwen_model_ready; then
  errors+=("Qwen3 CustomVoice model incomplete: $QWEN_CUSTOM_DIR — run moxin-init or launch the app")
fi
if ! ensure_model_complete "$QWEN_BASE_DIR" "$QWEN_BASE_REPO" qwen_model_ready; then
  errors+=("Qwen3 Base model incomplete: $QWEN_BASE_DIR — run moxin-init or launch the app")
fi

# Check Qwen3.5 translator model (required)
if ! ensure_model_complete "$QWEN35_TRANSLATOR_MODEL_DIR" "$QWEN35_TRANSLATOR_REPO" qwen35_translation_model_ready; then
  errors+=("Qwen3.5 translator model incomplete: $QWEN35_TRANSLATOR_MODEL_DIR — run moxin-init or launch the app")
fi

if [[ "$MODE" != "--quick" ]]; then
  echo "=== Moxin Voice Preflight (Qwen3-only) ==="
  echo "Resources:  $APP_RESOURCES"
  echo "Dataflow:   $DATAFLOW_PATH"
  echo "ASR model:  $QWEN_ASR_MODEL_DIR"
  echo "Qwen3.5 translator model: $QWEN35_TRANSLATOR_MODEL_DIR"
  echo "Qwen root:  $QWEN_ROOT"
  echo ""
fi

if ((${#warnings[@]} > 0)) && [[ "$MODE" != "--quiet" ]]; then
  echo "Warnings:"
  for w in "${warnings[@]}"; do
    echo "  - $w"
  done
  echo ""
fi

if ((${#errors[@]} > 0)); then
  if [[ "$MODE" != "--quiet" ]]; then
    echo "Preflight failed:"
    for e in "${errors[@]}"; do
      echo "  - $e"
    done
  fi
  exit 1
fi

if [[ "$MODE" != "--quiet" ]]; then
  echo "Preflight passed."
fi
