# macOS 快速开始

> Moxin Voice · Apple Silicon · Qwen3-TTS + Qwen3-ASR

## 环境要求

- macOS 14.0+（Sonoma），Apple Silicon（M1/M2/M3/M4）
- Rust 1.82+
- [Dora CLI](https://github.com/dora-rs/dora)：`cargo install dora-cli`
- Python 3.8+（仅用于首次模型下载脚本，运行时不需要）

## 1. 下载模型（首次运行）

```bash
bash scripts/init_qwen3_models.sh
```

下载内容（共约 8GB，HuggingFace）：

| 模型 | 用途 | 大小 |
|------|------|------|
| `Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit` | 预置 9 个内置音色 | ~3GB |
| `Qwen3-TTS-12Hz-1.7B-Base` | x-vector 零样本语音克隆 | ~3GB |
| `Qwen3-ASR-1.7B-MLX` | 语音识别与实时转写 | ~2.5GB |

所有模型存储于 `~/.OminiX/models/`。

这一步对开发者是可选的。如果本地缺少模型，应用会在首次启动时自动进入 bootstrap 流程。

## 2. 构建

```bash
cargo build --release
```

构建所有二进制，包括 `dora-qwen3-asr`（ASR Dora 节点）和 `qwen-tts-node`。

## 3. 运行（开发态）

```bash
cargo run -p moxin-voice-shell
```

应用会自动执行 preflight、首次模型 bootstrap，以及 Dora runtime 启动。

## 音色克隆与长音频播放

- 快速克隆使用 x-vector，只需 3–6 秒参考音频，不需要参考文本。
- 底层库仍保留实验性 ICL API，但应用产品入口不会调用它。
- “生成语速”会改变重新生成的音频和下载文件；播放器中的“预览倍速”只影响本地播放，采用变速不变调，不改写导出音频。
- 播放器正式支持至少 60 分钟音频。未缓存位置会在后台按 10 秒块准备，同一音频、倍率和位置的后续播放会复用缓存。

## 构建 macOS App Bundle

```bash
bash scripts/build_macos_app.sh \
  --icon moxin-widgets/resources/moxin_icon_fixed.png
bash scripts/build_macos_dmg.sh
```

App Bundle 版本默认读取根目录 `Cargo.toml` 中的 workspace version。

## 分发（用户机器首次启动）

应用内置 bootstrap 向导，自动完成模型下载与初始化，用户无需手动执行 `macos_bootstrap.sh`。

---

> **注**：不再需要 conda/pip 安装任何 Python 包用于运行时。Python 仅在 `init_qwen3_models.sh` 中用于 TTS 模型下载脚本。
