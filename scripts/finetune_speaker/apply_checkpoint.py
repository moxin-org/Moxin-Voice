#!/usr/bin/env python3
"""
apply_checkpoint.py — Apply a trained speaker embedding to model.safetensors.

Patches talker.model.codec_embedding.weight at a specific speaker row index
(`--speaker_id`). Appends only when speaker_id == current row count.

Usage:
    python apply_checkpoint.py \
        --model_dir  ~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit \
        --checkpoint baiyang/checkpoints_baiyang/spk_emb_final.npz \
        --speaker_id 3067
"""

import argparse
import shutil
import sys
from pathlib import Path

import mlx.core as mx
import numpy as np


def main():
    parser = argparse.ArgumentParser(description="Apply trained speaker embedding to model.safetensors")
    parser.add_argument(
        "--model_dir",
        default="~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit",
        help="Path to model directory containing model.safetensors",
    )
    parser.add_argument(
        "--checkpoint",
        required=True,
        help="Path to spk_emb_final.npz (or any spk_emb_step*.npz)",
    )
    parser.add_argument(
        "--speaker_id",
        type=int,
        default=3067,
        help="Row index to insert into codec_embedding (default: 3067)",
    )
    args = parser.parse_args()

    model_dir = Path(args.model_dir).expanduser()
    ckpt_path = Path(args.checkpoint).expanduser()
    out_path  = model_dir / "model.safetensors"
    backup    = model_dir / "model.safetensors.bak"

    if not ckpt_path.exists():
        print(f"ERROR: checkpoint not found: {ckpt_path}")
        sys.exit(1)

    if not out_path.exists():
        print(f"ERROR: model.safetensors not found at {out_path}")
        sys.exit(1)

    # Backup original weights (only once)
    if not backup.exists():
        shutil.copy2(str(out_path), str(backup))
        print(f"Backup → {backup}")
    else:
        print(f"Backup already exists → {backup}")

    # Load trained embedding
    data    = np.load(str(ckpt_path))
    spk_emb = mx.array(data["emb"])       # [2048]
    print(f"Loaded checkpoint: {ckpt_path}  shape={spk_emb.shape}")

    # Load full model weights (mx.load handles bfloat16 + uint32 natively)
    print("Loading model.safetensors (this may take a moment) ...")
    weights = mx.load(str(out_path))

    emb_key = "talker.model.codec_embedding.weight"
    old_emb = weights[emb_key]                              # [N, 2048] bfloat16
    n_rows = int(old_emb.shape[0])
    if args.speaker_id < 0:
        print(f"ERROR: speaker_id must be non-negative, got {args.speaker_id}")
        sys.exit(1)
    if args.speaker_id > n_rows:
        print(
            f"ERROR: speaker_id={args.speaker_id} out of range for embedding rows={n_rows}. "
            f"Use id in [0, {n_rows}]"
        )
        sys.exit(1)

    new_row = spk_emb.reshape(1, 2048).astype(mx.bfloat16)
    if args.speaker_id == n_rows:
        updated = mx.concatenate([old_emb, new_row], axis=0)
        action = "appended"
    else:
        before = old_emb[:args.speaker_id, :]
        after = old_emb[args.speaker_id + 1:, :]
        updated = mx.concatenate([before, new_row, after], axis=0)
        action = "updated"

    weights[emb_key] = updated
    mx.eval(weights[emb_key])

    old_n, new_n = old_emb.shape[0], weights[emb_key].shape[0]
    print(
        f"{emb_key}: [{old_n}, 2048] → [{new_n}, 2048]  "
        f"({action} row {args.speaker_id})"
    )

    mx.save_safetensors(str(out_path), weights)
    print(f"\nDone → {out_path}")
    print()
    print("Next step: register the speaker in config.json:")
    print(f"  python register_speaker.py \\")
    print(f"      --model_dir {args.model_dir} \\")
    print(f"      --speaker_name <name> \\")
    print(f"      --speaker_id {args.speaker_id} \\")
    print(f"      --language chinese")


if __name__ == "__main__":
    main()
