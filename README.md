# Moxin-Voice

> Standalone AI-powered Text-to-Speech desktop application with voice cloning capabilities

[![License](https://img.shields.io/badge/license-Apache%202.0-blue.svg)](LICENSE)
[![Rust](https://img.shields.io/badge/rust-2021-orange.svg)](https://www.rust-lang.org)

Moxin TTS is a modern, GPU-accelerated desktop application for text-to-speech synthesis and voice cloning. Built entirely in Rust using the [Makepad](https://github.com/makepad/makepad) UI framework, it provides a beautiful, responsive interface with native performance. Powered by GPT-SoVITS v2 for state-of-the-art voice cloning and synthesis.

## ✨ Features

- **🎨 Beautiful UI** - GPU-accelerated rendering with smooth animations
- **🌓 Dark Mode** - Seamless dark theme with native Makepad performance
- **🌍 Multi-Language Support** - English and Simplified Chinese with runtime language switching
- **🎙️ Zero-Shot Voice Cloning** - Clone any voice with just 5-10 seconds of audio (Express mode)
- **🔧 Few-Shot Training** - High-quality voice cloning with 3-10 minutes of audio (Pro mode)
- **🎵 Text-to-Speech** - Natural-sounding speech synthesis with 14+ preset voices
- **🎤 Audio Recording** - Built-in audio recording with real-time visualization
- **🔍 Speech Recognition** - Automatic text recognition from audio (ASR integration)
- **💾 Audio Export** - Save generated speech as WAV files
- **🚀 Native Performance** - Built with Rust for maximum efficiency

## 🏗️ Architecture

Moxin TTS uses a modular workspace structure focused on TTS functionality:

```
moxin-tts/
├── moxin-tts-shell/      # Standalone TTS application entry
├── apps/mofa-tts/        # TTS application logic
├── mofa-widgets/         # Shared UI components
├── mofa-ui/              # Application infrastructure
├── mofa-dora-bridge/     # Dora dataflow integration
└── node-hub/             # Python Dora nodes (TTS & ASR)
    ├── dora-primespeech/ # GPT-SoVITS TTS engine
    └── dora-asr/         # Speech recognition
```

### Key Design Principles

- **Standalone Application** - Focused solely on TTS and voice cloning
- **Dora Integration** - Uses Dora dataflow for audio processing pipeline
- **Makepad Native** - Leverages Makepad's GPU-accelerated UI framework
- **Modular Architecture** - Clean separation between UI, logic, and processing

## 🚀 Quick Start

### macOS Users

**Quick Setup** (5 minutes):
```bash
# Install system dependencies
./install_macos_deps.sh

# One-click setup
cd models/setup-local-models
./quick_setup_macos.sh
```

See [QUICKSTART_MACOS.md](QUICKSTART_MACOS.md) for details or [MACOS_SETUP.md](MACOS_SETUP.md) for complete guide.

### Prerequisites

- **Rust** 1.70+ (2021 edition)
- **Python** 3.8+
- **Cargo** package manager
- **Git** for cloning the repository
- **macOS Users**: See [MACOS_SETUP.md](MACOS_SETUP.md) for detailed setup instructions

### TTS Setup

#### 1. Environment Setup

```bash
cd models/setup-local-models
./setup_isolated_env.sh
```

This creates a conda environment `mofa-studio` with:

- Python 3.12
- PyTorch 2.2.0, NumPy 1.26.4, Transformers 4.45.0

#### 2. Install All Packages

After the conda environment is created, install all Python and Rust components:

```bash
conda activate mofa-studio
./install_all_packages.sh
```

This installs:

- Shared library: `dora-common`
- Python nodes: `dora-asr`, `dora-primespeech`
- Dora CLI

Verify installation:

```bash
python test_dependencies.py
```

#### 3. Model Downloads

```bash
cd models/model-manager

# ASR models (FunASR Paraformer + punctuation)
python download_models.py --download funasr

# PrimeSpeech TTS (base + voices)
python download_models.py --download primespeech

# List available voices
python download_models.py --list-voices

# Download specific voice
python download_models.py --voice "Luo Xiang"
```

#### 3. Running the Application

**Default Layout (Original MoFA style):**

```bash
cargo run -p moxin-tts
```

**MoYoYo.tts Style Layout (Modern sidebar design):**

```bash
cargo run -p moxin-tts --features moyoyo-ui
```

The application provides two distinct UI layouts:

- **Default**: Original MoFA-style layout with system monitor and log panel
- **MoYoYo UI**: Modern sidebar layout inspired by MoYoYo.tts, featuring a cleaner interface with a left navigation sidebar and simplified main content area

You can switch between layouts by using the `--features moyoyo-ui` flag when running the application.

For more details on the MoYoYo.tts UI implementation, see [doc/MOYOYO_UI_IMPLEMENTATION.md](doc/MOYOYO_UI_IMPLEMENTATION.md).
Models are stored in:

| Location                      | Contents                      |
| ----------------------------- | ----------------------------- |
| `~/.dora/models/asr/funasr/`  | FunASR ASR models             |
| `~/.dora/models/primespeech/` | PrimeSpeech TTS base + voices |

### Build & Run

```bash
# Clone the repository
git clone https://github.com/alan0x/moxin-tts.git
cd moxin-tts

# Build in release mode
cargo build -p moxin-tts --release

# Run the application
cargo run -p moxin-tts --release
```

The application window will open at 1200x800 pixels by default.

### Development Build

```bash
# Fast debug build
cargo build -p moxin-tts

# Run with debug logging
cargo run -p moxin-tts -- --log-level debug
```

## 📦 Project Structure

Moxin TTS is organized as a Cargo workspace with 5 core crates:

| Crate              | Type    | Description                               |
| ------------------ | ------- | ----------------------------------------- |
| `moxin-tts-shell`  | Binary  | Standalone TTS application entry point    |
| `mofa-tts`         | Library | TTS UI and application logic              |
| `mofa-widgets`     | Library | Shared UI components (theme, audio, etc.) |
| `mofa-ui`          | Library | Application infrastructure and widgets    |
| `mofa-dora-bridge` | Library | Dora dataflow integration bridge          |

### Python Nodes (node-hub/)

| Node               | Type   | Description                     |
| ------------------ | ------ | ------------------------------- |
| `dora-primespeech` | Python | GPT-SoVITS TTS synthesis engine |
| `dora-asr`         | Python | Speech recognition (FunASR)     |
| `dora-common`      | Python | Shared utilities and logging    |

### Key Documentation

- **[BUILDING.md](moxin-tts-shell/BUILDING.md)** - Detailed build instructions
- **[CONTEXT_RESUME.md](doc/CONTEXT_RESUME.md)** - Project context and progress
- **[Implementation Summary](moxin-tts-shell/IMPLEMENTATION_SUMMARY.md)** - Phase 1-4 summary
- **[TTS Timbre Options](docs/TTS_TIMBRE_OPTIONS.md)** - 语速/音调参数、默认值、边界与回退策略

## 🎯 Current Status

Moxin TTS is a **functional standalone application** with the following capabilities:

### ✅ Implemented

**Phase 1-4 Complete**:

- ✅ Standalone application shell (moxin-tts-shell)
- ✅ TTS screen with voice selection and text input
- ✅ Zero-shot voice cloning UI (Express mode)
- ✅ Few-shot training UI (Pro mode)
- ✅ Audio recording and playback
- ✅ Dora dataflow integration
- ✅ Codebase cleanup (removed unused apps, 24K lines)

### 🚧 In Progress

**Phase 5: Testing & Polish**:

- 🚧 TTS generation testing
- 🚧 Voice cloning verification
- 🚧 Few-shot training backend integration
- 🚧 Performance optimization

## 🎙️ Voice Cloning Modes

### Express Mode (Zero-Shot)

- **Audio Length**: 5-10 seconds
- **Use Case**: Quick voice cloning
- **Quality**: Good for most use cases
- **Process**: Upload/record → Clone immediately

### Pro Mode (Few-Shot)

- **Audio Length**: 3-10 minutes
- **Use Case**: High-quality professional voices
- **Quality**: Exceptional fidelity
- **Process**: Upload/record → Train model → Clone

## 🛠️ Development

### Build Commands

```bash
# Development build
cargo build -p moxin-tts

# Release build (optimized)
cargo build -p moxin-tts --release

# Run with custom log level
cargo run -p moxin-tts -- --log-level debug

# Clean build artifacts
cargo clean
```

### Run Dora Dataflow

```bash
# Start the Dora daemon
dora up

# Navigate to TTS dataflow
cd apps/mofa-tts/dataflow

# Start the dataflow
dora start tts-dataflow.yml

# Check status
dora list

# Stop dataflow
dora stop <dataflow-id>
```

## 🌍 Internationalization (i18n)

Moxin TTS supports multiple languages with runtime language switching.

### Supported Languages

- **English** (`en`) - Default language
- **Simplified Chinese** (`zh-CN`) - 简体中文

### Using Language Settings

1. Launch the application
2. Click the **Settings** button (⚙) in the navigation
3. Select your preferred language from the language options
4. The UI will update immediately without requiring a restart
5. Your language preference is saved and persists across sessions

### Language Detection

On first launch, Moxin TTS automatically detects your system locale and sets the appropriate language if supported. If your system language is not yet supported, it defaults to English.

### Adding New Languages

Want to add support for your language? See our [Translation Guide](docs/TRANSLATION_GUIDE.md) for detailed instructions on:

- Creating translation files
- Translation guidelines and best practices
- Testing your translations
- Contributing translations back to the project

Translation files are located in `locales/[language-code]/` and use YAML format for easy editing.

### Technical Details

- **i18n Library**: `rust-i18n` with YAML translation files
- **Translation Storage**: Compiled into the binary for fast loading
- **Preference Storage**: `~/.moxin-tts/preferences.json`
- **Fallback**: Missing translations automatically fall back to English

## 🔧 Technology Stack

- **[Rust](https://www.rust-lang.org/)** - Systems programming language
- **[Makepad](https://github.com/makepad/makepad)** - GPU-accelerated UI framework
- **[GPT-SoVITS v2](https://github.com/RVC-Boss/GPT-SoVITS)** - Voice cloning and TTS engine
- **[Dora](https://github.com/dora-rs/dora)** - Robotics dataflow framework
- **[CPAL](https://github.com/RustAudio/cpal)** - Cross-platform audio I/O
- **[Tokio](https://tokio.rs/)** - Async runtime
- **[Serde](https://serde.rs/)** - Serialization framework

## 🤝 Contributing

Contributions are welcome! Please see [CONTRIBUTING.md](CONTRIBUTING.md) for guidelines.

### Development Workflow

1. Fork the repository
2. Create a feature branch (`git checkout -b feature/amazing-feature`)
3. Make your changes
4. Test thoroughly (`cargo test`, `cargo build -p moxin-tts`)
5. Commit your changes (`git commit -m 'Add amazing feature'`)
6. Push to the branch (`git push origin feature/amazing-feature`)
7. Open a Pull Request

## 📝 License

This project is licensed under the Apache License 2.0 - see the [LICENSE](LICENSE) file for details.

```
Copyright 2026 Moxin TTS Authors

Licensed under the Apache License, Version 2.0 (the "License");
you may not use this file except in compliance with the License.
You may obtain a copy of the License at

    http://www.apache.org/licenses/LICENSE-2.0
```

## 🙏 Acknowledgments

- **[Makepad](https://github.com/makepad/makepad)** - For the incredible GPU-accelerated UI framework
- **[GPT-SoVITS](https://github.com/RVC-Boss/GPT-SoVITS)** - For the excellent voice cloning technology
- **[Dora Robotics Framework](https://github.com/dora-rs/dora)** - For the dataflow architecture
- **[MoFA Studio](https://github.com/mofa-org/mofa-studio)** - Original multi-app platform (upstream)
- **Rust Community** - For excellent tooling and libraries

## 📧 Contact

- **Repository**: https://github.com/alan0x/moxin-tts
- **Issues**: https://github.com/alan0x/moxin-tts/issues
- **Developer**: alan0x

---

_Built with ❤️ using Rust, Makepad, and GPT-SoVITS_
