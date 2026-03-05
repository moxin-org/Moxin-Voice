# Setup New Dora Chatbot Environment

This directory provides scripts and documentation for provisioning a clean environment for Dora voice-chat examples. It combines the quick setup instructions with the key manual steps and dependency references.

---

## 1. Quick Setup

```bash
cd examples/setup-new-chatbot
./setup_isolated_env.sh
```

The script will:

1. Verify prerequisites (Conda, Python, Git; Cargo optional)
2. Create a conda env `moxin-studio` with Python 3.12
3. Install pinned versions of NumPy (1.26.4), PyTorch (2.2.0), Transformers (4.45.0)
4. Install all voice-chat Python nodes (ASR, PrimeSpeech, Text Segmenter, Qwen3, SpeechMonitor)
5. Build Rust components (maas-client, openai-websocket) if Cargo is present
6. Offer to run validation scripts (ASR, PrimeSpeech, dependency checks)

Activate the environment afterwards:

```bash
conda activate moxin-studio
python test_dependencies.py
```

---

## 2. Dependencies Installation

Install the pinned dependencies manually by activating the environment and running the platform script:

```bash
conda activate moxin-studio

# Linux
./install_all_packages.sh
```

These scripts reproduce the package set from the automated setup, reinstall all Dora voice nodes in editable mode, and build the Rust components when Cargo is available.

For reference, the core Python commands they execute are listed below. You can run them individually if you need to customise the installation:

```bash
# Core libraries
pip install numpy==1.26.4 scipy==1.11.4 torchmetrics==1.0.0
pip install torch==2.2.0 torchaudio==2.2.0 torchvision==0.17.0
pip install transformers==4.45.0 huggingface-hub tqdm

# Dora voice nodes
pip install -e ../../node-hub/dora-asr[gpu]
pip install -e ../../node-hub/dora-primespeech
pip install -e ../../node-hub/dora-text-segmenter
pip install -e ../../node-hub/dora-speechmonitor
pip install -e ../../node-hub/dora-qwen3[torch]

# Optional Rust components
cargo build --release -p dora-openai-websocket
cargo build --release -p dora-maas-client
```

For a full dependency matrix, consult [DEPENDENCIES.md](./DEPENDENCIES.md).

---

## 3. Validation

```bash
python test_dependencies.py

# ASR
cd asr-validation
./run_all_tests.sh

# PrimeSpeech TTS
cd ../primespeech-validation
python test_tts_direct.py
```

---

## 4. Environment Variables

Set these if you need deterministic/offline behaviour:

```bash
export TRANSFORMERS_OFFLINE="1"
export HF_HUB_OFFLINE="1"
export PYTORCH_ENABLE_MPS_FALLBACK="1"   # macOS Metal fallback
```

---

## 5. Next Steps

1. Keep the environment activated (`conda activate moxin-studio`).
2. Download required models using `examples/model-manager/README.md` (FunASR, PrimeSpeech, Kokoro, Qwen MLX, etc.).
3. Run the voice-chat dataflows under `examples/mac-aec-chat/` following their README.

Enjoy building Dora chatbots!
