# Moxin Voice

> AI-powered Text-to-Speech desktop application with voice cloning — built on [OminiX MLX](https://github.com/OminiX-ai/OminiX-MLX)

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Apple%20Silicon-lightgrey.svg)](https://developer.apple.com/silicon/)

Moxin Voice is a modern, GPU-accelerated desktop TTS application built entirely in Rust. It uses the [Makepad](https://github.com/makepad/makepad) UI framework for native performance and the [OminiX MLX](https://github.com/OminiX-ai/OminiX-MLX) inference stack for high-speed, Python-free speech synthesis on Apple Silicon.

---

## ⚡ Powered by OminiX MLX

The inference engine behind Moxin Voice is **[OminiX MLX](https://github.com/OminiX-ai/OminiX-MLX)** — a comprehensive Rust-native ML inference ecosystem for Apple Silicon.

OminiX MLX provides:

- **Pure Rust inference** — no Python runtime required at synthesis time
- **Metal GPU acceleration** — optimized for M1/M2/M3/M4 chips via Apple's MLX framework
- **Unified memory** — zero-copy CPU/GPU data sharing
- **Qwen3-TTS-MLX** — the TTS engine used by Moxin Voice (9 built-in voices, 12 languages, ICL voice cloning, 2.3× real-time on M3 Max)

> Moxin Voice uses OminiX MLX's `dora-qwen3-tts-mlx` node as its sole TTS backend.
> Source: `node-hub/dora-qwen3-tts-mlx/`

---

## ✨ Features

- **🎙️ Zero-Shot Voice Cloning** — Clone any voice with 5–30 seconds of audio (ICL Express mode)
- **🎵 Text-to-Speech** — 9 preset voices across Chinese, English, Japanese, and Korean
- **🔮 Qwen3-TTS-MLX Backend** — 2.3× real-time synthesis via OminiX MLX on Apple Silicon
- **🎤 Audio Recording** — Built-in real-time recording with waveform visualization
- **🔍 ASR Integration** — Automatic text transcription for cloning reference audio
- **💾 Audio Export** — Save generated speech as WAV files
- **🌓 Dark Mode** — Native dark theme via Makepad GPU rendering
- **🌐 Bilingual UI** — Chinese and English interface

---

## 🏗️ Architecture

```
moxin-voice/
├── moxin-voice-shell/          # Application entry point (binary)
├── apps/moxin-voice/           # UI + application logic
│   └── dataflow/tts.yml        # Dora dataflow graph
├── moxin-widgets/              # Shared Makepad UI components
├── moxin-ui/                   # Application infrastructure
├── moxin-dora-bridge/          # Dora dataflow integration bridge
└── node-hub/
    ├── dora-qwen3-tts-mlx/     # ★ OminiX MLX Qwen3-TTS Rust node
    │   └── previews/           # Pre-generated voice preview WAVs
    └── dora-asr/               # FunASR speech recognition (Python)
```

The TTS pipeline runs as a [Dora](https://github.com/dora-rs/dora) dataflow: the UI sends text, the `qwen-tts-node` (built from `dora-qwen3-tts-mlx`) synthesizes audio using OminiX MLX, and the audio player receives the stream.

---

## 🚀 Quick Start (macOS)

### Prerequisites

- macOS 14.0+ (Sonoma), Apple Silicon (M1/M2/M3/M4)
- Rust 1.82+
- [Dora CLI](https://github.com/dora-rs/dora) (`cargo install dora-cli`)

### 1. Download Qwen3-TTS Models

```bash
bash scripts/init_qwen3_models.sh
```

This downloads the two model snapshots into `~/.OminiX/models/qwen3-tts-mlx/`:

| Model | Purpose |
|-------|---------|
| `Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit` | Preset voice synthesis |
| `Qwen3-TTS-12Hz-1.7B-Base` | ICL zero-shot voice cloning |

### 2. Run

```bash
dora up
cargo run -p moxin-voice-shell
```

### First-Time Distribution (macOS .app)

For end-users receiving the distributed `.app`, model download and initialization happen automatically via the in-app bootstrap wizard on first launch.

---

## 🔮 Qwen3-TTS Voice Library

9 built-in preset voices, UI names localized to Chinese or English:

| ID | Language | Character |
|----|----------|-----------|
| `vivian` | zh | 薇薇安 — bright, slightly edgy young female |
| `serena` | zh | 赛琳娜 — warm, gentle young female |
| `uncle_fu` | zh | 傅叔 — low, mellow seasoned male |
| `dylan` | zh | 迪伦 — clear Beijing young male |
| `eric` | zh | 埃里克 — lively Chengdu young male |
| `ryan` | en | Ryan — dynamic male with rhythmic drive |
| `aiden` | en | Aiden — sunny American male |
| `ono_anna` | ja | 小野安奈 — playful Japanese female |
| `sohee` | ko | 素熙 — warm Korean female |

### Voice Cloning (Express Mode)

Upload or record 5–30 seconds of reference audio. Moxin Voice uses Qwen3-TTS's **In-Context Learning (ICL)** to clone the voice in real time — no training required.

---

## 📦 Build

### Development

```bash
cargo build -p moxin-voice-shell
```

### macOS App Bundle

```bash
bash scripts/build_macos_app.sh --version 0.1.0
bash scripts/build_macos_dmg.sh
```

### Distribution Bootstrap (user machine)

```bash
bash scripts/macos_bootstrap.sh
```

Downloads FunASR (ASR) and Qwen3-TTS models, sets up the app-private conda env.

---

## 🔧 Technology Stack

| Component | Technology |
|-----------|-----------|
| UI framework | [Makepad](https://github.com/makepad/makepad) — GPU-accelerated, pure Rust |
| TTS inference | [OminiX MLX](https://github.com/OminiX-ai/OminiX-MLX) · Qwen3-TTS-MLX |
| TTS model | [Qwen3-TTS](https://huggingface.co/Qwen/Qwen3-TTS-12Hz-1.7B-Base) (Alibaba) |
| ML runtime | Apple MLX via `mlx-sys` / `mlx-rs` (OminiX MLX) |
| Dataflow | [Dora](https://github.com/dora-rs/dora) |
| Audio I/O | [CPAL](https://github.com/RustAudio/cpal) |
| ASR | FunASR Paraformer (Python, via dora-asr) |
| Language | Rust 2021 edition |

---

## 📝 License

Apache License 2.0 — see [LICENSE](LICENSE).

---

## 🙏 Acknowledgments

- **[OminiX MLX](https://github.com/OminiX-ai/OminiX-MLX)** — the core ML inference engine powering all synthesis in this project
- **[Qwen3-TTS](https://huggingface.co/Qwen)** — the TTS model (Alibaba)
- **[Makepad](https://github.com/makepad/makepad)** — GPU-accelerated UI framework
- **[Dora](https://github.com/dora-rs/dora)** — dataflow architecture
- **[Apple MLX](https://github.com/ml-explore/mlx)** — foundation for OminiX MLX

---

**Repository**: https://github.com/moxin-org/Moxin-Voice
