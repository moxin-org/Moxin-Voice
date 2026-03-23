# Qwen3-TTS Fine-tuning for moxin-tts

This directory now provides **two paths**:

- **Recommended (official)**: full fine-tuning aligned with Qwen3-TTS official `finetuning/` scripts.
- **Legacy (MLX row update)**: the old `train.py` path that only optimizes one speaker row in `codec_embedding`.

If your goal is stable custom voice quality, use the official path.

## Official path (recommended)

Reference implementation (Qwen official):
- `QwenLM/Qwen3-TTS` `finetuning/README.md`
- `prepare_data.py`, `dataset.py`, `sft_12hz.py`

Local scripts in this repo:
- `build_train_jsonl.py` (new)
- `official_prepare_data.py` (official-style)
- `official_dataset.py` (official-style)
- `official_sft_12hz.py` (official-style + safer speaker_id handling)

### 0) Environment

Official SFT uses **PyTorch + CUDA**, not MLX.

```bash
conda create -n qwen3-tts-ft python=3.12 -y
conda activate qwen3-tts-ft
pip install -U qwen-tts accelerate transformers safetensors soundfile librosa
pip install -U flash-attn --no-build-isolation   # optional but recommended
```

If you are not on CUDA, you can still try:
- `--device_map mps` (Apple Silicon) or `--device_map cpu`
- `--dtype float16` or `--dtype float32`
- `--attn_implementation eager`

### 1) Build `train_raw.jsonl`

Your wav/txt pairs are expected as same stem names (e.g. `001.wav` + `001.txt`).

```bash
python build_train_jsonl.py \
  --audio_dir /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/raw \
  --text_dir /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/text \
  --ref_audio /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/raw/001.wav \
  --output_jsonl /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/train_raw.jsonl \
  --language Chinese
```

Notes:
- Keep `ref_audio` fixed across all rows (official recommendation).
- Audio should be 24kHz mono wav.

### 2) Prepare audio codes

```bash
python official_prepare_data.py \
  --device cuda:0 \
  --tokenizer_model_path Qwen/Qwen3-TTS-Tokenizer-12Hz \
  --input_jsonl /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/train_raw.jsonl \
  --output_jsonl /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/train_with_codes.jsonl
```

### 3) Run official-style SFT

```bash
python official_sft_12hz.py \
  --init_model_path Qwen/Qwen3-TTS-12Hz-1.7B-Base \
  --output_model_path /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/output_official \
  --train_jsonl /Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/train_with_codes.jsonl \
  --device_map cuda:0 \
  --dtype bfloat16 \
  --attn_implementation flash_attention_2 \
  --batch_size 2 \
  --lr 2e-6 \
  --num_epochs 10 \
  --speaker_name yangyang_new \
  --speaker_id 3068
```

Output checkpoints:
- `output_official/checkpoint-epoch-0`
- `output_official/checkpoint-epoch-1`
- ...

Each checkpoint includes:
- `config.json` with `tts_model_type=custom_voice`
- `talker_config.spk_id = {"yangyang_new": 3068}`
- patched `model.safetensors`

### 4) Plug into moxin-tts node

Point node to the trained checkpoint directory:

```bash
export QWEN3_TTS_CUSTOMVOICE_MODEL_DIR=/Users/alan0x/Documents/projects/moxin-tts/scripts/finetune_speaker/yangyang_new/output_official/checkpoint-epoch-9
```

Then start backend as usual.

If you want it persistent, copy that checkpoint to:
- `~/.OminiX/models/qwen3-tts-mlx/<your-custom-model-dir>`
- and set `QWEN3_TTS_CUSTOMVOICE_MODEL_DIR` to that path.

## Legacy path (kept for compatibility)

Existing scripts are unchanged:
- `encode_audio.py`
- `train.py`
- `apply_checkpoint.py`
- `register_speaker.py`

Use this only for fast experiments; quality/stability is generally worse than official SFT.

## Troubleshooting

- `PyTorch not found` / `torch>=2.4` warnings: official path needs a clean CUDA torch env.
- `loss=nan` at step 1: lower LR, ensure bf16 support, verify audio/text quality and sample rate.
- Generated noise after training: test earlier checkpoints and verify `QWEN3_TTS_CUSTOMVOICE_MODEL_DIR` points to the intended checkpoint.
