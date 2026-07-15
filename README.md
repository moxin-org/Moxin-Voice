# Moxin Voice

> AI-powered Text-to-Speech desktop application with voice cloning — built on [OminiX MLX](https://github.com/OminiX-ai/OminiX-MLX)

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org)
[![Platform](https://img.shields.io/badge/platform-Apple%20Silicon-lightgrey.svg)](https://developer.apple.com/silicon/)

Moxin Voice is a modern, GPU-accelerated desktop TTS application built entirely in Rust. It uses the [Makepad](https://github.com/makepad/makepad) UI framework for native performance and the [OminiX MLX](https://github.com/OminiX-ai/OminiX-MLX) inference stack for high-speed, Python-free speech synthesis on Apple Silicon.

---

## 🪄 New: Live Translation

Moxin Voice now includes a built-in **Live Translation** mode for real-time bilingual subtitles.

- **Microphone or system audio input** — translate speech from your mic, browser, meeting app, or video player
- **Real-time subtitle overlay** — compact or fullscreen floating window with adjustable text size, position, and opacity
- **Low-latency streaming pipeline** — VAD-segmented ASR + rolling translation commits for readable subtitle chunks
- **Bilingual display** — original text and translated text shown together in the overlay
- **No extra virtual audio driver required** — system audio capture uses macOS ScreenCaptureKit directly

### Hardware / System Requirements

- **Apple Silicon Mac required** — M1 / M2 / M3 / M4
- **macOS 14.0+ recommended for the full app**
- **16 GB unified memory recommended for Live Translation**
  ASR + translator + TTS models combined occupy several GB of resident memory once warmed up; 8 GB Macs will swap heavily during Live Translation. TTS-only use is lighter and fine on 8 GB.
- **Live Translation system audio input is macOS-only**
- **System audio capture requires Screen Recording permission**
  On first use, macOS will prompt for Screen Recording access because system audio capture is implemented with ScreenCaptureKit.
- **A display must be available**
  ScreenCaptureKit requires a display-backed capture session even when you only want audio.

If Screen Recording permission is denied or ScreenCaptureKit is unavailable, Live Translation still works with the microphone input source.

**Memory profile.** Live Translation loads three MLX models (ASR, translator, TTS). Total resident memory grows during model warm-up and then stays bounded; long sessions do not accumulate state.

---

## ⚡ Powered by OminiX MLX

The inference engine behind Moxin Voice is **[OminiX MLX](https://github.com/OminiX-ai/OminiX-MLX)** — a comprehensive Rust-native ML inference ecosystem for Apple Silicon.

OminiX MLX provides:

- **Pure Rust inference** — no Python runtime required at synthesis time
- **Metal GPU acceleration** — optimized for M1/M2/M3/M4 chips via Apple's MLX framework
- **Unified memory** — zero-copy CPU/GPU data sharing
- **Qwen3-TTS-MLX** — the TTS engine used by Moxin Voice (9 built-in voices, 12 languages, x-vector voice cloning, 2.3× real-time on M3 Max)

> Moxin Voice uses OminiX MLX's `dora-qwen3-tts-mlx` node as its sole TTS backend.
> Source: `node-hub/dora-qwen3-tts-mlx/`

---

## ✨ Features

- **🎙️ Zero-Shot Voice Cloning** — Clone a voice from 3–6 seconds of reference audio (x-vector Express mode; no transcript required)
- **🎵 Text-to-Speech** — 9 preset voices across Chinese, English, Japanese, and Korean
- **🌍 Live Translation** — Real-time subtitles from microphone or system audio with a floating overlay
- **🔮 Qwen3-TTS-MLX Backend** — 2.3× real-time synthesis via OminiX MLX on Apple Silicon
- **🎤 Audio Recording** — Built-in real-time recording with waveform visualization
- **🔍 ASR Integration** — Speech transcription for Live Translation and dormant training workflows
- **⏱️ Long-form Preview** — Stream and seek TTS audio up to at least 60 minutes, with cached pitch-preserving preview speeds from 0.75x to 2x
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
    └── dora-qwen3-asr/         # ★ OminiX MLX Qwen3-ASR Rust node
```

The TTS pipeline runs as a [Dora](https://github.com/dora-rs/dora) dataflow: the UI sends text, the `qwen-tts-node` (built from `dora-qwen3-tts-mlx`) synthesizes audio using OminiX MLX, and the audio player receives the stream.

---

## 🚀 Quick Start (macOS)

### Prerequisites

- macOS 14.0+ (Sonoma), Apple Silicon (M1/M2/M3/M4)
- Rust 1.82+
- [Dora CLI](https://github.com/dora-rs/dora) (`cargo install dora-cli`)
- Python 3.8+ (for the one-time model download script; not required at runtime)

### 1. Download Models

```bash
bash scripts/init_qwen3_models.sh
```

This downloads all three model snapshots into `~/.OminiX/models/`:

| Model | Purpose |
|-------|---------|
| `Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit` | Preset voice synthesis |
| `Qwen3-TTS-12Hz-1.7B-Base-8bit` | x-vector zero-shot voice cloning |
| `Qwen3-ASR-1.7B-8bit` | Speech recognition and Live Translation |

`huggingface_hub` is installed automatically if not present.

This step is optional for development. If models are missing, the app can bootstrap them on first launch.

### 2. Build

```bash
cargo build --release
```

This builds all binaries including `dora-qwen3-asr` (the ASR Dora node) and `qwen-tts-node`.

### 3. Run (Development)

```bash
cargo run -p moxin-voice-shell
```

The app handles preflight checks, first-run bootstrap, and Dora runtime startup automatically.

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

Upload or record 3–6 seconds of reference audio. Moxin Voice uses Qwen3-TTS's
**x-vector** speaker embedding path to clone the voice in real time—no training
or reference transcript is required. If generation ends before the requested text
is complete, the backend retries once with a different deterministic seed and
reports an error rather than publishing the second incomplete result.

The bundled Qwen library retains an experimental ICL API for developers, but the
Moxin Voice product does not expose or invoke it.

### Generation speed vs. preview speed

**Generation speed** is a synthesis setting. Regenerating with a different value
changes the canonical audio and therefore changes saved and downloaded files.

**Preview speed** is the 0.75x–2x control in the audio player. It preserves pitch,
only affects local playback, and never rewrites exported audio. Long recordings
are prepared in 10-second blocks on a background worker; the current block and
nearby blocks are cached, so the first visit to an uncached seek position may take
roughly 100–200 ms on the reference Apple M4 system while repeat visits reuse the
cache.

---

## 📦 Build

### Development

```bash
cargo build --release
```

### macOS App Bundle

```bash
bash scripts/build_macos_app.sh \
  --icon moxin-widgets/resources/moxin_icon_fixed.png
bash scripts/build_macos_dmg.sh
```

The app bundle version defaults to the workspace version in `Cargo.toml`. The packaged `.app` bundles the launcher, runtime nodes, bootstrap scripts, and update helper. End users do not need to run bootstrap scripts manually.

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
| ASR | [OminiX MLX](https://github.com/OminiX-ai/OminiX-MLX) · Qwen3-ASR-MLX (Rust, Metal GPU) |
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
