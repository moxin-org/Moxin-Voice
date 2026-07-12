# TTS Long-Text Reliability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Keep long TTS synthesis within short sentence groups and detect/retry clone results that end before their streamed text is consumed.

**Architecture:** `TTSScreen` dispatches at most 120-character groups. The Qwen node records whether clone generation ended by EOS before all streamed text tokens were supplied, retries once with a derived seed, then emits an audio payload and a structured `segment_complete` payload. The audio-player bridge routes completion events to a bounded shared-state queue; the screen appends audio independently and dispatches the next text group only after a successful completion event.

**Tech Stack:** Rust, Makepad, Dora, Arrow, serde JSON, Qwen3-TTS MLX.

---

### Task 1: Sentence-group splitting contract

**Files:**
- Modify: `apps/moxin-voice/src/screen.rs:403-490,29300-29330`

- [ ] **Step 1: Write failing tests for the 120-character contract**

```rust
#[test]
fn long_tts_text_is_split_into_120_character_sentence_groups() {
    let text = "甲。".repeat(100);
    let segments = split_tts_text_segments(&text, TTS_INPUT_MAX_CHARS);
    assert!(segments.iter().all(|segment| segment.chars().count() <= 120));
    assert_eq!(segments.concat(), text);
}

#[test]
fn tts_text_split_keeps_complete_sentences_when_they_fit() {
    let first = format!("{}。", "甲".repeat(70));
    let second = format!("{}。", "乙".repeat(50));
    assert_eq!(split_tts_text_segments(&format!("{first}{second}"), 120), vec![first, second]);
}
```

- [ ] **Step 2: Run the two tests and verify the 120-character test fails under the current 1000-character limit**

Run: `cargo test -p moxin-voice long_tts_text_is_split_into_120_character_sentence_groups -- --exact`

Expected: FAIL because one returned segment exceeds 120 characters.

- [ ] **Step 3: Change the production limit**

```rust
const TTS_INPUT_MAX_CHARS: usize = 120;
```

Keep `split_tts_text_segments` unchanged: it already selects paragraph, sentence, then clause boundaries before a hard character cut.

- [ ] **Step 4: Run the focused splitter tests**

Run: `cargo test -p moxin-voice 'long_tts_text_is_split_into_120_character_sentence_groups|tts_text_split_keeps_complete_sentences_when_they_fit|tts_text_split_prefers_sentence_boundaries_before_limit'`

Expected: PASS.

### Task 2: Clone generation completeness metadata

**Files:**
- Modify: `node-hub/dora-qwen3-tts-mlx/patches/qwen3-tts-mlx/src/generate.rs:95-100,713-833,863-1013`
- Modify: `node-hub/dora-qwen3-tts-mlx/patches/qwen3-tts-mlx/src/lib.rs:286-1000`
- Test: `node-hub/dora-qwen3-tts-mlx/patches/qwen3-tts-mlx/src/generate.rs`

- [ ] **Step 1: Write a failing metadata classification test**

```rust
#[test]
fn clone_result_is_incomplete_when_eos_precedes_remaining_text() {
    assert!(GenerationTiming::from_termination(8, Some(16), true).is_incomplete_clone());
    assert!(!GenerationTiming::from_termination(16, Some(16), true).is_incomplete_clone());
    assert!(!GenerationTiming::from_termination(8, None, true).is_incomplete_clone());
}
```

- [ ] **Step 2: Run the test and verify it fails because the constructor and predicate do not exist**

Run: `cargo test -p qwen3-tts-mlx clone_result_is_incomplete_when_eos_precedes_remaining_text -- --exact`

Expected: compile failure referencing `from_termination`.

- [ ] **Step 3: Add termination metadata and populate clone paths**

```rust
pub struct GenerationTiming {
    pub prefill_ms: f64,
    pub generation_ms: f64,
    pub generation_frames: usize,
    pub ended_by_eos: bool,
    pub streamed_text_tokens: Option<usize>,
}

impl GenerationTiming {
    pub fn is_incomplete_clone(&self) -> bool {
        self.ended_by_eos
            && self.streamed_text_tokens.is_some_and(|tokens| self.generation_frames < tokens)
    }
}
```

Track `ended_by_eos` inside each generation loop. Set `streamed_text_tokens` to `Some(trailing_len)` for x-vector clone mode; retain `None` for CustomVoice and ICL modes when no remaining streamed text exists. Thread the new timing values through library synthesis timing APIs.

- [ ] **Step 4: Run the metadata unit test and package tests**

Run: `cargo test -p qwen3-tts-mlx`

Expected: PASS.

### Task 3: One-retry Qwen node result protocol

**Files:**
- Modify: `node-hub/dora-qwen3-tts-mlx/src/main.rs:20-80,421-550,571-620`
- Test: `node-hub/dora-qwen3-tts-mlx/src/main.rs`

- [ ] **Step 1: Write failing tests for deterministic retry selection and result serialization**

```rust
#[test]
fn retry_seed_is_stable_and_changes_between_attempts() {
    assert_eq!(retry_seed("VOICE:CUSTOM|ref|text", 0), retry_seed("VOICE:CUSTOM|ref|text", 0));
    assert_ne!(retry_seed("VOICE:CUSTOM|ref|text", 0), retry_seed("VOICE:CUSTOM|ref|text", 1));
}

#[test]
fn segment_result_reports_second_incomplete_attempt_as_failure() {
    let result = SegmentResult::from_attempts(2, true, 44);
    assert!(!result.complete);
    assert_eq!(result.attempts, 2);
}
```

- [ ] **Step 2: Run the tests and verify they fail because retry/result helpers do not exist**

Run: `cargo test -p dora-qwen3-tts-mlx retry_seed_is_stable_and_changes_between_attempts -- --exact`

Expected: compile failure referencing `retry_seed`.

- [ ] **Step 3: Implement a bounded retry and structured completion payload**

```rust
#[derive(Serialize)]
struct SegmentResult { complete: bool, attempts: u8, generation_frames: usize }

fn send_segment_complete(node: &mut DoraNode, result: &SegmentResult) -> Result<()> {
    node.send_output("segment_complete".into(), Default::default(), vec![serde_json::to_string(result)?].into_arrow())
}
```

Use timing-aware synthesis APIs. For a proven incomplete clone result, do not emit its audio, retry once with `SynthesizeOptions { seed: Some(retry_seed(&text, 1)), .. }`, and emit audio only for the successful attempt. Emit `SegmentResult { complete: false, attempts: 2, .. }` and status `error:` after the second incomplete result. Preserve existing behavior for routes without completeness metadata.

- [ ] **Step 4: Run node tests**

Run: `cargo test -p dora-qwen3-tts-mlx`

Expected: PASS.

### Task 4: Completion event bridge and UI dispatch

**Files:**
- Modify: `apps/moxin-voice/dataflow/tts.yml:48-55`
- Modify: `moxin-dora-bridge/src/data.rs`
- Modify: `moxin-dora-bridge/src/shared_state.rs`
- Modify: `moxin-dora-bridge/src/widgets/audio_player.rs`
- Modify: `apps/moxin-voice/src/screen.rs:406-445,10142-10190,18493-18550,29335-29360`

- [ ] **Step 1: Write failing dispatcher tests**

```rust
#[test]
fn tts_segment_dispatch_waits_for_explicit_completion() {
    let mut dispatch = TtsSegmentDispatch::new(vec!["第一句。".into(), "第二句。".into()]);
    assert!(dispatch.take_next().is_some());
    assert!(!dispatch.mark_audio_received());
    assert!(dispatch.take_next().is_none());
    assert!(dispatch.mark_completed());
    assert!(dispatch.take_next().is_some());
}
```

- [ ] **Step 2: Run the test and verify it fails because audio and completion state are not separate**

Run: `cargo test -p moxin-voice tts_segment_dispatch_waits_for_explicit_completion -- --exact`

Expected: compile failure referencing `mark_audio_received` and `mark_completed`.

- [ ] **Step 3: Add a bounded shared completion-event queue and dataflow route**

```yaml
inputs:
  audio: primespeech-tts/audio
  segment_complete: primespeech-tts/segment_complete
```

Define a deserializable `TtsSegmentEvent` in `moxin-dora-bridge::data`. Add a bounded `TtsSegmentEventState` beside `AudioState`, with `push`, `drain`, and `clear` methods backed by `RwLock<VecDeque<TtsSegmentEvent>>`; expose it as `SharedDoraState::tts_segment_events`. Have `AudioPlayerBridge` parse its `segment_complete` StringArray input and push the event. Add parser and queue unit tests for valid JSON, malformed JSON, and drain-once semantics.

- [ ] **Step 4: Separate audio receipt from completion in `TTSScreen`**

```rust
for event in dora.shared_dora_state().tts_segment_events.drain() {
    if !event.complete {
        self.fail_pending_tts_generation(cx, "[ERROR] [tts] Segment generation ended before all text was synthesized");
        break;
    }
    self.pending_generation_dispatch.mark_completed();
    if !self.pending_generation_dispatch.is_complete() {
        self.send_next_pending_tts_segment(cx);
    }
}
```

Keep audio draining solely responsible for appending samples. Remove `mark_received()` from the audio loop. Add a test that audio alone cannot advance dispatch and a completion event advances exactly one pending segment.

- [ ] **Step 5: Run focused bridge, node, and UI tests**

Run: `cargo test -p moxin-dora-bridge && cargo test -p dora-qwen3-tts-mlx && cargo test -p moxin-voice`

Expected: PASS.

### Task 5: End-to-end regression checks and documentation

**Files:**
- Modify: `docs/superpowers/specs/2026-07-12-tts-reliability-design.md`

- [ ] **Step 1: Run formatting and workspace checks**

Run: `cargo fmt --check && cargo check -p moxin-dora-bridge -p dora-qwen3-tts-mlx -p moxin-voice`

Expected: PASS.

- [ ] **Step 2: Perform a local 2000-character clone smoke test**

Run the existing `synthesize` example with the Base model and a bundled reference voice, then inspect node logs for one completion event per 120-character group and no incomplete result marked complete.

- [ ] **Step 3: Commit implementation**

```bash
git add apps/moxin-voice/src/screen.rs apps/moxin-voice/dataflow/tts.yml moxin-dora-bridge/src/data.rs moxin-dora-bridge/src/shared_state.rs moxin-dora-bridge/src/widgets/audio_player.rs node-hub/dora-qwen3-tts-mlx/src/main.rs node-hub/dora-qwen3-tts-mlx/patches/qwen3-tts-mlx/src/generate.rs node-hub/dora-qwen3-tts-mlx/patches/qwen3-tts-mlx/src/lib.rs docs/superpowers/specs/2026-07-12-tts-reliability-design.md docs/superpowers/plans/2026-07-12-tts-reliability.md
git commit -m "fix: improve long text tts reliability"
```
