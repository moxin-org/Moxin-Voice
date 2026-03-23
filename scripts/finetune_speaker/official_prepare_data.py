#!/usr/bin/env python3
# coding=utf-8
"""
Official-style data preparation for Qwen3-TTS finetuning.

Converts train_raw.jsonl -> train_with_codes.jsonl by adding `audio_codes`.
"""

import argparse
import json

from qwen_tts import Qwen3TTSTokenizer

BATCH_INFER_NUM = 32


def main() -> None:
    parser = argparse.ArgumentParser()
    parser.add_argument("--device", type=str, default="cuda:0")
    parser.add_argument("--tokenizer_model_path", type=str, default="Qwen/Qwen3-TTS-Tokenizer-12Hz")
    parser.add_argument("--input_jsonl", type=str, required=True)
    parser.add_argument("--output_jsonl", type=str, required=True)
    args = parser.parse_args()

    tokenizer_12hz = Qwen3TTSTokenizer.from_pretrained(
        args.tokenizer_model_path,
        device_map=args.device,
    )

    with open(args.input_jsonl, "r", encoding="utf-8") as f:
        total_lines = [json.loads(line.strip()) for line in f if line.strip()]

    final_lines = []
    batch_lines = []
    batch_audios = []

    for line in total_lines:
        batch_lines.append(line)
        batch_audios.append(line["audio"])

        if len(batch_lines) >= BATCH_INFER_NUM:
            enc_res = tokenizer_12hz.encode(batch_audios)
            for code, item in zip(enc_res.audio_codes, batch_lines):
                item["audio_codes"] = code.cpu().tolist()
                final_lines.append(item)
            batch_lines.clear()
            batch_audios.clear()

    if batch_audios:
        enc_res = tokenizer_12hz.encode(batch_audios)
        for code, item in zip(enc_res.audio_codes, batch_lines):
            item["audio_codes"] = code.cpu().tolist()
            final_lines.append(item)

    with open(args.output_jsonl, "w", encoding="utf-8") as f:
        for item in final_lines:
            f.write(json.dumps(item, ensure_ascii=False) + "\n")

    print(f"[prepare_data] input={len(total_lines)} output={len(final_lines)} -> {args.output_jsonl}")


if __name__ == "__main__":
    main()
