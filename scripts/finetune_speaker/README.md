# Qwen3-TTS CustomVoice Finetune (MLX only)

This directory is for **MLX-side speaker embedding finetune** used by moxin-tts.

Goal:
- finetune one speaker embedding row for `Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit`
- save speaker embedding checkpoints (`spk_emb_*.npz`)
- (optional, dangerous) patch `model.safetensors`

## Pipeline

1. Prepare wav + text pairs
- audio files: `<speaker>/raw/*.wav`
- transcript files: `<speaker>/text/*.txt` (same stem names)

2. Encode audio to codec frames

```bash
python encode_audio.py \
  --in_dir /path/to/<speaker>/raw \
  --out_dir /path/to/<speaker>/encoded \
  --tokenizer_path ~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit/speech_tokenizer
```

3. Train speaker embedding (MLX)

```bash
python train.py \
  --model_dir ~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit \
  --encoded_dir /path/to/<speaker>/encoded \
  --text_dir /path/to/<speaker>/text \
  --speaker_name <speaker_name> \
  --speaker_id <new_id> \
  --language chinese \
  --epochs 20 \
  --batch_size 4
```

Notes:
- `--speaker_id` is required (manual mode).
- if `speaker_id` is already used by existing speakers, train will fail by default.
- default is **safe mode**: train only saves `spk_emb_*.npz`, does not modify model weights.
- tokenizer fails fast on invalid fallback (no char-level fallback).

If you still want to patch model weights (not recommended on 8bit models):

```bash
python train.py ... --patch_model --allow_patch_quantized
```

4. Register speaker in config (only if you actually patched model weights)

```bash
python register_speaker.py \
  --model_dir ~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit \
  --speaker_name <speaker_name> \
  --speaker_id <resolved_id> \
  --language chinese
```

5. Restart backend and test in UI/node

## Utilities

- `apply_checkpoint.py`: apply saved `spk_emb_*.npz` to model weights
- `build_train_jsonl.py`, `official_prepare_data.py`, `official_dataset.py`:
  kept as auxiliary/reference tools only, not the primary MLX training path
