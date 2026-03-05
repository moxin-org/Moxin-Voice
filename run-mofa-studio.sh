#!/bin/bash
# Moxin Studio Launcher
# Ensures pixi environment is activated before launching

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
cd "$SCRIPT_DIR"

# Check if pixi is installed
if ! command -v pixi &> /dev/null; then
    echo "Error: pixi is not installed"
    echo "Install with: curl -fsSL https://pixi.sh/install.sh | bash"
    exit 1
fi

# Check if environment exists, install if not
if [ ! -d ".pixi/envs/default" ]; then
    echo "First run: Installing dependencies..."
    pixi install
    pixi run setup
fi

# Launch moxin-studio within pixi environment
exec pixi run cargo run --release "$@"
