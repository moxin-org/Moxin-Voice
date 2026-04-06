# Realtime Translation Cleanup And Takeover Design

## Goal

Restore the realtime translation pipeline to a recent stable baseline, keep only the clearly low-risk fixes that improve input quality, and establish a clean ownership boundary for the next round of latency and sentence segmentation work.

## Current Problem

Recent uncommitted work introduced sentence-commit logic in both the ASR node and the translator node. That means both layers now try to decide:

- which text is already committed
- which text should still stream as a tail
- which prefix should be stripped on later updates

This overlap makes the pipeline harder to reason about and increases the risk of repeated text, dropped text, wrong ordering, stale tails, and regressions from further tuning.

## Approved Cleanup Direction

Use the recent committed code as the stable baseline and keep only the small, local fixes that have clear value and minimal blast radius.

### Keep

- `apps/moxin-voice/src/screen.rs`
  - lower VAD RMS thresholds for system audio input so clean captured audio is less likely to be gated out
  - refresh translation merge button state when translation page defaults are initialized
- `moxin-dora-bridge/src/widgets/aec_input.rs`
  - clear `speech_buffer` when a segment is force-closed by max segment size so the next segment does not replay overlapping tail audio

### Revert

- `node-hub/dora-qwen3-asr/src/main.rs`
  - remove all uncommitted progressive sentence upgrade and prefix-stripping logic
- `node-hub/dora-qwen35-translator/src/main.rs`
  - remove all uncommitted sentence-cursor and mid-session commit logic

## Responsibility Boundary After Cleanup

After cleanup, the ASR node should return to a simple role:

- emit progressive ASR text
- emit final ASR text
- avoid any mid-burst sentence promotion or translator-facing commit bookkeeping

The translator should also return to its last stable committed behavior during the cleanup phase. It will not keep the current uncommitted sentence-cursor experiment.

This gives the pipeline a single stable baseline before new latency work begins.

## Takeover Direction

After cleanup, the next optimization pass should give sentence-commit ownership to one layer only. The recommended owner is the translator node.

Why translator:

- it sees text rather than raw audio
- it is the layer that already owns translation timing and output emission
- it can make sentence-boundary decisions without coupling ASR to translator-specific state

The ASR node should stay simple so the boundary remains easy to test and reason about.

## Data Flow

### Cleanup Phase

1. Audio input and VAD produce audio segments as they do today.
2. ASR emits progressive and final text without mid-burst commit logic.
3. Translator consumes the stable ASR stream using the last committed implementation.
4. Overlay/UI continue to display source and translation using the stable event model.

### Post-Cleanup Optimization Phase

1. Keep ASR output contract unchanged.
2. Add earlier sentence segmentation only in translator.
3. Keep translator responsible for any future partial-commit policy, tail display policy, and finalize policy.

## Error Handling And Safety

- Cleanup should prefer removing overlapping logic over patching around it.
- Any change kept during cleanup must be local and independently understandable.
- If a kept small fix affects only input quality or UI initialization and does not alter cross-node commit semantics, it is acceptable to preserve.
- Validation should focus on catching duplication, truncation, stale tails, and ordering regressions.

## Validation Strategy

### After Cleanup

- verify the repo contains only the intended low-risk uncommitted changes
- run targeted tests for the touched crates where available
- run a manual realtime translation smoke test with both microphone and system audio

### During Takeover Work

- add tests around translator-side sentence commit behavior before changing implementation
- verify lower latency without reintroducing duplicate or missing segments

## Success Criteria

- uncommitted ASR and translator sentence-commit experiments are removed
- only the approved low-risk fixes remain
- realtime translation is back on a predictable baseline
- the next latency iteration starts from a single clear ownership model instead of a split-brain design
