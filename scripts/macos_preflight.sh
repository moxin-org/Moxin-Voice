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

if [[ -z "$APP_RESOURCES" ]]; then
  # Works when launched from app bundle (script lives in Contents/Resources/scripts)
  APP_RESOURCES="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fi

if [[ -z "$DATAFLOW_PATH" ]]; then
  if [[ -f "$APP_RESOURCES/dataflow/tts.yml" ]]; then
    DATAFLOW_PATH="$APP_RESOURCES/dataflow/tts.yml"
  else
    DATAFLOW_PATH="$APP_RESOURCES/apps/moxin-voice/dataflow/tts.yml"
  fi
fi

if [[ -f "$APP_RESOURCES/../MacOS/moxin-voice-shell-bin" ]]; then
  APP_BIN_PATH="$APP_RESOURCES/../MacOS/moxin-voice-shell-bin"
  TTS_BIN_PATH="$APP_RESOURCES/../MacOS/moxin-tts-node"
else
  APP_BIN_PATH="$APP_RESOURCES/target/release/moxin-voice-shell"
  TTS_BIN_PATH="$APP_RESOURCES/target/release/moxin-tts-node"
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

check_cmd dora "Dora CLI"

if [[ ! -x "$CONDA_BIN" ]] && command -v conda >/dev/null 2>&1; then
  CONDA_BIN="$(command -v conda)"
fi

if [[ ! -x "$CONDA_BIN" ]]; then
  errors+=("Conda missing (expected: $CONDA_ROOT)")
elif [[ ! -x "$CONDA_ENV_PREFIX/bin/python" ]]; then
  errors+=("Conda env missing: $CONDA_ENV_PREFIX")
else
  # Quick mode is used on app startup path and should not block launch.
  # Skip expensive Python import checks here; full mode keeps them.
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

if [[ "$MODE" != "--quick" ]]; then
  echo "=== Moxin Voice Preflight ==="
  echo "Resources: $APP_RESOURCES"
  echo "Dataflow: $DATAFLOW_PATH"
  echo "Conda root: $CONDA_ROOT"
  echo "Conda env: $CONDA_ENV_PREFIX"
  echo "MLX models: $MODEL_DIR"
  echo "ASR models: $ASR_MODEL_DIR"
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
