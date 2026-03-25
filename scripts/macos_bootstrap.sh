#!/usr/bin/env bash
set -euo pipefail

APP_RESOURCES="${MOXIN_APP_RESOURCES:-}"
if [[ -z "$APP_RESOURCES" ]]; then
  APP_RESOURCES="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
fi

# Bundle layout path (Contents/Resources/python-src)
PY_SRC="$APP_RESOURCES/python-src"

# Dev fallback (repository root)
REPO_ROOT="$APP_RESOURCES"
if [[ ! -d "$REPO_ROOT/models/model-manager" ]] && [[ -d "$APP_RESOURCES/../models/model-manager" ]]; then
  REPO_ROOT="$(cd "$APP_RESOURCES/.." && pwd)"
fi

ENV_NAME="${MOXIN_CONDA_ENV:-moxin-studio}"
CONDA_ROOT="${MOXIN_CONDA_ROOT:-$HOME/.moxinvoice/conda}"
CONDA_BIN="${MOXIN_CONDA_BIN:-$CONDA_ROOT/bin/conda}"
CONDA_ENV_PREFIX="${MOXIN_CONDA_ENV_PREFIX:-$CONDA_ROOT/envs/$ENV_NAME}"

# MODEL_DIR and PRIMESPEECH_DIR kept for reference; not used in Qwen3-only mode.
# See doc/REFACTOR_QWEN3_ONLY.md to restore PrimeSpeech steps.
MODEL_DIR="${GPT_SOVITS_MODEL_DIR:-$HOME/.OminiX/models/gpt-sovits-mlx}"
PRIMESPEECH_DIR="${PRIMESPEECH_MODEL_DIR:-$HOME/.dora/models/primespeech}"
ASR_MODEL_DIR="${ASR_MODEL_DIR:-$HOME/.dora/models/asr/funasr}"
QWEN_ASR_ROOT="${QWEN3_ASR_MODEL_ROOT:-$HOME/.OminiX/models}"
QWEN_ASR_DIR="${QWEN3_ASR_MODEL_PATH:-$QWEN_ASR_ROOT/qwen3-asr-1.7b}"
QWEN_ASR_REPO="${QWEN3_ASR_REPO:-mlx-community/Qwen3-ASR-1.7B-8bit}"

QWEN_ROOT="${QWEN3_TTS_MODEL_ROOT:-$HOME/.OminiX/models/qwen3-tts-mlx}"
QWEN_CUSTOM_DIR="${QWEN3_TTS_CUSTOMVOICE_MODEL_DIR:-$QWEN_ROOT/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit}"
QWEN_BASE_DIR="${QWEN3_TTS_BASE_MODEL_DIR:-$QWEN_ROOT/Qwen3-TTS-12Hz-1.7B-Base-8bit}"
QWEN_CUSTOM_REPO="${QWEN3_TTS_CUSTOMVOICE_REPO:-mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit}"
QWEN_BASE_REPO="${QWEN3_TTS_BASE_REPO:-mlx-community/Qwen3-TTS-12Hz-1.7B-Base-8bit}"

# Qwen3-only: always download both models.
INFERENCE_BACKEND="qwen3_tts_mlx"
ZERO_SHOT_BACKEND="qwen3_tts_mlx"

STATE_PATH="${MOXIN_BOOTSTRAP_STATE_PATH:-$HOME/Library/Logs/MoxinVoice/bootstrap_state.txt}"
TOTAL_STEPS=10

# Resolve source paths for bundle/dev
DORA_COMMON_SRC=""
DORA_ASR_SRC=""
DORA_PRIMESPEECH_SRC=""
MODEL_MANAGER_SCRIPT=""
CONVERT_SCRIPT=""
EXPORT_VITS_SCRIPT=""
EXTRACT_SEM_SCRIPT=""
QWEN_DOWNLOAD_SCRIPT=""
OMINIX_SCRIPTS_DIR=""
OMINIX_EXPORT_VITS_SCRIPT=""

if [[ -d "$PY_SRC" ]]; then
  DORA_COMMON_SRC="$PY_SRC/libs/dora-common"
  DORA_ASR_SRC="$PY_SRC/node-hub/dora-asr"
  DORA_PRIMESPEECH_SRC="$PY_SRC/node-hub/dora-primespeech"
  MODEL_MANAGER_SCRIPT="$PY_SRC/models/model-manager/download_models.py"
  CONVERT_SCRIPT="$PY_SRC/scripts/convert_all_voices.py"
  EXPORT_VITS_SCRIPT="$PY_SRC/scripts/export_all_vits_onnx.py"
  EXTRACT_SEM_SCRIPT="$PY_SRC/scripts/extract_all_prompt_semantic.py"
  QWEN_DOWNLOAD_SCRIPT="$PY_SRC/scripts/download_qwen3_tts_models.py"
  OMINIX_SCRIPTS_DIR="$PY_SRC/omx-scripts"
  OMINIX_EXPORT_VITS_SCRIPT="$PY_SRC/omx-scripts/export_vits_onnx.py"
else
  DORA_COMMON_SRC="$REPO_ROOT/libs/dora-common"
  DORA_ASR_SRC="$REPO_ROOT/node-hub/dora-asr"
  DORA_PRIMESPEECH_SRC="$REPO_ROOT/node-hub/dora-primespeech"
  MODEL_MANAGER_SCRIPT="$REPO_ROOT/models/model-manager/download_models.py"
  CONVERT_SCRIPT="$REPO_ROOT/scripts/convert_all_voices.py"
  EXPORT_VITS_SCRIPT="$REPO_ROOT/scripts/export_all_vits_onnx.py"
  EXTRACT_SEM_SCRIPT="$REPO_ROOT/scripts/extract_all_prompt_semantic.py"
  QWEN_DOWNLOAD_SCRIPT="$REPO_ROOT/scripts/download_qwen3_tts_models.py"
  OMINIX_SCRIPTS_DIR="$REPO_ROOT/node-hub/dora-primespeech-mlx/patches/gpt-sovits-mlx/scripts"
  OMINIX_EXPORT_VITS_SCRIPT="$OMINIX_SCRIPTS_DIR/export_vits_onnx.py"
fi

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

# Qwen3-only: always download both CustomVoice and Base models.
NEED_QWEN_CUSTOM=1
NEED_QWEN_BASE=1

echo "=== Moxin Voice Bootstrap ==="
echo "Resources: $APP_RESOURCES"
echo "Python source bundle: $PY_SRC"
echo "Repo root fallback: $REPO_ROOT"
echo "Conda root: $CONDA_ROOT"
echo "Conda env: $CONDA_ENV_PREFIX"
echo "MLX model dir: $MODEL_DIR"
echo "PrimeSpeech model dir: $PRIMESPEECH_DIR"
echo "ASR model dir: $ASR_MODEL_DIR"
echo "Inference backend: $INFERENCE_BACKEND"
echo "Zero-shot backend: $ZERO_SHOT_BACKEND"
echo "Qwen model root: $QWEN_ROOT"
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


write_step 2 "Install Git LFS" "Checking or installing git-lfs in private runtime"
if "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" git lfs version >/dev/null 2>&1; then
  echo "git-lfs already present, skipping conda install."
else
  echo "Installing git-lfs into app-private conda ..."
  "$CONDA_BIN" install -p "$CONDA_ENV_PREFIX" -y -c conda-forge git git-lfs
  "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" git lfs install --skip-repo
fi

write_step 3 "Install Base Python" "Upgrading pip/setuptools/wheel"
"$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" python -m pip install --upgrade pip setuptools wheel

write_step 4 "Install Dora Common" "Installing dora-common"
if [[ -d "$DORA_COMMON_SRC" ]]; then
  "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" python -m pip install -e "$DORA_COMMON_SRC"
fi

write_step 5 "Skip Python ASR Node" "dora-asr replaced by Rust dora-qwen3-asr — no Python install needed"

# Steps 6-8: PrimeSpeech-specific — skipped in Qwen3-only mode.
# See doc/REFACTOR_QWEN3_ONLY.md to restore:
#   Step 6: Install dora-primespeech python node
#   Step 7: Download funasr + primespeech model files
#   Step 8: Convert/export PrimeSpeech models to MLX layout
write_step 6 "Skip PrimeSpeech Node" "PrimeSpeech removed — Qwen3-only mode"
write_step 7 "Optional Qwen3 ASR Model" "Downloading qwen3-asr-mlx model files (optional)"
if [[ ! -f "$QWEN_ASR_DIR/config.json" ]]; then
  echo "Qwen3-ASR model missing; downloading to $QWEN_ASR_DIR ..."
  mkdir -p "$QWEN_ASR_DIR"
  # Install hf_transfer for faster downloads (best-effort; fall back gracefully if unavailable)
  "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" python -m pip install -q hf_transfer || true
  "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" env \
    HF_HUB_ENABLE_HF_TRANSFER="${HF_HUB_ENABLE_HF_TRANSFER:-1}" \
    huggingface-cli download "$QWEN_ASR_REPO" --local-dir "$QWEN_ASR_DIR" || true
  if [[ ! -f "$QWEN_ASR_DIR/config.json" ]]; then
    echo "WARN: Qwen3-ASR model download failed; continuing without ASR auto-transcription."
  fi
else
  echo "Qwen3-ASR model already present at $QWEN_ASR_DIR"
fi
write_step 8 "Skip Model Conversion" "PrimeSpeech model conversion skipped"

write_step 9 "Download Qwen3 Models" "Preparing qwen3-tts-mlx model files if required"
if [[ "$NEED_QWEN_CUSTOM" == "1" || "$NEED_QWEN_BASE" == "1" ]]; then
  if [[ ! -f "$QWEN_DOWNLOAD_SCRIPT" ]]; then
    echo "ERROR: missing qwen download script: $QWEN_DOWNLOAD_SCRIPT"
    exit 1
  fi

  if [[ "$NEED_QWEN_CUSTOM" == "1" ]] && ! qwen_model_ready "$QWEN_CUSTOM_DIR"; then
    echo "Qwen CustomVoice model missing; downloading..."
  fi
  if [[ "$NEED_QWEN_BASE" == "1" ]] && ! qwen_model_ready "$QWEN_BASE_DIR"; then
    echo "Qwen Base model missing; downloading..."
  fi

  # Install hf_transfer before download — required when HF_HUB_ENABLE_HF_TRANSFER=1.
  # Without this, huggingface_hub raises ImportError and the download fails,
  # causing bootstrap to exit at step 9 and the app to re-trigger bootstrap every launch.
  "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" python -m pip install -q hf_transfer || true

  "$CONDA_BIN" run -p "$CONDA_ENV_PREFIX" env \
    HF_HUB_ENABLE_HF_TRANSFER="${HF_HUB_ENABLE_HF_TRANSFER:-1}" \
    python "$QWEN_DOWNLOAD_SCRIPT" \
      --root "$QWEN_ROOT" \
      --custom-repo "$QWEN_CUSTOM_REPO" \
      --base-repo "$QWEN_BASE_REPO" \
      $([[ "$NEED_QWEN_CUSTOM" == "1" ]] && echo "--need-custom") \
      $([[ "$NEED_QWEN_BASE" == "1" ]] && echo "--need-base")
fi

write_step 10 "Finalize" "Runtime initialization complete"

cat <<'MSG'

Bootstrap completed.

Important:
1) Runtime Python dependencies were installed into app-private conda (TTS only).
2) Qwen3-ASR model downloaded (Rust native — no Python ASR dependency).
3) Qwen3-TTS CustomVoice and Base model snapshots downloaded.
4) If ASR model is unavailable, voice cloning still works with manual reference text input.
5) You can relaunch the app and start TTS without manual setup.

MSG
