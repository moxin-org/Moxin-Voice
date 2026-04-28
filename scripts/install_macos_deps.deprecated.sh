#!/bin/bash

# Deprecated helper for legacy macOS bootstrap flows.
# This script is not part of the current primary setup/build/update path.

echo "Installing macOS dependencies via Homebrew..."
brew install portaudio ffmpeg git-lfs openblas libomp

echo ""
echo "✓ All dependencies installed!"
echo ""
echo "This script is deprecated and kept only for older local setup flows."
echo "Prefer following docs/getting-started/MACOS_SETUP.md for the current setup path."
