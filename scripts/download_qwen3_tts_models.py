#!/usr/bin/env python3
"""Download Qwen3 TTS model snapshots for the MLX backend."""

from __future__ import annotations

import argparse
import sys
from pathlib import Path


def ensure_hf_hub():
    try:
        from huggingface_hub import snapshot_download
        return snapshot_download
    except Exception:
        import subprocess

        subprocess.check_call([sys.executable, "-m", "pip", "install", "huggingface-hub"])
        from huggingface_hub import snapshot_download

        return snapshot_download


def model_ready(model_dir: Path) -> bool:
    required = [
        model_dir / "config.json",
        model_dir / "generation_config.json",
        model_dir / "vocab.json",
        model_dir / "merges.txt",
        model_dir / "speech_tokenizer" / "config.json",
        model_dir / "speech_tokenizer" / "model.safetensors",
    ]
    has_model_weights = (model_dir / "model.safetensors").exists() or (
        model_dir / "model.safetensors.index.json"
    ).exists()
    return has_model_weights and all(p.exists() for p in required)


def download_repo(snapshot_download, repo_id: str, target_dir: Path) -> None:
    target_dir.mkdir(parents=True, exist_ok=True)
    print(f"\n[Qwen3-TTS] Downloading {repo_id}")
    print(f"[Qwen3-TTS] Target: {target_dir}")
    snapshot_download(
        repo_id=repo_id,
        local_dir=str(target_dir),
        local_dir_use_symlinks=False,
        resume_download=True,
    )
    print(f"[Qwen3-TTS] Done: {repo_id}")


def main() -> int:
    parser = argparse.ArgumentParser(description="Download Qwen3 TTS models for the MLX backend")
    parser.add_argument("--root", required=True, help="Root dir for qwen3-tts models")
    parser.add_argument("--custom-repo", default="mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit")
    parser.add_argument("--base-repo", default="Qwen/Qwen3-TTS-12Hz-1.7B-Base")
    parser.add_argument("--custom-dir", default="Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit")
    parser.add_argument("--base-dir", default="Qwen3-TTS-12Hz-1.7B-Base")
    parser.add_argument("--need-custom", action="store_true", help="Require/download CustomVoice model")
    parser.add_argument("--need-base", action="store_true", help="Require/download Base model")
    args = parser.parse_args()

    snapshot_download = ensure_hf_hub()

    root = Path(args.root).expanduser().resolve()
    custom_dir = root / args.custom_dir
    base_dir = root / args.base_dir

    if not args.need_custom and not args.need_base:
        print("[Qwen3-TTS] Nothing requested, skip")
        return 0

    if args.need_custom:
        if model_ready(custom_dir):
            print(f"[Qwen3-TTS] CustomVoice already ready: {custom_dir}")
        else:
            download_repo(snapshot_download, args.custom_repo, custom_dir)
            if not model_ready(custom_dir):
                print(f"[Qwen3-TTS] ERROR: CustomVoice model incomplete: {custom_dir}")
                return 1

    if args.need_base:
        if model_ready(base_dir):
            print(f"[Qwen3-TTS] Base already ready: {base_dir}")
        else:
            download_repo(snapshot_download, args.base_repo, base_dir)
            if not model_ready(base_dir):
                print(f"[Qwen3-TTS] ERROR: Base model incomplete: {base_dir}")
                return 1

    print("[Qwen3-TTS] All requested models are ready")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
