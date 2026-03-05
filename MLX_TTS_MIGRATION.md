# TTS 推理引擎迁移方案：从 dora-primespeech 到 MLX

> 研究日期：2026-03-05

## 背景

当前项目使用 `dora-primespeech`（基于 GPT-SoVITS）作为 TTS 推理后端。本文档分析如何将其替换为 MLX 加速推理引擎（如 OminiX-MLX 或已有的 `dora-kokoro-tts`）。

---

## 系统架构分析

### Dora 数据流拓扑（`apps/moxin-voice/dataflow/tts.yml`）

```
moxin-audio-input  →  asr  →  moxin-asr-listener
                                      ↕ (语音识别结果)
moxin-prompt-input → primespeech-tts → audio → moxin-audio-player
                                      → status/segment_complete/log
```

### Rust 与 Python 的边界

Rust 侧（`mofa-dora-bridge`）只通过 Dora 协议与 Python 节点通信，**不感知推理引擎的具体实现**。替换推理引擎只需满足节点接口契约，Rust 代码无需修改。

### 接口契约（任何替换节点必须遵守）

| 方向 | 通道名 | 类型 | 必需字段 | 说明 |
|------|--------|------|----------|------|
| Input | `text` | Arrow string | - | 待合成文本 |
| Output | `audio` | float32 array | `sample_rate`（元数据）| 音频波形，Rust 播放器依赖 `sample_rate` |
| Output | `segment_complete` | string | - | "completed" / "skipped" / "error" |
| Output | `log` | JSON string | `node`, `level`, `message`, `timestamp` | 结构化日志 |

---

## 现有节点对比

### 当前：dora-primespeech

- **引擎**：GPT-SoVITS v2
- **模型大小**：~2-4 GB/voice
- **采样率**：32000 Hz
- **性能**（Apple Silicon）：0.76x 实时（慢于实时）
- **特点**：支持中文语音克隆（Express/Pro Mode），多语音角色（14+）
- **路径**：`node-hub/dora-primespeech/`

### 已有 MLX 替代：dora-kokoro-tts

- **引擎**：Kokoro-82M（hexgrad/Kokoro-82M）
- **后端**：CPU（PyTorch）或 MLX（Metal GPU）
- **采样率**：24000 Hz（节点内部已重采样至 32000 Hz 匹配 PrimeSpeech）
- **性能对比**（Apple Silicon，15.2s 英文音频）：

  | 后端 | 耗时 | RTF | 相对速度 |
  |------|------|-----|----------|
  | PrimeSpeech（基线） | 41.84s | 1.31x | 1x |
  | Kokoro CPU | 3.70s | 0.24x | **5.4x 更快** |
  | **Kokoro MLX** | **2.32s** | **0.15x** | **8.7x 更快** |

- **路径**：`node-hub/dora-kokoro-tts/`
- **接口**：与 dora-primespeech 相同（PrimeSpeech Compatible）

---

## 迁移方案

### 方案 A：切换到 dora-kokoro-tts（推荐，无需写代码）

适用场景：只需 MLX 加速，不强依赖 GPT-SoVITS 的语音克隆能力。

**只需修改一个文件**：`apps/moxin-voice/dataflow/tts.yml`

```yaml
# 将原来的 primespeech-tts 节点替换为：
- id: primespeech-tts          # 保持 id 不变，下游 moxin-audio-player 无需改
  path: dora-kokoro-tts
  inputs:
    text: moxin-prompt-input/control
  outputs:
    - audio
    - segment_complete
    - log
  env:
    BACKEND: "auto"             # macOS 自动选 MLX，其他平台降级到 CPU
    LANGUAGE: "zh"              # zh / en / ja / ko
    VOICE: "zf_xiaoxiao"        # Kokoro 语音名称，见下方列表
    SPEED_FACTOR: "1.0"
    LOG_LEVEL: "INFO"
```

**安装 MLX 支持**（Apple Silicon）：
```bash
pip install -e "node-hub/dora-kokoro-tts[mlx]"
```

**常用中文语音**（Kokoro-82M）：
- `zf_xiaoxiao` - 中文女声
- `zm_yunxi` - 中文男声

**后端选项**：
- `BACKEND: auto` - 推荐，macOS 自动选 MLX，否则 CPU
- `BACKEND: mlx` - 强制 MLX（需 Apple Silicon + mlx-audio）
- `BACKEND: cpu` - 强制 CPU（跨平台）

---

### 方案 B：集成 OminiX-MLX（新推理引擎）

适用场景：需要 [OminiX-MLX](https://github.com/OminiX-ai/OminiX-MLX) 特有的模型或能力。

#### 步骤 1：创建新 Dora 节点

```
node-hub/dora-ominiX-mlx/
├── pyproject.toml
├── dora_ominiX_mlx/
│   ├── __init__.py
│   ├── __main__.py        # 入口：from .main import main; main()
│   └── main.py
```

**`pyproject.toml`**：
```toml
[build-system]
requires = ["hatchling"]
build-backend = "hatchling.build"

[project]
name = "dora-ominiX-mlx"
version = "0.1.0"
dependencies = [
    "dora-rs",
    "pyarrow",
    "numpy",
    # "ominiX-mlx",  # 填入实际包名
]

[project.scripts]
dora-ominiX-mlx = "dora_ominiX_mlx.main:main"
```

**`main.py` 模板**（填入 OminiX-MLX 实际 API）：
```python
"""
Dora OminiX-MLX TTS Node
MLX-accelerated text-to-speech for Apple Silicon.
"""

import os
import sys
import time
import json
import traceback
import numpy as np
import pyarrow as pa
from dora import Node

LANGUAGE = os.getenv("LANGUAGE", "zh")
VOICE = os.getenv("VOICE", "default")
SPEED = float(os.getenv("SPEED_FACTOR", "1.0"))
MODEL_PATH = os.getenv("MODEL_PATH", "")
LOG_LEVEL = os.getenv("LOG_LEVEL", "INFO")


def send_log(node, level, message):
    log_data = {
        "node": "ominiX-mlx",
        "level": level,
        "message": f"[{level}] {message}",
        "timestamp": time.time()
    }
    print(log_data["message"], file=sys.stderr if level == "ERROR" else sys.stdout, flush=True)
    if node is not None:
        node.send_output("log", pa.array([json.dumps(log_data)]))


def main():
    node = Node()

    # === 初始化 OminiX-MLX 引擎 ===
    # 根据 OminiX-MLX 实际 API 调整以下代码：
    #
    # from ominiX_mlx import TTSEngine
    # engine = TTSEngine(model_path=MODEL_PATH, language=LANGUAGE)
    #
    engine = None  # 替换为实际初始化代码

    send_log(node, "INFO", f"OminiX-MLX TTS initialized (voice={VOICE}, lang={LANGUAGE})")

    for event in node:
        if event["type"] == "INPUT" and event["id"] == "text":
            text = event["value"][0].as_py()
            metadata = event.get("metadata", {}) or {}

            # 跳过纯标点/空白
            if not text.strip():
                node.send_output("segment_complete", pa.array(["skipped"]), metadata=metadata)
                continue

            send_log(node, "INFO", f"Synthesizing: '{text[:60]}...' (len={len(text)})")
            start_time = time.time()

            try:
                # === 调用 OminiX-MLX 推理 ===
                # 根据实际 API 调整：
                #
                # audio_array, sample_rate = engine.synthesize(
                #     text=text,
                #     voice=VOICE,
                #     speed=SPEED,
                # )
                #
                audio_array = np.zeros(32000, dtype=np.float32)  # 替换为实际推理调用
                sample_rate = 32000                               # 替换为实际采样率

                duration = len(audio_array) / sample_rate
                elapsed = time.time() - start_time
                send_log(node, "INFO",
                         f"Done: {duration:.2f}s audio in {elapsed:.3f}s "
                         f"(RTF: {elapsed/duration:.3f}x)")

                # 发送音频（必须包含 sample_rate 元数据，Rust 播放器依赖）
                node.send_output(
                    "audio",
                    pa.array([audio_array.astype(np.float32)]),
                    metadata={
                        **metadata,
                        "sample_rate": sample_rate,
                        "duration": duration,
                        "backend": "ominiX-mlx",
                    }
                )
                node.send_output("segment_complete", pa.array(["completed"]), metadata=metadata)

            except Exception as e:
                send_log(node, "ERROR", f"Synthesis failed: {e}\n{traceback.format_exc()}")
                node.send_output(
                    "segment_complete",
                    pa.array(["error"]),
                    metadata={**metadata, "error": str(e)}
                )

        elif event["type"] == "STOP":
            break

    send_log(node, "INFO", "OminiX-MLX TTS node stopped")


if __name__ == "__main__":
    main()
```

#### 步骤 2：更新 tts.yml

```yaml
- id: primespeech-tts          # 保持 id 不变
  build: pip install -e ../../../node-hub/dora-ominiX-mlx
  path: dora-ominiX-mlx
  inputs:
    text: moxin-prompt-input/control
  outputs:
    - audio
    - segment_complete
    - log
  env:
    MODEL_PATH: "$HOME/.dora/models/ominiX-mlx"
    LANGUAGE: "zh"
    VOICE: "default"
    SPEED_FACTOR: "1.0"
    LOG_LEVEL: "INFO"
```

---

## 关键注意事项

### 1. 采样率兼容性

Rust 播放器通过元数据中的 `sample_rate` 字段决定播放参数，替换节点必须正确设置此字段。`dora-kokoro-tts` 内部已将 Kokoro 的 24000 Hz 重采样到 32000 Hz。

### 2. 语音克隆功能影响

`dora-primespeech` 支持 Express Mode（零样本克隆）和 Pro Mode（Few-Shot 训练），这些功能通过特殊协议传递：

```
VOICE:CUSTOM|<ref_audio>|<prompt_text>|<lang>
VOICE:TRAINED|<gpt>|<sovits>|<ref>|<prompt>|<lang>|<text>
```

切换引擎后，**语音克隆功能需要重新评估**：
- 如果 OminiX-MLX 支持零样本克隆，需在新节点中实现相同的协议解析
- 如果不支持，Express/Pro Mode 将失效

### 3. 节点 ID 不变

将 `tts.yml` 中节点 `id` 保持为 `primespeech-tts`，下游的 `moxin-audio-player` 引用 `primespeech-tts/audio` 无需改动。

---

## 快速决策

```
需要语音克隆（Express/Pro Mode）？
  ├── 是 → 方案 B（OminiX-MLX，需确认其克隆能力）
  └── 否 → 需要 Apple Silicon MLX 加速？
              ├── 是 → 方案 A（dora-kokoro-tts，5分钟完成）
              └── 否 → 维持现状 或 方案 A CPU 模式
```

---

## 相关文件

| 文件 | 说明 |
|------|------|
| `apps/moxin-voice/dataflow/tts.yml` | Dora 数据流配置，修改节点在此 |
| `node-hub/dora-primespeech/` | 当前 GPT-SoVITS 节点 |
| `node-hub/dora-kokoro-tts/` | 已有 MLX 替代节点（Kokoro-82M）|
| `node-hub/dora-kokoro-tts/dora_kokoro_tts/main.py` | 新节点实现参考模板 |
