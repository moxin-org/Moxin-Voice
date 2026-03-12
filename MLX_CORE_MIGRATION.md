# MLX 核心迁移总文档（PrimeSpeech -> moxin-tts-node）

> 文档目的：将 `MIGRATION_PLAN.md`、`MLX_TTS_MIGRATION.md`、`MIGRATION_CHECKLIST.md` 合并为一份可执行文档，覆盖迁移背景、调研结论、实施计划与进度、遗留问题、开发者升级动作。  
> 适用范围：`Moxin-Voice` 当前代码基线（2026-03）。

---

## 1. 背景

### 1.1 迁移动机

原 TTS 核心为 `dora-primespeech`（Python）。本次迁移的目标是将 TTS 推理核心切换为 `moxin-tts-node`（Rust + MLX），核心收益：

- 降低 Python 运行时依赖，统一主链路到 Rust
- 利用 Apple Silicon 上的 MLX 推理能力
- 保持现有产品能力与协议兼容：
  - 内置音色 TTS
  - Zero-shot（Express）
  - Few-shot（Pro，训练后推理）

### 1.2 本次迁移边界

- 已迁移：TTS 推理核心（`dora-primespeech` -> `moxin-tts-node`）
- 保持不变：ASR 仍使用 `dora-asr`（Python）
- 训练策略：
  - 默认稳定路径：Option A（旧 Python few-shot 训练）
  - 实验路径：Option B（Rust/MLX few-shot 训练器）

---

## 2. 前期调研结论

### 2.1 方案对比结论

前期对比了 `gpt-sovits-mlx` 与 `qwen3-tts-mlx` 两条路线。最终选型：

- 采用 `gpt-sovits-mlx` 作为迁移主路线（最小改动、与既有 GPT-SoVITS 资产兼容度最高）
- 暂不切换到 qwen3-tts-mlx（需要额外适配与行为对齐）

### 2.2 架构契约结论

Rust Dora 节点要保持与原链路一致的输入输出契约：

- 输入：`text`
- 输出：`audio` / `segment_complete` / `log`
- 协议兼容：
  - 普通文本
  - `VOICE:CUSTOM|...`
  - `VOICE:TRAINED|...`

### 2.3 模型与目录结论

新的默认模型目录统一为：

- `~/.OminiX/models/gpt-sovits-mlx`

核心内容：

- `encoders/`（HuBERT、BERT）
- `bert-tokenizer/`
- `voices/voices.json`
- 每个音色目录下的 `gpt.safetensors` / `sovits.safetensors` / `reference.wav`
- 可选对齐文件：`vits.onnx`、`prompt_semantic.npy`

---

## 3. 实施计划与当前进度

本节按“计划阶段”描述，并标记当前状态。

### Phase 1：模型层迁移（已完成，可复跑）

目标：将旧模型布局转为 MLX 节点所需结构。  
当前状态：

- 已提供批量转换脚本：
  - `scripts/convert_all_voices.py`
- 已提供音质对齐辅助脚本：
  - `scripts/export_all_vits_onnx.py`
  - `scripts/extract_all_prompt_semantic.py`
- 已形成开发者可执行清单（并入本文第 5 节）

### Phase 2：节点层迁移（已完成）

目标：落地 Rust TTS 节点并替换 dataflow。  
当前状态：

- 已落地新节点 crate：
  - `node-hub/dora-primespeech-mlx`
- dataflow 已指向 Rust 节点：
  - `apps/moxin-voice/dataflow/tts.yml`
- 节点产物：
  - `target/release/moxin-tts-node`
  - `target/release/moxin-fewshot-trainer`（实验训练器）

### Phase 3：依赖管理与源码组织（已完成）

目标：去除本地 vendor 复制模式，切到远端 git 依赖，并保留本地 patch 能力。  
当前状态：

- `moxin-tts-node` 已通过 git 引用 OminiX-MLX
- workspace 使用 `[patch."https://github.com/OminiX-ai/OminiX-MLX.git"]` 覆盖 `gpt-sovits-mlx` 到本地 `patches/`，用于快速修复与验证
- `vendor/` 已移除

### Phase 4：训练路径切换能力（已完成）

目标：支持 few-shot 训练后端可切换。  
当前状态：

- 已支持环境变量开关：
  - `MOXIN_TRAINING_BACKEND=option_a|option_b`
- 默认保持 `option_a`（稳定）
- `option_b` 需要 `reference_text`
- 可选 trainer 路径：
  - `MOXIN_FEWSHOT_TRAINER_BIN=/abs/path/to/moxin-fewshot-trainer`

---

## 4. 目前仍存在的问题（真实状态）

### 4.1 Option B（Rust/MLX few-shot 训练）效果尚未达到旧流程

现状：

- Option B 可跑通训练并产出可用权重
- 但音色克隆质量与旧 Python few-shot 仍有差距

主要原因（实现边界）：

- 当前 Option B 训练链路以 SoVITS 训练为主，GPT/T2S 训练未完整接入当前产品路径
- 训练数据构建仍偏简化（与旧流程相比，切片/组织策略未完全对齐）
- 训练超参与收敛控制仍偏实验态

结论：

- 生产默认仍应使用 Option A（Python）
- Option B 用于持续迭代验证，不建议替代稳定训练路径

### 4.2 ASR 尚未迁移出 Python

现状：

- `dora-asr` 仍是 Python 节点
- 本次不在迁移范围内

影响：

- “全链路无 Python”尚未达成（当前仅 TTS 推理主链路 Rust 化）

### 4.3 模型资产一致性依赖本地初始化质量

现状：

- 若开发者本地缺失 `prompt_semantic.npy` 或 `vits.onnx`，可用但音质/风格可能退化
- 需要按本文第 5 节补齐资产

---

## 5. 其他开发者拉取更新后必须做的事

本节是迁移后开发者最小可运行路径。

### 5.1 同步代码并编译节点（必做）

```bash
cd /path/to/Moxin-Voice
git pull
cargo build -p dora-primespeech-mlx --release
```

应看到：

- `target/release/moxin-tts-node`
- `target/release/moxin-fewshot-trainer`

### 5.2 初始化/升级模型目录（必做）

```bash
cd /path/to/Moxin-Voice
conda run -n mofa-studio python3 scripts/convert_all_voices.py
```

应生成：

- `~/.OminiX/models/gpt-sovits-mlx/encoders/hubert.safetensors`
- `~/.OminiX/models/gpt-sovits-mlx/encoders/bert.safetensors`
- `~/.OminiX/models/gpt-sovits-mlx/bert-tokenizer/tokenizer.json`
- `~/.OminiX/models/gpt-sovits-mlx/voices/voices.json`
- `~/.OminiX/models/gpt-sovits-mlx/voices/<Voice>/*`

### 5.3 补齐音质对齐文件（强烈建议）

```bash
cd /path/to/Moxin-Voice
conda run -n mofa-studio python3 scripts/export_all_vits_onnx.py
conda run -n mofa-studio python3 scripts/extract_all_prompt_semantic.py
```

### 5.4 快速自检（必做）

```bash
test -f ~/.OminiX/models/gpt-sovits-mlx/voices/voices.json && echo "voices.json OK"
test -f ~/.OminiX/models/gpt-sovits-mlx/encoders/hubert.safetensors && echo "hubert OK"
test -f ~/.OminiX/models/gpt-sovits-mlx/encoders/bert.safetensors && echo "bert OK"
test -f ~/.OminiX/models/gpt-sovits-mlx/voices/Doubao/gpt.safetensors && echo "Doubao GPT OK"
test -f ~/.OminiX/models/gpt-sovits-mlx/voices/Doubao/sovits.safetensors && echo "Doubao SoVITS OK"
test -f ~/.OminiX/models/gpt-sovits-mlx/voices/Doubao/reference.wav && echo "Doubao ref OK"
```

### 5.5 训练后端开关（按需）

默认（推荐）：

```bash
export MOXIN_TRAINING_BACKEND=option_a
```

实验：

```bash
export MOXIN_TRAINING_BACKEND=option_b
export MOXIN_FEWSHOT_TRAINER_BIN=/abs/path/to/target/release/moxin-fewshot-trainer
```

注意：

- Option B 训练要求 `reference_text` 非空
- 当前质量基线仍以 Option A 为准

---

## 6. 发布与协作建议

- 团队协作阶段：保留 Option A 为默认，Option B 做持续验证
- 分发阶段：将模型初始化脚本纳入发布前/安装后流程，避免不同机器模型资产不一致
- 后续迭代优先级：
  - P1：Option B 训练数据构建与收敛质量对齐
  - P2：ASR Rust 化评估（如需全链路无 Python）

---

## 7. 结论

本次迁移已经完成“**TTS 推理核心 MLX 化**”这一主目标，且保留了原有协议与功能形态。  
当前未闭环项集中在“**few-shot Rust 训练质量对齐**”和“**ASR 仍为 Python**”，不影响当前团队继续开发与联调，但决定了默认训练路径仍应使用 Option A。
