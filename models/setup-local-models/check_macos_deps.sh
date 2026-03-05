#!/bin/bash

# macOS Dependency Checker for Moxin TTS
# Verifies all required system dependencies are installed

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

print_header "Moxin TTS - macOS Dependency Checker"

MISSING_DEPS=()
WARNINGS=()

# Check Homebrew
print_info "Checking Homebrew..."
if command -v brew &> /dev/null; then
    print_success "Homebrew installed: $(brew --version | head -1)"
else
    print_error "Homebrew not found"
    MISSING_DEPS+=("homebrew")
    echo "  Install: /bin/bash -c \"\$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
fi

# Check Homebrew packages
if command -v brew &> /dev/null; then
    print_info "Checking Homebrew packages..."
    
    BREW_PACKAGES=("portaudio" "ffmpeg" "git-lfs" "openblas" "libomp")
    for package in "${BREW_PACKAGES[@]}"; do
        if brew list "$package" &> /dev/null 2>&1; then
            print_success "$package installed"
        else
            print_error "$package not found"
            MISSING_DEPS+=("$package")
        fi
    done
fi

# Check Conda
print_info "Checking Conda..."
if command -v conda &> /dev/null; then
    print_success "Conda installed: $(conda --version)"
    
    # Check if moxin-studio environment exists
    if conda env list | grep -q "^moxin-studio "; then
        print_success "moxin-studio environment exists"
    else
        print_warning "moxin-studio environment not found"
        WARNINGS+=("Run ./setup_isolated_env.sh to create the environment")
    fi
else
    print_error "Conda not found"
    MISSING_DEPS+=("conda")
    echo "  Install Miniconda: https://docs.conda.io/en/latest/miniconda.html"
fi

# Check Rust
print_info "Checking Rust..."
if command -v cargo &> /dev/null; then
    print_success "Rust installed: $(rustc --version)"
    print_success "Cargo installed: $(cargo --version)"
else
    print_error "Rust not found"
    MISSING_DEPS+=("rust")
    echo "  Install: curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh"
fi

# Check Git
print_info "Checking Git..."
if command -v git &> /dev/null; then
    print_success "Git installed: $(git --version)"
else
    print_error "Git not found"
    MISSING_DEPS+=("git")
    echo "  Install: xcode-select --install"
fi

# Check Xcode Command Line Tools
print_info "Checking Xcode Command Line Tools..."
if xcode-select -p &> /dev/null; then
    print_success "Xcode Command Line Tools installed"
else
    print_warning "Xcode Command Line Tools not found"
    WARNINGS+=("Run: xcode-select --install")
fi

# Check Python architecture (Apple Silicon)
if [[ $(uname -m) == "arm64" ]]; then
    print_info "Checking Python architecture..."
    if command -v python &> /dev/null; then
        PYTHON_ARCH=$(python -c "import platform; print(platform.machine())" 2>/dev/null || echo "unknown")
        if [[ "$PYTHON_ARCH" == "arm64" ]]; then
            print_success "Python is ARM64 native (optimal for Apple Silicon)"
        else
            print_warning "Python is running under Rosetta ($PYTHON_ARCH)"
            WARNINGS+=("Consider using ARM64 native Python for better performance")
        fi
    fi
fi

# Summary
print_header "Summary"

if [ ${#MISSING_DEPS[@]} -eq 0 ]; then
    print_success "All required dependencies are installed!"
    echo ""
    
    if [ ${#WARNINGS[@]} -gt 0 ]; then
        echo "Warnings:"
        for warning in "${WARNINGS[@]}"; do
            echo "  ⚠ $warning"
        done
        echo ""
    fi
    
    echo "Next steps:"
    echo "  1. Run: ./setup_isolated_env.sh"
    echo "  2. Run: conda activate moxin-studio && ./install_all_packages.sh"
    echo "  3. Download models: cd ../model-manager && python download_models.py"
    echo ""
    print_success "You're ready to set up Moxin TTS!"
else
    print_error "Missing dependencies detected!"
    echo ""
    echo "Please install the following:"
    for dep in "${MISSING_DEPS[@]}"; do
        echo "  ✗ $dep"
    done
    echo ""
    
    if [[ " ${MISSING_DEPS[@]} " =~ " homebrew " ]]; then
        echo "Install Homebrew first:"
        echo "  /bin/bash -c \"\$(curl -fsSL https://raw.githubusercontent.com/Homebrew/install/HEAD/install.sh)\""
        echo ""
    fi
    
    if command -v brew &> /dev/null; then
        BREW_TO_INSTALL=()
        for dep in "${MISSING_DEPS[@]}"; do
            if [[ "$dep" != "homebrew" && "$dep" != "conda" && "$dep" != "rust" && "$dep" != "git" ]]; then
                BREW_TO_INSTALL+=("$dep")
            fi
        done
        
        if [ ${#BREW_TO_INSTALL[@]} -gt 0 ]; then
            echo "Install Homebrew packages:"
            echo "  brew install ${BREW_TO_INSTALL[*]}"
            echo ""
        fi
    fi
    
    echo "See MACOS_SETUP.md for detailed instructions"
    exit 1
fi
