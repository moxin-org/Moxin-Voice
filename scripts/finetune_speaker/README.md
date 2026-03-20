# Qwen3-TTS CustomVoice Speaker Fine-tuning

Add a new speaker identity to the `Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit` model
by training a single 2048-dim embedding row.  The 28-layer transformer stays
completely frozen, so training is fast and requires no GPU.

## Architecture background

| Component | Detail |
|---|---|
| Speaker token | Row index in `talker.model.codec_embedding.weight` [3072, 2048] |
| Existing speakers | vivian=3065, serena=3066, ryan=3061, … (9 total) |
| Next free ID | **3067** |
| `codec_embedding` | **Not quantized** — plain bfloat16, directly editable |
| Trainable params | 1 × 2048 = 2 048 floats |
| Training loss | Cross-entropy on `codec_head` logits predicting codebook-0 frames |

## Quick start (end-to-end example for speaker "alice")

```bash
# 0. Activate an environment that has the required packages
conda activate mofa-studio   # has mlx 0.30, torch, transformers, librosa, safetensors

# 1. Prepare directories
mkdir -p data/raw/alice data/encoded/alice data/text/alice

# Copy ~10–30 WAV files (each 3–15 s, 24 kHz mono preferred) into data/raw/alice/
# Copy matching .txt transcripts into data/text/alice/
# (filenames must match: e.g. 001.wav ↔ 001.txt)

# 2. Encode audio → codec frames
python encode_audio.py \
    --audio_dir  data/raw/alice/ \
    --out_dir    data/encoded/alice/ \
    --tokenizer_path ~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit/speech_tokenizer

# Expected output per file:
#   001.wav → 001.npz  shape=(125, 16)  (10.0s)

# 3. Fine-tune the speaker embedding
python train.py \
    --model_dir  ~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit \
    --encoded_dir data/encoded/alice/ \
    --text_dir    data/text/alice/ \
    --speaker_name alice \
    --speaker_id   3067 \
    --language     chinese \
    --lr 1e-3 --epochs 20 --batch_size 4

# 4. Register the speaker in config.json
python register_speaker.py \
    --model_dir  ~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit \
    --speaker_name alice \
    --speaker_id   3067 \
    --language     chinese
```

## Dependencies

```bash
pip install mlx safetensors "transformers>=4.57" librosa soundfile tokenizers
```

| Package | Minimum version | Purpose |
|---|---|---|
| `mlx` | 0.25 | Forward pass + gradient |
| `safetensors` | 0.4 | Load / save model weights |
| `transformers` | 4.57 | Speech tokenizer (encode_audio.py) |
| `librosa` | 0.10 | Audio resampling |
| `soundfile` | 0.12 | WAV loading |
| `tokenizers` | 0.15 | BPE text tokenization |

> **transformers < 4.57**: `encode_audio.py` will attempt a MimiModel fallback.
> Results may be slightly different.  Upgrade is strongly recommended.

## Script reference

### `encode_audio.py`

Encodes WAV files to codec frame `.npz` archives (one per audio file).

```
Arguments:
  --audio_dir        Directory of input WAV files
  --out_dir          Output directory for .npz files
  --tokenizer_path   Path to speech_tokenizer directory
                     (default: ~/.OminiX/models/…/speech_tokenizer)
  --sample_rate      Target sample rate (default: 24000)
  --ext              File extension to scan (default: wav)
```

Output format: `codes` array of shape `[T, 16]` int16, where T = frames at 12.5 Hz.

### `train.py`

MLX fine-tuning loop.  Adds one new 2048-dim row to `codec_embedding.weight`.

```
Arguments:
  --model_dir        Model directory
  --encoded_dir      Directory of .npz codec frame files
  --text_dir         Directory of .txt transcript files (same stem as .npz)
  --speaker_name     New speaker name
  --speaker_id       New speaker token ID (default: 3067)
  --language         Language for codec control token (default: chinese)
  --lr               Learning rate (default: 1e-3)
  --epochs           Training epochs (default: 20)
  --batch_size       Gradient accumulation steps (default: 4)
  --checkpoint_every Save embedding .npz every N gradient steps (default: 50)
  --max_frames       Truncate audio longer than N codec frames (default: 600)
  --noise_scale      Init noise stddev added to mean speaker embedding (default: 0.01)
  --seed             Random seed (default: 42)
```

**Checkpoints** are saved to `<encoded_dir>/../checkpoints_<speaker>/`:
- `spk_emb_step<N>.npz` — intermediate checkpoints
- `spk_emb_final.npz`   — final embedding after all epochs

**Output**: overwrites `model_dir/model.safetensors` with the extended
`codec_embedding.weight` ([3073, 2048]).  A backup is created at
`model.safetensors.bak` before first modification.

### `register_speaker.py`

Updates `config.json` to recognise the new speaker.

```
Arguments:
  --model_dir        Model directory
  --speaker_name     Speaker name to register
  --speaker_id       Token ID used during training
  --language         Language code (default: chinese)
  --dialect          Optional dialect key (e.g. sichuan_dialect)
```

Prints step-by-step instructions for adding the voice to the Moxin Voice UI
(`apps/moxin-voice/src/screen.rs`).

## Data preparation guidelines

| Attribute | Recommendation |
|---|---|
| Quantity | 10–30 sentences, ≥ 2 minutes total |
| Duration per clip | 3–15 seconds |
| Format | WAV, 24 kHz, mono, 16-bit |
| Content | Varied sentences covering target language phonemes |
| Noise | Clean, minimal background noise |
| Transcripts | Accurate text matching the audio exactly |

## Token sequence (for reference)

```
Pos 0-2:  text=[im_start, assistant, \n],      codec=[pad, pad, pad]
Pos 3-7:  text=[tts_pad×5],                    codec=[think, think_bos, lang, think_eos, spk_id]
Pos 8:    text=[tts_bos],                       codec=[pad]
Pos 9:    text=[text_token_0],                  codec=[codec_bos]
Pos 10+:  text=[text_tokens…, tts_eos, pad…],  codec=[frame_0, frame_1, …, codec_eos]
```

Training loss is computed only on positions 9 → end (codec frame prediction).

## Verification checklist

```bash
# 1. Encoding: verify .npz files have correct shape
python -c "
import numpy as np, glob
for p in sorted(glob.glob('data/encoded/alice/*.npz'))[:3]:
    d = np.load(p)
    print(p, d['codes'].shape, d['codes'].dtype)
"
# Expected: (T, 16) int16

# 2. Training smoke test (1 epoch)
python train.py \
    --model_dir  ~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit \
    --encoded_dir data/encoded/alice/ \
    --text_dir    data/text/alice/ \
    --speaker_name alice --speaker_id 3067 --epochs 1

# 3. Config registration
python register_speaker.py --model_dir ... --speaker_name alice --speaker_id 3067

# 4. Rebuild and test
cargo build -p dora-qwen3-tts-mlx --release
# Launch app and select 'alice' voice
```

## Troubleshooting

**`KeyError: 'qwen3_tts_tokenizer_12hz'` in encode_audio.py**
→ Upgrade transformers: `pip install "transformers>=4.57"`

**Loss not decreasing after epoch 5**
→ Try reducing `--lr` to `3e-4`, or increase `--noise_scale` to `0.05`

**`codec_embedding.weight` shape mismatch after patching**
→ Restore from backup: `cp model.safetensors.bak model.safetensors`
→ Re-run `train.py` after fixing the issue

**Out of memory**
→ Reduce `--max_frames` (e.g. `--max_frames 300`)
→ Reduce `--batch_size` to 1

**Speaker sounds like default voice**
→ Train for more epochs (try 50–100)
→ Use more and more varied training audio
→ Check transcript accuracy
