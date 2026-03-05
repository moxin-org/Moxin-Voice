{
  description = "Moxin Studio 一键启动 (Nix 封装)";

  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    flake-utils.url = "github:numtide/flake-utils";
    rust-overlay.url = "github:oxalica/rust-overlay";
  };

  outputs = { self, nixpkgs, flake-utils, rust-overlay }:
    flake-utils.lib.eachDefaultSystem (system:
      let
        overlays = [ rust-overlay.overlays.default ];
        pkgs = import nixpkgs {
          inherit system overlays;
          config = { allowUnfree = true; };
        };

        rustToolchain = pkgs.rust-bin.stable.latest.default;
        python = pkgs.python312;
        nodejs = pkgs.nodejs_20;

        runScript = pkgs.writeShellApplication {
          name = "run-moxin";
          runtimeInputs = [
            rustToolchain
            python
            nodejs
            pkgs.git
            pkgs.cmake
            pkgs.pkg-config
            pkgs.openssl
            pkgs.portaudio
          ];
          text = ''
            set -euo pipefail

            ROOT="''${MOFA_STUDIO_DIR:-$PWD}"
            if [ ! -d "$ROOT" ]; then
              echo "[Moxin][Nix] 无法找到源码目录 $ROOT" >&2
              exit 1
            fi

            # Check for at least one app's dataflow directory
            FM_DATAFLOW_DIR="$ROOT/apps/moxin-fm/dataflow"
            DEBATE_DATAFLOW_DIR="$ROOT/apps/moxin-debate/dataflow"
            if [ ! -d "$FM_DATAFLOW_DIR" ] && [ ! -d "$DEBATE_DATAFLOW_DIR" ]; then
              echo "[Moxin][Nix] 缺少 dataflow 目录：$FM_DATAFLOW_DIR 或 $DEBATE_DATAFLOW_DIR" >&2
              exit 1
            fi
            # Use FM dataflow dir for dora daemon startup (default)
            DATAFLOW_DIR="$FM_DATAFLOW_DIR"
            if [ ! -d "$DATAFLOW_DIR" ]; then
              DATAFLOW_DIR="$DEBATE_DATAFLOW_DIR"
            fi

            STATE_DIR="''${MOFA_STATE_DIR:-$ROOT/.nix-moxin}"
            INSTALL_ROOT="$STATE_DIR"
            BIN_DIR="$INSTALL_ROOT/bin"
            mkdir -p "$BIN_DIR"

            export CARGO_HOME="''${MOFA_CARGO_HOME:-$STATE_DIR/cargo}"
            export PATH="$BIN_DIR:$PATH"
            TARGET_DORA_RS_VERSION="''${MOFA_DORA_RS_VERSION:-0.3.12}"

            if [ "''${MOFA_SKIP_BOOTSTRAP:-0}" != 1 ]; then
              if [ ! -x "$BIN_DIR/dora" ]; then
                echo "[Moxin][Nix] 安装 dora-cli..."
                cargo install --locked \
                  --git https://github.com/dora-rs/dora.git \
                  --rev b56884441c249ed5d0a6e4d066dea16a246d578d \
                  dora-cli \
                  --root "$INSTALL_ROOT"
              fi
            else
              echo "[Moxin][Nix] 跳过 dora-cli 安装 (MOFA_SKIP_BOOTSTRAP=1)"
            fi

            VENV_DIR="''${MOFA_VENV_DIR:-$ROOT/.venv-moxin}"
            if [ ! -d "$VENV_DIR" ]; then
              echo "[Moxin][Nix] 创建 Python venv ($VENV_DIR)..."
              python3 -m venv "$VENV_DIR"
            fi
            # shellcheck source=/dev/null
            source "$VENV_DIR/bin/activate"
            if [ "''${MOFA_SKIP_BOOTSTRAP:-0}" != 1 ]; then
              if [ ! -f "$VENV_DIR/.ready" ]; then
                pip install --upgrade pip wheel setuptools
                pip install "dora-rs==$TARGET_DORA_RS_VERSION"
                pip install -e "$ROOT/libs/dora-common" \
                  -e "$ROOT/node-hub/dora-text-segmenter" \
                  -e "$ROOT/node-hub/dora-primespeech"
                touch "$VENV_DIR/.ready"
              fi
            else
              echo "[Moxin][Nix] 跳过 Python 依赖安装 (MOFA_SKIP_BOOTSTRAP=1)"
            fi

            get_installed_dora_rs_version() {
              python3 - <<'PY' 2>/dev/null || true
import sys
try:
    import importlib.metadata as metadata
except Exception:
    import importlib_metadata as metadata
try:
    print(metadata.version("dora-rs"))
except Exception:
    pass
PY
            }

            INSTALLED_DORA_RS_VERSION=$(get_installed_dora_rs_version | tr -d '\r')
            if [ -z "$INSTALLED_DORA_RS_VERSION" ] || [ "$INSTALLED_DORA_RS_VERSION" != "$TARGET_DORA_RS_VERSION" ]; then
              if [ "''${MOFA_SKIP_BOOTSTRAP:-0}" != 1 ]; then
                echo "[Moxin][Nix] 调整 dora-rs 版本 -> $TARGET_DORA_RS_VERSION (当前: ''${INSTALLED_DORA_RS_VERSION:-无})"
                pip install --upgrade --force-reinstall "dora-rs==$TARGET_DORA_RS_VERSION"
              else
                echo "[Moxin][Nix] ⚠️ dora-rs 当前版本 ''${INSTALLED_DORA_RS_VERSION:-未安装} 与预期 $TARGET_DORA_RS_VERSION 不一致。请执行一次未设置 MOFA_SKIP_BOOTSTRAP=1 的 nix run 或手动运行:"
                echo "       source $VENV_DIR/bin/activate && pip install --upgrade --force-reinstall 'dora-rs==$TARGET_DORA_RS_VERSION'"
              fi
            fi

            export PATH="$VENV_DIR/bin:$PATH"

            # 预编译 dataflow 里会用到的 Rust 节点，免得 Dora 运行时找不到可执行文件
            echo "[Moxin][Nix] 预编译 Dora 节点..."
            cd "$ROOT"
            for manifest in \
              "node-hub/dora-conference-bridge/Cargo.toml" \
              "node-hub/dora-conference-controller/Cargo.toml" \
              "node-hub/dora-maas-client/Cargo.toml"
            do
              if [ -f "$manifest" ]; then
                echo "  cargo build --release --manifest-path $manifest"
                cargo build --release --manifest-path "$manifest"
              else
                echo "  [warn] 未找到 $manifest，跳过" >&2
              fi
            done

            echo "[Moxin][Nix] pkill -f dora 清理残留进程..."
            pkill -f dora || true

            echo "[Moxin][Nix] 启动 Dora daemon..."
            cd "$DATAFLOW_DIR"
            dora up >/tmp/moxin-dora.log 2>&1 &
            sleep 2

            if dora list >/dev/null 2>&1; then
              echo "[Moxin][Nix] 清理历史 dataflow..."
              dora list | awk 'NR>1 {print $1}' | while read -r id; do
                if [ -n "$id" ]; then
                  dora stop --grace-duration 0s "$id" || true
                fi
              done
            fi

            export MOFA_AUTO_START=1

            if [ "''${MOFA_DRY_RUN:-0}" = 1 ]; then
              echo "[Moxin][Nix] Dry run 已完成，未启动 GUI。"
              exit 0
            fi

            echo "[Moxin][Nix] 启动 GUI..."
            cd "$ROOT"
            cargo run --release
          '';
        };
      in
      {
        packages.default = runScript;
        apps.default = {
          type = "app";
          program = "${runScript}/bin/run-moxin";
        };
        devShells.default = pkgs.mkShell {
          packages = [
            rustToolchain
            python
            nodejs
            pkgs.git
            pkgs.cmake
            pkgs.pkg-config
            pkgs.openssl
            pkgs.portaudio
          ];
        };
      });
}
