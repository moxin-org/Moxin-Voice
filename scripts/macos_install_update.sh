#!/usr/bin/env bash
set -euo pipefail

DMG_PATH=""
APP_NAME="Moxin Voice"
CURRENT_APP=""
WAIT_PID=""
MOUNT_DIR=""
TMP_TARGET=""

usage() {
  cat <<EOF
Usage:
  $(basename "$0") --dmg <path> [--app-name <name>] [--current-app <path>] [--wait-pid <pid>]
EOF
}

notify_failure() {
  local message="Moxin Voice could not install the downloaded update automatically. The installer will be revealed in Finder."
  /usr/bin/osascript -e "display dialog \"$message\" buttons {\"OK\"} default button \"OK\" with title \"Moxin Voice Update\"" >/dev/null 2>&1 || true
  if [[ -f "$DMG_PATH" ]]; then
    open -R "$DMG_PATH" >/dev/null 2>&1 || open "$DMG_PATH" >/dev/null 2>&1 || true
  fi
}

cleanup() {
  if [[ -n "$MOUNT_DIR" && -d "$MOUNT_DIR" ]]; then
    hdiutil detach "$MOUNT_DIR" -quiet >/dev/null 2>&1 || true
    rm -rf "$MOUNT_DIR"
  fi
  if [[ -n "$TMP_TARGET" ]]; then
    rm -rf "$TMP_TARGET" >/dev/null 2>&1 || true
  fi
}

on_error() {
  cleanup
  notify_failure
}

trap on_error ERR
trap cleanup EXIT

while [[ $# -gt 0 ]]; do
  case "$1" in
    --dmg)
      DMG_PATH="$2"
      shift 2
      ;;
    --app-name)
      APP_NAME="$2"
      shift 2
      ;;
    --current-app)
      CURRENT_APP="$2"
      shift 2
      ;;
    --wait-pid)
      WAIT_PID="$2"
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

if [[ -z "$DMG_PATH" || ! -f "$DMG_PATH" ]]; then
  echo "Downloaded DMG not found: $DMG_PATH" >&2
  exit 1
fi

choose_target_app() {
  if [[ -n "$CURRENT_APP" && "$CURRENT_APP" == "/Applications/"*.app ]]; then
    echo "$CURRENT_APP"
    return 0
  fi

  if [[ -d "/Applications" && -w "/Applications" ]]; then
    echo "/Applications/${APP_NAME}.app"
    return 0
  fi

  mkdir -p "$HOME/Applications"
  echo "$HOME/Applications/${APP_NAME}.app"
}

wait_for_pid_exit() {
  if [[ -z "$WAIT_PID" ]]; then
    return 0
  fi

  while kill -0 "$WAIT_PID" >/dev/null 2>&1; do
    sleep 0.5
  done
}

TARGET_APP="$(choose_target_app)"
TARGET_PARENT="$(dirname "$TARGET_APP")"
mkdir -p "$TARGET_PARENT"

MOUNT_DIR="$(mktemp -d /tmp/moxin-update.XXXXXX)"
hdiutil attach "$DMG_PATH" -nobrowse -quiet -mountpoint "$MOUNT_DIR"

SOURCE_APP="$(find "$MOUNT_DIR" -maxdepth 1 -name "*.app" -print -quit)"
if [[ -z "$SOURCE_APP" || ! -d "$SOURCE_APP" ]]; then
  echo "No .app bundle found inside mounted DMG" >&2
  exit 1
fi

TMP_TARGET="${TARGET_APP}.updating"
rm -rf "$TMP_TARGET"
ditto "$SOURCE_APP" "$TMP_TARGET"

wait_for_pid_exit

rm -rf "$TARGET_APP"
mv "$TMP_TARGET" "$TARGET_APP"
TMP_TARGET=""

cleanup
rm -f "$DMG_PATH"

open -n "$TARGET_APP"
