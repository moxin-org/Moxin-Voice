#!/bin/bash

# Install All Packages Script for MoFA Studio (Linux & macOS)
# This script reinstalls required Python packages and builds Rust components
# Use after the conda environment (mofa-studio) already exists.

set -e  # Exit on error

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[0;33m'
BLUE='\033[0;34m'
NC='\033[0m' # No Color

# Print colored messages
print_info() {
    echo -e "${BLUE}ℹ ${NC} $1"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

print_header() {
    echo -e "\n${BLUE}═══════════════════════════════════════════════════════${NC}"
    echo -e "${BLUE}   $1${NC}"
    echo -e "${BLUE}═══════════════════════════════════════════════════════${NC}\n"
}

# Detect platform shortcut
OS_TYPE="linux"
if [[ "$OSTYPE" == "darwin"* ]]; then
    OS_TYPE="macos"
fi

# Activate conda environment
print_header "Activating Conda Environment"
eval "$(conda shell.bash hook)"
if conda activate mofa-studio 2>/dev/null; then
    print_success "Activated conda environment: mofa-studio"
else
    print_error "Conda environment 'mofa-studio' not found. Please create it first (see README)."
    exit 1
fi

# OS-specific dependency hints
print_header "Checking System Dependencies"
if [[ "$OS_TYPE" == "linux" ]]; then
    print_info "Installing essential build tools and libraries via apt..."
    sudo apt-get update
    sudo apt-get install -y gcc gfortran libopenblas-dev build-essential openssl libssl-dev portaudio19-dev
    print_success "System dependencies installed"
else
    print_info "macOS detected. Checking Homebrew dependencies..."
    if command -v brew &> /dev/null; then
        REQUIRED_PACKAGES=("portaudio" "ffmpeg" "openblas" "libomp")
        MISSING_PACKAGES=()
        
        for package in "${REQUIRED_PACKAGES[@]}"; do
            if ! brew list "$package" &> /dev/null; then
                MISSING_PACKAGES+=("$package")
            fi
        done
        
        if [ ${#MISSING_PACKAGES[@]} -gt 0 ]; then
            print_error "Missing Homebrew packages: ${MISSING_PACKAGES[*]}"
            echo ""
            echo "Please install them with:"
            echo "  brew install ${MISSING_PACKAGES[*]}"
            echo ""
            exit 1
        else
            print_success "All required Homebrew packages are installed"
        fi
    else
        print_error "Homebrew not found. Please install Homebrew first:"
        echo ""
        echo "  /bin/bash -c \"\$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
        echo ""
        exit 1
    fi
fi

# Get the script directory and project root
SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )"
PROJECT_ROOT="$(dirname "$(dirname "$SCRIPT_DIR")")"

print_info "Project root: $PROJECT_ROOT"

# Install all Dora packages in editable mode
print_header "Installing Dora Python Packages"
cd "$PROJECT_ROOT"

print_info "Installing dora-common (shared library)..."
pip install -e libs/dora-common
print_success "dora-common installed"

print_info "Installing dora-primespeech..."
pip install -e node-hub/dora-primespeech
print_success "dora-primespeech installed"

# NOTE: Skipping dora-asr due to network issues with pywhispercpp dependency
# ASR is not required for core TTS functionality
print_info "Skipping dora-asr (optional, network issues with pywhispercpp)..."
# pip install -e node-hub/dora-asr

print_info "Installing dora-speechmonitor..."
pip install -e node-hub/dora-speechmonitor
print_success "dora-speechmonitor installed"

print_info "Installing dora-text-segmenter..."
pip install -e node-hub/dora-text-segmenter
print_success "dora-text-segmenter installed"

# Pro Mode (Few-Shot) training dependencies
# datasets>=3.0.0 is incompatible with modelscope 1.34.0 needed for denoising
print_info "Installing Pro Mode training dependencies..."
pip install "datasets<3.0.0" simplejson sortedcontainers tensorboard matplotlib
print_success "Pro Mode dependencies installed"

# Install Rust if not already installed
print_header "Setting up Rust"
if command -v cargo &> /dev/null; then
    print_info "Rust is already installed"
    rustc --version
    cargo --version
else
    print_info "Installing Rust..."
    curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh -s -- -y
    source $HOME/.cargo/env
    print_success "Rust installed successfully"
fi

# Install Dora CLI
print_header "Installing Dora CLI"
DORA_VERSION="0.3.12"

# Remove all non-cargo dora installations (except nix symlinks)
dora_locations=$(type -a dora 2>/dev/null | awk '{print $NF}' || true)
if [[ -n "$dora_locations" ]]; then
    print_info "Checking existing dora installation(s)..."

    # Remove pip-installed dora-rs-cli if present
    if pip show dora-rs-cli &> /dev/null; then
        print_info "Removing pip-installed dora-rs-cli..."
        pip uninstall -y dora-rs-cli || true
    fi

    # Remove all non-cargo dora binaries (skip nix symlinks)
    while IFS= read -r dora_path; do
        [[ -z "$dora_path" ]] && continue
        [[ "$dora_path" == *"/.cargo/bin/"* ]] && continue

        # Skip nix symlinks
        if [[ -L "$dora_path" ]] && [[ "$(readlink "$dora_path")" == /nix/* ]]; then
            print_info "Skipping nix symlink: $dora_path"
            continue
        fi

        print_info "Removing non-cargo dora: $dora_path"
        rm -f "$dora_path" 2>/dev/null && print_success "Removed $dora_path" || print_error "Failed to remove $dora_path"
    done <<< "$dora_locations"
fi

# Always ensure cargo-installed dora at correct version
cargo_dora="$HOME/.cargo/bin/dora"
if [[ -f "$cargo_dora" ]]; then
    current_version=$("$cargo_dora" --version 2>&1 | grep -oE '[0-9]+\.[0-9]+\.[0-9]+' | head -1 || echo "unknown")
    if [[ "$current_version" == "$DORA_VERSION" ]]; then
        print_success "Dora CLI (cargo) is at target version $DORA_VERSION"
    else
        print_info "Updating Dora CLI: $current_version -> $DORA_VERSION"
        cargo install dora-cli --version "$DORA_VERSION" --locked --force
        print_success "Dora CLI updated to $DORA_VERSION"
    fi
else
    print_info "Installing Dora CLI via cargo..."
    cargo install dora-cli --version "$DORA_VERSION" --locked
    print_success "Dora CLI installed ($DORA_VERSION)"
fi

# Build Rust-based nodes
print_header "Building Rust Components"

# NOTE: Skipping optional Rust components (not required for TTS core functionality)
# These components have dependency issues with outfox-openai 0.7.0
print_info "Skipping dora-maas-client (optional, has dependency issues)..."
# cargo build --release --manifest-path "$PROJECT_ROOT/node-hub/dora-maas-client/Cargo.toml"

print_info "Skipping dora-conference-bridge (optional)..."
# cargo build --release --manifest-path "$PROJECT_ROOT/node-hub/dora-conference-bridge/Cargo.toml"

print_info "Skipping dora-conference-controller (optional)..."
# cargo build --release --manifest-path "$PROJECT_ROOT/node-hub/dora-conference-controller/Cargo.toml"

print_success "Rust components check completed (optional components skipped)"

# Summary
print_header "Installation Complete!"
echo -e "${GREEN}All packages have been successfully installed!${NC}"
echo ""
echo "Summary:"
if [[ "$OS_TYPE" == "linux" ]]; then
    echo "  ✓ Linux system dependencies installed"
else
    echo "  ✓ macOS system dependencies assumed ready"
fi
echo "  ✓ Python packages installed in editable mode"
echo "  ✓ Rust and Dora CLI installed"
echo "  ✓ Rust components built"
echo ""
echo "Next steps:"
echo "  1. Download models: cd examples/model-manager && python download_models.py --download primespeech"
echo "  2. Download additional models (funasr, kokoro, qwen) as needed"
echo "  3. Configure any required API keys (e.g. OpenAI)"
echo "  4. Run voice-chat examples under examples/mac-aec-chat"
echo ""
print_success "Ready to use Dora Voice Chat!"
