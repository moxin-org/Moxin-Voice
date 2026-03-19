#!/usr/bin/env bash
# Preflight checks for Moxin Voice (Qwen3-TTS-MLX only mode).
# PrimeSpeech-specific checks removed. See doc/REFACTOR_QWEN3_ONLY.md to restore.
set -euo pipefail

MODE="${1:-}"
APP_RESOURCES="${MOXIN_APP_RESOURCES:-}"
QWEN_ASR_MODEL_DIR="${QWEN3_ASR_MODEL_PATH:-$HOME/.OminiX/models/qwen3-asr-1.7b}"
DATAFLOW_PATH="${MOXIN_DATAFLOW_PATH:-}"
APP_BIN_PATH=""

CONDA_ROOT="${MOXIN_CONDA_ROOT:-$HOME/.moxinvoice/conda}"
ENV_NAME="${MOXIN_CONDA_ENV:-moxin-studio}"
CONDA_ENV_PREFIX="${MOXIN_CONDA_ENV_PREFIX:-$CONDA_ROOT/envs/$ENV_NAME}"
CONDA_BIN="${MOXIN_CONDA_BIN:-$CONDA_ROOT/bin/conda}"

# Qwen3-only: always qwen3 backends.
QWEN_ROOT="${QWEN3_TTS_MODEL_ROOT:-$HOME/.OminiX/models/qwen3-tts-mlx}"
QWEN_CUSTOM_DIR="${QWEN3_TTS_CUSTOMVOICE_MODEL_DIR:-$QWEN_ROOT/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit}"
QWEN_BASE_DIR="${QWEN3_TTS_BASE_MODEL_DIR:-$QWEN_ROOT/Qwen3-TTS-12Hz-1.7B-Base}"

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

check_cmd dora "Dora CLI"

if [[ ! -x "$CONDA_BIN" ]] && command -v conda >/dev/null 2>&1; then
  CONDA_BIN="$(command -v conda)"
fi

# dora-asr (Python) replaced by dora-qwen3-asr (Rust). Conda no longer required for ASR.
# Conda still needed for TTS model download script in bootstrap.
if [[ ! -x "$CONDA_BIN" ]]; then
  warnings+=("Conda missing (expected: $CONDA_ROOT) — needed for first-run bootstrap only")
elif [[ ! -x "$CONDA_ENV_PREFIX/bin/python" ]]; then
  warnings+=("Conda env missing: $CONDA_ENV_PREFIX — run scripts/macos_bootstrap.sh on first launch")
fi

check_file "$DATAFLOW_PATH" "Dataflow file"
check_file "$APP_BIN_PATH" "App runtime binary"
# moxin-tts-node (PrimeSpeech) check removed. See doc/REFACTOR_QWEN3_ONLY.md.

# Check dora-qwen3-asr binary
if [[ ! -x "${APP_RESOURCES}/../MacOS/dora-qwen3-asr" ]] && \
   [[ ! -x "${APP_RESOURCES}/target/debug/dora-qwen3-asr" ]] && \
   [[ ! -x "${APP_RESOURCES}/target/release/dora-qwen3-asr" ]]; then
  warnings+=("dora-qwen3-asr binary not found — voice cloning transcription will be unavailable (run: cargo build -p dora-qwen3-asr)")
fi

# Check Qwen3-ASR model
if [[ ! -f "$QWEN_ASR_MODEL_DIR/config.json" ]]; then
  warnings+=("Qwen3-ASR model not found: $QWEN_ASR_MODEL_DIR (run scripts/init_qwen3_models.sh)")
fi

# Qwen3 node check
resolve_qwen_node
if [[ "$qwen_node_resolved" != "1" ]]; then
  if [[ -f "$APP_RESOURCES/../MacOS/moxin-voice-shell-bin" ]]; then
    errors+=("qwen-tts-node binary missing from app bundle. Run build_macos_app.sh.")
  else
    warnings+=("qwen-tts-node not found yet in dev tree; it will be built on-demand when dataflow starts.")
  fi
fi

# Qwen3 model checks (required)
if ! qwen_model_ready "$QWEN_CUSTOM_DIR"; then
  errors+=("Qwen3 CustomVoice model incomplete: $QWEN_CUSTOM_DIR — run scripts/init_qwen3_models.sh")
fi
if ! qwen_model_ready "$QWEN_BASE_DIR"; then
  errors+=("Qwen3 Base model incomplete: $QWEN_BASE_DIR — run scripts/init_qwen3_models.sh")
fi

if [[ "$MODE" != "--quick" ]]; then
  echo "=== Moxin Voice Preflight (Qwen3-only) ==="
  echo "Resources:  $APP_RESOURCES"
  echo "Dataflow:   $DATAFLOW_PATH"
  echo "Conda env:  $CONDA_ENV_PREFIX"
  echo "ASR model:  $QWEN_ASR_MODEL_DIR"
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
