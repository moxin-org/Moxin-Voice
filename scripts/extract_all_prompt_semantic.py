#!/usr/bin/env python3
"""为所有预置音色预计算 Python CNHubert 语义编码，保存为 .npy 文件供 Rust 使用。

这样 Rust TTS 节点可以使用 set_reference_with_precomputed_codes() 实现与
Python dora-primespeech 相同的 few-shot 质量。

使用方法:
    conda run -n mofa-studio python3 scripts/extract_all_prompt_semantic.py
"""

import sys
import os
import types
import numpy as np
from pathlib import Path

# ---------------------------------------------------------------------------
# 路径配置
# ---------------------------------------------------------------------------

MOYOYO_TTS = Path(__file__).parent.parent / "node-hub/dora-primespeech/dora_primespeech/moyoyo_tts"
SRC_WEIGHTS = Path.home() / ".dora/models/primespeech/moyoyo"
DST_VOICES  = Path.home() / ".OminiX/models/gpt-sovits-mlx/voices"

# 使用 Doubao 基础模型提取 ssl_proj + quantizer（这些权重在各音色中通常相同）
BASE_SOVITS_PTH = SRC_WEIGHTS / "SoVITS_weights/doubao-mixed.pth"

# 预置音色列表（与 convert_all_voices.py 保持一致）
VOICES = [
    "Doubao", "LuoXiang", "YangMi", "ZhouJielun", "MaYun",
    "ChenYifan", "ZhaoDaniu", "Maple", "Cove", "BYS",
    "Ellen", "Juniper", "MaBaoguo", "ShenYi", "Trump",
]


def setup_imports():
    """设置 moyoyo_tts 模块路径（与 export_vits_onnx.py 一致）"""
    sys.path.insert(0, str(MOYOYO_TTS))
    parent = str(MOYOYO_TTS.parent)
    sys.path.insert(0, parent)
    dummy = types.ModuleType("moyoyo_tts")
    dummy.__path__ = [str(MOYOYO_TTS)]
    dummy.__package__ = "moyoyo_tts"
    sys.modules["moyoyo_tts"] = dummy


def load_models():
    """加载 CNHubert 和 SoVITS 基础模型"""
    import torch
    import librosa

    # 导入 moyoyo_tts 内部模块
    from feature_extractor.cnhubert import CNHubert
    from module.models import SynthesizerTrn

    print(f"Loading CNHubert from {SRC_WEIGHTS / 'chinese-hubert-base'}...")
    cnhubert = CNHubert(str(SRC_WEIGHTS / "chinese-hubert-base"))
    cnhubert.eval()

    print(f"Loading SoVITS base model from {BASE_SOVITS_PTH}...")
    dict_s2 = torch.load(str(BASE_SOVITS_PTH), map_location="cpu", weights_only=False)
    hps = dict_s2["config"]

    if dict_s2['weight']['enc_p.text_embedding.weight'].shape[0] == 322:
        version = "v1"
    else:
        version = "v2"

    model_config = vars(hps.model) if hasattr(hps.model, '__dict__') else dict(hps.model)
    model_config["version"] = version
    model_config["semantic_frame_rate"] = "25hz"

    vits = SynthesizerTrn(
        hps.data.filter_length // 2 + 1,
        hps.train.segment_size // hps.data.hop_length,
        n_speakers=hps.data.n_speakers,
        **model_config,
    )
    vits.eval()
    vits.load_state_dict(dict_s2["weight"], strict=False)

    print(f"Loaded SoVITS v{version}")
    return cnhubert, vits


def extract_semantic_codes(cnhubert, vits, ref_wav_path: Path) -> np.ndarray:
    """提取语义编码（复现 Python TTS_mid._set_prompt_semantic 的完整流程）"""
    import torch
    import librosa

    # 以 16kHz 加载参考音频
    wav16k, sr = librosa.load(str(ref_wav_path), sr=16000)
    print(f"   Loaded {ref_wav_path.name}: {len(wav16k)/16000:.2f}s @ 16kHz")

    # 追加 0.3s 静音（与 Python 完全一致）
    zero_wav = np.zeros(int(16000 * 0.3), dtype=np.float32)
    wav16k = np.concatenate([wav16k.astype(np.float32), zero_wav])

    with torch.no_grad():
        wav_tensor = torch.from_numpy(wav16k).unsqueeze(0)  # [1, T]

        # CNHubert 特征: [1, T_feat, 768] -> transpose -> [1, 768, T_feat]
        hubert_feature = cnhubert.model(wav_tensor)["last_hidden_state"].transpose(1, 2)

        # ssl_proj + quantizer -> codes [batch, 1, T_codes]
        codes = vits.extract_latent(hubert_feature)

        # prompt_semantic: 1D int64 tensor [T_codes]
        prompt_semantic = codes[0, 0].cpu()

    codes_np = prompt_semantic.numpy().astype(np.int32)
    print(f"   Semantic codes shape: {codes_np.shape}, min={codes_np.min()}, max={codes_np.max()}")
    return codes_np


def main():
    if not MOYOYO_TTS.exists():
        print(f"❌ moyoyo_tts not found: {MOYOYO_TTS}")
        sys.exit(1)

    if not BASE_SOVITS_PTH.exists():
        print(f"❌ Base SoVITS model not found: {BASE_SOVITS_PTH}")
        print(f"   Run scripts/convert_all_voices.py first to download source models")
        sys.exit(1)

    setup_imports()

    cnhubert, vits = load_models()

    failed = []

    for name in VOICES:
        voice_dir = DST_VOICES / name
        ref_wav = voice_dir / "reference.wav"
        out_npy = voice_dir / "prompt_semantic.npy"

        if out_npy.exists():
            print(f"⏭  {name}: already extracted ({out_npy.stat().st_size} bytes), skipping")
            continue

        if not ref_wav.exists():
            print(f"❌ {name}: reference.wav not found at {ref_wav}")
            failed.append(name)
            continue

        print(f"\n🎤 {name}: extracting semantic codes from {ref_wav}...")
        try:
            codes = extract_semantic_codes(cnhubert, vits, ref_wav)
            np.save(str(out_npy), codes)
            print(f"   ✅ Saved {out_npy} ({codes.shape[0]} codes, {out_npy.stat().st_size} bytes)")
        except Exception as e:
            import traceback
            print(f"   ❌ Failed: {e}")
            traceback.print_exc()
            failed.append(name)

    print("\n" + "=" * 50)
    if failed:
        print(f"⚠️  Failed: {failed}")
        sys.exit(1)
    else:
        print(f"🎉 All {len(VOICES)} voices processed")
        print(f"   prompt_semantic.npy files saved to {DST_VOICES}/<Voice>/")


if __name__ == "__main__":
    main()
