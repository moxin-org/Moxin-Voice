# TTS 推理引擎迁移方案：从 dora-primespeech 到 OminiX-MLX

> ⚠️ 本文档已并入统一文档：`MLX_CORE_MIGRATION.md`。  
> 当前请优先参考：`/Users/alan0x/Documents/projects/moxin-tts/MLX_CORE_MIGRATION.md`。

> 初稿：2026-03-05 | 更新：2026-03-05（OminiX-MLX 本地代码完整分析）

---

## 背景

当前项目使用 `dora-primespeech`（基于 GPT-SoVITS，Python）作为 TTS 推理后端，`dora-asr`（基于 FunASR，Python）作为 ASR 后端。

**目标**：替换为 OminiX-MLX（纯 Rust + Apple MLX GPU 加速），实现：
- 零 Python 依赖
- Apple Silicon MLX 加速
- 保留语音克隆能力（Express/Pro Mode）

---

## 系统架构分析

### 当前 Dora 数据流拓扑

```
moxin-audio-input  →  dora-asr (Python/FunASR)  →  moxin-asr-listener
                                                          ↕
moxin-prompt-input → primespeech-tts (Python/GPT-SoVITS) → audio → moxin-audio-player
                                                          → segment_complete / log
```

### 目标架构（纯 Rust）

```
moxin-audio-input  →  moxin-asr-node (Rust/OminiX-MLX)  →  moxin-asr-listener
                                                                ↕
moxin-prompt-input → moxin-tts-node (Rust/OminiX-MLX)  → audio → moxin-audio-player
                                                          → segment_complete / log
```

### Dora Rust 节点机制

dora-rs 框架本身是纯 Rust 实现，Rust 是一等公民。节点就是编译好的 Rust 二进制文件，通过 `dora-node-api` crate 与 Dora 通信：

```toml
[dependencies]
dora-node-api = { version = "0.4" }
```

节点在 `tts.yml` 中声明为可执行文件路径，**无需 Python 运行时**。

### 接口契约（Rust 节点必须遵守）

| 方向 | 通道名 | 类型 | 必需字段 | 说明 |
|------|--------|------|----------|------|
| Input | `text` | Arrow string | — | 待合成文本或克隆协议 |
| Output | `audio` | float32 array | `sample_rate`（元数据）| 音频波形 |
| Output | `segment_complete` | string | — | `"completed"` / `"skipped"` / `"error"` |
| Output | `log` | JSON string | `node`, `level`, `message`, `timestamp` | 结构化日志 |

---

## OminiX-MLX 能力全景（本地代码分析）

OminiX-MLX 是 moxin-org 生态的**纯 Rust MLX 推理库**，包含 28 个 crate，涵盖：

### TTS 模块

| Crate | 引擎 | 输出采样率 | 语音克隆 | 性能（Apple Silicon） |
|-------|------|-----------|----------|-----------------------|
| `gpt-sovits-mlx` | GPT-SoVITS v2 | **32000 Hz** | ✅ Zero-shot + Few-shot | ~4x 实时 |
| `qwen3-tts-mlx` | Qwen3-TTS 1.7B | **24000 Hz** | ✅ x-vector + ICL | ~2.3x 实时 |

### ASR 模块

| Crate | 引擎 | 语言支持 | 性能 |
|-------|------|----------|------|
| `qwen3-asr-mlx` | Qwen3-ASR 1.7B | 30+ 语言 + 22 种中文方言 | ~30x 实时 |
| `funasr-mlx` | Paraformer-large | 仅中文 | ~18x 实时 |

---

## TTS 方案对比：gpt-sovits-mlx vs qwen3-tts-mlx

### gpt-sovits-mlx（推荐，直接替换）

**核心 API：**

```rust
use gpt_sovits_mlx::{VoiceCloner, VoiceClonerConfig, AudioOutput};

// 加载（使用默认路径 ~/.OminiX/models/gpt-sovits-mlx/）
let mut cloner = VoiceCloner::with_defaults()?;

// 设置参考音频（三种模式）
cloner.set_reference_audio("reference.wav")?;                          // Zero-shot
cloner.set_reference_audio_with_text("reference.wav", "参考文本")?;    // Few-shot（更好）

// 合成
let audio: AudioOutput = cloner.synthesize("你好，世界！")?;
// audio.samples: Vec<f32>，audio.sample_rate: 32000
```

**AudioOutput 结构：**
```rust
pub struct AudioOutput {
    pub samples: Vec<f32>,     // 音频样本，范围 [-1, 1]
    pub sample_rate: u32,      // 32000 Hz（与 dora-primespeech 一致！）
    pub duration: f32,         // 秒数
    pub num_tokens: usize,     // 生成的语义 tokens
}
```

**语音克隆模式：**

| 模式 | API | 质量 | 说明 |
|------|-----|------|------|
| Zero-shot | `set_reference_audio(path)` | ⭐⭐ | 仅参考频谱 |
| Few-shot | `set_reference_audio_with_text(path, text)` | ⭐⭐⭐ | 参考频谱 + 转录 |
| 预计算 | `set_reference_with_precomputed_codes(path, text, codes)` | ⭐⭐⭐ | 最佳，复用 codes |

**所需模型文件：**
```
~/.OminiX/models/gpt-sovits-mlx/
├── doubao_mixed_gpt_new.safetensors     # T2S 模型
├── doubao_mixed_sovits_new.safetensors  # VITS Vocoder
├── hubert.safetensors                    # CNHubert（Few-shot 需要）
├── bert.safetensors                      # 中文 BERT
└── chinese-roberta-tokenizer/
    └── tokenizer.json
```

**与 dora-primespeech 兼容性：**
- ✅ 同为 GPT-SoVITS 引擎，模型可复用
- ✅ 输出 32000 Hz，无需重采样，Rust 播放器零改动
- ✅ 支持与 dora-primespeech 相同的克隆能力（Zero-shot / Few-shot）

---

### qwen3-tts-mlx（更强大，但需适配）

**核心 API：**

```rust
use qwen3_tts_mlx::{Synthesizer, SynthesizeOptions};

let mut synth = Synthesizer::load("~/.OminiX/models/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit")?;

// 预置语音（CustomVoice 模型）
let opts = SynthesizeOptions { speaker: "vivian", language: "chinese", ..Default::default() };
let samples: Vec<f32> = synth.synthesize("你好，世界！", &opts)?;

// 语音克隆（Base 模型）—— x-vector 方式（推荐）
let (ref_samples, ref_sr) = load_wav("reference.wav")?;
let samples = synth.synthesize_voice_clone("目标文本", &ref_samples, "chinese", &opts)?;

// 流式输出
let mut session = synth.start_streaming("文本", &opts, 48 /* chunk_frames */)?;
while let Some(chunk) = session.next_chunk()? {
    // 处理每个音频块
}
```

**预置语音列表：** vivian, serena, ryan, aiden, eric, dylan, uncle_fu, ono_anna, sohee

**注意：**
- 输出 24000 Hz（与 dora-primespeech 的 32000 Hz 不同，需在 Rust 节点内重采样）
- 比 GPT-SoVITS 速度更快（2.3x vs 4x 实时）
- 支持更多语言，支持流式输出

---

## ASR 方案对比

### 当前：dora-asr（Python/FunASR）→ 替换为 funasr-mlx 或 qwen3-asr-mlx

| | funasr-mlx | qwen3-asr-mlx |
|-|------------|----------------|
| 语言 | 仅中文 | 30+ 语言 + 22 种中文方言 |
| 性能 | ~18x 实时 | ~30x 实时 |
| 模型大小 | 较小 | 2.46 GB（8-bit） |
| API | `Paraformer::transcribe(audio)` | `Qwen3ASR::transcribe_samples(samples, lang)` |
| 推荐场景 | 纯中文，资源有限 | 多语言，质量优先 |

**funasr-mlx API：**
```rust
use funasr_mlx::{load_model, parse_cmvn_file, Vocabulary};

let mut model = load_model("paraformer.safetensors")?;
let (addshift, rescale) = parse_cmvn_file("am.mvn")?;
model.set_cmvn(addshift, rescale);

let vocab = Vocabulary::load("tokens.txt")?;
let token_ids = model.transcribe(&audio_array)?;
let text = vocab.decode(&token_ids);
```

**qwen3-asr-mlx API：**
```rust
use qwen3_asr_mlx::{Qwen3ASR, default_model_path};

let mut asr = Qwen3ASR::load(default_model_path())?;
// 从原始样本（16kHz mono f32）转录
let text = asr.transcribe_samples(&samples_16khz, "Chinese")?;
```

---

## 迁移方案

### 方案一：gpt-sovits-mlx（最小改动，推荐优先验证）

**适合场景：** 保留 GPT-SoVITS 引擎，获得 MLX 加速，维持完整克隆能力。

**改动范围：**

1. 新建 Rust Dora 节点 `node-hub/moxin-tts-node/`
2. 修改 `tts.yml` 替换 Python 节点为 Rust 二进制
3. 可选：同步替换 ASR 节点

**节点结构：**
```
node-hub/moxin-tts-node/
├── Cargo.toml
└── src/
    └── main.rs
```

**Cargo.toml：**
```toml
[package]
name = "moxin-tts-node"
version = "0.1.0"
edition = "2021"

[[bin]]
name = "moxin-tts-node"
path = "src/main.rs"

[dependencies]
dora-node-api = { version = "0.4" }
gpt-sovits-mlx = { path = "../../OminiX-MLX/gpt-sovits-mlx" }
mlx-rs-core = { path = "../../OminiX-MLX/mlx-rs-core" }
eyre = "0.6"
serde_json = "1"
```

**main.rs 核心逻辑：**
```rust
use dora_node_api::{DoraNode, Event, IntoArrow};
use gpt_sovits_mlx::VoiceCloner;
use std::time::SystemTime;

fn main() -> eyre::Result<()> {
    let (mut node, mut events) = DoraNode::init_from_env()?;

    // 初始化 GPT-SoVITS（从环境变量读取模型路径）
    let mut cloner = VoiceCloner::with_defaults()?;

    while let Some(event) = events.recv() {
        match event {
            Event::Input { id, metadata, data } if id.as_str() == "text" => {
                let text = data.as_any().downcast_ref::<arrow2::array::Utf8Array<i32>>()
                    .and_then(|a| a.get(0))
                    .unwrap_or("")
                    .to_string();

                if text.trim().is_empty() {
                    node.send_output("segment_complete", metadata.parameters.clone(),
                        "skipped".into_arrow())?;
                    continue;
                }

                // 解析克隆协议（与 dora-primespeech 兼容）
                // VOICE:CUSTOM|<ref_audio>|<prompt_text>|<lang>
                // VOICE:TRAINED|<gpt>|<sovits>|<ref>|<prompt>|<lang>|<text>
                // 其余视为普通文本合成

                let audio = cloner.synthesize(&text)?;

                // 发送音频（含 sample_rate 元数据）
                node.send_output(
                    "audio",
                    {
                        let mut p = metadata.parameters.clone();
                        p.insert("sample_rate".into(), audio.sample_rate.into());
                        p
                    },
                    audio.samples.into_arrow(),
                )?;
                node.send_output("segment_complete", metadata.parameters,
                    "completed".into_arrow())?;
            }
            Event::Stop(_) => break,
            _ => {}
        }
    }
    Ok(())
}
```

**tts.yml 修改（仅改这两行）：**
```yaml
- id: primespeech-tts                  # 保持 id 不变！
  path: moxin-tts-node                 # Rust 编译产物
  inputs:
    text: moxin-prompt-input/control
  outputs:
    - audio
    - segment_complete
    - log
  env:
    GPT_SOVITS_MODEL_DIR: "$HOME/.OminiX/models/gpt-sovits-mlx"
    LANGUAGE: "zh"
```

---

### 方案二：qwen3-tts-mlx（更强性能，需重采样）

**适合场景：** 追求更高性能、多语言、流式输出。需要处理 24000→32000 Hz 重采样。

**关键差异：** 在 Rust 节点内重采样输出音频后，`sample_rate` 元数据填 32000，播放器无感知。

---

### 方案三：完整替换（TTS + ASR，全纯 Rust）

同时替换 `dora-asr`（Python）→ `moxin-asr-node`（Rust/`qwen3-asr-mlx`）：

```yaml
# tts.yml 中 ASR 节点
- id: dora-asr                          # 保持 id 不变
  path: moxin-asr-node                  # Rust 编译产物
  inputs:
    audio: moxin-audio-input/audio
  outputs:
    - text
  env:
    QWEN3_ASR_MODEL_PATH: "$HOME/.OminiX/models/qwen3-asr-1.7b"
    LANGUAGE: "Chinese"
```

---

## 关键技术问题

### 1. 采样率兼容性

| 方案 | TTS 输出 | 是否需要重采样 |
|------|---------|---------------|
| gpt-sovits-mlx | 32000 Hz | ❌ 无需，与现有播放器完全兼容 |
| qwen3-tts-mlx | 24000 Hz | ✅ 需在节点内重采样至 32000 Hz |

`mlx-rs-core` 提供 `audio::resample()` 函数可直接使用。

### 2. 语音克隆协议适配

现有 `dora-primespeech` 的克隆协议（通过 text 通道传递）：
```
VOICE:CUSTOM|<ref_audio>|<prompt_text>|<lang>         # Express Mode
VOICE:TRAINED|<gpt>|<sovits>|<ref>|<prompt>|<lang>|<text>  # Pro Mode
```

Rust 节点需要解析这些协议，调用对应的 `VoiceCloner` API：
- `VOICE:CUSTOM` → `set_reference_audio_with_text(ref_audio, prompt_text)` + `synthesize(text)`
- `VOICE:TRAINED` → `VoiceClonerConfig` 中指定自定义模型路径 + `synthesize(text)`

### 3. 节点 ID 不变原则

`tts.yml` 中节点 `id` 保持 `primespeech-tts`，下游 `moxin-audio-player` 引用 `primespeech-tts/audio` 无需任何改动。

### 4. 模型路径

OminiX-MLX 默认读取 `~/.OminiX/models/` 目录（通过环境变量可覆盖），需与现有 `~/.dora/models/` 区分。

---

## 快速决策

```
需要语音克隆（Express/Pro Mode）？
  └── 是
      ├── 优先验证 → 方案一（gpt-sovits-mlx）
      │   ✅ 同引擎，32000 Hz，零适配，最小风险
      │
      └── 追求更高性能/多语言 → 方案二（qwen3-tts-mlx）
          ⚠️ 需处理 24000 Hz 重采样 + 克隆协议适配

是否同时替换 ASR？
  ├── 中文为主 → funasr-mlx（更轻量）
  └── 多语言 → qwen3-asr-mlx（更强，30+ 语言）
```

**建议实施顺序：**
1. 先用 `gpt-sovits-mlx` 实现 TTS 节点，验证端到端流程
2. 确认克隆协议解析正确后，再考虑迁移 ASR 节点
3. 待稳定后，评估是否升级到 `qwen3-tts-mlx`

---

## 相关文件

| 文件 | 说明 |
|------|------|
| `apps/moxin-voice/dataflow/tts.yml` | Dora 数据流配置 |
| `node-hub/dora-primespeech/` | 当前 Python TTS 节点 |
| `node-hub/dora-asr/` | 当前 Python ASR 节点 |
| `node-hub/moxin-tts-node/`（待创建）| Rust TTS 节点（gpt-sovits-mlx） |
| `node-hub/moxin-asr-node/`（待创建）| Rust ASR 节点（qwen3-asr-mlx） |
| `/Users/alan0x/Documents/projects/OminiX-MLX/gpt-sovits-mlx/` | GPT-SoVITS Rust 实现 |
| `/Users/alan0x/Documents/projects/OminiX-MLX/qwen3-tts-mlx/` | Qwen3-TTS Rust 实现 |
| `/Users/alan0x/Documents/projects/OminiX-MLX/qwen3-asr-mlx/` | Qwen3-ASR Rust 实现 |
| `/Users/alan0x/Documents/projects/OminiX-MLX/funasr-mlx/` | FunASR Rust 实现 |
