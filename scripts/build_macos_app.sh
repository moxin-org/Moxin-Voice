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

echo "Building binary..."
cargo build -p moxin-voice-shell --profile "$PROFILE" --manifest-path "$ROOT_DIR/Cargo.toml"

BIN_PATH="$ROOT_DIR/target/$PROFILE/$BIN_NAME"
if [[ ! -f "$BIN_PATH" ]]; then
  echo "Binary not found: $BIN_PATH"
  exit 1
fi

APP_DIR="$OUT_DIR/$APP_NAME.app"
CONTENTS_DIR="$APP_DIR/Contents"
MACOS_DIR="$CONTENTS_DIR/MacOS"
RES_DIR="$CONTENTS_DIR/Resources"

rm -rf "$APP_DIR"
mkdir -p "$MACOS_DIR" "$RES_DIR"

cp "$BIN_PATH" "$MACOS_DIR/$BIN_NAME"
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
    TMP_ICONSET="$(mktemp -d)/AppIcon.iconset"
    mkdir -p "$TMP_ICONSET"

    sips -z 16 16     "$ICON_PATH" --out "$TMP_ICONSET/icon_16x16.png" >/dev/null
    sips -z 32 32     "$ICON_PATH" --out "$TMP_ICONSET/icon_16x16@2x.png" >/dev/null
    sips -z 32 32     "$ICON_PATH" --out "$TMP_ICONSET/icon_32x32.png" >/dev/null
    sips -z 64 64     "$ICON_PATH" --out "$TMP_ICONSET/icon_32x32@2x.png" >/dev/null
    sips -z 128 128   "$ICON_PATH" --out "$TMP_ICONSET/icon_128x128.png" >/dev/null
    sips -z 256 256   "$ICON_PATH" --out "$TMP_ICONSET/icon_128x128@2x.png" >/dev/null
    sips -z 256 256   "$ICON_PATH" --out "$TMP_ICONSET/icon_256x256.png" >/dev/null
    sips -z 512 512   "$ICON_PATH" --out "$TMP_ICONSET/icon_256x256@2x.png" >/dev/null
    sips -z 512 512   "$ICON_PATH" --out "$TMP_ICONSET/icon_512x512.png" >/dev/null
    sips -z 1024 1024 "$ICON_PATH" --out "$TMP_ICONSET/icon_512x512@2x.png" >/dev/null

    iconutil -c icns "$TMP_ICONSET" -o "$RES_DIR/AppIcon.icns"
    ICON_FILE_NAME="AppIcon"
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
  <string>1</string>
  <key>CFBundleShortVersionString</key>
  <string>1.0</string>
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
