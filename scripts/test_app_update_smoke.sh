#!/usr/bin/env bash
set -euo pipefail

APP_PATH=""
DMG_PATH=""
VERSION=""
MODE="fake-install"

usage() {
  cat <<EOF
Usage:
  $(basename "$0") --app </path/to/Moxin Voice.app> --dmg </path/to/update.dmg> --version <x.y.z> [--mode fake-install|real-install]

Examples:
  $(basename "$0") --app "/Applications/Moxin Voice.app" --dmg "./dist/Moxin-Voice-v0.0.5.dmg" --version 0.0.5
  $(basename "$0") --app "./dist/Moxin Voice.app" --dmg "./dist/Moxin-Voice-v0.0.5.dmg" --version 0.0.5 --mode real-install
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --app)
      APP_PATH="$2"
      shift 2
      ;;
    --dmg)
      DMG_PATH="$2"
      shift 2
      ;;
    --version)
      VERSION="$2"
      shift 2
      ;;
    --mode)
      MODE="$2"
      shift 2
      ;;
    -h|--help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown option: $1" >&2
      usage >&2
      exit 1
      ;;
  esac
done

if [[ -z "$APP_PATH" || -z "$DMG_PATH" || -z "$VERSION" ]]; then
  usage >&2
  exit 1
fi

if [[ ! -d "$APP_PATH" ]]; then
  echo "App bundle not found: $APP_PATH" >&2
  exit 1
fi

if [[ ! -f "$DMG_PATH" ]]; then
  echo "DMG not found: $DMG_PATH" >&2
  exit 1
fi

APP_PATH="$(cd "$(dirname "$APP_PATH")" && pwd)/$(basename "$APP_PATH")"
DMG_PATH="$(cd "$(dirname "$DMG_PATH")" && pwd)/$(basename "$DMG_PATH")"
APP_LAUNCHER="$APP_PATH/Contents/MacOS/moxin-voice-shell"

if [[ ! -x "$APP_LAUNCHER" ]]; then
  echo "App launcher not found or not executable: $APP_LAUNCHER" >&2
  exit 1
fi

TEST_ROOT="$(mktemp -d /tmp/moxin-update-smoke.XXXXXX)"
CACHE_DIR="$TEST_ROOT/cache"
RELEASE_JSON="$TEST_ROOT/latest.json"
INSTALL_LOG="$TEST_ROOT/install-invocation.txt"
INSTALL_SCRIPT="$TEST_ROOT/fake-installer.sh"
APP_STDOUT="$TEST_ROOT/app.stdout.log"
APP_STDERR="$TEST_ROOT/app.stderr.log"

mkdir -p "$CACHE_DIR"

cat > "$RELEASE_JSON" <<EOF
{"tag_name":"v$VERSION","assets":[{"name":"Moxin-Voice-v$VERSION.dmg","browser_download_url":"file://$DMG_PATH"}]}
EOF

cat > "$INSTALL_SCRIPT" <<EOF
#!/usr/bin/env bash
set -euo pipefail
printf '%s\n' "\$@" > "$INSTALL_LOG"
exit 0
EOF
chmod +x "$INSTALL_SCRIPT"

UPDATE_SCRIPT="$INSTALL_SCRIPT"
if [[ "$MODE" == "real-install" ]]; then
  UPDATE_SCRIPT=""
elif [[ "$MODE" != "fake-install" ]]; then
  echo "Unsupported mode: $MODE" >&2
  exit 1
fi

echo "Smoke test workspace: $TEST_ROOT"
echo "Release JSON: $RELEASE_JSON"
echo "Cache dir: $CACHE_DIR"
if [[ -n "$UPDATE_SCRIPT" ]]; then
  echo "Installer mode: fake"
  echo "Installer invocation log: $INSTALL_LOG"
else
  echo "Installer mode: real"
fi
echo "Launching app..."

if [[ -n "$UPDATE_SCRIPT" ]]; then
  env \
    MOXIN_UPDATE_RELEASE_API="file://$RELEASE_JSON" \
    MOXIN_UPDATE_CACHE_DIR="$CACHE_DIR" \
    MOXIN_UPDATE_INSTALL_SCRIPT="$UPDATE_SCRIPT" \
    "$APP_LAUNCHER" \
    >"$APP_STDOUT" \
    2>"$APP_STDERR" &
else
  env \
    MOXIN_UPDATE_RELEASE_API="file://$RELEASE_JSON" \
    MOXIN_UPDATE_CACHE_DIR="$CACHE_DIR" \
    "$APP_LAUNCHER" \
    >"$APP_STDOUT" \
    2>"$APP_STDERR" &
fi

APP_PID=$!
echo "App PID: $APP_PID"
echo "Wait about 8-15 seconds, then verify:"
echo "  1. App started normally"
echo "  2. Toast appears after update download"
echo "  3. Left-bottom settings entry shows 新版本 / New Version"
echo "  4. Settings > 系统 shows 新版本 / New Version"
echo "  5. About page shows 安装新版本 / Install New Version"
echo
echo "After clicking install in fake mode, inspect:"
echo "  $INSTALL_LOG"
echo
echo "App stdout log: $APP_STDOUT"
echo "App stderr log: $APP_STDERR"
