# Qwen3-TTS-MLX 接入状态（moxin-tts）

> 范围：已完成“工程接入 + 模型生命周期接入（开发态/分发态）”，用于把 `qwen3-tts-mlx` 作为可选推理/zero-shot 后端挂入现有 Dora 数据流。qwen 推理仍需要本机提供 qwen 节点可执行文件（`MOXIN_QWEN3_TTS_NODE_BIN` 或 `qwen3-tts-node`）。

## 目标
- 将 `qwen3-tts-mlx` 接入为：
  - 推理节点可选后端
  - zero-shot 可选后端
- 在设置页可切换：
  - 推理后端
  - zero-shot 后端

## 8 环节对照（按迁移流程）
1. 协议/输入输出契约：已完成（接入层）
- 维持现有 `prompt` 载荷和 `VOICE:*` 协议不变。
- 新增后端选择仅影响“启动哪个 TTS 节点进程”。

2. 节点可执行发现与启动：已完成
- 新增开发脚本 `scripts/run_tts_backend.sh`。
- 新增打包脚本 `scripts/macos_run_tts_backend.sh`。
- 通过 `MOXIN_INFERENCE_BACKEND` 选择 `primespeech_mlx` / `qwen3_tts_mlx`。

3. 数据流编排：已完成
- `apps/moxin-voice/dataflow/tts.yml` 改为调用后端分发脚本。
- `scripts/dataflow/tts.bundle.yml` 同步改为后端分发脚本。
- 增加占位符：`__MOXIN_INFERENCE_BACKEND__`、`__MOXIN_ZERO_SHOT_BACKEND__`。

4. 运行时配置注入：已完成
- App 启动数据流前会生成 `~/.dora/runtime/dataflow/tts.runtime.yml`。
- 生成时将占位符替换为当前设置值，并写入运行时环境。

5. 设置项与持久化：已完成
- `AppPreferences` 新增：
  - `inference_backend`
  - `zero_shot_backend`
- 设置页新增两个下拉项并持久化。

6. 打包分发链路：已完成（基础）
- `scripts/build_macos_app.sh` 已把 `macos_run_tts_backend.sh` 打进 app。
- 打包生成的 dataflow 也使用后端分发脚本。

7. 模型与依赖准备：已完成（节点二进制除外）
- PrimeSpeech MLX 路径完整可用。
- Qwen 模型目录已标准化：
  - `~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit`
  - `~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-Base`
- 首启 bootstrap 按后端选择自动下载 qwen 模型：
  - 推理后端=qwen：拉取 CustomVoice
  - zero-shot 后端=qwen：拉取 Base
- preflight 会校验 qwen 模型完整性与 qwen 节点可执行文件可用性。

8. 验证与回退：已完成（框架层）
- 可在设置页一键切换回 `PrimeSpeech MLX`。
- 切换推理后端时会自动重启数据流。

## 当前已知限制
- 本次未在仓库内引入完整的 qwen 节点 Rust crate 实现；当前仍通过外部 qwen 节点二进制接入。
- 若选择 `qwen3_tts_mlx` 但本机缺少 qwen 节点，可执行会报错并提示设置 `MOXIN_QWEN3_TTS_NODE_BIN`。
- zero-shot/qwen 的效果仍取决于 qwen 节点对当前 `VOICE:*` 协议与参数的实际兼容度。

## 本次主要改动文件
- `apps/moxin-voice/src/app_preferences.rs`
- `apps/moxin-voice/src/screen.rs`
- `apps/moxin-voice/dataflow/tts.yml`
- `scripts/dataflow/tts.bundle.yml`
- `scripts/build_macos_app.sh`
- `scripts/run_tts_backend.sh`
- `scripts/macos_run_tts_backend.sh`
- `scripts/macos_bootstrap.sh`
- `scripts/macos_preflight.sh`
- `scripts/download_qwen3_tts_models.py`

## 本地验证建议
1. `cargo run -p moxin-voice-shell`
2. 设置 -> 数据 -> 实验功能：切换“推理后端”。
3. 观察日志是否出现 runtime dataflow 生成与数据流重启。
4. 如测 qwen：先配置 `MOXIN_QWEN3_TTS_NODE_BIN` 后再切换到 qwen 后端。
