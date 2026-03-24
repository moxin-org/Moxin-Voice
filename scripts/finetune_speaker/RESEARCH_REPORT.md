# Speaker Embedding Fine-tune 调研报告

**日期**: 2026-03-24
**调研人**: alan0x
**结论**: 当前方案（embedding only）不值得继续投入；官方 SFT / QLoRA 路径可行但成本更高

---

## 背景与目标

目标：为 Moxin Voice 新增自定义说话人，达到**内置音色的质量 + 内置音色的速度**。

内置音色（CustomVoice 模型的 9 个预置音色）的推理路径：
- `codec_embedding[spk_id]` → 固定 [2048] 向量注入到 codec prefix pos 7
- 无需参考音频，无需 speaker encoder 前向计算
- 速度最快，质量稳定

---

## 方案一：Speaker Embedding 训练（已验证，放弃）

### 描述

在冻结量化模型（`Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit`）的前提下，只训练
`codec_embedding` 表中新增的一行 [2048] 向量，以此代表新说话人。

训练完成后通过两种方式使用：
1. **Patch 方式**：将训练好的向量写回 `model.safetensors` 对应行
2. **NPZ 注入方式**：保存为外部 `.npz` 文件，推理时动态注入

### 关键发现

#### 发现 1：Patch 方式会破坏量化模型

`apply_checkpoint.py` 和 `train.py --patch_model` 的操作：

```
mx.load(model.safetensors)        # 读取全部权重，含 uint32 量化张量
修改 codec_embedding.weight 一行  # 目标操作
mx.save_safetensors(全部权重)     # 重写整个文件  ← 问题所在
```

`mx.save_safetensors()` 重建 safetensors header，对量化层（uint32 packed weights +
bfloat16 scales/biases）的 round-trip 不完全可靠，导致量化 transformer 层被破坏。

**症状**：patch 后所有音色（包括原始内置音色）输出全为噪音。

`codec_embedding` 本身未量化（Rust 加载代码明确注释 "NOT quantized"），所以问题
不在于"量化模型不能训练"，而在于**重写整个 safetensors 文件的副作用**。

#### 发现 2：NPZ 注入方式可用

将训练向量保存为 `.npz`，推理时注入到 pos 7，输出为正常语音，无噪音，模型完好。
（默认 safe mode 已实现；Rust 侧注入曾在另一台机器实现但未提交。）

#### 发现 3：训练未有效收敛

| 项目 | 数值 |
|------|------|
| 训练数据 | 35 句自录音频，约 4 分钟 |
| 训练轮数 | 20 epochs |
| 初始 loss | ~9.x |
| 最终 loss | ~8.6 |
| 随机基线 | ln(3072) ≈ 8.03 |

loss 8.6 仍接近随机水平。**实际听感**：输出为正常语音，但音色不像目标说话人，
退化为某种"平均音色"。

#### 发现 4：方案存在根本性架构限制

CustomVoice 模型的 9 个内置音色，其 embedding 是在**模型整体预训练过程中与
transformer 权重共同学习**的。本方案将 transformer 完全冻结，只训练一个新向量，
要求模型泛化到从未见过的 embedding，存在无法通过增加数据或训练轮数解决的上限。

### 结论

**放弃。** 理由：
- 质量无法达到目标（架构限制，不只是数据问题）
- 速度优势相比 x-vector BundledIcl 不显著
- 已有可用的 x-vector BundledIcl 方案（yangyang/baiyang）

---

## 方案二：官方 SFT（调研，未实施）

### 官方方案描述

Qwen3-TTS 官方仓库提供了微调脚本：
`https://github.com/QwenLM/Qwen3-TTS/tree/main/finetuning`

关键文件：`sft_12hz.py`、`prepare_data.py`、`dataset.py`

**与方案一的根本差异**：

| 维度 | 方案一（我们做的）| 官方 SFT |
|------|-----------------|---------|
| 训练目标 | 只训练一个 [2048] embedding 向量 | `AdamW(model.parameters())` 全模型参数 |
| 模型 | CustomVoice-8bit（量化）| Base 模型（bfloat16 全精度）|
| 框架 | MLX | PyTorch + Accelerate |

这解释了为什么方案一训练不收敛：transformer 冻结，无法适应新音色。

### 硬件要求

**1.7B 模型在不同硬件上的可行性**：

| 硬件 | 显存/内存 | bfloat16 支持 | 可行性 |
|------|----------|--------------|--------|
| Mac Mini M4（16GB 统一内存）| 16 GB | 原生支持 | 1.7B 极度紧张，0.6B 可行 |
| GTX 1080 Ti（12GB VRAM）| 12 GB VRAM | **不支持**（退回 float32）| 1.7B OOM，0.6B 可行 |
| RTX 3060 及以上 | ≥12 GB VRAM | 支持 | 0.6B 舒适，1.7B 视显存而定 |

1.7B 模型全量训练内存估算（含梯度 + AdamW 优化器状态）：
- bfloat16 原生：~13.6 GB
- float32 回退（如 1080 Ti）：~24 GB

**1080 Ti 上可以跑 0.6B 模型**，但需将 `torch_dtype=bfloat16` 改为 `float16`。

### 质量与速度预期

- **质量**：全模型参数均参与更新，理论上可达到接近内置音色的水平
- **推理**：生成时不需要参考音频（模型已"记住"目标音色）
- **速度**：和 Base 模型推理速度相同

### 结论

技术上可行，但需要额外环境（PyTorch、全精度 Base 模型）。**暂不实施**，作为
未来备选路径保留。

---

## 方案三：QLoRA（调研，未实施）

### 描述

QLoRA = 量化基础模型（冻结）+ LoRA adapter（小矩阵，可训练）。

LoRA 原理：不修改原始权重 W，在旁边插入两个低秩矩阵 A × B，只训练这两个小矩阵。
例如 2048×2048 权重矩阵有 4M 参数，rank=8 的 LoRA 只需训练 32,768 个参数（0.8%）。

相比官方 SFT：
- 可在 8-bit 量化 Base 模型上训练（量化层冻结，adapter 全精度）
- 显存需求大幅降低
- 产出：原始量化模型 + 小 adapter 文件（数十 MB）

### 推理方式

| 方式 | 速度 | 代价 |
|------|------|------|
| Merged（adapter 合并回模型）| 与原始模型相同 | 每个说话人需一份合并后模型 |
| Unmerged（实时叠加）| 略慢 | 一份模型 + 多个小 adapter |

### 质量预期

高于方案一（embedding only），可能接近全量 SFT，但 Qwen3-TTS 上无实测数据。

### 现状

官方 `sft_12hz.py` **不支持 QLoRA**，需自行用 `peft` 库改造。无现成方案，
需要额外开发工作。**暂不实施。**

---

## 各方案综合对比

| 维度 | Embedding 训练 | 官方 SFT | QLoRA | X-vector BundledIcl |
|------|---------------|---------|-------|---------------------|
| 音色质量 | 差（不像目标）| 最好 | 较好（未验证）| 合理 |
| 推理速度 | 等同内置音色 | 等同内置音色 | 接近内置音色 | 略慢 |
| 需要参考音频（推理）| 否 | 否 | 否 | 是 |
| 所需硬件 | Mac M4 可跑 | RTX 30 系以上或 0.6B | RTX 20 系以上 | Mac M4 可跑 |
| 工程复杂度 | 中 | 中（官方脚本）| 高（需改造）| 低（已上线）|
| 方案成熟度 | 已验证，失败 | 官方支持 | 无 TTS 实例 | 稳定可用 |

---

## 遗留代码状态

| 文件 | 状态 | 说明 |
|------|------|------|
| `train.py` | 可用，默认 safe mode | 产出 `.npz`，不修改模型 |
| `apply_checkpoint.py` | **危险，不应使用** | 重写 safetensors 会破坏量化模型 |
| `encode_audio.py` | 可用 | 音频编码为 codec frames |
| `register_speaker.py` | 无用 | 依赖已废弃的 patch 流程 |
| Rust 侧 npz 注入 | **未提交** | 昨天在另一台机器实现，未合并 |

`apply_checkpoint.py` 的 `--allow_patch_quantized` 参数应视为无效，
不应在 8-bit 量化模型上使用。
