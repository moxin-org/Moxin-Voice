#!/usr/bin/env python3
"""
train.py — MLX fine-tuning: add new CustomVoice speaker embedding.

Trains ONE new 2048-dim row in talker.model.codec_embedding.weight.
The 28-layer transformer and all other weights stay completely frozen.

Strategy:
  - Keep a trainable variable `new_spk_emb` [2048] (just the new row)
  - Initialize from mean of existing 9 speaker rows + small Gaussian noise
  - Forward pass: text_proj(text_embed) + codec_embed → 28-layer Qwen3 transformer
  - Loss: cross_entropy(logits[9:-1], codec_frame_targets)
  - Gradient only flows through new_spk_emb (everything else frozen)

Usage:
    python train.py \
        --model_dir ~/.OminiX/models/qwen3-tts-mlx/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit \
        --encoded_dir data/encoded/alice/ \
        --text_dir data/text/alice/ \
        --speaker_name alice \
        --speaker_id 3067 \
        --lr 1e-3 --epochs 20 --batch_size 4
"""

import argparse
import json
import math
import os
import random
import sys
import time
from pathlib import Path

import mlx.core as mx
import mlx.nn as nn
import numpy as np

# ─────────────────────────────────────────────────────────────────────────────
# Hyperparams / special token IDs (match config.json)
# ─────────────────────────────────────────────────────────────────────────────

IM_START_ID    = 151644
ASSISTANT_ID   = 77091
NEWLINE_ID     = 198
TTS_PAD_ID     = 151671
TTS_BOS_ID     = 151672
TTS_EOS_ID     = 151673

CODEC_PAD_ID      = 2148
CODEC_BOS_ID      = 2149
CODEC_EOS_ID      = 2150
CODEC_THINK_ID    = 2154
CODEC_NOTHINK_ID  = 2155
CODEC_THINK_BOS   = 2156
CODEC_THINK_EOS   = 2157

QUANT_GROUP_SIZE = 64
QUANT_BITS       = 8

# ─────────────────────────────────────────────────────────────────────────────
# Primitive ops on frozen MLX arrays
# ─────────────────────────────────────────────────────────────────────────────

def rms_norm(x: mx.array, weight: mx.array, eps: float = 1e-6) -> mx.array:
    x = x.astype(mx.float32)
    rms = mx.rsqrt(mx.mean(x * x, axis=-1, keepdims=True) + eps)
    return (x * rms * weight.astype(mx.float32)).astype(mx.bfloat16)


def quant_linear(
    x: mx.array,
    w_packed: mx.array,    # [out, in//group_size*bits//32] uint32
    scales: mx.array,      # [out, in//group_size] bf16
    biases: mx.array,      # [out, in//group_size] bf16
    bias: mx.array = None, # [out] bf16 or None
) -> mx.array:
    w = mx.dequantize(w_packed, scales, biases, QUANT_GROUP_SIZE, QUANT_BITS)
    out = x @ w.T
    if bias is not None:
        out = out + bias
    return out


def text_projection(
    x: mx.array, W,
) -> mx.array:
    """2-layer MLP: fc2(silu(fc1(x)))"""
    h = quant_linear(x, W["fc1_w"], W["fc1_s"], W["fc1_b"])
    if W.get("fc1_bias") is not None:
        h = h + W["fc1_bias"]
    h = nn.silu(h)
    h = quant_linear(h, W["fc2_w"], W["fc2_s"], W["fc2_b"])
    if W.get("fc2_bias") is not None:
        h = h + W["fc2_bias"]
    return h


# ─────────────────────────────────────────────────────────────────────────────
# Rotary Position Embedding (standard, interleaved = traditional in MLX)
# ─────────────────────────────────────────────────────────────────────────────

def build_rope_freqs(head_dim: int, base: float, max_len: int = 4096) -> mx.array:
    """Returns cos, sin of shape [max_len, head_dim]"""
    half = head_dim // 2
    idx = mx.arange(0, half, dtype=mx.float32)
    inv_freq = 1.0 / (base ** (idx * 2.0 / head_dim))
    positions = mx.arange(0, max_len, dtype=mx.float32)
    angles = mx.outer(positions, inv_freq)  # [max_len, half]
    cos = mx.concatenate([mx.cos(angles), mx.cos(angles)], axis=-1)  # [max_len, head_dim]
    sin = mx.concatenate([mx.sin(angles), mx.sin(angles)], axis=-1)
    return cos, sin


def apply_rope_interleaved(x: mx.array, cos: mx.array, sin: mx.array) -> mx.array:
    """
    Interleaved (traditional) RoPE.
    x: [B, heads, L, head_dim]
    cos, sin: [L, head_dim]
    """
    head_dim = x.shape[-1]
    half = head_dim // 2
    # Reshape to [B, heads, L, half, 2], rotate, reshape back
    x2 = x.reshape(x.shape[:-1] + (half, 2))
    x_even = x2[..., 0]   # [B, heads, L, half]
    x_odd  = x2[..., 1]
    # Interleaved rotation: (x0, x1) → (x0*cos - x1*sin, x0*sin + x1*cos)
    cos_h = cos[:, :half].reshape(1, 1, cos.shape[0], half)
    sin_h = sin[:, :half].reshape(1, 1, sin.shape[0], half)
    r_even = x_even * cos_h - x_odd * sin_h
    r_odd  = x_even * sin_h + x_odd * cos_h
    # Interleave back
    out = mx.stack([r_even, r_odd], axis=-1).reshape(x.shape)
    return out


# ─────────────────────────────────────────────────────────────────────────────
# GQA Attention (frozen)
# ─────────────────────────────────────────────────────────────────────────────

def attention_forward(
    x: mx.array,
    layer_w: dict,
    cos: mx.array,
    sin: mx.array,
    n_heads: int,
    n_kv_heads: int,
    head_dim: int,
    scale: float,
) -> mx.array:
    B, L, D = x.shape

    q = quant_linear(x, layer_w["q_w"], layer_w["q_s"], layer_w["q_b"])
    k = quant_linear(x, layer_w["k_w"], layer_w["k_s"], layer_w["k_b"])
    v = quant_linear(x, layer_w["v_w"], layer_w["v_s"], layer_w["v_b"])

    # [B, heads, L, head_dim]
    q = q.reshape(B, L, n_heads, head_dim).transpose(0, 2, 1, 3)
    k = k.reshape(B, L, n_kv_heads, head_dim).transpose(0, 2, 1, 3)
    v = v.reshape(B, L, n_kv_heads, head_dim).transpose(0, 2, 1, 3)

    # QK norm
    q = rms_norm(q, layer_w["q_norm"])
    k = rms_norm(k, layer_w["k_norm"])

    # RoPE (interleaved)
    q = apply_rope_interleaved(q.astype(mx.float32), cos[:L], sin[:L]).astype(mx.bfloat16)
    k = apply_rope_interleaved(k.astype(mx.float32), cos[:L], sin[:L]).astype(mx.bfloat16)

    # GQA: repeat kv heads to match q heads
    if n_kv_heads < n_heads:
        repeat = n_heads // n_kv_heads
        k = mx.repeat(k, repeat, axis=1)
        v = mx.repeat(v, repeat, axis=1)

    # Scaled dot-product attention with causal mask
    qk = (q.astype(mx.float32) @ k.astype(mx.float32).transpose(0, 1, 3, 2)) * scale
    if L > 1:
        causal_mask = mx.triu(mx.full((L, L), float("-inf")), k=1)
        qk = qk + causal_mask.reshape(1, 1, L, L)
    attn = mx.softmax(qk, axis=-1).astype(mx.bfloat16)
    out = (attn @ v).transpose(0, 2, 1, 3).reshape(B, L, -1)

    return quant_linear(out, layer_w["o_w"], layer_w["o_s"], layer_w["o_b"])


# ─────────────────────────────────────────────────────────────────────────────
# SwiGLU MLP (frozen)
# ─────────────────────────────────────────────────────────────────────────────

def mlp_forward(x: mx.array, layer_w: dict) -> mx.array:
    gate = nn.silu(quant_linear(x, layer_w["gate_w"], layer_w["gate_s"], layer_w["gate_b"]))
    up   = quant_linear(x, layer_w["up_w"],   layer_w["up_s"],   layer_w["up_b"])
    return quant_linear(gate * up, layer_w["down_w"], layer_w["down_s"], layer_w["down_b"])


# ─────────────────────────────────────────────────────────────────────────────
# Full transformer forward (frozen except codec_embedding)
# ─────────────────────────────────────────────────────────────────────────────

def transformer_forward(
    input_embeds: mx.array,   # [1, L, 2048]
    frozen: dict,             # pre-loaded frozen weights
    rope_cos: mx.array,
    rope_sin: mx.array,
) -> mx.array:
    """Run 28-layer Qwen3-TTS talker, return logits [1, L, 3072]."""
    h = input_embeds
    n_heads   = frozen["n_heads"]
    n_kv      = frozen["n_kv_heads"]
    head_dim  = frozen["head_dim"]
    scale     = 1.0 / math.sqrt(head_dim)
    n_layers  = frozen["n_layers"]

    for i in range(n_layers):
        lw = frozen["layers"][i]
        # Pre-norm attention
        normed = rms_norm(h, lw["in_norm"])
        attn_out = attention_forward(normed, lw, rope_cos, rope_sin,
                                     n_heads, n_kv, head_dim, scale)
        h = h + attn_out

        # Pre-norm MLP
        normed = rms_norm(h, lw["post_norm"])
        h = h + mlp_forward(normed, lw)

    # Final norm + codec head
    h = rms_norm(h, frozen["norm"])
    logits = quant_linear(h, frozen["codec_head_w"], frozen["codec_head_s"], frozen["codec_head_b"])
    return logits


# ─────────────────────────────────────────────────────────────────────────────
# Build the full input embedding sequence
# ─────────────────────────────────────────────────────────────────────────────

def build_input_embeds(
    text_ids: list,
    codec_frames: np.ndarray,
    spk_id: int,
    lang_id: int,
    frozen: dict,
    new_spk_emb: mx.array,  # [2048] trainable new speaker embedding
) -> mx.array:
    """
    Build combined text+codec embedding [1, L, 2048] for training.

    Sequence layout:
      Pos 0-2:  role tokens    — text_proj(text_embed(im_start, assistant, \\n))
      Pos 3-7:  tts_pad×5     — text_proj(text_embed(tts_pad)) + codec_embed(think, think_bos, lang, think_eos, spk)
      Pos 8:    tts_bos        — text_proj(text_embed(tts_bos)) + codec_embed(pad)
      Pos 9:    text_tokens[0] — text_proj(text_embed(t)) + codec_embed(codec_bos)
      Pos 10..: text_tokens[1..]+tts_eos+tts_pad — text_proj + codec_embed(frames[0..T-1])
      Last pos: codec_eos appended as text+codec target sentinel
    """
    W = frozen["text_proj"]
    text_emb_w = frozen["text_emb"]
    codec_emb_w = frozen["codec_emb"]  # [3072, 2048] bfloat16 (existing rows)

    def te(tok_id: int) -> mx.array:
        """text_proj(text_embed(tok_id)) → [1, 1, 2048]"""
        idx = mx.array([[tok_id]], dtype=mx.uint32)
        emb = text_emb_w[idx]  # [1, 1, 2048]
        return text_projection(emb, W)

    def ce(tok_id: int) -> mx.array:
        """codec_embed(tok_id) → [1, 1, 2048]"""
        if tok_id == spk_id:
            # Trainable new speaker embedding
            return new_spk_emb.reshape(1, 1, -1)
        idx = mx.array([[tok_id]], dtype=mx.uint32)
        return codec_emb_w[idx]  # [1, 1, 2048]

    T = codec_frames.shape[0]  # number of codec frames

    parts = []

    # Pos 0-2: role tokens (text only)
    for tid in [IM_START_ID, ASSISTANT_ID, NEWLINE_ID]:
        parts.append(te(tid))

    # Pos 3-7: tts_pad + codec control tokens
    codec_ctrl = [CODEC_THINK_ID, CODEC_THINK_BOS, lang_id, CODEC_THINK_EOS, spk_id]
    for cid in codec_ctrl:
        parts.append(te(TTS_PAD_ID) + ce(cid))

    # Pos 8: tts_bos + codec_pad
    parts.append(te(TTS_BOS_ID) + ce(CODEC_PAD_ID))

    # Pos 9: first text token + codec_bos
    first_text = text_ids[0] if text_ids else TTS_PAD_ID
    parts.append(te(first_text) + ce(CODEC_BOS_ID))

    # Pos 10..: remaining text + frames
    n_frames_to_embed = T  # we embed frames[0..T-1] at positions 10..9+T
    trailing_text = list(text_ids[1:]) + [TTS_EOS_ID]
    for i in range(n_frames_to_embed):
        t_id = trailing_text[i] if i < len(trailing_text) else TTS_PAD_ID
        frame_id = int(codec_frames[i, 0])  # codebook 0
        parts.append(te(t_id) + ce(frame_id))

    return mx.concatenate(parts, axis=1)  # [1, L, 2048]


# ─────────────────────────────────────────────────────────────────────────────
# Loss function
# ─────────────────────────────────────────────────────────────────────────────

def compute_loss(
    text_ids: list,
    codec_frames: np.ndarray,   # [T, 16] int16
    spk_id: int,
    lang_id: int,
    frozen: dict,
    new_spk_emb: mx.array,
    rope_cos: mx.array,
    rope_sin: mx.array,
) -> mx.array:
    """
    Forward pass + cross-entropy loss on codec frame predictions.

    Targets: codec_frames_codebook0 at positions 10..9+T, plus codec_eos at 9+T+1
    """
    T = codec_frames.shape[0]

    embeds = build_input_embeds(
        text_ids, codec_frames, spk_id, lang_id, frozen, new_spk_emb
    )
    logits = transformer_forward(embeds, frozen, rope_cos, rope_sin)
    # logits: [1, L, 3072]

    # Targets: frame_0 .. frame_{T-1}, then codec_eos
    # logits[9] predicts frame_0, logits[9+k] predicts frame_k
    target_frames = codec_frames[:, 0].astype(np.int32)  # [T] codebook-0
    targets_np = np.append(target_frames, CODEC_EOS_ID).astype(np.int32)  # [T+1]
    targets = mx.array(targets_np, dtype=mx.int32)  # [T+1]

    # Slice logits: positions 9..9+T (inclusive) → [T+1, 3072]
    pred_logits = logits[0, 9:9 + T + 1, :]  # [T+1, 3072]

    loss = mx.mean(nn.losses.cross_entropy(pred_logits, targets))
    return loss


# ─────────────────────────────────────────────────────────────────────────────
# Weight loading
# ─────────────────────────────────────────────────────────────────────────────

def load_frozen_weights(model_dir: Path, n_speakers_existing: int) -> dict:
    """Load all frozen model weights into MLX arrays."""
    print("[train] Loading model weights ...")
    weights = mx.load(str(model_dir / "model.safetensors"))

    pfx = "talker."
    mp  = "talker.model."

    frozen = {}

    # Text embedding & codec embedding (frozen part)
    frozen["text_emb"]   = weights[mp + "text_embedding.weight"]       # [151936, 2048]
    frozen["codec_emb"]  = weights[mp + "codec_embedding.weight"]      # [3072, 2048]

    # Text projection
    frozen["text_proj"] = {
        "fc1_w": weights[pfx + "text_projection.linear_fc1.weight"],
        "fc1_s": weights[pfx + "text_projection.linear_fc1.scales"],
        "fc1_b": weights[pfx + "text_projection.linear_fc1.biases"],
        "fc1_bias": weights.get(pfx + "text_projection.linear_fc1.bias"),
        "fc2_w": weights[pfx + "text_projection.linear_fc2.weight"],
        "fc2_s": weights[pfx + "text_projection.linear_fc2.scales"],
        "fc2_b": weights[pfx + "text_projection.linear_fc2.biases"],
        "fc2_bias": weights.get(pfx + "text_projection.linear_fc2.bias"),
    }

    # Final norm
    frozen["norm"] = weights[mp + "norm.weight"]

    # Codec head
    frozen["codec_head_w"] = weights[pfx + "codec_head.weight"]
    frozen["codec_head_s"] = weights[pfx + "codec_head.scales"]
    frozen["codec_head_b"] = weights[pfx + "codec_head.biases"]

    # Transformer config
    frozen["n_heads"]    = 16
    frozen["n_kv_heads"] = 8
    frozen["head_dim"]   = 128
    frozen["n_layers"]   = 28

    # Transformer layers
    frozen["layers"] = []
    for i in range(frozen["n_layers"]):
        lp = f"{mp}layers.{i}."
        lw = {
            "in_norm":   weights[lp + "input_layernorm.weight"],
            "post_norm": weights[lp + "post_attention_layernorm.weight"],
            "q_w": weights[lp + "self_attn.q_proj.weight"],
            "q_s": weights[lp + "self_attn.q_proj.scales"],
            "q_b": weights[lp + "self_attn.q_proj.biases"],
            "k_w": weights[lp + "self_attn.k_proj.weight"],
            "k_s": weights[lp + "self_attn.k_proj.scales"],
            "k_b": weights[lp + "self_attn.k_proj.biases"],
            "v_w": weights[lp + "self_attn.v_proj.weight"],
            "v_s": weights[lp + "self_attn.v_proj.scales"],
            "v_b": weights[lp + "self_attn.v_proj.biases"],
            "o_w": weights[lp + "self_attn.o_proj.weight"],
            "o_s": weights[lp + "self_attn.o_proj.scales"],
            "o_b": weights[lp + "self_attn.o_proj.biases"],
            "q_norm": weights[lp + "self_attn.q_norm.weight"],
            "k_norm": weights[lp + "self_attn.k_norm.weight"],
            "gate_w": weights[lp + "mlp.gate_proj.weight"],
            "gate_s": weights[lp + "mlp.gate_proj.scales"],
            "gate_b": weights[lp + "mlp.gate_proj.biases"],
            "up_w":   weights[lp + "mlp.up_proj.weight"],
            "up_s":   weights[lp + "mlp.up_proj.scales"],
            "up_b":   weights[lp + "mlp.up_proj.biases"],
            "down_w": weights[lp + "mlp.down_proj.weight"],
            "down_s": weights[lp + "mlp.down_proj.scales"],
            "down_b": weights[lp + "mlp.down_proj.biases"],
        }
        frozen["layers"].append(lw)

    print(f"[train] Loaded {frozen['n_layers']}-layer transformer weights.")
    return frozen


def init_new_speaker_emb(frozen: dict, spk_ids: list, noise_scale: float = 0.01) -> mx.array:
    """
    Initialize new speaker embedding as mean of existing speakers + small noise.

    spk_ids: list of existing speaker token IDs (e.g. [3065, 3066, 3010, ...])
    """
    codec_emb = frozen["codec_emb"].astype(mx.float32)
    idx = mx.array(spk_ids, dtype=mx.uint32)
    existing_rows = codec_emb[idx]          # [n_existing, 2048]
    mean_row = mx.mean(existing_rows, axis=0)  # [2048]
    noise = mx.random.normal(mean_row.shape) * noise_scale
    new_emb = mean_row + noise
    return new_emb.astype(mx.bfloat16)


def save_checkpoint(new_spk_emb: mx.array, path: Path):
    mx.eval(new_spk_emb)
    np.savez_compressed(str(path), emb=np.array(new_spk_emb.astype(mx.float32)))
    print(f"[train] Checkpoint saved → {path}")


def patch_and_save_safetensors(
    model_dir: Path,
    frozen: dict,
    new_spk_emb: mx.array,
    new_spk_id: int,
    out_path: Path,
):
    """
    Write a new model.safetensors where talker.model.codec_embedding.weight
    is extended from [3072, 2048] to [3073, 2048] with the new speaker row at index new_spk_id.
    All other tensors are copied unchanged.

    Uses mx.load / mx.save_safetensors to handle bfloat16 and uint32 natively.
    """
    print(f"[train] Patching model.safetensors → {out_path}")

    # mx.load handles bfloat16 and uint32 (quantized weights) without issues
    weights = mx.load(str(model_dir / "model.safetensors"))

    emb_key = "talker.model.codec_embedding.weight"
    old_emb = weights[emb_key]                                  # [3072, 2048] bfloat16
    new_row = new_spk_emb.reshape(1, 2048).astype(mx.bfloat16)
    weights[emb_key] = mx.concatenate([old_emb, new_row], axis=0)
    mx.eval(weights[emb_key])
    print(f"[train]   {emb_key}: {old_emb.shape} → {weights[emb_key].shape}")

    mx.save_safetensors(str(out_path), weights)
    print(f"[train] Saved patched weights → {out_path}")


# ─────────────────────────────────────────────────────────────────────────────
# Tokenizer (BPE via tokenizers library)
# ─────────────────────────────────────────────────────────────────────────────

def load_tokenizer(model_dir: Path):
    """Load BPE tokenizer from vocab.json + merges.txt."""
    try:
        from tokenizers import Tokenizer
        tok_path = model_dir / "tokenizer.json"
        if tok_path.exists():
            return Tokenizer.from_file(str(tok_path))
    except ImportError:
        pass

    # Fallback: use transformers tokenizer
    try:
        from transformers import AutoTokenizer
        tok = AutoTokenizer.from_pretrained(str(model_dir))
        return tok
    except Exception as e:
        print(f"[train] WARNING: could not load tokenizer: {e}")
        return None


def tokenize_text(tokenizer, text: str) -> list:
    """Tokenize text to token IDs."""
    if tokenizer is None:
        # Simple character-level fallback (not recommended)
        return [ord(c) % 1000 + 100 for c in text[:50]]

    # tokenizers library
    if hasattr(tokenizer, "encode") and callable(tokenizer.encode):
        enc = tokenizer.encode(text)
        if hasattr(enc, "ids"):
            return enc.ids
        else:
            return list(enc)
    # transformers
    return tokenizer.encode(text, add_special_tokens=False)


# ─────────────────────────────────────────────────────────────────────────────
# Dataset loading
# ─────────────────────────────────────────────────────────────────────────────

def load_dataset(encoded_dir: Path, text_dir: Path, tokenizer,
                 segment_frames: int = 0) -> list:
    """
    Load training pairs: (text_ids, codec_frames)
    encoded_dir: .npz files with key 'codes' [T, 16] int16
    text_dir:    .txt files with the transcript (one sentence per file)

    segment_frames > 0: split each audio into non-overlapping segments of this
    many codec frames (e.g. 37 ≈ 3s at 12.5Hz).  The same text is reused for
    every segment (speaker-identity training doesn't require exact alignment).
    """
    samples = []
    npz_files = sorted(encoded_dir.glob("*.npz"))
    if not npz_files:
        print(f"[train] ERROR: No .npz files in {encoded_dir}")
        sys.exit(1)

    for npz_path in npz_files:
        txt_path = text_dir / (npz_path.stem + ".txt")
        if not txt_path.exists():
            print(f"[train] WARNING: no transcript for {npz_path.name}, skipping")
            continue

        try:
            codec_frames = np.load(str(npz_path))["codes"]  # [T, 16]
            text = txt_path.read_text(encoding="utf-8").strip()
            if not text:
                continue
            text_ids = tokenize_text(tokenizer, text)

            if segment_frames > 0 and codec_frames.shape[0] > segment_frames:
                # Split into non-overlapping segments; reuse the same text for all
                n_segs = codec_frames.shape[0] // segment_frames
                for i in range(n_segs):
                    seg = codec_frames[i * segment_frames:(i + 1) * segment_frames]
                    samples.append((text_ids, seg))
                # Keep the leftover tail if it's at least half a segment
                tail = codec_frames[n_segs * segment_frames:]
                if tail.shape[0] >= segment_frames // 2:
                    samples.append((text_ids, tail))
            else:
                samples.append((text_ids, codec_frames))

        except Exception as e:
            print(f"[train] WARNING: error loading {npz_path.name}: {e}")

    print(f"[train] Loaded {len(samples)} training samples "
          f"({'segmented' if segment_frames > 0 else 'full files'})")
    return samples


# ─────────────────────────────────────────────────────────────────────────────
# AdamW (simple, single-parameter)
# ─────────────────────────────────────────────────────────────────────────────

class AdamW:
    def __init__(self, lr: float = 1e-3, beta1: float = 0.9, beta2: float = 0.999,
                 eps: float = 1e-8, weight_decay: float = 1e-2):
        self.lr = lr
        self.beta1 = beta1
        self.beta2 = beta2
        self.eps = eps
        self.wd = weight_decay
        self.m = None
        self.v = None
        self.t = 0

    def step(self, param: mx.array, grad: mx.array) -> mx.array:
        self.t += 1
        g = grad.astype(mx.float32)
        p = param.astype(mx.float32)

        if self.m is None:
            self.m = mx.zeros_like(g)
            self.v = mx.zeros_like(g)

        self.m = self.beta1 * self.m + (1 - self.beta1) * g
        self.v = self.beta2 * self.v + (1 - self.beta2) * (g * g)

        m_hat = self.m / (1 - self.beta1 ** self.t)
        v_hat = self.v / (1 - self.beta2 ** self.t)

        update = m_hat / (mx.sqrt(v_hat) + self.eps)
        p = p * (1 - self.lr * self.wd) - self.lr * update

        # Force eval to detach
        mx.eval(p, self.m, self.v)
        return p.astype(mx.bfloat16)


# ─────────────────────────────────────────────────────────────────────────────
# Main training loop
# ─────────────────────────────────────────────────────────────────────────────

def main():
    parser = argparse.ArgumentParser(description="Fine-tune Qwen3-TTS speaker embedding (MLX)")
    parser.add_argument("--model_dir", required=True)
    parser.add_argument("--encoded_dir", required=True)
    parser.add_argument("--text_dir", required=True)
    parser.add_argument("--speaker_name", required=True)
    parser.add_argument("--speaker_id", type=int, default=3067,
                        help="New speaker token ID (default: 3067, next after existing 9)")
    parser.add_argument("--language", default="chinese",
                        help="Language for lang codec token (default: chinese)")
    parser.add_argument("--lr", type=float, default=1e-3)
    parser.add_argument("--epochs", type=int, default=20)
    parser.add_argument("--batch_size", type=int, default=4,
                        help="Gradient accumulation steps per update")
    parser.add_argument("--checkpoint_every", type=int, default=50,
                        help="Save embedding checkpoint every N gradient steps")
    parser.add_argument("--noise_scale", type=float, default=0.01,
                        help="Init noise added to mean speaker embedding")
    parser.add_argument("--max_frames", type=int, default=600,
                        help="Max codec frames per sample (truncate long sequences)")
    parser.add_argument("--segment_frames", type=int, default=0,
                        help="Split audio into segments of N codec frames for training "
                             "(0 = disabled). E.g. 37 ≈ 3s. Recommended for single-file "
                             "training: --segment_frames 37")
    parser.add_argument("--seed", type=int, default=42)
    args = parser.parse_args()

    mx.random.seed(args.seed)
    random.seed(args.seed)
    np.random.seed(args.seed)

    model_dir   = Path(args.model_dir).expanduser()
    encoded_dir = Path(args.encoded_dir).expanduser()
    text_dir    = Path(args.text_dir).expanduser()
    ckpt_dir    = encoded_dir.parent / f"checkpoints_{args.speaker_name}"
    ckpt_dir.mkdir(parents=True, exist_ok=True)

    # ── Load config ──────────────────────────────────────────────────────────
    with open(model_dir / "config.json") as f:
        config = json.load(f)
    talker_cfg = config["talker_config"]

    existing_spk_ids = list(talker_cfg["spk_id"].values())
    lang_id = talker_cfg["codec_language_id"].get(args.language)
    if lang_id is None:
        available = list(talker_cfg["codec_language_id"].keys())
        print(f"[train] ERROR: language '{args.language}' not in config. Available: {available}")
        sys.exit(1)

    print(f"[train] Speaker: '{args.speaker_name}' → id={args.speaker_id}")
    print(f"[train] Language: '{args.language}' → codec_id={lang_id}")

    # ── Load tokenizer & dataset ─────────────────────────────────────────────
    tokenizer = load_tokenizer(model_dir)
    samples   = load_dataset(encoded_dir, text_dir, tokenizer,
                             segment_frames=args.segment_frames)
    if not samples:
        print("[train] ERROR: no training samples loaded")
        sys.exit(1)

    # ── Load frozen weights ──────────────────────────────────────────────────
    frozen = load_frozen_weights(model_dir, len(existing_spk_ids))
    rope_cos, rope_sin = build_rope_freqs(
        frozen["head_dim"],
        base=talker_cfg.get("rope_theta", 1_000_000),
        max_len=talker_cfg.get("max_position_embeddings", 32768),
    )

    # ── Initialize trainable embedding ──────────────────────────────────────
    new_spk_emb = init_new_speaker_emb(frozen, existing_spk_ids, args.noise_scale)
    print(f"[train] Initialized speaker embedding: mean={float(mx.mean(new_spk_emb.astype(mx.float32))):.4f}")

    optimizer = AdamW(lr=args.lr)

    # ── Training ─────────────────────────────────────────────────────────────
    global_step = 0
    for epoch in range(args.epochs):
        random.shuffle(samples)
        epoch_loss = 0.0
        n_updates  = 0

        # Accumulate gradients over batch_size samples
        accum_grad = None
        accum_loss = 0.0

        for sample_idx, (text_ids, codec_frames) in enumerate(samples):
            # Truncate very long sequences
            if codec_frames.shape[0] > args.max_frames:
                codec_frames = codec_frames[:args.max_frames]

            # Define loss as function of new_spk_emb only
            def loss_fn(emb):
                return compute_loss(
                    text_ids, codec_frames,
                    args.speaker_id, lang_id,
                    frozen, emb, rope_cos, rope_sin
                )

            loss, grad = mx.value_and_grad(loss_fn)(new_spk_emb)
            mx.eval(loss, grad)

            loss_val = float(loss)
            accum_loss += loss_val
            accum_grad = grad if accum_grad is None else accum_grad + grad

            # Update every batch_size samples (gradient accumulation)
            if (sample_idx + 1) % args.batch_size == 0 or (sample_idx + 1) == len(samples):
                # Use actual count in this (possibly partial) batch
                actual_count = (sample_idx % args.batch_size) + 1
                avg_grad = accum_grad / actual_count
                new_spk_emb = optimizer.step(new_spk_emb, avg_grad)
                avg_loss = accum_loss / actual_count
                epoch_loss += avg_loss
                n_updates += 1
                global_step += 1

                if global_step % 10 == 0:
                    print(f"[train] epoch={epoch+1:3d}  step={global_step:5d}  "
                          f"loss={avg_loss:.4f}")

                if global_step % args.checkpoint_every == 0:
                    ckpt_path = ckpt_dir / f"spk_emb_step{global_step}.npz"
                    save_checkpoint(new_spk_emb, ckpt_path)

                accum_grad = None
                accum_loss = 0.0

        if n_updates > 0:
            print(f"[train] ── epoch {epoch+1}/{args.epochs}  "
                  f"avg_loss={epoch_loss/n_updates:.4f} ──")

    # ── Final checkpoint ─────────────────────────────────────────────────────
    final_ckpt = ckpt_dir / "spk_emb_final.npz"
    save_checkpoint(new_spk_emb, final_ckpt)

    # ── Patch and save model.safetensors ────────────────────────────────────
    out_model_path = model_dir / "model.safetensors"
    backup_path = model_dir / "model.safetensors.bak"
    if not backup_path.exists():
        import shutil
        shutil.copy2(str(out_model_path), str(backup_path))
        print(f"[train] Backup saved → {backup_path}")

    patch_and_save_safetensors(
        model_dir, frozen, new_spk_emb, args.speaker_id, out_model_path
    )

    print(f"\n[train] ✓ Training complete!")
    print(f"[train]   New speaker: '{args.speaker_name}' (id={args.speaker_id})")
    print(f"[train]   Embedding checkpoint: {final_ckpt}")
    print(f"[train]   Patched model: {out_model_path}")
    print(f"\n[train] Next step: run register_speaker.py to update config.json")


if __name__ == "__main__":
    main()
