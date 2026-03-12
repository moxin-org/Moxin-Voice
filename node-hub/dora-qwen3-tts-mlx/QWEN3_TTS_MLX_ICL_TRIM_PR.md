# Qwen3-TTS-MLX ICL Trim Fix Proposal

## Summary

This project found a reproducible truncation issue in `qwen3-tts-mlx` ICL voice cloning on Apple Silicon.

When `synthesize_voice_clone_icl()` is used with:

- a valid reference audio
- a valid reference transcript
- a longer target sentence

the generated audio may lose the beginning of the target sentence.

The current implementation trims the generated codec frames using a heuristic based on:

- `ref_text_len`
- `target_text_len`

and assumes the generated frame sequence can be split proportionally by text token counts.

In practice, that assumption is too aggressive and can remove valid target speech.

## Reproduction

Reference text:

```text
骑亭下马解秋衣请宜宜阳一壶酒，湖中换天云不开白昼。万里嫌妻迷主人，劝我养心骨，莫受俗物香田絮。
```

Target text:

```text
复杂的问题背后也许没有统一的答案，选择站在正方还是反方，其实取决于你对一系列价值判断的回答。
```

Observed logs from `qwen3-tts-mlx`:

```text
Reference audio encoded: 93 frames
ICL text tokens: target=(25), ref=(16)
VoiceClone ICL generation complete: 135 frames
ICL: 93 ref frames, 135 gen frames, ref_ratio=0.381, trim=51 frames, 84 target frames
Generated 6.72s of audio
```

Observed result:

- generated audio only contains the latter half of the sentence
- specifically, the beginning segment is missing

## Root Cause

Current code in `src/lib.rs`:

```rust
let ref_ratio = ref_text_len as f64 / (ref_text_len + text_token_ids.len() + 1) as f64;
let trim_frames = (ref_ratio * gen_codes.len() as f64) as usize;
let target_codes = &gen_codes[trim_frames..];
```

Then later:

```rust
codes_for_decode.extend_from_slice(&ref_code_frames);
codes_for_decode.extend_from_slice(target_codes);

let ref_audio_cut = ...;
let samples = all_samples[ref_audio_cut..].to_vec();
```

The waveform path already:

1. prepends `ref_code_frames` as decoder warmup context
2. decodes the whole sequence
3. removes the decoded reference portion from the waveform

That means the additional `trim_frames` step is a second removal step based on a text-token heuristic.

This heuristic does not reliably map:

- reference text token ratio

to:

- generated acoustic frame ratio

because acoustic duration also depends on:

- speech rate
- pauses
- punctuation
- prosody
- language-specific timing

## Proposed Fix

Remove the proportional trim entirely and keep the full generated codec sequence:

```rust
let trim_frames = 0usize;
let target_codes = &gen_codes[..];
```

Keep the existing decoder warmup and waveform-side `ref_audio_cut` logic unchanged.

## Why This Should Be Merged Upstream

This change:

- fixes a clear correctness issue where valid target content is dropped
- is small and localized
- does not change the higher-level ICL API
- keeps the existing decoder warmup behavior

The existing proportional trim is heuristic and can silently corrupt output by removing real target speech.
That makes it a correctness bug rather than a tuning preference.

## Notes

- `ICL` is already documented upstream as experimental on Apple Silicon.
- This fix does not claim to make ICL fully robust.
- It only removes a clearly harmful post-processing heuristic that can truncate valid output.

