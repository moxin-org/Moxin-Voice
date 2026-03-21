# IndexTTS 接入 Moxin Voice 可行性研究

> 创建日期：2026-03-21
> 研究目标：评估将 IndexTTS 作为第二 TTS 后端接入当前项目的可行性，并与 Qwen3-TTS 进行对比分析

---

## 一、IndexTTS 概述

IndexTTS 是 Bilibili 开源的工业级零样本语音克隆系统（Apache 2.0），发布于 2025 年 2 月。其核心优势是：

- 零样本音色克隆**无需参考文本**（Qwen3-TTS ICL 模式需要）
- 中文优化（字符+拼音混合建模，解决多音字）
- 已在与 CosyVoice2、Fish-Speech、F5-TTS 的对比中取得 SOTA 性能

当前最新稳定版：**IndexTTS-1.5**（2025-05），下一代 **IndexTTS2** 已于 2025-09 开源（增加情绪控制、时长控制）。

---

## 二、IndexTTS 技术架构

### 2.1 核心组件（三段式流水线）

```
参考音频 WAV
    │
    ▼
┌─────────────────────────────┐
│  Conformer 说话人编码器       │  → 将参考音频压缩为 1~32 个说话人潜变量
│  (Perceiver-style, 无需文本) │    用作 GPT 的条件向量
└─────────────────────────────┘
    │
    ▼ 说话人条件向量
┌─────────────────────────────┐
│  BPE 文本 Tokenizer          │  → 12,000 词表（8,400 汉字+拼音+英文pieces）
│  （SentencePiece）           │
└─────────────────────────────┘
    │
    ▼ 文本 token + 说话人条件
┌─────────────────────────────┐
│  GPT Decoder-only LM         │  → 自回归生成 FSQ codec token 序列
│  （Transformer + Conformer   │    25 Hz，codebook 大小 ~8192
│    条件注入）                 │
└─────────────────────────────┘
    │
    ▼ codec token 序列
┌─────────────────────────────┐
│  DVAE / FSQ 解码器           │  → codec token → 声学特征（100 Hz latent）
└─────────────────────────────┘
    │
    ▼
┌─────────────────────────────┐
│  BigVGAN2 声码器             │  → 声学特征 → 24kHz PCM 波形
└─────────────────────────────┘
```

### 2.2 关键参数对比（IndexTTS vs Qwen3-TTS）

| 参数 | IndexTTS-1.5 | Qwen3-TTS (CustomVoice) |
|------|-------------|------------------------|
| 架构基础 | GPT（XTTS/Tortoise 系） | Qwen2.5 LLM 派生 |
| 参数量 | ~未公开（估计 ~1B） | 1.7B |
| Codec | FSQ，25 Hz，~8192 码本 | RVQ 16 codebook，12.5 Hz，2048 |
| 声码器 | BigVGAN2 | SEANet decoder |
| 说话人编码 | Conformer Perceiver（无需参考文本） | ECAPA-TDNN（ICL 模式需要参考文本） |
| 输出采样率 | 24 kHz | 24 kHz |
| 预设音色 | 无（纯零样本） | 9 个内置音色 |
| 中文优化 | 专项（字符+拼音混合） | 通用多语言 |
| 训练数据量 | 34,000 小时（中英） | 未公开 |
| 推理框架 | PyTorch（CUDA/MPS/CPU） | MLX（Apple Silicon 原生） |
| 开源协议 | Apache 2.0 | Apache 2.0 |

### 2.3 推理质量基准（IndexTTS-1.5，seed-tts-eval 数据集）

| 指标 | IndexTTS-1.5 | CosyVoice2 | Fish-Speech | XTTS |
|------|-------------|-----------|------------|------|
| MOS 音色相似度 | 4.20 | 3.97 | 3.86 | 3.24 |
| MOS 整体质量 | 4.01 | 3.81 | 3.72 | 3.11 |
| 说话人相似度 (SS) | 0.771 | 0.742 | 0.698 | 0.601 |
| 中文 WER | 0.821% | — | — | — |
| 英文 WER | 1.606% | — | — | — |

> 注：Qwen3-TTS 无公开 MOS 基准数据，因此直接数字对比不可靠，需实测。

---

## 三、Checkpoint 文件结构

IndexTTS-1.5 需要以下文件（`checkpoints/` 目录）：

```
checkpoints/
├── config.yaml              # 主配置
├── gpt.pth                  # GPT LM 权重（最大，估计 ~2-3 GB）
├── dvae.pth                 # DVAE/FSQ codec 编解码器
├── bigvgan_generator.pth    # BigVGAN2 生成器
├── bigvgan_discriminator.pth # 鉴别器（推理不需要，可忽略）
├── bpe.model                # SentencePiece 分词器
└── unigram_12000.vocab      # 词表
```

HuggingFace: `IndexTeam/Index-TTS`

---

## 四、接入需求分析

### 4.1 当前项目 dora 节点接口

现有 `dora-qwen3-tts-mlx` 节点的接口规范：

**输入：**
```
text (StringArray) — VOICE:* 协议消息
```

**协议格式：**
```
VOICE:<name>|<text>                          # 预设音色
VOICE:CUSTOM|<ref_wav>|<ref_text>|<lang>|<text>  # 零样本克隆
VOICE:TRAINED|<gpt>|<sovits>|<ref>|...|<text>    # 训练音色
```

**输出：**
```
audio           (Float32Array, 24kHz PCM, metadata: sample_rate)
status          (StringArray)
segment_complete (Float32Array, empty signal)
log             (StringArray)
```

**关键约束：**
- 输出必须是 24kHz Float32 PCM
- 协议格式需与现有 Rust 端 `screen.rs` 中的消息构建代码兼容
- 节点 ID 在 `tts.yml` 中固定为 `primespeech-tts`（已重命名为 `primespeech-tts` 保持兼容性）

### 4.2 IndexTTS 的协议映射

IndexTTS 没有预设音色（纯零样本），因此只能映射到 `VOICE:CUSTOM` 协议：

```
VOICE:CUSTOM|<ref_wav>|<ref_text_可选>|<lang>|<text>
    → tts.infer(audio_prompt=ref_wav, text=text, output_path=...)
```

**优势**：IndexTTS 的 Conformer 编码器**不需要参考文本**，`ref_text` 字段留空也能正常工作。

---

## 五、接入方案分析

### 方案 A：原生 MLX 移植（Rust + MLX）

将 IndexTTS 全部组件重写为 MLX Rust 实现，与现有 `qwen3-tts-mlx` 库同级。

**需要移植的组件：**
1. FSQ DVAE 编码器（Finite Scalar Quantization）
2. GPT Decoder with Conformer conditioning
3. BigVGAN2 声码器（复杂 GAN，带 anti-aliased 多周期性滤波器）
4. SentencePiece BPE 分词器

**评估：**

| 维度 | 评价 |
|------|------|
| 技术可行性 | 理论可行，BigVGAN2 在 MLX 中无先例 |
| 工程量 | 极高（3–6 个月专职 ML 工程师）|
| 推理速度 | 最优（原生 MLX，Apple Silicon 专用） |
| 维护成本 | 高，需跟随 IndexTTS 上游更新 |
| 优先级建议 | 长期目标，当前不可行 |

**结论：当前阶段不建议。**

---

### 方案 B：Python PyTorch/MPS dora 节点（推荐）

仿照现有 `dora-primespeech`（Python dora 节点），创建 `dora-indextts` Python 节点，使用官方 `indextts` 包 + MPS 加速推理。

**节点结构：**

```
node-hub/dora-indextts/
├── dora_indextts/
│   └── main.py        # dora 节点主程序
├── pyproject.toml
└── requirements.txt
```

**main.py 核心逻辑：**

```python
import dora
from indextts.infer import IndexTTS
import soundfile as sf
import numpy as np

tts = IndexTTS(model_dir="~/.OminiX/models/indextts", device="mps")

for event in node:
    if event["type"] == "INPUT" and event["id"] == "text":
        msg = event["value"][0].as_py()
        # 解析 VOICE:CUSTOM|ref|ref_text|lang|text
        request = parse_protocol(msg)
        samples = tts.infer(audio_prompt=request.ref_wav, text=request.text)
        node.send_output("audio", pa.array(samples, type=pa.float32()),
                         {"sample_rate": 24000})
        node.send_output("segment_complete", pa.array([], type=pa.float32()))
```

**评估：**

| 维度 | 评价 |
|------|------|
| 技术可行性 | 高，`pip install indextts` 即可 |
| Apple Silicon 支持 | MPS（PyTorch Metal），非原生 MLX |
| 工程量 | 低（1–2 天）|
| 推理速度 | 中等（MPS 比 MLX 慢，估计 RTF ~2–5×）|
| 与现有接口兼容 | 完全兼容，协议层不变 |
| 模型存储 | `~/.OminiX/models/indextts/` 统一管理 |
| 维护成本 | 低（跟随官方 PyPI 包更新）|

**结论：当前最可行的方案。**

---

### 方案 C：Python 进程桥接（折中）

在 Rust dora 节点中启动 Python 子进程处理 IndexTTS 推理，通过 stdin/stdout 通信。避免引入新的 Python dora 节点，但架构更复杂。

**评估：** 工程复杂度高于方案 B，优势不明显。**不推荐。**

---

## 六、OminiX 生态兼容性分析

当前项目的 OminiX 模型管理约定：
- 模型存储于 `~/.OminiX/models/<framework>/<model-name>/`
- Qwen3-TTS 路径：`~/.OminiX/models/qwen3-tts-mlx/`
- IndexTTS 建议路径：`~/.OminiX/models/indextts/`

**问题核心**：IndexTTS **没有官方 MLX 移植**，也没有 `mlx-community` 量化版本。

这意味着"使用 OminiMLX 进行推理"这一需求，在 IndexTTS 上**当前无法满足**——除非：
1. 自行将 BigVGAN2 + GPT 移植到 MLX（方案 A，工程量极大）
2. 接受以 MPS（PyTorch Metal）作为 Apple Silicon 加速路径的替代

方案 B 使用 MPS，在 OminiX 模型存储约定下工作，功能上可与现有后端切换，但推理框架与 MLX 不同。如果"OminiMLX"是一个硬性需求，IndexTTS 接入需要等待社区出现 MLX 移植版本。

---

## 七、Qwen3-TTS vs IndexTTS 全面对比

| 对比维度 | Qwen3-TTS（当前） | IndexTTS（候选） |
|---------|-----------------|----------------|
| **推理框架** | MLX（Apple Silicon 原生） | PyTorch / MPS |
| **推理速度（RTF）** | 内置音色 0.6–1.2×，ICL 5–10× | 估计 MPS ~2–5×（未实测） |
| **克隆需要参考文本** | 是（ICL 模式） | **否**（Conformer 直接提取） |
| **预设内置音色** | 9 个（CustomVoice） | **无** |
| **中文质量** | 良好 | 优秀（专项优化，字符+拼音） |
| **英文质量** | 良好 | 良好 |
| **克隆 MOS（公开）** | 无公开数据 | 4.01 |
| **模型大小** | ~1.7 GB（8-bit） | ~4–5 GB（未量化） |
| **情绪控制** | 无 | IndexTTS2 支持 8 种情绪 |
| **时长控制** | 无 | IndexTTS2 支持 |
| **开源协议** | Apache 2.0 | Apache 2.0 |
| **社区活跃度** | Alibaba/Qwen | Bilibili，更新活跃 |
| **接入工程量** | 已完成 | 低（方案 B） |

### 关键权衡

**选择 IndexTTS 的理由：**
- 克隆无需参考文本，用户体验更好（Express Mode 不用做 ASR）
- 中文发音更稳定（拼音控制）
- 更高的音色相似度 MOS
- IndexTTS2 的情绪/时长控制是独特能力

**留在 Qwen3-TTS 的理由：**
- 原生 MLX，推理速度快（内置音色 RTF < 1.2×）
- 9 个高质量内置音色，零延迟切换
- 无 Python 依赖，纯 Rust 运行
- Apple Silicon 优化最充分

---

## 八、接入实施计划（方案 B）

### 阶段 1：验证（1–2 天）

1. 下载 IndexTTS-1.5 模型到 `~/.OminiX/models/indextts/`
2. 用 `pip install indextts` + MPS 跑基础推理，测 RTF 和克隆质量
3. 对比白杨/杨阳参考音频的克隆效果：IndexTTS vs Qwen3-TTS ICL

```bash
python3 -c "
from indextts.infer import IndexTTS
tts = IndexTTS(model_dir='~/.OminiX/models/indextts', device='mps')
tts.infer(
    audio_prompt='node-hub/dora-qwen3-tts-mlx/voices/baiyang/ref.wav',
    text='人工智能正在改变我们生活的方方面面，从医疗到教育，无处不在。',
    output_path='/tmp/indextts_baiyang.wav'
)
"
```

### 阶段 2：dora 节点实现（2–3 天）

1. 创建 `node-hub/dora-indextts/` Python 节点
2. 实现 `VOICE:CUSTOM` 协议解析
3. 实现 `audio`、`status`、`segment_complete`、`log` 输出
4. 添加 `~/.OminiX/models/indextts/` 模型路径检测和下载脚本

### 阶段 3：后端切换集成（2–3 天）

1. 新增 `tts.yml` 的 IndexTTS 变体（`tts_indextts.yml`）
2. 在 Rust 端 `screen.rs` 添加后端选择逻辑（或复用现有 `MOXIN_INFERENCE_BACKEND` 环境变量）
3. 测试完整流程：文本输入 → 音频播放

### 阶段 4：质量对比实测

使用统一测试集（中英文各 10 条）对比：
- IndexTTS MPS vs Qwen3-TTS ICL（速度、音色相似度、自然度）
- 主观评分

---

## 九、结论与建议

### 可行性评级：**中高**（方案 B）

IndexTTS 接入在技术上完全可行，核心工作量约 **1 周**（验证 + 节点实现 + 集成），不需要修改现有 Qwen3-TTS 路径。

### 主要风险

| 风险 | 概率 | 影响 | 缓解 |
|------|------|------|------|
| MPS 推理速度不可接受 | 中 | 高 | 阶段 1 先实测 RTF |
| IndexTTS Python 环境与现有 conda 冲突 | 低 | 中 | 独立 venv |
| BigVGAN2 在 MPS 上有 bug | 低 | 中 | 官方有 CPU fallback |
| 克隆质量不如预期 | 低 | 中 | MOS 数据显示质量领先 |

### 建议行动

1. **立即可做**：阶段 1 验证（安装 + 跑一条语音），成本极低，可在 30 分钟内得到初步结论
2. **如果质量/速度满意**：推进方案 B 实现，作为可选后端，与 Qwen3-TTS 并行存在
3. **不建议**：现阶段不做 MLX 原生移植（工程量过大，优先级低）
4. **关注**：`mlx-community` 是否会发布 IndexTTS MLX 版本——一旦出现，可升级为方案 A

---

## 参考资料

- [IndexTTS GitHub](https://github.com/index-tts/index-tts)
- [IndexTTS 论文 arXiv:2502.05512](https://arxiv.org/abs/2502.05512)
- [IndexTTS2 论文 arXiv:2506.21619](https://arxiv.org/abs/2506.21619)
- [HuggingFace: IndexTeam/Index-TTS](https://huggingface.co/IndexTeam/Index-TTS)
- [IndexTTS Demo](https://index-tts.github.io/)
