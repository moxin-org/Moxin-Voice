#!/usr/bin/env bash
set -euo pipefail

APP_RESOURCES="${MOXIN_APP_RESOURCES:-}"
if [[ -z "$APP_RESOURCES" ]]; then
  APP_RESOURCES="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fi

PY_SRC="$APP_RESOURCES/python-src"
ENV_NAME="${MOXIN_CONDA_ENV:-moxin-studio}"
MODEL_DIR="${GPT_SOVITS_MODEL_DIR:-$HOME/.OminiX/models/gpt-sovits-mlx}"
PRIMESPEECH_DIR="${PRIMESPEECH_MODEL_DIR:-$HOME/.dora/models/primespeech}"
ASR_MODEL_DIR="${ASR_MODEL_DIR:-$HOME/.dora/models/asr/funasr}"

echo "=== Moxin Voice Bootstrap ==="
echo "Resources: $APP_RESOURCES"
echo "Python source bundle: $PY_SRC"
echo "Conda env: $ENV_NAME"
echo "MLX model dir: $MODEL_DIR"
echo "PrimeSpeech model dir: $PRIMESPEECH_DIR"
echo

if ! command -v conda >/dev/null 2>&1; then
  echo "ERROR: conda not found."
  echo "Install Miniconda first:"
  echo "https://docs.conda.io/en/latest/miniconda.html"
  exit 1
fi

if ! conda env list | awk '{print $1}' | grep -qx "$ENV_NAME"; then
  echo "Creating conda env $ENV_NAME..."
  conda create -n "$ENV_NAME" python=3.12 -y
fi

echo "Installing Python dependencies into $ENV_NAME..."
conda run -n "$ENV_NAME" python -m pip install --upgrade pip setuptools wheel

if [[ -d "$PY_SRC/libs/dora-common" ]]; then
  conda run -n "$ENV_NAME" python -m pip install -e "$PY_SRC/libs/dora-common"
fi
if [[ -d "$PY_SRC/node-hub/dora-asr" ]]; then
  conda run -n "$ENV_NAME" python -m pip install -e "$PY_SRC/node-hub/dora-asr"
fi
if [[ -d "$PY_SRC/node-hub/dora-primespeech" ]]; then
  conda run -n "$ENV_NAME" python -m pip install -e "$PY_SRC/node-hub/dora-primespeech"
fi

conda run -n "$ENV_NAME" python -m pip install "datasets<3.0.0" simplejson sortedcontainers tensorboard matplotlib

if ! command -v dora >/dev/null 2>&1; then
  if command -v cargo >/dev/null 2>&1; then
    echo "Installing dora-cli (0.3.12) via cargo..."
    cargo install dora-cli --version 0.3.12 --locked
  else
    echo "ERROR: dora not found and cargo is not available."
    echo "Install Rust/cargo, then run: cargo install dora-cli --version 0.3.12 --locked"
    exit 1
  fi
fi

if [[ -f "$PY_SRC/models/model-manager/download_models.py" ]]; then
  echo "Downloading required models (ASR + PrimeSpeech)..."
  conda run -n "$ENV_NAME" env PRIMESPEECH_MODEL_DIR="$PRIMESPEECH_DIR" python "$PY_SRC/models/model-manager/download_models.py" --download funasr
  conda run -n "$ENV_NAME" env PRIMESPEECH_MODEL_DIR="$PRIMESPEECH_DIR" python "$PY_SRC/models/model-manager/download_models.py" --download primespeech
fi

if [[ -f "$PY_SRC/scripts/convert_all_voices.py" ]]; then
  echo "Converting PrimeSpeech models to MLX layout..."
  conda run -n "$ENV_NAME" env \
    OMINIX_SCRIPTS="$PY_SRC/omx-scripts" \
    PRIMESPEECH_MOYOYO_SRC="$PRIMESPEECH_DIR/moyoyo" \
    GPT_SOVITS_MODEL_DIR="$MODEL_DIR" \
    python "$PY_SRC/scripts/convert_all_voices.py"
fi

if [[ -f "$PY_SRC/scripts/export_all_vits_onnx.py" ]]; then
  echo "Exporting optional VITS ONNX files..."
  conda run -n "$ENV_NAME" env \
    OMINIX_EXPORT_VITS_SCRIPT="$PY_SRC/omx-scripts/export_vits_onnx.py" \
    PRIMESPEECH_SOVITS_SRC="$PRIMESPEECH_DIR/moyoyo/SoVITS_weights" \
    GPT_SOVITS_VOICES_DIR="$MODEL_DIR/voices" \
    python "$PY_SRC/scripts/export_all_vits_onnx.py" || true
fi

if [[ -f "$PY_SRC/scripts/extract_all_prompt_semantic.py" ]]; then
  echo "Extracting optional prompt semantic caches..."
  conda run -n "$ENV_NAME" env \
    PRIMESPEECH_MOYOYO_SRC="$PRIMESPEECH_DIR/moyoyo" \
    GPT_SOVITS_VOICES_DIR="$MODEL_DIR/voices" \
    python "$PY_SRC/scripts/extract_all_prompt_semantic.py" || true
fi

cat <<'EOF'

Bootstrap completed.

Important:
1) This bootstrap installs runtime dependencies only.
2) You still need required models on disk:
   - ~/.OminiX/models/gpt-sovits-mlx
   - ~/.dora/models/asr/funasr
3) If MLX models are missing, follow project migration checklist to initialize them.

EOF
