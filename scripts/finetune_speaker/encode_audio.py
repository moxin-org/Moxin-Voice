#!/usr/bin/env python3
"""
encode_audio.py - Encode WAV files to codec frame .npz files.

Implements the Qwen3-TTS speech encoder (SEANet + 8-layer transformer + 16-codebook RVQ)
directly in MLX, loading weights from model.safetensors.
No transformers or torch dependency required.

Requires: mlx, safetensors, librosa, soundfile, numpy

Usage:
    python encode_audio.py \
        --audio_dir data/raw/alice/ \
        --out_dir   data/encoded/alice/ \
        --tokenizer_path ~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit/speech_tokenizer
"""

import argparse
import os
import sys
from pathlib import Path

import mlx.core as mx
import numpy as np


# =============================================================================
# Primitive helpers
# =============================================================================

def elu(x: mx.array) -> mx.array:
    return mx.where(x > 0, x, mx.exp(x) - 1)


def layer_norm(x: mx.array, w: mx.array, b: mx.array, eps: float = 1e-5) -> mx.array:
    x = x.astype(mx.float32)
    mean = mx.mean(x, axis=-1, keepdims=True)
    diff = x - mean
    var  = mx.mean(diff * diff, axis=-1, keepdims=True)
    return (diff * mx.rsqrt(var + eps)) * w.astype(mx.float32) + b.astype(mx.float32)


def gelu(x: mx.array) -> mx.array:
    # GELU approximation: x * 0.5 * (1 + tanh(sqrt(2/pi) * (x + 0.044715*x^3)))
    x = x.astype(mx.float32)
    return x * 0.5 * (1.0 + mx.tanh(0.7978845608 * (x + 0.044715 * x * x * x)))


# =============================================================================
# Causal Conv1d
# Matches HF MimiConv1d: left-pad (kernel-stride) + optional extra right pad.
# x: [B, T, C] (MLX channel-last);  weight: [out, kernel, in]
# =============================================================================

def causal_conv1d(x: mx.array, weight: mx.array, bias, stride: int,
                  replicate: bool = False) -> mx.array:
    kernel    = weight.shape[1]
    left_pad  = kernel - stride

    length = x.shape[1]
    n_ceil = (length - kernel + left_pad + stride - 1) // stride + 1
    ideal  = (n_ceil - 1) * stride + kernel - left_pad
    rpad   = max(0, ideal - length)

    if left_pad > 0 or rpad > 0:
        parts = []
        if left_pad > 0:
            edge = mx.broadcast_to(x[:, :1, :], (x.shape[0], left_pad, x.shape[2])) \
                   if replicate else mx.zeros((x.shape[0], left_pad, x.shape[2]))
            parts.append(edge)
        parts.append(x)
        if rpad > 0:
            edge = mx.broadcast_to(x[:, -1:, :], (x.shape[0], rpad, x.shape[2])) \
                   if replicate else mx.zeros((x.shape[0], rpad, x.shape[2]))
            parts.append(edge)
        x = mx.concatenate(parts, axis=1)

    out = mx.conv1d(x, weight, stride=stride, padding=0)
    return out + bias if bias is not None else out


def _load_conv(weights: dict, prefix: str, stride: int, replicate: bool = False):
    # PyTorch layout [out, in, kernel] -> MLX layout [out, kernel, in]
    w = weights[f"{prefix}.weight"].transpose(0, 2, 1)
    b = weights.get(f"{prefix}.bias")
    return lambda x: causal_conv1d(x, w, b, stride, replicate)


# =============================================================================
# Encoder Residual Block
# ELU -> conv1(k=3) -> ELU -> conv2(k=1) + skip
# =============================================================================

def _load_res_block(weights: dict, prefix: str):
    c1 = _load_conv(weights, f"{prefix}.block.1.conv", stride=1)
    c2 = _load_conv(weights, f"{prefix}.block.3.conv", stride=1)
    sc = _load_conv(weights, f"{prefix}.shortcut.conv", stride=1) \
         if f"{prefix}.shortcut.conv.weight" in weights else None

    def forward(x):
        h    = c2(elu(c1(elu(x))))
        skip = sc(x) if sc else x
        return h + skip
    return forward


# =============================================================================
# Encoder Transformer Layer
# Pre-LN, causal sliding-window attention (window=250), GELU MLP, layer scale
# =============================================================================

def _build_rope(T: int, head_dim: int, base: float = 10000.0):
    half      = head_dim // 2
    inv_freq  = 1.0 / (base ** (np.arange(half, dtype=np.float32) * 2.0 / head_dim))
    angles    = np.outer(np.arange(T, dtype=np.float32), inv_freq)
    return mx.array(np.cos(angles)), mx.array(np.sin(angles))   # [T, half]


def _rope(x: mx.array, cos: mx.array, sin: mx.array) -> mx.array:
    half = x.shape[-1] // 2
    x1, x2 = x[..., :half], x[..., half:]
    c = cos.reshape(1, 1, cos.shape[0], half)
    s = sin.reshape(1, 1, sin.shape[0], half)
    return mx.concatenate([x1 * c - x2 * s, x2 * c + x1 * s], axis=-1)


def _load_enc_transformer(weights: dict, prefix: str,
                           hidden: int = 512, n_heads: int = 8, window: int = 250):
    head_dim = hidden // n_heads
    scale    = head_dim ** -0.5

    ln1_w = weights[f"{prefix}.input_layernorm.weight"]
    ln1_b = weights[f"{prefix}.input_layernorm.bias"]
    ln2_w = weights[f"{prefix}.post_attention_layernorm.weight"]
    ln2_b = weights[f"{prefix}.post_attention_layernorm.bias"]

    q_w = weights[f"{prefix}.self_attn.q_proj.weight"]
    k_w = weights[f"{prefix}.self_attn.k_proj.weight"]
    v_w = weights[f"{prefix}.self_attn.v_proj.weight"]
    o_w = weights[f"{prefix}.self_attn.o_proj.weight"]
    a_s = weights[f"{prefix}.self_attn_layer_scale.scale"]
    m_s = weights[f"{prefix}.mlp_layer_scale.scale"]
    f1  = weights[f"{prefix}.mlp.fc1.weight"]
    f2  = weights[f"{prefix}.mlp.fc2.weight"]

    def forward(x: mx.array) -> mx.array:
        B, T, D = x.shape

        n = layer_norm(x, ln1_w, ln1_b)
        q = (n @ q_w.T).reshape(B, T, n_heads, head_dim).transpose(0, 2, 1, 3)
        k = (n @ k_w.T).reshape(B, T, n_heads, head_dim).transpose(0, 2, 1, 3)
        v = (n @ v_w.T).reshape(B, T, n_heads, head_dim).transpose(0, 2, 1, 3)

        cos, sin = _build_rope(T, head_dim)
        q = _rope(q.astype(mx.float32), cos, sin)
        k = _rope(k.astype(mx.float32), cos, sin)

        sc = (q @ k.transpose(0, 1, 3, 2)) * scale
        rows = mx.arange(T).reshape(T, 1)
        cols = mx.arange(T).reshape(1, T)
        mask = mx.where((cols > rows) | ((rows - cols) >= window),
                        mx.array(float("-inf")), mx.zeros((T, T)))
        sc = sc + mask.reshape(1, 1, T, T)
        attn = mx.softmax(sc, axis=-1)
        out  = (attn @ v).transpose(0, 2, 1, 3).reshape(B, T, D)
        x    = x + (out @ o_w.T) * a_s

        n2 = layer_norm(x, ln2_w, ln2_b)
        x  = x + (gelu(n2 @ f1.T) @ f2.T) * m_s
        return x

    return forward


# =============================================================================
# RVQ codebook - nearest-neighbour lookup
# =============================================================================

def _load_codebook(weights: dict, prefix: str):
    es  = weights[f"{prefix}.codebook.embed_sum"].astype(mx.float32)
    cu  = weights[f"{prefix}.codebook.cluster_usage"].astype(mx.float32)
    cb  = es / mx.maximum(cu.reshape(-1, 1), mx.array(1e-5))   # [size, dim]

    def quantize(x: mx.array):
        x_sq = mx.sum(x * x, axis=-1, keepdims=True)
        e_sq = mx.sum(cb * cb, axis=-1)
        dists = x_sq - 2 * (x @ cb.T) + e_sq
        codes = mx.argmin(dists, axis=-1)                       # [B, T]
        quant = cb[codes.reshape(-1)].reshape(x.shape)
        return codes, quant

    return quantize


def _proj1x1(weights: dict, prefix: str):
    w = weights[f"{prefix}.weight"].reshape(weights[f"{prefix}.weight"].shape[0], -1)
    return lambda x: x @ w.T


# =============================================================================
# Build the full speech encoder from safetensors weights
# =============================================================================

def build_speech_encoder(tokenizer_path: str):
    """
    Load weights and return encode(audio_np) -> [T, 16] int16 codec frames.
    audio_np: float32 numpy array, mono, 24 kHz.
    """
    print(f"[encode] Loading speech tokenizer from {tokenizer_path}")
    raw = mx.load(str(Path(tokenizer_path) / "model.safetensors"))
    W   = {k: v.astype(mx.float32) for k, v in raw.items()}

    p = "encoder"

    # -- SEANet encoder layers ------------------------------------------------
    enc_layers = []
    for idx in range(16):
        ck = f"{p}.encoder.layers.{idx}.conv.weight"
        bk = f"{p}.encoder.layers.{idx}.block.1.conv.weight"
        if ck in W:
            w      = W[ck]                   # [out, in, kernel]
            kernel = w.shape[2]
            in_ch  = w.shape[1]
            out_ch = w.shape[0]
            stride = kernel // 2 if (kernel > 3 and out_ch > in_ch and in_ch > 1) else 1
            enc_layers.append(("conv", _load_conv(W, f"{p}.encoder.layers.{idx}.conv", stride)))
        elif bk in W:
            enc_layers.append(("res", _load_res_block(W, f"{p}.encoder.layers.{idx}")))
        elif enc_layers and idx <= 14:
            enc_layers.append(("elu", None))

    # -- Encoder transformer (8 layers) --------------------------------------
    trans_layers = [
        _load_enc_transformer(W, f"{p}.encoder_transformer.layers.{i}")
        for i in range(8)
        if f"{p}.encoder_transformer.layers.{i}.self_attn.q_proj.weight" in W
    ]

    # -- Downsample (25 Hz -> 12.5 Hz, replicate padding) --------------------
    ds = _load_conv(W, f"{p}.downsample.conv", stride=2, replicate=True)

    # -- Split RVQ (1 semantic + 15 acoustic) ---------------------------------
    sp = f"{p}.quantizer.semantic_residual_vector_quantizer"
    ap = f"{p}.quantizer.acoustic_residual_vector_quantizer"

    sem_in = _proj1x1(W, f"{sp}.input_proj")
    sem_cb = _load_codebook(W, f"{sp}.layers.0")

    acou_in = _proj1x1(W, f"{ap}.input_proj")
    acou_cbs = [
        _load_codebook(W, f"{ap}.layers.{i}")
        for i in range(15)
        if f"{ap}.layers.{i}.codebook.embed_sum" in W
    ]

    n_conv  = sum(1 for t, _ in enc_layers if t in ("conv", "res"))
    print(f"[encode] SEANet: {n_conv} layers | Transformer: {len(trans_layers)} layers | "
          f"RVQ: 1 semantic + {len(acou_cbs)} acoustic codebooks")

    def encode(audio_np: np.ndarray) -> np.ndarray:
        x = mx.array(audio_np.reshape(1, -1, 1))   # [1, N, 1]

        for kind, fn in enc_layers:
            x = elu(x) if kind == "elu" else fn(x)

        for layer_fn in trans_layers:
            x = layer_fn(x)
        mx.eval(x)

        x = ds(x)
        mx.eval(x)

        # Semantic (codebook 0)
        sem_codes, _ = sem_cb(sem_in(x))

        # Acoustic (codebooks 1-15), residual
        residual = acou_in(x)
        acou_codes = []
        for cb in acou_cbs:
            codes_i, quant_i = cb(residual)
            residual = residual - quant_i
            acou_codes.append(codes_i)

        mx.eval(sem_codes, *acou_codes)

        cols = [np.array(sem_codes[0]).astype(np.int16)]
        for c in acou_codes:
            cols.append(np.array(c[0]).astype(np.int16))
        return np.stack(cols, axis=1)   # [T, 16]

    return encode


# =============================================================================
# Per-file encoding helper
# =============================================================================

def encode_file(encode_fn, audio_path: Path, sample_rate: int = 24000) -> np.ndarray:
    import soundfile as sf
    import librosa

    audio, sr = sf.read(str(audio_path), dtype="float32", always_2d=False)
    if audio.ndim > 1:
        audio = audio.mean(axis=1)
    if sr != sample_rate:
        audio = librosa.resample(audio, orig_sr=sr, target_sr=sample_rate)
    peak = np.abs(audio).max()
    if peak > 0:
        audio = audio / peak * 0.95
    return encode_fn(audio)


# =============================================================================
# CLI
# =============================================================================

def main():
    parser = argparse.ArgumentParser(description="Encode WAV files to codec frame .npz")
    parser.add_argument("--audio_dir",  required=True)
    parser.add_argument("--out_dir",    required=True)
    parser.add_argument(
        "--tokenizer_path",
        default=os.path.expanduser(
            "~/.OminiX/models/qwen3-tts-mlx/"
            "Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit/speech_tokenizer"
        ),
    )
    parser.add_argument("--sample_rate", type=int, default=24000)
    parser.add_argument("--ext", default="wav")
    args = parser.parse_args()

    audio_dir = Path(args.audio_dir)
    out_dir   = Path(args.out_dir)
    out_dir.mkdir(parents=True, exist_ok=True)

    audio_files = sorted(audio_dir.glob(f"*.{args.ext}"))
    if not audio_files:
        audio_files = sorted(audio_dir.glob(f"*.{args.ext.upper()}"))
    if not audio_files:
        print(f"[encode] No .{args.ext} files in {audio_dir}")
        sys.exit(1)

    print(f"[encode] {len(audio_files)} file(s) found in {audio_dir}")
    encode_fn = build_speech_encoder(args.tokenizer_path)

    ok = 0
    for ap in audio_files:
        out_path = out_dir / (ap.stem + ".npz")
        try:
            codes = encode_file(encode_fn, ap, args.sample_rate)
            np.savez_compressed(str(out_path), codes=codes)
            dur = codes.shape[0] / 12.5
            print(f"[encode]   {ap.name} -> {out_path.name}  shape={codes.shape}  ({dur:.1f}s)")
            ok += 1
        except Exception as e:
            print(f"[encode] ERROR {ap.name}: {e}")
            import traceback; traceback.print_exc()

    print(f"[encode] Done: {ok}/{len(audio_files)} encoded -> {out_dir}")


if __name__ == "__main__":
    main()
