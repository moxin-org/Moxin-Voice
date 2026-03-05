#!/bin/bash
# Moxin Studio - Pixi Activation Script
# This script runs when the pixi environment is activated

# Set TTS backend based on platform
if [[ "$OSTYPE" == "darwin"* ]]; then
    # macOS - default to auto-detection
    export BACKEND="${BACKEND:-auto}"
    echo "Moxin Studio: TTS backend set to $BACKEND (macOS detected)"
else
    # Linux/Windows - use CPU backend
    export BACKEND="${BACKEND:-cpu}"
    echo "Moxin Studio: TTS backend set to $BACKEND"
fi

# Add cargo bin to PATH for dora-cli
if [ -d "$HOME/.cargo/bin" ]; then
    export PATH="$HOME/.cargo/bin:$PATH"
fi

# Set up Rust node paths
export DORA_MAAS_CLIENT="$PIXI_PROJECT_ROOT/target/release/dora-maas-client"
export DORA_CONFERENCE_BRIDGE="$PIXI_PROJECT_ROOT/target/release/dora-conference-bridge"
export DORA_CONFERENCE_CONTROLLER="$PIXI_PROJECT_ROOT/target/release/dora-conference-controller"
