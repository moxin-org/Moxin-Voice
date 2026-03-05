# Building Moxin Voice

## Quick Build

```bash
# From the project root (moxin-studio/)
cargo build --package moxin-voice --release

# Or shorter form
cargo build -p moxin-voice --release

# Output: ./target/release/moxin-voice.exe (Windows) or ./target/release/moxin-voice (Unix)
```

## Development Build

```bash
# Faster compile, larger binary, includes debug info
cargo build -p moxin-voice

# Run directly without building first
cargo run -p moxin-voice

# With custom log level
cargo run -p moxin-voice -- --log-level debug

# With custom dataflow
cargo run -p moxin-voice -- --dataflow path/to/dataflow.yml
```

## Build Options

### Optimization Levels

```bash
# Release build (optimized, smaller)
cargo build -p moxin-voice --release

# Dev build (fast compile, debug symbols)
cargo build -p moxin-voice

# Maximum optimization (slower build)
RUSTFLAGS="-C target-cpu=native" cargo build -p moxin-voice --release
```

### Platform-Specific

#### Windows

```powershell
# Build
cargo build -p moxin-voice --release

# Run
.\target\release\moxin-voice.exe

# Create installer (requires additional tools)
# TODO: Add installer instructions
```

#### macOS

```bash
# Build
cargo build -p moxin-voice --release

# Run
./target/release/moxin-voice

# Create app bundle (requires additional tools)
# TODO: Add app bundle instructions
```

#### Linux

```bash
# Build
cargo build -p moxin-voice --release

# Run
./target/release/moxin-voice

# Create .deb package (requires cargo-deb)
cargo install cargo-deb
cargo deb -p moxin-voice
```

## Troubleshooting

### Build Errors

#### Missing Dependencies

```bash
# Update Rust
rustup update

# Clean build cache
cargo clean

# Rebuild
cargo build -p moxin-voice --release
```

#### Linking Errors

```bash
# On Windows, ensure Visual Studio Build Tools are installed
# On Linux, ensure build-essential is installed
sudo apt-get install build-essential

# On macOS, ensure Xcode Command Line Tools are installed
xcode-select --install
```

### Runtime Errors

#### Missing Python Nodes

```bash
# Install dora-primespeech
cd node-hub/dora-primespeech
pip install -e .

# Install dora-asr
cd ../dora-asr
pip install -e .
```

#### GPU Acceleration

```bash
# CUDA support (NVIDIA)
# Ensure CUDA toolkit is installed
# PyTorch will be installed with CUDA support via pip

# Check GPU availability
python -c "import torch; print(torch.cuda.is_available())"
```

## Build Times

Approximate build times on different systems:

| Configuration | System            | Time     |
| ------------- | ----------------- | -------- |
| Debug         | AMD Ryzen 9 5900X | ~2 min   |
| Release       | AMD Ryzen 9 5900X | ~5 min   |
| Debug         | Apple M1          | ~1.5 min |
| Release       | Apple M1          | ~4 min   |

## Binary Sizes

| Configuration   | Size    |
| --------------- | ------- |
| Debug           | ~200 MB |
| Release         | ~50 MB  |
| Release + strip | ~30 MB  |

### Reducing Binary Size

```bash
# Strip symbols
cargo build -p moxin-voice --release
strip target/release/moxin-voice  # Unix
strip target/release/moxin-voice.exe  # Windows (requires GNU binutils)

# Or use cargo configuration
# Add to Cargo.toml:
[profile.release]
strip = true
opt-level = "z"  # Optimize for size
lto = true       # Link-time optimization
```

## Cross-Compilation

### Windows → Linux

```bash
# Install cross-compilation target
rustup target add x86_64-unknown-linux-gnu

# Build
cargo build -p moxin-voice --release --target x86_64-unknown-linux-gnu
```

### macOS → Windows

```bash
# Install cross-compilation target
rustup target add x86_64-pc-windows-gnu

# Build (requires mingw-w64)
cargo build -p moxin-voice --release --target x86_64-pc-windows-gnu
```

## CI/CD

See `.github/workflows/` (if configured) for automated build pipelines.

## Next Steps

After building:

1. Run the application: `./target/release/moxin-voice`
2. Test TTS functionality
3. Test voice cloning
4. Package for distribution (see packaging guide)

## Support

For build issues, please file an issue at:
https://github.com/alan0x/moxin-voice/issues
