#!/usr/bin/env python3
"""批量导出所有预置音色的 VITS ONNX 模型。

使用方法:
    conda run -n mofa-studio python3 scripts/export_all_vits_onnx.py
"""

import sys
import os
import subprocess
from pathlib import Path

MOYOYO_TTS = Path(__file__).parent.parent / "node-hub/dora-primespeech/dora_primespeech/moyoyo_tts"
if not MOYOYO_TTS.exists():
    # Bundle path fallback
    MOYOYO_TTS = Path(__file__).parent / "node-hub/dora-primespeech/dora_primespeech/moyoyo_tts"

SRC_WEIGHTS = Path(
    os.environ.get(
        "PRIMESPEECH_SOVITS_SRC",
        str(Path.home() / ".dora/models/primespeech/moyoyo/SoVITS_weights"),
    )
)
DST_BASE = Path(
    os.environ.get(
        "GPT_SOVITS_VOICES_DIR",
        str(Path.home() / ".OminiX/models/gpt-sovits-mlx/voices"),
    )
)

EXPORT_SCRIPT = Path(
    os.environ.get(
        "OMINIX_EXPORT_VITS_SCRIPT",
        str(
            Path(__file__).parent.parent
            / "node-hub/moxin-tts-node/patches/gpt-sovits-mlx/scripts/export_vits_onnx.py"
        ),
    )
)
if not EXPORT_SCRIPT.exists():
    EXPORT_SCRIPT = Path(__file__).parent / "omx-scripts/export_vits_onnx.py"

# (voice_dir_name, sovits_pth_filename)
VOICES = [
    ("Doubao",     "doubao-mixed.pth"),
    ("LuoXiang",   "luoxiang_best_sovits.pth"),
    ("YangMi",     "yangmi_best_sovits.pth"),
    ("ZhouJielun", "zhoujielun_best_sovits.pth"),
    ("MaYun",      "mayun_best_sovits.pth"),
    ("ChenYifan",  "yfc_best_sovits.pth"),
    ("ZhaoDaniu",  "dnz_best_sovits.pth"),
    ("Maple",      "maple_best_sovits.pth"),
    ("Cove",       "cove_best_sovits.pth"),
    ("BYS",        "bys_best_sovits.pth"),
    ("Ellen",      "ellen_best_sovits.pth"),
    ("Juniper",    "juniper_best_sovits.pth"),
    ("MaBaoguo",   "mabaoguo_best_sovits.pth"),
    ("ShenYi",     "shenyi_best_sovits.pth"),
    ("Trump",      "trump_best_sovits.pth"),
]

failed = []

for (name, pth_file) in VOICES:
    dst_onnx = DST_BASE / name / "vits.onnx"
    if dst_onnx.exists():
        print(f"⏭  {name}: already exported, skipping")
        continue

    src_pth = SRC_WEIGHTS / pth_file
    if not src_pth.exists():
        print(f"❌ {name}: source not found: {src_pth}")
        failed.append(name)
        continue

    print(f"\n🔄 Exporting {name} ({pth_file}) ...")
    result = subprocess.run(
        [sys.executable, str(EXPORT_SCRIPT),
         "--moyoyo-tts", str(MOYOYO_TTS),
         "--checkpoint", str(src_pth),
         "--output", str(dst_onnx)],
        capture_output=False,
    )
    if result.returncode == 0:
        size_mb = dst_onnx.stat().st_size // (1024 * 1024)
        print(f"✅ {name}: {dst_onnx} ({size_mb} MB)")
    else:
        print(f"❌ {name}: export failed (exit code {result.returncode})")
        failed.append(name)

print("\n" + "="*50)
if failed:
    print(f"⚠️  Failed: {failed}")
    sys.exit(1)
else:
    print(f"🎉 All VITS ONNX files exported to {DST_BASE}")
