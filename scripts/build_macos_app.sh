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
resolve_mlx_prebuilt_path() {
  local root_dir="$1"
  local profile="$2"
  local build_dir="$root_dir/target/$profile/build"
  if [[ -d "$build_dir" ]]; then
    find "$build_dir" -type d -path '*mlx-sys-*/out/mlx-prebuilt' 2>/dev/null | tail -n 1 || true
  fi
}

run_cargo_build() {
  local root_dir="$1"
  local profile="$2"
  shift 2
  local mlx_prebuilt_path=""
  mlx_prebuilt_path="$(resolve_mlx_prebuilt_path "$root_dir" "$profile")"
  if [[ -n "$mlx_prebuilt_path" ]]; then
    MLX_PREBUILT_PATH="$mlx_prebuilt_path" cargo build "$@"
  else
    cargo build "$@"
  fi
}

MAKEPAD=apple_bundle MAKEPAD_PACKAGE_DIR=makepad run_cargo_build "$ROOT_DIR" "$PROFILE" -p moxin-voice-shell --profile "$PROFILE" --manifest-path "$ROOT_DIR/Cargo.toml"
# dora-primespeech-mlx build removed (Qwen3-only mode). See doc/REFACTOR_QWEN3_ONLY.md.
run_cargo_build "$ROOT_DIR" "$PROFILE" -p dora-qwen3-tts-mlx --profile "$PROFILE" --manifest-path "$ROOT_DIR/Cargo.toml"
run_cargo_build "$ROOT_DIR" "$PROFILE" -p dora-qwen3-asr --profile "$PROFILE" --manifest-path "$ROOT_DIR/Cargo.toml"
run_cargo_build "$ROOT_DIR" "$PROFILE" -p moxin-init --profile "$PROFILE" --manifest-path "$ROOT_DIR/Cargo.toml"

SHELL_BIN_PATH="$ROOT_DIR/target/$PROFILE/$BIN_NAME"
QWEN_TTS_BIN_PATH="$ROOT_DIR/target/$PROFILE/qwen-tts-node"
QWEN_ASR_BIN_PATH="$ROOT_DIR/target/$PROFILE/dora-qwen3-asr"
MOXIN_INIT_BIN_PATH="$ROOT_DIR/target/$PROFILE/moxin-init"
TRAINER_BIN_PATH="$ROOT_DIR/target/$PROFILE/moxin-fewshot-trainer"
MLX_METALLIB_PATH="$ROOT_DIR/target/$PROFILE/mlx.metallib"
DORA_BIN_PATH="$(command -v dora || true)"
if [[ ! -f "$SHELL_BIN_PATH" ]]; then
  echo "Binary not found: $SHELL_BIN_PATH"
  exit 1
fi
if [[ ! -f "$QWEN_TTS_BIN_PATH" ]]; then
  echo "Binary not found: $QWEN_TTS_BIN_PATH"
  exit 1
fi
if [[ ! -f "$QWEN_ASR_BIN_PATH" ]]; then
  echo "Binary not found: $QWEN_ASR_BIN_PATH"
  exit 1
fi
if [[ ! -f "$MOXIN_INIT_BIN_PATH" ]]; then
  echo "Binary not found: $MOXIN_INIT_BIN_PATH"
  exit 1
fi
if [[ -z "$DORA_BIN_PATH" || ! -x "$DORA_BIN_PATH" ]]; then
  echo "dora CLI not found in PATH. Install dora-cli before packaging."
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
cp "$QWEN_TTS_BIN_PATH" "$MACOS_DIR/qwen-tts-node"
cp "$QWEN_ASR_BIN_PATH" "$MACOS_DIR/dora-qwen3-asr"
cp "$MOXIN_INIT_BIN_PATH" "$MACOS_DIR/moxin-init"

# Bundle Qwen3-TTS voice preview WAV files (pre-generated, committed to repo)
QWEN_PREVIEWS_SRC="$ROOT_DIR/node-hub/dora-qwen3-tts-mlx/previews"
if [[ -d "$QWEN_PREVIEWS_SRC" ]]; then
  mkdir -p "$RES_DIR/qwen3-previews"
  cp "$QWEN_PREVIEWS_SRC"/*.wav "$RES_DIR/qwen3-previews/"
fi

# Bundle Qwen3-TTS bundled ICL voice reference audio and transcripts
QWEN_VOICES_SRC="$ROOT_DIR/node-hub/dora-qwen3-tts-mlx/voices"
if [[ -d "$QWEN_VOICES_SRC" ]]; then
  mkdir -p "$RES_DIR/qwen3-voices"
  cp -r "$QWEN_VOICES_SRC"/. "$RES_DIR/qwen3-voices/"
fi
cp "$DORA_BIN_PATH" "$MACOS_DIR/dora"
if [[ -f "$MLX_METALLIB_PATH" ]]; then
  cp "$MLX_METALLIB_PATH" "$MACOS_DIR/mlx.metallib"
fi
if [[ -f "$TRAINER_BIN_PATH" ]]; then
  cp "$TRAINER_BIN_PATH" "$MACOS_DIR/moxin-fewshot-trainer"
fi
chmod +x "$MACOS_DIR/${BIN_NAME}-bin" "$MACOS_DIR/qwen-tts-node" "$MACOS_DIR/dora-qwen3-asr" "$MACOS_DIR/moxin-init"
chmod +x "$MACOS_DIR/dora"
if [[ -f "$MACOS_DIR/moxin-fewshot-trainer" ]]; then
  chmod +x "$MACOS_DIR/moxin-fewshot-trainer"
fi

cp "$ROOT_DIR/scripts/macos_preflight.sh" "$SCRIPTS_DIR/macos_preflight.sh"
cp "$ROOT_DIR/scripts/macos_bootstrap.sh" "$SCRIPTS_DIR/macos_bootstrap.sh"
cp "$ROOT_DIR/scripts/macos_run_tts_backend.sh" "$SCRIPTS_DIR/macos_run_tts_backend.sh"
chmod +x "$SCRIPTS_DIR/macos_preflight.sh" "$SCRIPTS_DIR/macos_bootstrap.sh" "$SCRIPTS_DIR/macos_run_tts_backend.sh"

cp "$ROOT_DIR/scripts/dataflow/tts.bundle.yml" "$DATAFLOW_DIR/tts.yml"

# Bundle Makepad live resources for distributable app builds.
# With MAKEPAD_PACKAGE_DIR=makepad, runtime dependency paths resolve under:
#   Contents/Resources/makepad/<crate_name>/resources
MAKEPAD_RES_ROOT="$RES_DIR/makepad"
mkdir -p "$MAKEPAD_RES_ROOT"

mkdir -p "$MAKEPAD_RES_ROOT/moxin_widgets"
cp -R "$ROOT_DIR/moxin-widgets/resources" "$MAKEPAD_RES_ROOT/moxin_widgets/resources"

if [[ -d "$ROOT_DIR/moxin-ui/resources" ]]; then
  mkdir -p "$MAKEPAD_RES_ROOT/moxin_ui"
  cp -R "$ROOT_DIR/moxin-ui/resources" "$MAKEPAD_RES_ROOT/moxin_ui/resources"
fi

MAKEPAD_SRC_DIR="$(find "$HOME/.cargo/git/checkouts" -maxdepth 4 -type d -path "*/makepad-*/*/widgets" 2>/dev/null | head -n 1 | sed 's#/widgets$##')"
if [[ -n "$MAKEPAD_SRC_DIR" && -d "$MAKEPAD_SRC_DIR/widgets/resources" ]]; then
  mkdir -p "$MAKEPAD_RES_ROOT/makepad_widgets"
  cp -R "$MAKEPAD_SRC_DIR/widgets/resources" "$MAKEPAD_RES_ROOT/makepad_widgets/resources"

  if [[ -d "$MAKEPAD_SRC_DIR/widgets/fonts/emoji/resources" ]]; then
    mkdir -p "$MAKEPAD_RES_ROOT/makepad_fonts_emoji"
    cp -R "$MAKEPAD_SRC_DIR/widgets/fonts/emoji/resources" "$MAKEPAD_RES_ROOT/makepad_fonts_emoji/resources"
  fi
  if [[ -d "$MAKEPAD_SRC_DIR/widgets/fonts/chinese_regular/resources" ]]; then
    mkdir -p "$MAKEPAD_RES_ROOT/makepad_fonts_chinese_regular"
    cp -R "$MAKEPAD_SRC_DIR/widgets/fonts/chinese_regular/resources" "$MAKEPAD_RES_ROOT/makepad_fonts_chinese_regular/resources"
  fi
  if [[ -d "$MAKEPAD_SRC_DIR/widgets/fonts/chinese_regular_2/resources" ]]; then
    mkdir -p "$MAKEPAD_RES_ROOT/makepad_fonts_chinese_regular_2"
    cp -R "$MAKEPAD_SRC_DIR/widgets/fonts/chinese_regular_2/resources" "$MAKEPAD_RES_ROOT/makepad_fonts_chinese_regular_2/resources"
  fi
  if [[ -d "$MAKEPAD_SRC_DIR/widgets/fonts/chinese_bold/resources" ]]; then
    mkdir -p "$MAKEPAD_RES_ROOT/makepad_fonts_chinese_bold"
    cp -R "$MAKEPAD_SRC_DIR/widgets/fonts/chinese_bold/resources" "$MAKEPAD_RES_ROOT/makepad_fonts_chinese_bold/resources"
  fi
  if [[ -d "$MAKEPAD_SRC_DIR/widgets/fonts/chinese_bold_2/resources" ]]; then
    mkdir -p "$MAKEPAD_RES_ROOT/makepad_fonts_chinese_bold_2"
    cp -R "$MAKEPAD_SRC_DIR/widgets/fonts/chinese_bold_2/resources" "$MAKEPAD_RES_ROOT/makepad_fonts_chinese_bold_2/resources"
  fi
fi

cat > "$MACOS_DIR/$BIN_NAME" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail

APP_ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RES_DIR="$APP_ROOT/Resources"
MACOS_DIR="$APP_ROOT/MacOS"
LOG_DIR="$HOME/Library/Logs/MoxinVoice"
mkdir -p "$LOG_DIR"
DORA_LOG_PATH="$LOG_DIR/dora_up.log"
PREFLIGHT_LOG_PATH="$LOG_DIR/preflight.log"
DORA_RUNTIME_DIR="${MOXIN_DORA_RUNTIME_DIR:-$HOME/.dora/runtime}"
mkdir -p "$DORA_RUNTIME_DIR"
cd "$DORA_RUNTIME_DIR"
: > "$DORA_LOG_PATH"

export MOXIN_APP_RESOURCES="$RES_DIR"
export MOXIN_FEWSHOT_TRAINER_BIN="$MACOS_DIR/moxin-fewshot-trainer"
export MOXIN_DORA_RUNTIME_DIR="$DORA_RUNTIME_DIR"
export QWEN3_TTS_MODEL_ROOT="${QWEN3_TTS_MODEL_ROOT:-$HOME/.OminiX/models/qwen3-tts-mlx}"
export QWEN3_TTS_CUSTOMVOICE_MODEL_DIR="${QWEN3_TTS_CUSTOMVOICE_MODEL_DIR:-$QWEN3_TTS_MODEL_ROOT/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit}"
export QWEN3_TTS_BASE_MODEL_DIR="${QWEN3_TTS_BASE_MODEL_DIR:-$QWEN3_TTS_MODEL_ROOT/Qwen3-TTS-12Hz-1.7B-Base-8bit}"
export QWEN3_ASR_MODEL_PATH="${QWEN3_ASR_MODEL_PATH:-$HOME/.OminiX/models/qwen3-asr-1.7b}"

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
    path: "$MACOS_DIR/dora-qwen3-asr"
    inputs:
      audio: moxin-audio-input/audio
    outputs:
      - transcription
      - log
    env:
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

  - id: qwen-tts
    path: "$RES_DIR/scripts/macos_run_tts_backend.sh"
    inputs:
      text: moxin-prompt-input/control
    outputs:
      - audio
      - status
      - segment_complete
      - log
    env:
      VOICE_NAME: "vivian"
      LOG_LEVEL: INFO

  - id: moxin-audio-player
    path: dynamic
    inputs:
      audio: qwen-tts/audio
    outputs:
      - buffer_status
YAML
export MOXIN_DATAFLOW_PATH="$RUNTIME_DATAFLOW_PATH"
export PATH="$MACOS_DIR:$HOME/.cargo/bin:$PATH"

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
  (dora up > "$DORA_LOG_PATH" 2>&1 || true) &
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
    osascript -e "display dialog \"Failed to start Dora runtime. Check: $DORA_LOG_PATH\" buttons {\"OK\"} default button \"OK\" with title \"Moxin Voice Startup\""
    exit 1
  fi
fi

# Run full preflight in background for diagnostics only, do not block startup.
("$RES_DIR/scripts/macos_preflight.sh" > "$PREFLIGHT_LOG_PATH" 2>&1 || true) &

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
