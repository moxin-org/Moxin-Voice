#!/usr/bin/env python3
"""批量转换 dora-primespeech 的所有预置音色到 gpt-sovits-mlx 格式。

使用方法:
    conda run -n mofa-studio python3 scripts/convert_all_voices.py
"""

import json
import shutil
import sys
from pathlib import Path

# 将 OminiX-MLX 的 scripts 目录加入 path，复用转换函数
OMINIX_SCRIPTS = Path.home() / "Documents/projects/OminiX-MLX/gpt-sovits-mlx/scripts"
sys.path.insert(0, str(OMINIX_SCRIPTS))

# SoVITS .pth 用 pickle 保存，加载时需要 GPT-SoVITS 的 utils 模块在 sys.path 上
GPTSOVITS_SRC = Path(__file__).parent.parent / "node-hub/dora-primespeech/dora_primespeech/moyoyo_tts"
sys.path.insert(0, str(GPTSOVITS_SRC.parent))  # 使 import moyoyo_tts.xxx 可用
sys.path.insert(0, str(GPTSOVITS_SRC))          # 使 import utils 可用

from setup_models import convert_gpt, convert_sovits, convert_hubert, convert_bert

# ---------------------------------------------------------------------------
# 路径配置
# ---------------------------------------------------------------------------

SRC = Path.home() / ".dora/models/primespeech/moyoyo"
DST = Path.home() / ".OminiX/models/gpt-sovits-mlx"

# ---------------------------------------------------------------------------
# 15 个预置音色（从 config.py VOICE_CONFIGS 提取的精确文件名）
# 格式: (目录名, gpt_ckpt, sovits_pth, ref_wav, prompt_text, language, speed)
# ---------------------------------------------------------------------------

VOICES = [
    ("Doubao",     "doubao-mixed.ckpt",        "doubao-mixed.pth",        "doubao_ref_mix_new.wav",
     "这家resturant的steak很有名，但是vegetable salad的price有点贵", "zh", 1.1),
    ("LuoXiang",   "luoxiang_best_gpt.ckpt",   "luoxiang_best_sovits.pth", "luoxiang_ref.wav",
     "复杂的问题背后也许没有统一的答案，选择站在正方还是反方，其实取决于你对一系列价值判断的回答。", "zh", 1.1),
    ("YangMi",     "yangmi_best_gpt.ckpt",     "yangmi_best_sovits.pth",   "yangmi_ref.wav",
     "你谁知道，人生只有一次啊。你怎么知道那样选，你当下来说，应该那样选，为什么没那样选呢，但你今天这样选了呀。", "zh", 1.1),
    ("ZhouJielun", "zhoujielun_best_gpt.ckpt", "zhoujielun_best_sovits.pth","zhoujielun_ref.wav",
     "其实我我现在讲的这些奥，都是我未来成功的一些关键。", "zh", 1.1),
    ("MaYun",      "mayun_best_gpt.ckpt",      "mayun_best_sovits.pth",    "mayun_ref.wav",
     "这是我们最大的希望能招聘的到人。所以今天阿里巴巴公司内部，我自己这么觉得，人才梯队的建设非常之好。", "zh", 1.1),
    ("ChenYifan",  "yfc_best_gpt.ckpt",        "yfc_best_sovits.pth",      "yfc_ref.wav",
     "他们的一个专家Marcel Marini的观点，他就提醒说这波投资热潮可能有点哎，我们说过热的迹象。", "zh", 1.1),
    ("ZhaoDaniu",  "dnz_best_gpt.ckpt",        "dnz_best_sovits.pth",      "dnz_ref.wav",
     "今天啊我们要跟你一起深入探讨一篇嗯，来自经济学院的the economist的文章。", "zh", 1.1),
    ("Maple",      "maple_best_gpt.ckpt",      "maple_best_sovits.pth",    "maple_ref.wav",
     "There was a little tea shop in a bustling village. Every morning, the owner, an elderly woman, would wake up early.", "en", 1.0),
    ("Cove",       "cove_best_gpt.ckpt",       "cove_best_sovits.pth",     "cove_ref.wav",
     "and he has a long career in the Senate representing Delaware. So both have had significant impacts on American politics and policies.", "en", 1.0),
    ("BYS",        "bys_best_gpt.ckpt",        "bys_best_sovits.pth",      "bys_ref.wav",
     "今天天气不错，适合出去走走。", "zh", 1.1),
    ("Ellen",      "ellen_best_gpt.ckpt",      "ellen_best_sovits.pth",    "ellen_ref.wav",
     "Welcome to the show! Today we have some amazing guests.", "en", 1.1),
    ("Juniper",    "juniper_best_gpt.ckpt",    "juniper_best_sovits.pth",  "juniper_ref.wav",
     "The forest was quiet, with only the sound of leaves rustling in the wind.", "en", 1.1),
    ("MaBaoguo",   "mabaoguo_best_gpt.ckpt",   "mabaoguo_best_sovits.pth", "mabaoguo_ref.wav",
     "年轻人不讲武德，偷袭我这个六十九岁的老同志。", "zh", 1.1),
    ("ShenYi",     "shenyi_best_gpt.ckpt",     "shenyi_best_sovits.pth",   "shenyi_ref.wav",
     "今天我们来分析一下这个案例的关键点。", "zh", 1.1),
    ("Trump",      "trump_best_gpt.ckpt",      "trump_best_sovits.pth",    "trump_ref.wav",
     "We're going to make America great again, and it's going to be tremendous.", "en", 1.1),
]


def convert_encoders():
    """转换共享编码器（HuBERT、BERT tokenizer）"""
    enc_dir = DST / "encoders"
    enc_dir.mkdir(parents=True, exist_ok=True)

    # HuBERT
    hubert_src = SRC / "chinese-hubert-base"
    hubert_dst = enc_dir / "hubert.safetensors"
    if hubert_dst.exists():
        print(f"⏭  HuBERT already converted, skipping")
    elif hubert_src.exists():
        print(f"🔄 Converting HuBERT...")
        convert_hubert(hubert_src, hubert_dst)
        print(f"✅ HuBERT → {hubert_dst}")
    else:
        print(f"⚠️  HuBERT source not found at {hubert_src}")

    # BERT weights
    bert_src_dir = SRC / "chinese-roberta-wwm-ext-large"
    bert_dst = enc_dir / "bert.safetensors"
    if bert_dst.exists():
        print(f"⏭  BERT already converted, skipping")
    elif bert_src_dir.exists():
        print(f"🔄 Converting BERT...")
        convert_bert(bert_src_dir, bert_dst)
        print(f"✅ BERT → {bert_dst}")
    else:
        print(f"⚠️  BERT source not found at {bert_src_dir}")

    # BERT tokenizer（直接复制目录）
    bert_src = SRC / "chinese-roberta-tokenizer"
    bert_dst = DST / "bert-tokenizer"
    if bert_dst.exists():
        print(f"⏭  BERT tokenizer already copied, skipping")
    elif bert_src.exists():
        print(f"📁 Copying BERT tokenizer...")
        shutil.copytree(str(bert_src), str(bert_dst))
        print(f"✅ BERT tokenizer → {bert_dst}")
    else:
        # 尝试从 chinese-roberta-wwm-ext-large 取 tokenizer 文件
        alt_src = SRC / "chinese-roberta-wwm-ext-large"
        if alt_src.exists():
            bert_dst.mkdir(parents=True, exist_ok=True)
            for fname in ["tokenizer.json", "vocab.txt", "tokenizer_config.json"]:
                f = alt_src / fname
                if f.exists():
                    shutil.copy(str(f), str(bert_dst / fname))
            print(f"✅ BERT tokenizer (from roberta) → {bert_dst}")
        else:
            print(f"⚠️  BERT tokenizer source not found")


def convert_all_voices():
    """批量转换所有预置音色"""
    voices_dir = DST / "voices"
    voices_dir.mkdir(parents=True, exist_ok=True)

    voices_json = {}
    failed = []

    for (name, gpt_file, sovits_file, ref_file, prompt, lang, speed) in VOICES:
        print(f"\n{'='*50}")
        print(f"🎤 Processing voice: {name}")

        voice_dir = voices_dir / name
        voice_dir.mkdir(exist_ok=True)

        gpt_src    = SRC / "GPT_weights"   / gpt_file
        sovits_src = SRC / "SoVITS_weights" / sovits_file
        ref_src    = SRC / "ref_audios"     / ref_file

        gpt_dst    = voice_dir / "gpt.safetensors"
        sovits_dst = voice_dir / "sovits.safetensors"
        ref_dst    = voice_dir / "reference.wav"

        ok = True

        # --- GPT 权重转换 ---
        if gpt_dst.exists():
            print(f"  ⏭  GPT already converted")
        elif gpt_src.exists():
            try:
                convert_gpt(gpt_src, gpt_dst)
                print(f"  ✅ GPT converted")
            except Exception as e:
                print(f"  ❌ GPT conversion failed: {e}")
                ok = False
        else:
            print(f"  ❌ GPT source not found: {gpt_src}")
            ok = False

        # --- SoVITS 权重转换 ---
        if sovits_dst.exists():
            print(f"  ⏭  SoVITS already converted")
        elif sovits_src.exists():
            try:
                convert_sovits(sovits_src, sovits_dst)
                print(f"  ✅ SoVITS converted")
            except Exception as e:
                print(f"  ❌ SoVITS conversion failed: {e}")
                ok = False
        else:
            print(f"  ❌ SoVITS source not found: {sovits_src}")
            ok = False

        # --- 参考音频（直接复制）---
        if ref_dst.exists():
            print(f"  ⏭  Reference audio already copied")
        elif ref_src.exists():
            shutil.copy(str(ref_src), str(ref_dst))
            print(f"  ✅ Reference audio copied")
        else:
            print(f"  ❌ Reference audio not found: {ref_src}")
            ok = False

        if ok:
            voices_json[name] = {
                "gpt":        f"{name}/gpt.safetensors",
                "sovits":     f"{name}/sovits.safetensors",
                "reference":  f"{name}/reference.wav",
                "prompt_text": prompt,
                "language":   lang,
                "speed_factor": speed,
            }
            print(f"  ✅ {name} complete")
        else:
            failed.append(name)

    # 写入 voices.json
    voices_json_path = voices_dir / "voices.json"
    with open(voices_json_path, "w", encoding="utf-8") as f:
        json.dump(voices_json, f, ensure_ascii=False, indent=2)
    print(f"\n📄 voices.json written: {voices_json_path}")
    print(f"   {len(voices_json)} voices registered")

    return failed


def verify():
    """验证转换结果"""
    print("\n" + "="*50)
    print("🔍 Verification:")
    voices_dir = DST / "voices"
    all_ok = True
    for (name, *_) in VOICES:
        d = voices_dir / name
        missing = []
        for f in ["gpt.safetensors", "sovits.safetensors", "reference.wav"]:
            if not (d / f).exists():
                missing.append(f)
        if missing:
            print(f"  ❌ {name}: missing {missing}")
            all_ok = False
        else:
            size_gpt = (d / "gpt.safetensors").stat().st_size // 1024 // 1024
            size_sov = (d / "sovits.safetensors").stat().st_size // 1024 // 1024
            print(f"  ✅ {name}: gpt={size_gpt}MB, sovits={size_sov}MB")
    return all_ok


if __name__ == "__main__":
    print(f"Source: {SRC}")
    print(f"Destination: {DST}")
    print(f"Voices to convert: {len(VOICES)}")

    # Step 1: 共享编码器
    print("\n" + "="*50)
    print("🔧 Step 1: Converting shared encoders...")
    convert_encoders()

    # Step 2: 所有音色
    print("\n🔧 Step 2: Converting all voices...")
    failed = convert_all_voices()

    # Step 3: 验证
    all_ok = verify()

    print("\n" + "="*50)
    if failed:
        print(f"⚠️  {len(failed)} voices failed: {failed}")
        sys.exit(1)
    elif all_ok:
        print(f"🎉 All {len(VOICES)} voices converted successfully!")
        print(f"   Models saved to: {DST}")
    else:
        print("❌ Some files missing after conversion")
        sys.exit(1)
