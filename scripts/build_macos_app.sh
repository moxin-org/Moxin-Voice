#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script only supports macOS."
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

APP_NAME="Moxin Voice"
BUNDLE_ID="com.moxin.voice"
BIN_NAME="moxin-voice-shell"
PROFILE="release"
ICON_PATH=""
OUT_DIR="$ROOT_DIR/dist"
INCLUDE_PY_SRC="true"
VERSION="1.0"

usage() {
  cat <<EOF
Usage:
  $(basename "$0") [options]

Options:
  --app-name <name>      App name shown in Dock/Finder (default: "$APP_NAME")
  --bundle-id <id>       CFBundleIdentifier (default: "$BUNDLE_ID")
  --icon <path>          .icns or .png icon path (optional)
  --profile <profile>    Cargo profile, e.g. release/dev (default: "$PROFILE")
  --out-dir <dir>        Output directory for .app (default: "$OUT_DIR")
  --include-python-src   Copy python node sources into app bundle (default: true)
  --no-python-src        Do not copy python node sources
  --version <version>    CFBundleShortVersionString (default: "$VERSION")
  -h, --help             Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app-name)
      APP_NAME="$2"
      shift 2
      ;;
    --bundle-id)
      BUNDLE_ID="$2"
      shift 2
      ;;
    --icon)
      ICON_PATH="$2"
      shift 2
      ;;
    --profile)
      PROFILE="$2"
      shift 2
      ;;
    --out-dir)
      OUT_DIR="$2"
      shift 2
      ;;
    --include-python-src)
      INCLUDE_PY_SRC="true"
      shift 1
      ;;
    --no-python-src)
      INCLUDE_PY_SRC="false"
      shift 1
      ;;
    --version)
      VERSION="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1"
      usage
      exit 1
      ;;
  esac
done

echo "Building binaries..."
cargo build -p moxin-voice-shell --profile "$PROFILE" --manifest-path "$ROOT_DIR/Cargo.toml"
cargo build -p moxin-tts-node --profile "$PROFILE" --manifest-path "$ROOT_DIR/Cargo.toml"

SHELL_BIN_PATH="$ROOT_DIR/target/$PROFILE/$BIN_NAME"
TTS_BIN_PATH="$ROOT_DIR/target/$PROFILE/moxin-tts-node"
TRAINER_BIN_PATH="$ROOT_DIR/target/$PROFILE/moxin-fewshot-trainer"
MLX_METALLIB_PATH="$ROOT_DIR/target/$PROFILE/mlx.metallib"
if [[ ! -f "$SHELL_BIN_PATH" ]]; then
  echo "Binary not found: $SHELL_BIN_PATH"
  exit 1
fi
if [[ ! -f "$TTS_BIN_PATH" ]]; then
  echo "Binary not found: $TTS_BIN_PATH"
  exit 1
fi

APP_DIR="$OUT_DIR/$APP_NAME.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RES_DIR="$CONTENTS_DIR/Resources"
SCRIPTS_DIR="$RES_DIR/scripts"
DATAFLOW_DIR="$RES_DIR/dataflow"

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RES_DIR" "$SCRIPTS_DIR" "$DATAFLOW_DIR"

cp "$SHELL_BIN_PATH" "$MACOS_DIR/${BIN_NAME}-bin"
cp "$TTS_BIN_PATH" "$MACOS_DIR/moxin-tts-node"
if [[ -f "$MLX_METALLIB_PATH" ]]; then
  cp "$MLX_METALLIB_PATH" "$MACOS_DIR/mlx.metallib"
fi
if [[ -f "$TRAINER_BIN_PATH" ]]; then
  cp "$TRAINER_BIN_PATH" "$MACOS_DIR/moxin-fewshot-trainer"
fi
chmod +x "$MACOS_DIR/${BIN_NAME}-bin" "$MACOS_DIR/moxin-tts-node"
if [[ -f "$MACOS_DIR/moxin-fewshot-trainer" ]]; then
  chmod +x "$MACOS_DIR/moxin-fewshot-trainer"
fi

cp "$ROOT_DIR/scripts/macos_preflight.sh" "$SCRIPTS_DIR/macos_preflight.sh"
cp "$ROOT_DIR/scripts/macos_bootstrap.sh" "$SCRIPTS_DIR/macos_bootstrap.sh"
cp "$ROOT_DIR/scripts/macos_run_dora_asr.sh" "$SCRIPTS_DIR/macos_run_dora_asr.sh"
chmod +x "$SCRIPTS_DIR/macos_preflight.sh" "$SCRIPTS_DIR/macos_bootstrap.sh" "$SCRIPTS_DIR/macos_run_dora_asr.sh"

cp "$ROOT_DIR/scripts/dataflow/tts.bundle.yml" "$DATAFLOW_DIR/tts.yml"

if [[ "$INCLUDE_PY_SRC" == "true" ]]; then
  PY_DST="$RES_DIR/python-src"
  mkdir -p "$PY_DST/libs" "$PY_DST/node-hub" "$PY_DST/models" "$PY_DST/scripts" "$PY_DST/omx-scripts"
  cp -R "$ROOT_DIR/libs/dora-common" "$PY_DST/libs/dora-common"
  cp -R "$ROOT_DIR/node-hub/dora-asr" "$PY_DST/node-hub/dora-asr"
  cp -R "$ROOT_DIR/node-hub/dora-primespeech" "$PY_DST/node-hub/dora-primespeech"
  cp -R "$ROOT_DIR/models/model-manager" "$PY_DST/models/model-manager"
  cp "$ROOT_DIR/scripts/convert_all_voices.py" "$PY_DST/scripts/convert_all_voices.py"
  cp "$ROOT_DIR/scripts/export_all_vits_onnx.py" "$PY_DST/scripts/export_all_vits_onnx.py"
  cp "$ROOT_DIR/scripts/extract_all_prompt_semantic.py" "$PY_DST/scripts/extract_all_prompt_semantic.py"
  cp -R "$ROOT_DIR/node-hub/moxin-tts-node/patches/gpt-sovits-mlx/scripts/." "$PY_DST/omx-scripts/"
fi

cat > "$MACOS_DIR/$BIN_NAME" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

APP_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RES_DIR="$APP_ROOT/Resources"
MACOS_DIR="$APP_ROOT/MacOS"
PRE="$RES_DIR/scripts/macos_preflight.sh"
BOOT="$RES_DIR/scripts/macos_bootstrap.sh"
LOG_DIR="$HOME/Library/Logs/MoxinVoice"
mkdir -p "$LOG_DIR"
DORA_RUNTIME_DIR="${MOXIN_DORA_RUNTIME_DIR:-$HOME/.dora/runtime}"
mkdir -p "$DORA_RUNTIME_DIR"
cd "$DORA_RUNTIME_DIR"
 : > "$LOG_DIR/dora_up.log"

export MOXIN_APP_RESOURCES="$RES_DIR"
export MOXIN_FEWSHOT_TRAINER_BIN="$MACOS_DIR/moxin-fewshot-trainer"
export MOXIN_DORA_RUNTIME_DIR="$DORA_RUNTIME_DIR"

# Generate a writable runtime dataflow with absolute node paths.
RUNTIME_DATAFLOW_DIR="$DORA_RUNTIME_DIR/dataflow"
RUNTIME_DATAFLOW_PATH="$RUNTIME_DATAFLOW_DIR/tts.yml"
mkdir -p "$RUNTIME_DATAFLOW_DIR"
cat > "$RUNTIME_DATAFLOW_PATH" <<YAML
nodes:
  - id: moxin-audio-input
    path: dynamic
    outputs:
      - audio

  - id: asr
    path: "$RES_DIR/scripts/macos_run_dora_asr.sh"
    inputs:
      audio: moxin-audio-input/audio
    outputs:
      - transcription
      - status
      - log
    env:
      USE_GPU: "false"
      ASR_ENGINE: "funasr"
      LANGUAGE: "zh"
      LOG_LEVEL: "INFO"

  - id: moxin-asr-listener
    path: dynamic
    inputs:
      transcription: asr/transcription

  - id: moxin-prompt-input
    path: dynamic
    outputs:
      - control

  - id: primespeech-tts
    path: "$MACOS_DIR/moxin-tts-node"
    inputs:
      text: moxin-prompt-input/control
    outputs:
      - audio
      - status
      - segment_complete
      - log
    env:
      VOICE_NAME: "Doubao"
      LOG_LEVEL: INFO

  - id: moxin-audio-player
    path: dynamic
    inputs:
      audio: primespeech-tts/audio
    outputs:
      - buffer_status
YAML
export MOXIN_DATAFLOW_PATH="$RUNTIME_DATAFLOW_PATH"
if [[ -d "$HOME/miniconda3/envs/moxin-studio" || -d "$HOME/anaconda3/envs/moxin-studio" ]]; then
  export MOXIN_CONDA_ENV="moxin-studio"
elif [[ -d "$HOME/miniconda3/envs/mofa-studio" || -d "$HOME/anaconda3/envs/mofa-studio" ]]; then
  export MOXIN_CONDA_ENV="mofa-studio"
else
  export MOXIN_CONDA_ENV="moxin-studio"
fi
export PATH="$HOME/miniconda3/envs/$MOXIN_CONDA_ENV/bin:$HOME/anaconda3/envs/$MOXIN_CONDA_ENV/bin:$HOME/.cargo/bin:$HOME/miniconda3/bin:$HOME/anaconda3/bin:$PATH"

# Ensure Dora is available with bounded wait.
# 1) If already healthy, do nothing.
# 2) Otherwise try a clean reset and bring-up.
# 3) Verify readiness; fail fast with a clear message if still unavailable.
if ! dora system status >/dev/null 2>&1; then
  # Reset first; run in background and don't let a stuck destroy block launch forever.
  (dora destroy >/dev/null 2>&1 || true) &
  DESTROY_PID=$!
  sleep 2
  kill "$DESTROY_PID" >/dev/null 2>&1 || true
fi

if ! dora system status >/dev/null 2>&1; then
  (dora up > "$LOG_DIR/dora_up.log" 2>&1 || true) &
  UP_PID=$!
  READY=0
  for _ in {1..20}; do
    if dora system status >/dev/null 2>&1; then
      READY=1
      break
    fi
    sleep 0.5
  done
  if [[ "$READY" != "1" ]]; then
    kill "$UP_PID" >/dev/null 2>&1 || true
    osascript -e 'display dialog "Failed to start Dora runtime. Check ~/Library/Logs/MoxinVoice/dora_up.log" buttons {"OK"} default button "OK" with title "Moxin Voice Startup"'
    exit 1
  fi
fi

if ! "$PRE" --quick >/dev/null 2>&1; then
  prompt='display dialog "Moxin Voice needs to initialize runtime dependencies before first launch." buttons {"Quit","Initialize"} default button "Initialize" with title "Moxin Voice Setup"'
  choice=$(osascript -e "$prompt" || true)
  if [[ "$choice" == *"Initialize"* ]]; then
    if ! "$BOOT" > "$LOG_DIR/bootstrap.log" 2>&1; then
      osascript -e 'display dialog "Initialization failed. Check ~/Library/Logs/MoxinVoice/bootstrap.log" buttons {"OK"} default button "OK" with title "Moxin Voice Setup"'
      exit 1
    fi
  else
    exit 0
  fi
fi

# Run full preflight in background for diagnostics only, do not block startup.
("$PRE" > "$LOG_DIR/preflight.log" 2>&1 || true) &

exec "$MACOS_DIR/moxin-voice-shell-bin"
EOF
chmod +x "$MACOS_DIR/$BIN_NAME"

ICON_FILE_NAME=""
if [[ -n "$ICON_PATH" ]]; then
  if [[ ! -f "$ICON_PATH" ]]; then
    echo "Icon file not found: $ICON_PATH"
    exit 1
  fi

  ext="${ICON_PATH##*.}"
  ext_lower="$(echo "$ext" | tr '[:upper:]' '[:lower:]')"

  if [[ "$ext_lower" == "icns" ]]; then
    cp "$ICON_PATH" "$RES_DIR/AppIcon.icns"
    ICON_FILE_NAME="AppIcon"
  elif [[ "$ext_lower" == "png" ]]; then
    cp "$ICON_PATH" "$RES_DIR/AppIcon.png"
    ICON_FILE_NAME="AppIcon.png"
  else
    echo "Unsupported icon format: $ICON_PATH (use .icns or .png)"
    exit 1
  fi
fi

PLIST_PATH="$CONTENTS_DIR/Info.plist"
cat > "$PLIST_PATH" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleDisplayName</key>
  <string>${APP_NAME}</string>
  <key>CFBundleIdentifier</key>
  <string>${BUNDLE_ID}</string>
  <key>CFBundleExecutable</key>
  <string>${BIN_NAME}</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleVersion</key>
  <string>${VERSION}</string>
  <key>CFBundleShortVersionString</key>
  <string>${VERSION}</string>
  <key>LSMinimumSystemVersion</key>
  <string>13.0</string>
  <key>NSHighResolutionCapable</key>
  <true/>
  <key>NSMicrophoneUsageDescription</key>
  <string>This app needs microphone access for voice cloning and recording.</string>
EOF

if [[ -n "$ICON_FILE_NAME" ]]; then
  cat >> "$PLIST_PATH" <<EOF
  <key>CFBundleIconFile</key>
  <string>${ICON_FILE_NAME}</string>
EOF
fi

cat >> "$PLIST_PATH" <<EOF
</dict>
</plist>
EOF

echo "App bundle created:"
echo "  $APP_DIR"
echo
echo "Run with:"
echo "  open \"$APP_DIR\""
echo
echo "If Dock icon/name is cached, run:"
echo "  killall Dock"
