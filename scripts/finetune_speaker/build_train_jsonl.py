#!/usr/bin/env python3
"""
Build Qwen3-TTS official training JSONL from local wav/txt pairs.

Each output line format:
  {"audio": "...wav", "text": "...", "ref_audio": "...wav", "language": "Auto"}
"""

import argparse
import json
from pathlib import Path


def load_text(path: Path) -> str:
    text = path.read_text(encoding="utf-8").strip()
    if not text:
        raise ValueError(f"Empty transcript: {path}")
    return text


def main() -> None:
    parser = argparse.ArgumentParser(description="Build train_raw.jsonl for Qwen3-TTS official finetuning")
    parser.add_argument("--audio_dir", required=True, help="Directory containing wav files")
    parser.add_argument("--text_dir", required=True, help="Directory containing txt files with same stems")
    parser.add_argument("--ref_audio", required=True, help="Reference speaker wav path (recommended same for all lines)")
    parser.add_argument("--output_jsonl", required=True, help="Output JSONL path")
    parser.add_argument("--language", default="Auto", help="Language field value, default Auto")
    parser.add_argument("--ext", default="wav", help="Audio extension to scan, default wav")
    args = parser.parse_args()

    audio_dir = Path(args.audio_dir).expanduser().resolve()
    text_dir = Path(args.text_dir).expanduser().resolve()
    ref_audio = Path(args.ref_audio).expanduser().resolve()
    output_jsonl = Path(args.output_jsonl).expanduser().resolve()

    if not audio_dir.is_dir():
        raise FileNotFoundError(f"audio_dir not found: {audio_dir}")
    if not text_dir.is_dir():
        raise FileNotFoundError(f"text_dir not found: {text_dir}")
    if not ref_audio.exists():
        raise FileNotFoundError(f"ref_audio not found: {ref_audio}")

    wavs = sorted(audio_dir.glob(f"*.{args.ext.lstrip('.')}"))
    if not wavs:
        raise RuntimeError(f"No .{args.ext} files found in {audio_dir}")

    rows = []
    for wav in wavs:
        txt = text_dir / f"{wav.stem}.txt"
        if not txt.exists():
            raise FileNotFoundError(f"Missing transcript for {wav.name}: {txt}")

        rows.append(
            {
                "audio": str(wav),
                "text": load_text(txt),
                "ref_audio": str(ref_audio),
                "language": args.language,
            }
        )

    output_jsonl.parent.mkdir(parents=True, exist_ok=True)
    with output_jsonl.open("w", encoding="utf-8") as f:
        for row in rows:
            f.write(json.dumps(row, ensure_ascii=False) + "\n")

    print(f"[build_jsonl] wrote {len(rows)} lines -> {output_jsonl}")
    print(f"[build_jsonl] ref_audio={ref_audio}")


if __name__ == "__main__":
    main()
