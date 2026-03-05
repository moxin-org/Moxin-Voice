#!/usr/bin/env bash
# Moxin Studio 启动脚本
# 自动处理 Nix Flakes 实验性功能

set -euo pipefail

echo "[Moxin] Starting Moxin Studio..."
nix --extra-experimental-features 'nix-command flakes' run .
