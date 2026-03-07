#!/usr/bin/env bash
set -euo pipefail

# Resolve dora-asr without relying on daemon PATH inheritance.
# Priority:
# 1) MOXIN_CONDA_ENV explicit env
# 2) known env names: moxin-studio / mofa-studio
# 3) fallback to command lookup

env_name="${MOXIN_CONDA_ENV:-}"
if [[ -z "$env_name" ]]; then
  for candidate in moxin-studio mofa-studio; do
    if [[ -x "$HOME/miniconda3/envs/$candidate/bin/dora-asr" || -x "$HOME/anaconda3/envs/$candidate/bin/dora-asr" ]]; then
      env_name="$candidate"
      break
    fi
  done
fi

if [[ -n "$env_name" ]]; then
  if [[ -x "$HOME/miniconda3/envs/$env_name/bin/dora-asr" ]]; then
    exec "$HOME/miniconda3/envs/$env_name/bin/dora-asr" "$@"
  fi
  if [[ -x "$HOME/anaconda3/envs/$env_name/bin/dora-asr" ]]; then
    exec "$HOME/anaconda3/envs/$env_name/bin/dora-asr" "$@"
  fi
fi

if command -v dora-asr >/dev/null 2>&1; then
  exec "$(command -v dora-asr)" "$@"
fi

echo "ERROR: dora-asr not found. Install it into moxin-studio/mofa-studio env." >&2
exit 127
