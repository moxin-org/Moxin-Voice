#!/usr/bin/env bash
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "This script only supports macOS."
  exit 1
fi

ROOT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
APP_NAME="Moxin Voice"
APP_PATH="$ROOT_DIR/dist/${APP_NAME}.app"
OUT_DIR="$ROOT_DIR/dist"
VOL_NAME="$APP_NAME Installer"
DMG_NAME="${APP_NAME}.dmg"

usage() {
  cat <<EOF
Usage:
  $(basename "$0") [options]

Options:
  --app-path <path>   Path to .app bundle (default: $APP_PATH)
  --out-dir <dir>     Output directory (default: $OUT_DIR)
  --vol-name <name>   DMG volume name (default: $VOL_NAME)
  --dmg-name <name>   DMG file name (default: $DMG_NAME)
  -h, --help          Show this help
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app-path)
      APP_PATH="$2"
      shift 2
      ;;
    --out-dir)
      OUT_DIR="$2"
      shift 2
      ;;
    --vol-name)
      VOL_NAME="$2"
      shift 2
      ;;
    --dmg-name)
      DMG_NAME="$2"
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

if [[ ! -d "$APP_PATH" ]]; then
  echo "App bundle not found: $APP_PATH"
  echo "Build it first with scripts/build_macos_app.sh"
  exit 1
fi

mkdir -p "$OUT_DIR"
DMG_PATH="$OUT_DIR/$DMG_NAME"
STAGING_DIR="$(mktemp -d)"

cp -R "$APP_PATH" "$STAGING_DIR/"
ln -s /Applications "$STAGING_DIR/Applications"

rm -f "$DMG_PATH"
hdiutil create \
  -volname "$VOL_NAME" \
  -srcfolder "$STAGING_DIR" \
  -ov \
  -format UDZO \
  "$DMG_PATH"

rm -rf "$STAGING_DIR"

echo "DMG created:"
echo "  $DMG_PATH"
