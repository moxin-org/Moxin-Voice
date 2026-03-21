#!/usr/bin/env bash
# Qwen3-TTS-MLX backend launcher (dev / repo build).
# PrimeSpeech/primespeech_mlx backend removed; see doc/REFACTOR_QWEN3_ONLY.md to restore.
set -euo pipefail

root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

qwen_root="${QWEN3_TTS_MODEL_ROOT:-$HOME/.OminiX/models/qwen3-tts-mlx}"
qwen_custom_dir="${QWEN3_TTS_CUSTOMVOICE_MODEL_DIR:-$qwen_root/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit}"
qwen_base_dir="${QWEN3_TTS_BASE_MODEL_DIR:-$qwen_root/Qwen3-TTS-12Hz-1.7B-Base-8bit}"

resolve_qwen_node() {
  local root_dir
  root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
  local debug_bin="$root_dir/target/debug/qwen-tts-node"

  if [[ -n "${MOXIN_QWEN3_TTS_NODE_BIN:-}" && -x "${MOXIN_QWEN3_TTS_NODE_BIN}" ]]; then
    echo "${MOXIN_QWEN3_TTS_NODE_BIN}"
    return 0
  fi

  # Dev convenience: rebuild when source is newer than the current debug binary.
  if [[ -f "$root_dir/Cargo.toml" ]] && command -v cargo >/dev/null 2>&1; then
    local needs_build="0"
    if [[ ! -x "$debug_bin" ]]; then
      needs_build="1"
    elif find "$root_dir/node-hub/dora-qwen3-tts-mlx" -type f \
      \( -name '*.rs' -o -name 'Cargo.toml' -o -name '*.yml' -o -name '*.yaml' \) \
      -newer "$debug_bin" | grep -q .; then
      needs_build="1"
    fi

    if [[ "$needs_build" == "1" ]]; then
      echo "[qwen-tts-node] building debug binary..." >&2
      local mlx_prebuilt_path=""
      mlx_prebuilt_path="$(find "$root_dir/target/debug/build" -type d -path '*mlx-sys-*/out/mlx-prebuilt' 2>/dev/null | tail -n 1 || true)"
      if [[ -n "$mlx_prebuilt_path" ]]; then
        (cd "$root_dir" && MLX_PREBUILT_PATH="$mlx_prebuilt_path" cargo build -p dora-qwen3-tts-mlx --bin qwen-tts-node >/dev/null 2>&1 || true)
      else
        (cd "$root_dir" && cargo build -p dora-qwen3-tts-mlx --bin qwen-tts-node >/dev/null 2>&1 || true)
      fi
    fi
  fi

  if [[ -x "$debug_bin" ]]; then
    echo "$debug_bin"
    return 0
  fi
  if [[ -x "$root_dir/target/release/qwen-tts-node" ]]; then
    echo "$root_dir/target/release/qwen-tts-node"
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
  echo "Run scripts/init_qwen3_models.sh first, or set QWEN3_TTS_CUSTOMVOICE_MODEL_DIR." >&2
  exit 2
fi

if ! qwen_model_ready "$qwen_base_dir"; then
  echo "ERROR: Qwen3 Base model not ready: $qwen_base_dir" >&2
  echo "Run scripts/init_qwen3_models.sh first, or set QWEN3_TTS_BASE_MODEL_DIR." >&2
  exit 2
fi

if node_bin="$(resolve_qwen_node)"; then
  export QWEN3_TTS_MODEL_DIR="$qwen_custom_dir"
  export QWEN3_TTS_CUSTOMVOICE_MODEL_DIR="$qwen_custom_dir"
  export QWEN3_TTS_BASE_MODEL_DIR="$qwen_base_dir"
  export MOXIN_QWEN3_TTS_MODEL_ROOT="$qwen_root"
  exec "$node_bin" "$@"
fi

echo "ERROR: qwen-tts-node not found. Build with: cargo build -p dora-qwen3-tts-mlx" >&2
exit 127

# --- PrimeSpeech backend (removed, see doc/REFACTOR_QWEN3_ONLY.md) ---
# resolve_primespeech_node() { ... }
# primespeech_mlx|*) exec "$node_bin" ...
