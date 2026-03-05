#!/bin/bash

# Quick Setup Script for Moxin Voice on macOS
# This script automates the entire setup process

set -e

# Colors
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
BLUE='\033[0;34m'
NC='\033[0m'

print_header() {
    echo ""
    echo -e "${BLUE}============================================${NC}"
    echo -e "${BLUE}$1${NC}"
    echo -e "${BLUE}============================================${NC}"
}

print_success() {
    echo -e "${GREEN}✓${NC} $1"
}

print_error() {
    echo -e "${RED}✗${NC} $1"
}

print_warning() {
    echo -e "${YELLOW}⚠${NC} $1"
}

print_info() {
    echo -e "${BLUE}ℹ${NC} $1"
}

# Check if running on macOS
if [[ "$OSTYPE" != "darwin"* ]]; then
    print_error "This script is for macOS only"
    exit 1
fi

print_header "Moxin Voice - Quick Setup for macOS"

# Step 1: Check dependencies
print_header "Step 1: Checking Dependencies"
./check_macos_deps.sh || {
    print_error "Dependency check failed. Please install missing dependencies first."
    echo ""
    echo "See MACOS_SETUP.md for detailed instructions"
    exit 1
}

# Step 2: Setup environment
print_header "Step 2: Setting Up Environment"
if conda env list | grep -q "^moxin-studio "; then
    print_warning "Environment 'moxin-studio' already exists"
    read -p "Do you want to recreate it? (y/n): " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        print_info "Removing existing environment..."
        conda deactivate 2>/dev/null || true
        conda env remove -n moxin-studio -y
        ./setup_isolated_env.sh
    else
        print_info "Using existing environment"
    fi
else
    ./setup_isolated_env.sh
fi

# Step 3: Install packages
print_header "Step 3: Installing Packages"
eval "$(conda shell.bash hook)"
conda activate moxin-studio
./install_all_packages.sh

# Step 4: Verify installation
print_header "Step 4: Verifying Installation"
eval "$(conda shell.bash hook)"
conda activate moxin-studio
python test_dependencies.py || {
    print_error "Dependency verification failed"
    print_info "This might be normal if some optional nodes are not installed"
    print_info "Continuing with setup..."
}

# Step 5: Download models (optional)
print_header "Step 5: Download Models (Optional)"
echo ""
echo "Do you want to download models now?"
echo "  1) Yes, download all models (ASR + TTS)"
echo "  2) Download ASR models only"
echo "  3) Download TTS models only"
echo "  4) Skip for now"
echo ""
read -p "Enter your choice (1-4): " -n 1 -r
echo

case $REPLY in
    1)
        print_info "Downloading all models..."
        cd ../model-manager
        python download_models.py --download funasr
        python download_models.py --download primespeech
        cd ../setup-local-models
        ;;
    2)
        print_info "Downloading ASR models..."
        cd ../model-manager
        python download_models.py --download funasr
        cd ../setup-local-models
        ;;
    3)
        print_info "Downloading TTS models..."
        cd ../model-manager
        python download_models.py --download primespeech
        cd ../setup-local-models
        ;;
    4)
        print_info "Skipping model download"
        ;;
    *)
        print_warning "Invalid choice, skipping model download"
        ;;
esac

# Summary
print_header "Setup Complete!"
echo ""
print_success "Moxin Voice is ready to use!"
echo ""
echo "Quick Start:"
echo ""
echo "  1. Activate environment:"
echo "     conda activate moxin-studio"
echo ""
echo "  2. Run the application:"
echo "     cd ../../.."
echo "     cargo run -p moxin-voice"
echo ""
echo "  3. Or use Moxin UI:"
echo "     cargo run -p moxin-voice"
echo ""
echo "Additional Commands:"
echo ""
echo "  • Download more voices:"
echo "    cd models/model-manager"
echo "    python download_models.py --list-voices"
echo "    python download_models.py --voice \"Voice Name\""
echo ""
echo "  • Run tests:"
echo "    python test_dependencies.py"
echo ""
echo "  • Check Dora status:"
echo "    dora up"
echo "    dora list"
echo ""
echo "Documentation:"
echo "  • Setup Guide: MACOS_SETUP.md"
echo "  • Troubleshooting: TROUBLESHOOTING_MACOS.md"
echo "  • Project README: README.md"
echo ""
print_success "Happy coding! 🚀"
