#!/usr/bin/env python3
"""
register_speaker.py — Register a trained speaker in model config.

Updates config.json to include the new speaker:
  - adds "alice": 3067 to talker_config.spk_id
  - adds "alice": false to talker_config.spk_is_dialect
  - prints next steps for adding to moxin-voice UI

Usage:
    python register_speaker.py \
        --model_dir ~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit \
        --speaker_name alice \
        --speaker_id 3067 \
        --language chinese
"""

import argparse
import json
import shutil
import sys
from pathlib import Path


def register_speaker(
    model_dir: Path,
    speaker_name: str,
    speaker_id: int,
    language: str,
    dialect_id: str = None,
):
    """
    Update config.json with the new speaker.

    Parameters
    ----------
    model_dir     Path to the model directory containing config.json
    speaker_name  Name for the new speaker (e.g. "alice")
    speaker_id    Codec embedding table row index (e.g. 3067)
    language      Language code (must exist in codec_language_id)
    dialect_id    If speaker uses a dialect, set to the dialect name
                  (e.g. "sichuan_dialect"), otherwise None / False
    """
    cfg_path = model_dir / "config.json"
    if not cfg_path.exists():
        print(f"ERROR: config.json not found at {cfg_path}")
        sys.exit(1)

    # Load current config
    with open(cfg_path) as f:
        config = json.load(f)

    talker = config["talker_config"]

    # Validate language
    available_langs = list(talker.get("codec_language_id", {}).keys())
    if language not in talker.get("codec_language_id", {}):
        print(f"WARNING: language '{language}' not in codec_language_id.")
        print(f"  Available: {available_langs}")
        print(f"  You can add a new language entry manually to config.json if needed.")

    # Check for conflicts
    existing_spk_ids = talker.get("spk_id", {})
    if speaker_name in existing_spk_ids:
        existing_id = existing_spk_ids[speaker_name]
        if existing_id == speaker_id:
            print(f"Speaker '{speaker_name}' (id={speaker_id}) already registered — no-op.")
            return
        else:
            print(f"ERROR: speaker name '{speaker_name}' already exists with id={existing_id}.")
            print(f"  Use a different name or remove the existing entry first.")
            sys.exit(1)

    for name, sid in existing_spk_ids.items():
        if sid == speaker_id:
            print(f"ERROR: speaker_id={speaker_id} already used by '{name}'.")
            print(f"  Use a higher ID. Next free ID after all existing speakers:")
            next_id = max(existing_spk_ids.values()) + 1
            print(f"  Suggested: --speaker_id {next_id}")
            sys.exit(1)

    # Backup config
    backup_path = cfg_path.with_suffix(".json.bak")
    if not backup_path.exists():
        shutil.copy2(str(cfg_path), str(backup_path))
        print(f"Backup: {backup_path}")

    # Register speaker
    talker["spk_id"][speaker_name] = speaker_id
    talker["spk_is_dialect"][speaker_name] = dialect_id if dialect_id else False

    # Write updated config
    with open(cfg_path, "w", encoding="utf-8") as f:
        json.dump(config, f, indent=4, ensure_ascii=False)

    print(f"✓ Registered speaker '{speaker_name}' (id={speaker_id}) in {cfg_path}")
    print()

    # Print current speaker list
    print("Updated spk_id table:")
    for name, sid in sorted(talker["spk_id"].items(), key=lambda x: x[1]):
        is_dialect = talker["spk_is_dialect"].get(name, False)
        dialect_str = f"  [{is_dialect}]" if is_dialect else ""
        marker = " ← NEW" if name == speaker_name else ""
        print(f"  {name:20s} id={sid}{dialect_str}{marker}")

    print()
    print("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")
    print("NEXT STEPS — add speaker to the Moxin Voice UI")
    print("━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━")
    print()
    print("1. Open  apps/moxin-voice/src/screen.rs")
    print()
    print("   Locate the Qwen3 voice list (search for 'vivian' or 'serena').")
    print("   Add a new entry for your speaker:")
    print()
    print(f'   Qwen3Voice {{')
    print(f'       name: "{speaker_name}",')
    print(f'       name_zh: "{speaker_name}",   // Chinese display name if desired')
    print(f'       description: "Custom trained voice",')
    print(f'       description_zh: "自定义训练音色",')
    print(f'       language: Language::{language.capitalize()},')
    print(f'   }},')
    print()
    print("2. Rebuild the qwen-tts node:")
    print("   cargo build -p dora-qwen3-tts-mlx --release")
    print()
    print("3. Test the new voice:")
    print(f"   VOICE_NAME={speaker_name} cargo run -p moxin-voice-shell")
    print()
    print("   Or restart the app and select the new voice from the UI.")
    print()
    print("4. (Optional) Generate a preview WAV:")
    print("   Run a test synthesis and copy the output to:")
    print(f"   node-hub/dora-qwen3-tts-mlx/previews/{speaker_name}_preview.wav")


def main():
    parser = argparse.ArgumentParser(description="Register trained speaker in config.json")
    parser.add_argument("--model_dir", required=True,
                        help="Path to model directory (contains config.json)")
    parser.add_argument("--speaker_name", required=True,
                        help="Speaker name to register (e.g. alice)")
    parser.add_argument("--speaker_id", type=int, required=True,
                        help="Codec embedding row ID (e.g. 3067)")
    parser.add_argument("--language", default="chinese",
                        help="Language code (default: chinese)")
    parser.add_argument("--dialect", default=None,
                        help="Dialect key if applicable (e.g. sichuan_dialect), else omit")
    args = parser.parse_args()

    model_dir = Path(args.model_dir).expanduser()
    register_speaker(
        model_dir,
        args.speaker_name,
        args.speaker_id,
        args.language,
        dialect_id=args.dialect,
    )


if __name__ == "__main__":
    main()
