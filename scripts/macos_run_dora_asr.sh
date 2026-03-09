#!/usr/bin/env bash
set -euo pipefail

# Resolve dora-asr without relying on daemon PATH inheritance.
# Priority:
# 1) App-private conda env prefix
# 2) App/private conda env name + root
# 3) legacy miniconda/anaconda env lookup
# 4) fallback to command lookup

env_name="${MOXIN_CONDA_ENV:-}"
conda_root="${MOXIN_CONDA_ROOT:-$HOME/.moxinvoice/conda}"
conda_env_prefix="${MOXIN_CONDA_ENV_PREFIX:-}"
if [[ -z "$conda_env_prefix" && -n "$env_name" ]]; then
  conda_env_prefix="$conda_root/envs/$env_name"
fi

if [[ -n "$conda_env_prefix" && -x "$conda_env_prefix/bin/dora-asr" ]]; then
  exec "$conda_env_prefix/bin/dora-asr" "$@"
fi

if [[ -z "$env_name" ]]; then
  for candidate in moxin-studio mofa-studio; do
    if [[ -x "$conda_root/envs/$candidate/bin/dora-asr" || -x "$HOME/miniconda3/envs/$candidate/bin/dora-asr" || -x "$HOME/anaconda3/envs/$candidate/bin/dora-asr" ]]; then
      env_name="$candidate"
      break
    fi
  done
fi

if [[ -n "$env_name" ]]; then
  if [[ -x "$conda_root/envs/$env_name/bin/dora-asr" ]]; then
    exec "$conda_root/envs/$env_name/bin/dora-asr" "$@"
  fi
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

echo "ERROR: dora-asr not found in app/private conda env. Run app initialization first." >&2
exit 127
