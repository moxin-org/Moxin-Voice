#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-}"
APP_RESOURCES="${MOXIN_APP_RESOURCES:-}"
MODEL_DIR="${GPT_SOVITS_MODEL_DIR:-$HOME/.OminiX/models/gpt-sovits-mlx}"
ASR_MODEL_DIR="${ASR_MODEL_DIR:-$HOME/.dora/models/asr/funasr}"
DATAFLOW_PATH="${MOXIN_DATAFLOW_PATH:-}"
APP_BIN_PATH=""
TTS_BIN_PATH=""

CONDA_ROOT="${MOXIN_CONDA_ROOT:-$HOME/.moxinvoice/conda}"
ENV_NAME="${MOXIN_CONDA_ENV:-moxin-studio}"
CONDA_ENV_PREFIX="${MOXIN_CONDA_ENV_PREFIX:-$CONDA_ROOT/envs/$ENV_NAME}"
CONDA_BIN="${MOXIN_CONDA_BIN:-$CONDA_ROOT/bin/conda}"

INFERENCE_BACKEND="${MOXIN_INFERENCE_BACKEND:-primespeech_mlx}"
ZERO_SHOT_BACKEND="${MOXIN_ZERO_SHOT_BACKEND:-primespeech_mlx}"
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
  TTS_BIN_PATH="$APP_RESOURCES/../MacOS/moxin-tts-node"
else
  APP_BIN_PATH="$APP_RESOURCES/target/debug/moxin-voice-shell"
  TTS_BIN_PATH="$APP_RESOURCES/target/debug/moxin-tts-node"
  if [[ ! -f "$APP_BIN_PATH" ]]; then
    APP_BIN_PATH="$APP_RESOURCES/target/release/moxin-voice-shell"
  fi
  if [[ ! -f "$TTS_BIN_PATH" ]]; then
    TTS_BIN_PATH="$APP_RESOURCES/target/release/moxin-tts-node"
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

check_dir() {
  local path="$1"
  local label="$2"
  if [[ ! -d "$path" ]]; then
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
    qwen_node_resolved=1
    return
  fi
  if [[ -x "$APP_RESOURCES/target/debug/qwen-tts-node" ]]; then
    qwen_node_resolved=1
    return
  fi
  if [[ -x "$APP_RESOURCES/target/release/qwen-tts-node" ]]; then
    qwen_node_resolved=1
    return
  fi
  if [[ -n "${MOXIN_QWEN3_TTS_NODE_BIN:-}" && -x "${MOXIN_QWEN3_TTS_NODE_BIN}" ]]; then
    qwen_node_resolved=1
    return
  fi
  if command -v qwen3-tts-node >/dev/null 2>&1; then
    qwen_node_resolved=1
    return
  fi
  if command -v qwen-tts-node >/dev/null 2>&1; then
    qwen_node_resolved=1
    return
  fi
}

check_cmd dora "Dora CLI"

if [[ ! -x "$CONDA_BIN" ]] && command -v conda >/dev/null 2>&1; then
  CONDA_BIN="$(command -v conda)"
fi

if [[ ! -x "$CONDA_BIN" ]]; then
  errors+=("Conda missing (expected: $CONDA_ROOT)")
elif [[ ! -x "$CONDA_ENV_PREFIX/bin/python" ]]; then
  errors+=("Conda env missing: $CONDA_ENV_PREFIX")
else
  if [[ "$MODE" != "--quick" ]]; then
    if ! "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" python -c "import dora_asr" >/dev/null 2>&1; then
      errors+=("Python package missing in $CONDA_ENV_PREFIX: dora-asr")
    fi
    if ! "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" python -c "import dora_primespeech" >/dev/null 2>&1; then
      warnings+=("Python package missing in $CONDA_ENV_PREFIX: dora-primespeech (Option A few-shot training unavailable)")
    fi
  fi
fi

check_file "$DATAFLOW_PATH" "Dataflow file"
check_file "$APP_BIN_PATH" "App runtime binary"
check_file "$TTS_BIN_PATH" "MLX TTS node binary"

check_file "$MODEL_DIR/encoders/hubert.safetensors" "MLX HuBERT model"
check_file "$MODEL_DIR/encoders/bert.safetensors" "MLX BERT model"
check_file "$MODEL_DIR/voices/voices.json" "MLX voices registry"

if [[ ! -d "$ASR_MODEL_DIR" ]]; then
  warnings+=("ASR model directory not found: $ASR_MODEL_DIR")
fi

if [[ "$INFERENCE_BACKEND" == "qwen3_tts_mlx" || "$ZERO_SHOT_BACKEND" == "qwen3_tts_mlx" ]]; then
  resolve_qwen_node
  if [[ "$qwen_node_resolved" != "1" ]]; then
    if [[ -f "$APP_RESOURCES/../MacOS/moxin-voice-shell-bin" ]]; then
      errors+=("Qwen backend selected but qwen node binary missing. Build/package qwen-tts-node or set MOXIN_QWEN3_TTS_NODE_BIN")
    else
      warnings+=("Qwen node binary not found yet in dev tree; it will be built on-demand when dataflow starts")
    fi
  fi
fi

if [[ "$INFERENCE_BACKEND" == "qwen3_tts_mlx" ]]; then
  if ! qwen_model_ready "$QWEN_CUSTOM_DIR"; then
    errors+=("Qwen CustomVoice model incomplete: $QWEN_CUSTOM_DIR")
  fi
fi
if [[ "$ZERO_SHOT_BACKEND" == "qwen3_tts_mlx" ]]; then
  if ! qwen_model_ready "$QWEN_BASE_DIR"; then
    errors+=("Qwen Base model incomplete: $QWEN_BASE_DIR")
  fi
fi

if [[ "$MODE" != "--quick" ]]; then
  echo "=== Moxin Voice Preflight ==="
  echo "Resources: $APP_RESOURCES"
  echo "Dataflow: $DATAFLOW_PATH"
  echo "Conda root: $CONDA_ROOT"
  echo "Conda env: $CONDA_ENV_PREFIX"
  echo "MLX models: $MODEL_DIR"
  echo "ASR models: $ASR_MODEL_DIR"
  echo "Inference backend: $INFERENCE_BACKEND"
  echo "Zero-shot backend: $ZERO_SHOT_BACKEND"
  echo "Qwen root: $QWEN_ROOT"
  echo
fi

if ((${#warnings[@]} > 0)) && [[ "$MODE" != "--quiet" ]]; then
  echo "Warnings:"
  for w in "${warnings[@]}"; do
    echo "  - $w"
  done
  echo
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
