#!/usr/bin/env bash
set -euo pipefail

APP_RESOURCES="${MOXIN_APP_RESOURCES:-}"
if [[ -z "$APP_RESOURCES" ]]; then
  APP_RESOURCES="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fi

PY_SRC="$APP_RESOURCES/python-src"
ENV_NAME="${MOXIN_CONDA_ENV:-moxin-studio}"
CONDA_ROOT="${MOXIN_CONDA_ROOT:-$HOME/.moxinvoice/conda}"
CONDA_BIN="${MOXIN_CONDA_BIN:-$CONDA_ROOT/bin/conda}"
CONDA_ENV_PREFIX="${MOXIN_CONDA_ENV_PREFIX:-$CONDA_ROOT/envs/$ENV_NAME}"
MODEL_DIR="${GPT_SOVITS_MODEL_DIR:-$HOME/.OminiX/models/gpt-sovits-mlx}"
PRIMESPEECH_DIR="${PRIMESPEECH_MODEL_DIR:-$HOME/.dora/models/primespeech}"
ASR_MODEL_DIR="${ASR_MODEL_DIR:-$HOME/.dora/models/asr/funasr}"
STATE_PATH="${MOXIN_BOOTSTRAP_STATE_PATH:-$HOME/Library/Logs/MoxinVoice/bootstrap_state.txt}"
TOTAL_STEPS=9

write_step() {
  local current="$1"
  local title="$2"
  local detail="$3"
  mkdir -p "$(dirname "$STATE_PATH")"
  printf '%s/%s|%s|%s\n' "$current" "$TOTAL_STEPS" "$title" "$detail" > "$STATE_PATH"
  echo "[BOOTSTEP $current/$TOTAL_STEPS] $title - $detail"
}

install_private_conda() {
  local arch installer_url tmp_installer
  arch="$(uname -m)"
  case "$arch" in
    arm64) installer_url="https://github.com/conda-forge/miniforge/releases/latest/download/Miniforge3-MacOSX-arm64.sh" ;;
    x86_64) installer_url="https://github.com/conda-forge/miniforge/releases/latest/download/Miniforge3-MacOSX-x86_64.sh" ;;
    *)
      echo "ERROR: Unsupported macOS arch for bundled bootstrap: $arch"
      exit 1
      ;;
  esac

  tmp_installer="$(mktemp -t moxinvoice-miniforge.XXXXXX).sh"
  echo "Conda not found, installing private Miniforge to $CONDA_ROOT ..."
  curl -fsSL "$installer_url" -o "$tmp_installer"
  bash "$tmp_installer" -b -p "$CONDA_ROOT"
  rm -f "$tmp_installer"
}

echo "=== Moxin Voice Bootstrap ==="
echo "Resources: $APP_RESOURCES"
echo "Python source bundle: $PY_SRC"
echo "Conda root: $CONDA_ROOT"
echo "Conda env: $CONDA_ENV_PREFIX"
echo "MLX model dir: $MODEL_DIR"
echo "PrimeSpeech model dir: $PRIMESPEECH_DIR"
echo "ASR model dir: $ASR_MODEL_DIR"
echo
write_step 1 "Prepare Runtime" "Checking or creating private conda runtime"

if [[ ! -x "$CONDA_BIN" ]] && command -v conda >/dev/null 2>&1; then
  CONDA_BIN="$(command -v conda)"
fi
if [[ ! -x "$CONDA_BIN" ]]; then
  install_private_conda
  CONDA_BIN="$CONDA_ROOT/bin/conda"
fi
if [[ ! -x "$CONDA_BIN" ]]; then
  echo "ERROR: failed to install or resolve conda binary."
  exit 1
fi

if [[ ! -x "$CONDA_ENV_PREFIX/bin/python" ]]; then
  echo "Creating conda env $CONDA_ENV_PREFIX ..."
  "$CONDA_BIN" create -p "$CONDA_ENV_PREFIX" python=3.12 -y
fi

write_step 2 "Install Git LFS" "Installing git and git-lfs in private runtime"
echo "Installing Git + Git LFS into app-private conda ..."
"$CONDA_BIN" install -p "$CONDA_ENV_PREFIX" -y -c conda-forge git git-lfs
"$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" git lfs install --skip-repo

write_step 3 "Install Base Python" "Upgrading pip/setuptools/wheel"
echo "Installing Python dependencies into $CONDA_ENV_PREFIX ..."
"$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" python -m pip install --upgrade pip setuptools wheel

write_step 4 "Install Dora Common" "Installing dora-common"
if [[ -d "$PY_SRC/libs/dora-common" ]]; then
  "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" python -m pip install -e "$PY_SRC/libs/dora-common"
fi

write_step 5 "Install ASR Node" "Installing dora-asr and ASR dependencies"
if [[ -d "$PY_SRC/node-hub/dora-asr" ]]; then
  "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" python -m pip install -e "$PY_SRC/node-hub/dora-asr"
fi

write_step 6 "Install PrimeSpeech Node" "Installing dora-primespeech and training dependencies"
if [[ -d "$PY_SRC/node-hub/dora-primespeech" ]]; then
  "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" python -m pip install -e "$PY_SRC/node-hub/dora-primespeech"
fi

"$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" python -m pip install "datasets<3.0.0" simplejson sortedcontainers tensorboard matplotlib

write_step 7 "Download Models" "Downloading ASR and PrimeSpeech model files"
if [[ -f "$PY_SRC/models/model-manager/download_models.py" ]]; then
  echo "Downloading required models (ASR + PrimeSpeech)..."
  "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" env PRIMESPEECH_MODEL_DIR="$PRIMESPEECH_DIR" python "$PY_SRC/models/model-manager/download_models.py" --download funasr
  "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" env PRIMESPEECH_MODEL_DIR="$PRIMESPEECH_DIR" python "$PY_SRC/models/model-manager/download_models.py" --download primespeech
fi

write_step 8 "Convert Models" "Converting PrimeSpeech models into MLX layout"
if [[ -f "$PY_SRC/scripts/convert_all_voices.py" ]]; then
  echo "Converting PrimeSpeech models to MLX layout..."
  "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" env \
    OMINIX_SCRIPTS="$PY_SRC/omx-scripts" \
    PRIMESPEECH_MOYOYO_SRC="$PRIMESPEECH_DIR/moyoyo" \
    GPT_SOVITS_MODEL_DIR="$MODEL_DIR" \
    python "$PY_SRC/scripts/convert_all_voices.py"
fi

if [[ -f "$PY_SRC/scripts/export_all_vits_onnx.py" ]]; then
  echo "Exporting optional VITS ONNX files..."
  "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" env \
    OMINIX_EXPORT_VITS_SCRIPT="$PY_SRC/omx-scripts/export_vits_onnx.py" \
    PRIMESPEECH_SOVITS_SRC="$PRIMESPEECH_DIR/moyoyo/SoVITS_weights" \
    GPT_SOVITS_VOICES_DIR="$MODEL_DIR/voices" \
    python "$PY_SRC/scripts/export_all_vits_onnx.py" || true
fi

if [[ -f "$PY_SRC/scripts/extract_all_prompt_semantic.py" ]]; then
  echo "Extracting optional prompt semantic caches..."
  "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" env \
    PRIMESPEECH_MOYOYO_SRC="$PRIMESPEECH_DIR/moyoyo" \
    GPT_SOVITS_VOICES_DIR="$MODEL_DIR/voices" \
    python "$PY_SRC/scripts/extract_all_prompt_semantic.py" || true
fi

write_step 9 "Finalize" "Runtime initialization complete"

cat <<'MSG'

Bootstrap completed.

Important:
1) Runtime Python/ASR dependencies were installed into app-private conda.
2) Models were downloaded/converted to local runtime model directories.
3) You can now relaunch the app and start TTS/ASR without manual conda setup.

MSG
