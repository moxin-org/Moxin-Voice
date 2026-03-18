#!/usr/bin/env bash
# Qwen3-TTS-MLX backend launcher (macOS app bundle).
# PrimeSpeech/primespeech_mlx backend removed; see doc/REFACTOR_QWEN3_ONLY.md to restore.
set -euo pipefail

script_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
app_root="$(cd "$script_dir/../.." && pwd)"

qwen_root="${QWEN3_TTS_MODEL_ROOT:-$HOME/.OminiX/models/qwen3-tts-mlx}"
qwen_custom_dir="${QWEN3_TTS_CUSTOMVOICE_MODEL_DIR:-$qwen_root/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit}"
qwen_base_dir="${QWEN3_TTS_BASE_MODEL_DIR:-$qwen_root/Qwen3-TTS-12Hz-1.7B-Base}"

resolve_qwen_node() {
  if [[ -n "${MOXIN_QWEN3_TTS_NODE_BIN:-}" && -x "${MOXIN_QWEN3_TTS_NODE_BIN}" ]]; then
    echo "${MOXIN_QWEN3_TTS_NODE_BIN}"
    return 0
  fi
  if [[ -x "$app_root/MacOS/qwen-tts-node" ]]; then
    echo "$app_root/MacOS/qwen-tts-node"
    return 0
  fi
  if command -v qwen3-tts-node >/dev/null 2>&1; then
    command -v qwen3-tts-node
    return 0
  fi
  if command -v qwen-tts-node >/dev/null 2>&1; then
    command -v qwen-tts-node
    return 0
  fi
  return 1
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

if ! qwen_model_ready "$qwen_custom_dir"; then
  echo "ERROR: Qwen3 CustomVoice model not ready: $qwen_custom_dir" >&2
  echo "Run app initialization first, or set QWEN3_TTS_CUSTOMVOICE_MODEL_DIR." >&2
  exit 2
fi

if ! qwen_model_ready "$qwen_base_dir"; then
  echo "ERROR: Qwen3 Base model not ready: $qwen_base_dir" >&2
  echo "Run app initialization first, or set QWEN3_TTS_BASE_MODEL_DIR." >&2
  exit 2
fi

if node_bin="$(resolve_qwen_node)"; then
  export QWEN3_TTS_MODEL_DIR="$qwen_custom_dir"
  export QWEN3_TTS_CUSTOMVOICE_MODEL_DIR="$qwen_custom_dir"
  export QWEN3_TTS_BASE_MODEL_DIR="$qwen_base_dir"
  export MOXIN_QWEN3_TTS_MODEL_ROOT="$qwen_root"
  exec "$node_bin" "$@"
fi

echo "ERROR: qwen-tts-node not found in app bundle. Bundle qwen-tts-node or set MOXIN_QWEN3_TTS_NODE_BIN." >&2
exit 127

# --- PrimeSpeech backend (removed, see doc/REFACTOR_QWEN3_ONLY.md) ---
# resolve_primespeech_node() { ... }
# primespeech_mlx|*) exec "$node_bin" ...
