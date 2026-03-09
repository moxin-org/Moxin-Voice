# Moxin Voice

A standalone desktop application for text-to-speech with voice cloning, powered by GPT-SoVITS.

## Features

- **Text-to-Speech**: Generate natural-sounding speech from text
- **Voice Selection**: Choose from 14+ pre-trained celebrity and character voices
- **Zero-Shot Voice Cloning**: Clone any voice with just 5-10 seconds of audio
- **Few-Shot Training**: Fine-tune custom voices with 1-10 minutes of audio
- **Multiple Languages**: Support for Chinese, English, Japanese, Korean, and Cantonese
- **Real-time ASR**: Automatic speech recognition for voice cloning
- **Audio Management**: Play, download, and manage generated audio

## Quick Start

### Build

```bash
# Build the application
cargo build --package moxin-voice --release

# Or use shorter form
cargo build -p moxin-voice --release
```

### Run

```bash
# Run directly
cargo run --package moxin-voice

# Run with custom log level
cargo run -p moxin-voice -- --log-level debug

# Run with custom dataflow
cargo run -p moxin-voice -- --dataflow path/to/dataflow.yml

# Run the compiled binary
./target/release/moxin-voice
```

## Command-Line Options

```
Moxin Voice - Voice Cloning & Text-to-Speech

Usage: moxin-voice [OPTIONS]

Options:
  -l, --log-level <LOG_LEVEL>    Log level (trace, debug, info, warn, error) [default: info]
  -d, --dataflow <DATAFLOW>      Dora dataflow YAML file path
  -h, --help                     Print help
  -V, --version                  Print version
```

## Architecture

Moxin Voice is a standalone application extracted from the Moxin Studio framework:

```
moxin-voice/
├── moxin-voice-shell/    # Standalone application entry (this crate)
├── apps/moxin-voice/      # TTS application logic
├── moxin-widgets/       # Shared UI components
├── moxin-ui/            # Application infrastructure
├── moxin-dora-bridge/   # Dora dataflow integration
└── node-hub/           # Python Dora nodes (TTS & ASR)
```

### Key Components

- **GPT-SoVITS**: Neural TTS engine with voice cloning
- **Dora**: Dataflow framework for node orchestration
- **Makepad**: GPU-accelerated UI framework
- **dora-primespeech**: TTS node for speech synthesis
- **dora-asr**: ASR node for voice recognition

## Development

### Prerequisites

- Rust 1.70+
- Python 3.8+ (for Dora nodes)
- GPU with CUDA support (optional, for faster inference)

### Project Structure

```
moxin-voice-shell/
├── Cargo.toml           # Package configuration
├── src/
│   ├── main.rs          # Application entry point
│   └── app.rs           # Main app logic and UI
├── resources/           # Fonts, icons, assets
└── README.md            # This file
```

### Building from Source

```bash
# Clone the repository
git clone https://github.com/moxin-org/Moxin-Voice.git
cd Moxin-Voice

# Install dependencies
# (Python dependencies for Dora nodes)
cd node-hub/dora-primespeech
pip install -e .
cd ../dora-asr
pip install -e .
cd ../..

# Build the application
cargo build --release -p moxin-voice

# Run
./target/release/moxin-voice
```

## License

Apache-2.0

## Credits

- [GPT-SoVITS](https://github.com/RVC-Boss/GPT-SoVITS) - Voice cloning engine
- [Makepad](https://github.com/makepad/makepad) - UI framework
- [Dora](https://github.com/dora-rs/dora) - Dataflow framework
- [Moxin Studio](https://github.com/moxin-org/moxin-studio) - Original framework

## Support

For issues and questions, please visit:

- GitHub Issues: https://github.com/moxin-org/Moxin-Voice/issues
