# Moxin Voice 三个推理/训练节点现状总结（2026-03）

> 本文基于当前仓库代码现状整理，重点覆盖：
> 1. 架构、依赖模型、推理框架
> 2. TTS / Voice Clone（zero-shot / few-shot / trained）支持情况
>
> 相关代码来源：
> - `node-hub/dora-primespeech`
> - `node-hub/dora-primespeech-mlx`
> - `node-hub/dora-qwen3-tts-mlx`
> - `apps/moxin-voice/src/training_manager.rs`
> - `scripts/run_tts_backend.sh`, `scripts/macos_run_tts_backend.sh`

---

## 1. 节点定位总览

| 节点 | 技术栈 | 在当前项目中的主要职责 | 当前是否用于主推理数据流 |
|---|---|---|---|
| `dora-primespeech` | Python + PyTorch + MoYoYo/GPT-SoVITS 生态 | 旧链路节点；当前仍用于 Option A few-shot 训练服务（`python -m dora_primespeech.moyoyo_tts.training_service`） | 否（主推理已切 MLX 路由） |
| `dora-primespeech-mlx` | Rust + MLX（`gpt-sovits-mlx`） | PrimeSpeech MLX 推理节点 + Rust few-shot 训练器（Option B） | 是（`primespeech_mlx`） |
| `dora-qwen3-tts-mlx` | Rust + MLX（`qwen3-tts-mlx`） | Qwen3 TTS MLX 推理节点（预置音色 + zero-shot clone） | 是（`qwen3_tts_mlx`） |

补充：当前 TTS 数据流通过 `run_tts_backend.sh` / `macos_run_tts_backend.sh` 做后端路由，主推理后端只在 `primespeech_mlx` 与 `qwen3_tts_mlx` 间切换。

---

## 2. dora-primespeech（Python）

## 2.1 架构与框架

- 语言/运行时：Python。
- 推理框架：PyTorch 生态（`torch`, `torchaudio`, `transformers` 等）。
- 节点入口：`dora_primespeech.main`（`pyproject.toml` 中 `dora-primespeech = "dora_primespeech.main:main"`）。
- 当前项目内的关键实际用途：
  - Option A few-shot 训练服务由 `TrainingManager` 直接启动：
    - `python -m dora_primespeech.moyoyo_tts.training_service`
  - 该训练服务执行完整 Python 训练流水线（切片、降噪、ASR、特征提取、GPT 训练、SoVITS 训练、导出）。

## 2.2 依赖模型

- 主模型目录：`~/.dora/models/primespeech`（可由 `PRIMESPEECH_MODEL_DIR` 覆盖）。
- 训练服务依赖：
  - GPT 预训练权重（例如 `s1bert25hz-5kh-longer-epoch=12-step=369668.ckpt`）
  - SoVITS 预训练权重（例如 `s2G2333k.pth` / `s2D2333k.pth`）
  - HuBERT、ASR（FunASR）等前处理模型。

## 2.3 TTS / Voice Clone 支持情况

- 预置音色 TTS：支持（Python 节点自身支持）。
- Zero-shot clone：支持（`VOICE:CUSTOM|...` 协议在节点侧可处理）。
- Few-shot 训练：支持（Option A，Python 训练服务）。
- Trained voice 推理：支持（训练产物可用于 `VOICE:TRAINED` 语义链路）。

## 2.4 在“当前项目主流程”中的状态

- 不是主推理后端（主推理已切到 MLX 路由）。
- 仍是生产可用的训练后端（Option A）。
- 与 `dora-asr` 搭配用于 reference text 自动回填的流程仍兼容。

---

## 3. dora-primespeech-mlx（Rust/MLX）

## 3.1 架构与框架

- 语言/运行时：Rust。
- 推理框架：MLX（`gpt-sovits-mlx` + `mlx-rs`）。
- crate：`node-hub/dora-primespeech-mlx/Cargo.toml`
- 二进制：
  - `moxin-tts-node`：推理节点。
  - `moxin-fewshot-trainer`：Rust few-shot 训练器（Option B）。

推理入口逻辑（`src/main.rs`）：
- 接收 Dora `text` 输入，支持协议：
  - `VOICE:<preset>|<text>`
  - `VOICE:CUSTOM|<ref_wav>|<prompt_text>|<lang>|<text>`
  - `VOICE:TRAINED|<gpt>|<sovits>|<ref_wav>|<prompt_text>|<lang>|<text>`
- 输出 `audio`（32kHz）、`status`、`segment_complete`、`log`。
- 支持运行时参数包装解析（speed/pitch/volume）并做后处理。

## 3.2 依赖模型

- 依赖 `gpt-sovits-mlx`（当前为本地 patch 路径）。
- 模型基础目录通常为 `~/.OminiX/models/gpt-sovits-mlx`。
- 预置音色依赖 `voices/<VoiceName>/gpt.safetensors`、`sovits.safetensors`、参考音频及 prompt。
- few-shot 高质量路径可用 `prompt_semantic.npy`（有则走更完整 few-shot 条件；无则 fallback zero-shot）。

## 3.3 TTS / Voice Clone 支持情况

- 预置音色 TTS：支持。
- Zero-shot clone（`VOICE:CUSTOM`）：支持。
- Trained voice 推理（`VOICE:TRAINED`）：支持。
- Few-shot 训练：
  - Option B（Rust/MLX）支持，由 `moxin-fewshot-trainer` 提供。
  - 训练请求要求 `reference_text`（训练器显式校验）。

## 3.4 当前已知特征/边界

- 这是当前项目“PrimeSpeech MLX 主后端”。
- 训练与推理都在同一后端目录内，符合“一个后端一个节点”的结构目标。
- Option A 训练仍可并行保留（由 `dora-primespeech` 承担）。

---

## 4. dora-qwen3-tts-mlx（Rust/MLX）

## 4.1 架构与框架

- 语言/运行时：Rust。
- 推理框架：MLX（`qwen3-tts-mlx`，当前项目内 path patch 版本）。
- crate：`node-hub/dora-qwen3-tts-mlx/Cargo.toml`
- 二进制：`qwen-tts-node`。

推理入口逻辑（`src/main.rs`）：
- 同样解析 `VOICE:*` 协议，便于与上层统一。
- `Preset`：走 Qwen CustomVoice 模型（预置 speaker）。
- `Custom`：走 Qwen Base 模型做 voice clone（优先 ICL；失败时 fallback x-vector）。
- `Trained`：明确返回不支持（当前代码直接报错）。
- 输出音频采样率为 24kHz（节点输出 metadata 标记 24000）。

## 4.2 依赖模型

- Qwen 根目录：`~/.OminiX/models/qwen3-tts-mlx`（可环境变量覆盖）。
- CustomVoice 模型：
  - `Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit`
  - 用于预置 speaker TTS。
- Base 模型：
  - `Qwen3-TTS-12Hz-1.7B-Base`
  - 用于 zero-shot clone（x-vector / ICL）。

模型就绪检查在运行脚本中有硬校验（`config.json`, tokenizer, safetensors, speech_tokenizer 等）。

## 4.3 TTS / Voice Clone 支持情况

- 预置音色 TTS：支持（CustomVoice speaker）。
- Zero-shot clone（`VOICE:CUSTOM`）：支持（Base）。
- ICL clone：支持，但上游文档标注实验性（Apple Silicon 稳定性有限）。
- x-vector clone：支持，通常更稳。
- Trained voice 推理（`VOICE:TRAINED`）：不支持（当前节点显式拒绝）。
- Few-shot 训练：本节点无训练二进制，无当前项目训练链路接入。

## 4.4 当前项目中已做的关键修复

- 运行时参数链路：已支持 speed/pitch/volume 后处理。
- 24kHz 音频在 App 播放链路中已做采样率归一处理（避免音调异常）。
- ICL 截断问题：已在本地 patch 版本修复过比例裁剪逻辑（并保留 PR 文档）。

---

## 5. 三节点能力矩阵（项目现状）

| 能力 | dora-primespeech | dora-primespeech-mlx | dora-qwen3-tts-mlx |
|---|---|---|---|
| 技术栈 | Python + PyTorch | Rust + MLX (`gpt-sovits-mlx`) | Rust + MLX (`qwen3-tts-mlx`) |
| 主推理后端可选 | 否（当前不走主路由） | 是 | 是 |
| 预置音色 TTS | 是 | 是 | 是 |
| Zero-shot clone | 是 | 是 | 是 |
| `VOICE:TRAINED` 推理 | 是 | 是 | 否 |
| Few-shot 训练（本后端内） | 是（Python 训练服务） | 是（`moxin-fewshot-trainer`） | 否 |
| 训练后端在 UI 的映射 | Option A | Option B | 不参与 |
| 采样率（节点输出） | 通常 32k（依配置） | 32k | 24k |

---

## 6. 与当前应用配置项的对应关系

当前设置里后端相关项是分开的：

- `inference_backend`：`primespeech_mlx` / `qwen3_tts_mlx`
- `zero_shot_backend`：`primespeech_mlx` / `qwen3_tts_mlx`
- `training_backend`：`option_a` / `option_b`

对应关系：

- `option_a` -> `dora-primespeech`（Python 训练服务）
- `option_b` -> `dora-primespeech-mlx` 的 `moxin-fewshot-trainer`
- `qwen3_tts_mlx` 当前仅参与推理与 zero-shot clone，不参与 few-shot 训练

---

## 7. 实用结论（给研发决策）

1. 如果目标是“稳定可用 + 支持 trained voice 推理”，当前主力仍是 `dora-primespeech-mlx`（训练可选 Option A/Option B）。
2. `dora-qwen3-tts-mlx` 适合做：
   - 预置音色 TTS
   - zero-shot clone（尤其 x-vector）
   但不适合作为当前项目的 few-shot/trained voice 闭环后端。
3. `dora-primespeech` 现在的核心价值主要在训练链（Option A），不是主推理链。

