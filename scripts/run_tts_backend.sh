#!/usr/bin/env bash
set -euo pipefail

backend="${MOXIN_INFERENCE_BACKEND:-primespeech_mlx}"
zero_shot_backend="${MOXIN_ZERO_SHOT_BACKEND:-primespeech_mlx}"

qwen_root="${QWEN3_TTS_MODEL_ROOT:-$HOME/.OminiX/models/qwen3-tts-mlx}"
qwen_custom_dir="${QWEN3_TTS_CUSTOMVOICE_MODEL_DIR:-$qwen_root/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit}"
qwen_base_dir="${QWEN3_TTS_BASE_MODEL_DIR:-$qwen_root/Qwen3-TTS-12Hz-1.7B-Base}"

resolve_primespeech_node() {
  local root_dir
  root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

  if [[ -n "${MOXIN_PRIMESPEECH_TTS_NODE_BIN:-}" && -x "${MOXIN_PRIMESPEECH_TTS_NODE_BIN}" ]]; then
    echo "${MOXIN_PRIMESPEECH_TTS_NODE_BIN}"
    return 0
  fi

  if [[ -x "$root_dir/target/debug/moxin-tts-node" ]]; then
    echo "$root_dir/target/debug/moxin-tts-node"
    return 0
  fi
  if [[ -x "$root_dir/target/release/moxin-tts-node" ]]; then
    echo "$root_dir/target/release/moxin-tts-node"
    return 0
  fi
  if command -v moxin-tts-node >/dev/null 2>&1; then
    command -v moxin-tts-node
    return 0
  fi
  return 1
}

resolve_qwen_node() {
  local root_dir
  root_dir="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

  if [[ -n "${MOXIN_QWEN3_TTS_NODE_BIN:-}" && -x "${MOXIN_QWEN3_TTS_NODE_BIN}" ]]; then
    echo "${MOXIN_QWEN3_TTS_NODE_BIN}"
    return 0
  fi
  if [[ -x "$root_dir/target/debug/qwen-tts-node" ]]; then
    echo "$root_dir/target/debug/qwen-tts-node"
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

  # Dev convenience: build node on demand if source tree is available.
  if [[ -f "$root_dir/Cargo.toml" ]] && command -v cargo >/dev/null 2>&1; then
    echo "[qwen-tts-node] building debug binary..." >&2
    (cd "$root_dir" && cargo build -p moxin-tts-node --bin qwen-tts-node >/dev/null 2>&1 || true)
    if [[ -x "$root_dir/target/debug/qwen-tts-node" ]]; then
      echo "$root_dir/target/debug/qwen-tts-node"
      return 0
    fi
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

case "$backend" in
  qwen3_tts_mlx)
    if ! qwen_model_ready "$qwen_custom_dir"; then
      echo "ERROR: qwen custom model not ready: $qwen_custom_dir" >&2
      echo "Run bootstrap/initialization first, or set QWEN3_TTS_CUSTOMVOICE_MODEL_DIR." >&2
      exit 2
    fi

    if [[ "$zero_shot_backend" == "qwen3_tts_mlx" ]] && ! qwen_model_ready "$qwen_base_dir"; then
      echo "ERROR: qwen base model not ready for zero-shot backend: $qwen_base_dir" >&2
      exit 2
    fi

    if node_bin="$(resolve_qwen_node)"; then
      export QWEN3_TTS_MODEL_DIR="$qwen_custom_dir"
      export QWEN3_TTS_CUSTOMVOICE_MODEL_DIR="$qwen_custom_dir"
      export QWEN3_TTS_BASE_MODEL_DIR="$qwen_base_dir"
      export MOXIN_QWEN3_TTS_MODEL_ROOT="$qwen_root"
      exec "$node_bin" "$@"
    fi
    echo "ERROR: qwen3_tts_mlx selected but no qwen node found. Build qwen-tts-node or set MOXIN_QWEN3_TTS_NODE_BIN." >&2
    exit 127
    ;;
  primespeech_mlx|*)
    if node_bin="$(resolve_primespeech_node)"; then
      exec "$node_bin" "$@"
    fi
    echo "ERROR: primespeech node not found. Build moxin-tts-node first." >&2
    exit 127
    ;;
esac
