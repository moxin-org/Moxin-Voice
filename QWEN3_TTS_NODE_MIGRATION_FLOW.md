# Qwen3-TTS 节点迁移流程与完成度检查

更新时间：2026-03-11
范围：`moxin-tts` 项目内将 `qwen3-tts-mlx` 接入为
- 推理节点（inference node）
- zero-shot 可选后端

---

## 1. 迁移流程（8 环节）

1. 能力与边界确认
- 明确目标：只做推理与 zero-shot，不做 few-shot 训练替换。
- 明确边界：few-shot 训练仍保留现有链路（Option A/Option B 体系）。

2. 节点二进制化（Node Binary）
- 将模型库封装为 Dora 节点可执行程序。
- 要求：可被 dataflow `path` 直接启动，不依赖手工命令行拼接。

3. 协议与 I/O 契约对齐
- 输入保持当前 `VOICE:*` 协议与 JSON 包裹参数（speed/pitch/volume）。
- 输出保持 `audio/status/segment_complete/log`。

4. 后端路由与切换控制
- 在设置页提供“推理后端 / zero-shot 后端”切换。
- 切换时按策略处理：
  - 资源就绪 -> 切换并重启 dataflow
  - 资源未就绪 -> 不切换，触发后台准备

5. 模型资产生命周期
- 统一目录、完整性检查、下载脚本、首启与按需补齐。
- 不影响当前已可用 PrimeSpeech 数据流。

6. 开发态与分发态一致性
- 开发态可自动发现/构建节点。
- 分发态打包内置节点与脚本，用户无需手工配置路径。

7. 运行时 UX
- 不阻塞当前可用功能（尤其是“未切换成功前”不停止现有数据流）。
- 提供可理解的状态提示（未就绪/下载中/就绪/失败）。

8. 验证与回归
- 编译验证、脚本语法验证、切换行为验证、失败场景验证。

---

## 2. 当前完成度（逐环节）

## 2.1 能力与边界确认
状态：已完成
- 现状与目标在代码中体现为“qwen 仅接管 inference + zero-shot”。

## 2.2 节点二进制化
状态：已完成
- 新增内置节点：`qwen-tts-node`
- 关键文件：
  - `node-hub/dora-qwen3-tts-mlx/src/main.rs`
  - `node-hub/dora-qwen3-tts-mlx/Cargo.toml`

## 2.3 协议与 I/O 契约对齐
状态：部分完成
- 已完成：
  - 输入解析兼容当前 `VOICE:*` 协议与 JSON 参数壳。
  - 输出通道与现有节点一致：`audio/status/segment_complete/log`。
- 未完成：
  - `VOICE:TRAINED|...` 在 qwen 节点中明确返回不支持（当前设计）。

## 2.4 后端路由与切换控制
状态：已完成
- 设置页支持：
  - 推理后端：`primespeech_mlx / qwen3_tts_mlx`
  - zero-shot 后端：`primespeech_mlx / qwen3_tts_mlx`
- 新策略已落地：
  - 模型未就绪时不切换，触发后台下载。
  - 下载完成后用户可再次切换。
- 关键文件：`apps/moxin-voice/src/screen.rs`

## 2.5 模型资产生命周期
状态：已完成
- 统一目录：`~/.OminiX/models/qwen3-tts-mlx`
- 自定义脚本：`scripts/download_qwen3_tts_models.py`
- 首启初始化：`scripts/macos_bootstrap.sh`
- 预检完整性：`scripts/macos_preflight.sh`

## 2.6 开发态与分发态一致性
状态：已完成
- 开发态：`scripts/run_tts_backend.sh` 可自动发现，缺失时按需构建 `qwen-tts-node`。
- 分发态：`scripts/build_macos_app.sh` 打包 `qwen-tts-node` 到 `.app`。
- 分发运行：`scripts/macos_run_tts_backend.sh` 优先使用 bundle 内置节点。

## 2.7 运行时 UX
状态：已完成（基础版）
- 未就绪切换时：不停止当前数据流，不阻塞现有 TTS 功能。
- 新增状态提示：Qwen 模型状态（未就绪/下载中/部分就绪/已就绪/失败）。
- 下载完成后：toast 通知用户重新切换。
- 备注：当前为“阶段状态提示”，非字节级百分比进度条。

## 2.8 验证与回归
状态：已完成（代码级）
- 已通过：
  - `cargo check -p dora-qwen3-tts-mlx`
  - `cargo check -p moxin-voice-shell`
- 脚本语法检查通过：
  - `scripts/run_tts_backend.sh`
  - `scripts/macos_run_tts_backend.sh`
  - `scripts/macos_preflight.sh`
  - `scripts/macos_bootstrap.sh`

---

## 3. 结论：当前节点是否“全部完成”

结论：
- 若按你本次目标（`qwen` 作为推理节点 + zero-shot 可选后端 + 开发/分发可用 + 未就绪后台下载）评估：**已完成**。
- 若按“完全覆盖当前所有 PrimeSpeech 协议能力”评估：**未完全完成**，主要缺口是 `VOICE:TRAINED` 在 qwen 节点尚不支持。

---

## 4. 已知限制与后续建议

1. `VOICE:TRAINED` 暂不支持
- 当前 qwen 节点未接入“自定义训练权重路径推理”语义。

2. 下载进度展示仍为阶段级
- 目前 UI 提示“下载中/完成/失败”，未显示实时百分比。

3. 语言/音色映射策略可继续细化
- 目前对旧内置音色到 qwen speaker 的映射是实用映射，后续可改成可配置映射表。

---

## 5. 关键文件索引

- 节点实现
  - `node-hub/dora-qwen3-tts-mlx/src/main.rs`
  - `node-hub/dora-qwen3-tts-mlx/src/protocol.rs`
  - `node-hub/dora-qwen3-tts-mlx/src/audio_post.rs`
  - `node-hub/dora-qwen3-tts-mlx/Cargo.toml`

- UI 与运行时切换
  - `apps/moxin-voice/src/app_preferences.rs`
  - `apps/moxin-voice/src/screen.rs`

- 数据流与路由
  - `apps/moxin-voice/dataflow/tts.yml`
  - `scripts/dataflow/tts.bundle.yml`
  - `scripts/run_tts_backend.sh`
  - `scripts/macos_run_tts_backend.sh`

- 初始化/预检/打包
  - `scripts/download_qwen3_tts_models.py`
  - `scripts/macos_bootstrap.sh`
  - `scripts/macos_preflight.sh`
  - `scripts/build_macos_app.sh`
