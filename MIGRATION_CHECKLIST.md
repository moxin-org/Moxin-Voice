# Developer Migration Checklist (PrimeSpeech -> MLX TTS Core)

> ⚠️ 本文档已并入统一文档：`MLX_CORE_MIGRATION.md`。  
> 当前请优先参考：`./MLX_CORE_MIGRATION.md`。

> 适用对象：已经在本地做过旧版环境初始化（参考 `models/setup-local-models/README.md`）的开发者。  
> 目标：拉取最新代码后，更新本地模型布局，恢复可继续开发的状态。

---

## 0. 这次迁移的核心变化

- TTS 推理核心从 `dora-primespeech`（Python）切到 `moxin-tts-node`（Rust/MLX）。
- 默认模型目录从旧路径切到：
  - `~/.OminiX/models/gpt-sovits-mlx`
- 预置音色读取：
  - `~/.OminiX/models/gpt-sovits-mlx/voices/voices.json`
- ASR 仍使用 `dora-asr`（Python），本次不迁移 ASR。

---

## 1. 代码同步（必做）

- [ ] 拉取最新代码
- [ ] 编译新 TTS 节点

```bash
cd /path/to/Moxin-Voice
git pull
cargo build -p dora-primespeech-mlx --release
```

期望产物：

- `target/release/moxin-tts-node`
- `target/release/moxin-fewshot-trainer`（当前仅保留为实验/备用二进制，**现在线上 few-shot 训练主流程仍是 Python**）

---

## 2. 模型目录升级（必做）

如果你本地已经有旧的 `~/.dora/models/primespeech/moyoyo`，按下面步骤转换到新目录。

- [ ] 确认源模型目录存在
- [ ] 执行权重/配置转换脚本

```bash
cd /path/to/Moxin-Voice

# 按你本地已有的 Python/conda 环境执行（示例环境名 mofa-studio）
conda run -n mofa-studio python3 scripts/convert_all_voices.py
```

这一步会生成：

- `~/.OminiX/models/gpt-sovits-mlx/encoders/hubert.safetensors`
- `~/.OminiX/models/gpt-sovits-mlx/encoders/bert.safetensors`
- `~/.OminiX/models/gpt-sovits-mlx/bert-tokenizer/tokenizer.json`
- `~/.OminiX/models/gpt-sovits-mlx/voices/<Voice>/{gpt.safetensors,sovits.safetensors,reference.wav}`
- `~/.OminiX/models/gpt-sovits-mlx/voices/voices.json`

---

## 3. 音质对齐文件（强烈建议）

为了让内置音色效果对齐旧 pipeline，请补齐以下两类文件：

- [ ] 导出每个音色的 `vits.onnx`
- [ ] 预提取每个音色的 `prompt_semantic.npy`

```bash
cd /path/to/Moxin-Voice
conda run -n mofa-studio python3 scripts/export_all_vits_onnx.py
conda run -n mofa-studio python3 scripts/extract_all_prompt_semantic.py
```

说明：

- 缺少 `prompt_semantic.npy` 时，节点会退化到 zero-shot fallback（能出声，但音色一致性更差）。
- 缺少 `vits.onnx` 时，会回退到 MLX VITS（可用，但与既有效果可能有差异）。

---

## 4. 快速校验（必做）

- [ ] 校验关键文件存在
- [ ] 启动应用后可正常用内置音色合成

```bash
# 1) 核心目录
test -f ~/.OminiX/models/gpt-sovits-mlx/voices/voices.json && echo "voices.json OK"
test -f ~/.OminiX/models/gpt-sovits-mlx/encoders/hubert.safetensors && echo "hubert OK"
test -f ~/.OminiX/models/gpt-sovits-mlx/encoders/bert.safetensors && echo "bert OK"

# 2) 示例音色文件（Doubao）
test -f ~/.OminiX/models/gpt-sovits-mlx/voices/Doubao/gpt.safetensors && echo "Doubao GPT OK"
test -f ~/.OminiX/models/gpt-sovits-mlx/voices/Doubao/sovits.safetensors && echo "Doubao SoVITS OK"
test -f ~/.OminiX/models/gpt-sovits-mlx/voices/Doubao/reference.wav && echo "Doubao ref OK"
test -f ~/.OminiX/models/gpt-sovits-mlx/voices/Doubao/vits.onnx && echo "Doubao ONNX OK"
test -f ~/.OminiX/models/gpt-sovits-mlx/voices/Doubao/prompt_semantic.npy && echo "Doubao semantic OK"
```

运行时日志中应看到类似：

- `Loaded <N> voices from .../voices.json`

---

## 5. 可选：自定义模型目录

默认目录是 `~/.OminiX/models/gpt-sovits-mlx`。  
如需使用自定义路径，设置：

```bash
export GPT_SOVITS_MODEL_DIR=/your/custom/path/gpt-sovits-mlx
```

并保证该目录结构与第 2、3 步产物一致。

---

## 6. 常见问题

### Q0: 为什么现在还不用 `moxin-fewshot-trainer`？

- 当前 `moxin-fewshot-trainer` 的训练能力还不等价于旧 Python pipeline，关键差异如下：
  - **只训练 SoVITS，不训练 GPT**：当前输出里 `gpt_weights` 仍指向基座 GPT（如 Doubao），并未产生训练后的 GPT 权重。
  - **数据集过于简化**：当前 Rust 训练流程只构建单样本数据（`num_samples: 1`），缺少旧流程中的完整切分/清洗/多片段训练组织。
  - **前处理链路不完整**：旧 Python 训练包含更成熟的音频处理与特征对齐步骤；Rust 版本目前是最小可运行路径，鲁棒性不足。
  - **收敛与质量控制不足**：训练超参、epoch 策略、监控与校验仍偏实验状态，实际音色克隆质量与旧 few-shot 有明显差距。
- 所以 `moxin-fewshot-trainer` 当前保留为实验/备用二进制，**默认 few-shot 训练仍使用 Python `dora_primespeech.moyoyo_tts.training_service`**。
- 现状拆分：
  - 预置音色/推理：已迁移到 Rust `moxin-tts-node`；
  - few-shot 训练：仍走 Python；
  - few-shot 推理：训练完成后仍可在当前 TTS 节点中使用训练权重。

### Q1: 内置音色能说话但效果明显变差

- 先检查是否缺 `prompt_semantic.npy`。
- 再检查是否缺 `vits.onnx`。

### Q2: 报错 `Cannot read voices.json`

- 检查 `~/.OminiX/models/gpt-sovits-mlx/voices/voices.json` 是否存在且 JSON 合法。

### Q3: few-shot 训练后可用但音色差

- 先确保第 2 步的基座模型已正确转换。
- few-shot 是增量训练，质量高度依赖输入音频质量、时长和文本匹配。

---

## 7. 本次迁移结论

完成本清单后，开发者在拉取最新提交后应可继续：

- 预置音色 TTS 开发（Rust `moxin-tts-node`）
- zero-shot/few-shot 相关联调
- ASR + TTS 端到端数据流调试

---

## 8. 训练流程开关（Option A / Option B）

项目已支持通过环境变量切换 few-shot 训练后端：

```bash
# Option A: 旧 Python 训练服务（默认）
export MOXIN_TRAINING_BACKEND=option_a

# Option B: Rust/MLX 训练服务（实验路径）
export MOXIN_TRAINING_BACKEND=option_b
```

可选：显式指定 Option B 可执行文件路径（找不到二进制时使用）：

```bash
export MOXIN_FEWSHOT_TRAINER_BIN=/abs/path/to/target/release/moxin-fewshot-trainer
```

注意：

- Option B 当前要求任务有 `reference_text`（建议先等 ASR 文本回填后再启动训练）。
- Option A 仍是默认稳定路径；Option B 用于对照验证与迭代开发。
