#!/bin/bash

# Quick verification script for Moxin Voice setup
# Checks if everything is ready to run the application

set -e

# Colors
GREEN='\033[0;32m'
RED='\033[0;31m'
BLUE='\033[0;34m'
NC='\033[0m'

echo -e "${BLUE}========================================${NC}"
echo -e "${BLUE}Moxin Voice Setup Verification${NC}"
echo -e "${BLUE}========================================${NC}"
echo ""

# Check if moxin-studio environment exists
if conda env list | grep -q "^moxin-studio "; then
    echo -e "${GREEN}✓${NC} moxin-studio environment exists"
else
    echo -e "${RED}✗${NC} moxin-studio environment not found"
    echo "  Run: ./setup_isolated_env.sh"
    exit 1
fi

# Activate environment and run tests
echo ""
echo "Activating moxin-studio environment..."
eval "$(conda shell.bash hook)"
conda activate moxin-studio

echo ""
echo "Running dependency tests..."
echo ""
python test_dependencies.py

if [ $? -eq 0 ]; then
    echo ""
    echo -e "${GREEN}========================================${NC}"
    echo -e "${GREEN}✓ Setup verification passed!${NC}"
    echo -e "${GREEN}========================================${NC}"
    echo ""
    echo "You can now:"
    echo "  1. Download models: cd ../model-manager && python download_models.py"
    echo "  2. Run the app: cd ../.. && cargo run -p moxin-voice"
    echo ""
else
    echo ""
    echo -e "${RED}========================================${NC}"
    echo -e "${RED}✗ Setup verification failed${NC}"
    echo -e "${RED}========================================${NC}"
    echo ""
    echo "Try:"
    echo "  1. Re-run setup: ./setup_isolated_env.sh"
    echo "  2. Install packages: ./install_all_packages.sh"
    echo "  3. Check docs: ../../TROUBLESHOOTING_MACOS.md"
    echo ""
    exit 1
fi
