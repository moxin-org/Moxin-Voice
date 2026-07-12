# TTS Segment Retry and Inspection Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Let users inspect, play, download, seek to, and retry individual long-text TTS synthesis segments while keeping the rebuilt main audio coherent.

**Architecture:** The Qwen node reports the post-processing audio sample count with every completion event. `TTSScreen` buffers one active request at a time into an in-memory segment model and only finalizes it after both audio and completion metadata agree. The main player and its existing export path keep using a merged cache rebuilt from these segment records; a floating `PortalList` popover renders segment controls and dispatches screen actions.

**Tech Stack:** Rust, Makepad (`PortalList`, `Widget`), Dora, Arrow, serde JSON, Qwen3-TTS MLX.

---

## File structure

- Create: `apps/moxin-voice/src/tts_segments.rs` — pure segment records, assembly state, offset calculation, and merged-audio helpers.
- Modify: `apps/moxin-voice/src/lib.rs` — expose the new internal module.
- Modify: `apps/moxin-voice/src/screen.rs` — capture generation results into segments, retry/preview/download behavior, and Makepad popover UI.
- Modify: `node-hub/dora-qwen3-tts-mlx/src/main.rs` — include final audio sample count in `segment_complete`.
- Modify: `moxin-dora-bridge/src/data.rs` — deserialize completion sample count.
- Modify: `moxin-dora-bridge/src/widgets/audio_player.rs` — test the extended completion payload path.

### Task 1: Make the completion protocol delimit audio precisely

**Files:**
- Modify: `node-hub/dora-qwen3-tts-mlx/src/main.rs:35-49,603-606,661-675`
- Modify: `moxin-dora-bridge/src/data.rs:148-152`
- Test: `node-hub/dora-qwen3-tts-mlx/src/main.rs`
- Test: `moxin-dora-bridge/src/data.rs`

- [ ] **Step 1: Write failing node and bridge tests for the count field.**

```rust
#[test]
fn segment_result_serializes_post_processed_audio_sample_count() {
    let result = SegmentResult::from_attempts(1, false, 44).with_sample_count(2_400);
    let value = serde_json::to_value(result).unwrap();
    assert_eq!(value["sample_count"], 2_400);
}

#[test]
fn tts_segment_event_deserializes_audio_sample_count() {
    let event: TtsSegmentEvent = serde_json::from_str(
        r#"{"complete":true,"attempts":1,"generation_frames":44,"sample_count":2400}"#,
    ).unwrap();
    assert_eq!(event.sample_count, Some(2_400));
}
```

- [ ] **Step 2: Run the focused tests and verify the missing field causes failure.**

Run: `cargo test -p dora-qwen3-tts-mlx segment_result_serializes_post_processed_audio_sample_count -- --exact && cargo test -p moxin-dora-bridge tts_segment_event_deserializes_audio_sample_count -- --exact`

Expected: FAIL because `with_sample_count` and `TtsSegmentEvent::sample_count` do not exist.

- [ ] **Step 3: Add the protocol field after audio post-processing.**

```rust
#[derive(Debug, Clone, Serialize)]
struct SegmentResult {
    complete: bool,
    attempts: u8,
    generation_frames: usize,
    sample_count: Option<usize>,
}

impl SegmentResult {
    fn with_sample_count(mut self, sample_count: usize) -> Self {
        self.sample_count = Some(sample_count);
        self
    }
}

// after apply_runtime_audio_params
let result = segment.result.with_sample_count(samples.len());
send_audio(&mut node, &samples, segment.sample_rate)?;
send_segment_complete(&mut node, &result)?;
```

Keep failed/incomplete completion results at `sample_count: None`. Make the
bridge field optional with `#[serde(default)]` so legacy completion payloads
continue to parse.

- [ ] **Step 4: Run the focused protocol tests.**

Run: `cargo test -p dora-qwen3-tts-mlx segment_result_serializes_post_processed_audio_sample_count -- --exact && cargo test -p moxin-dora-bridge tts_segment_event_deserializes_audio_sample_count -- --exact`

Expected: PASS.

- [ ] **Step 5: Commit the protocol boundary.**

```bash
git add node-hub/dora-qwen3-tts-mlx/src/main.rs moxin-dora-bridge/src/data.rs moxin-dora-bridge/src/widgets/audio_player.rs
git commit -m "feat: report TTS segment audio length"
```

### Task 2: Add the pure in-memory segment model

**Files:**
- Create: `apps/moxin-voice/src/tts_segments.rs`
- Modify: `apps/moxin-voice/src/lib.rs`
- Test: `apps/moxin-voice/src/tts_segments.rs`

- [ ] **Step 1: Write failing unit tests for merge, offsets, selection, and safe replacement.**

```rust
#[test]
fn merged_segments_expose_contiguous_offsets_and_current_segment() {
    let segments = TtsAudioSegments::from_completed(vec![
        TtsAudioSegment::completed("甲", "payload-1", vec![0.1, 0.2], 24_000),
        TtsAudioSegment::completed("乙", "payload-2", vec![0.3, 0.4, 0.5], 24_000),
    ]).unwrap();
    assert_eq!(segments.merged_samples(), vec![0.1, 0.2, 0.3, 0.4, 0.5]);
    assert_eq!(segments.start_sample(1), Some(2));
    assert_eq!(segments.index_at_sample(2), Some(1));
}

#[test]
fn replacement_changes_only_the_selected_segment() {
    let mut segments = two_completed_segments();
    segments.replace_samples(1, vec![0.9], 24_000).unwrap();
    assert_eq!(segments.segment(0).unwrap().samples, vec![0.1, 0.2]);
    assert_eq!(segments.merged_samples(), vec![0.1, 0.2, 0.9]);
}

#[test]
fn replacement_rejects_a_mismatched_sample_rate() {
    let mut segments = two_completed_segments();
    assert_eq!(segments.replace_samples(1, vec![0.9], 32_000), Err(SegmentAudioError::SampleRateMismatch));
}
```

- [ ] **Step 2: Run the module tests and verify they fail because the module does not exist.**

Run: `cargo test -p moxin-voice tts_segments::tests -- --nocapture`

Expected: FAIL with an unresolved `tts_segments` module.

- [ ] **Step 3: Implement the focused model and its active-request assembler.**

```rust
pub struct TtsAudioSegment {
    pub text: String,
    pub request_payload: String,
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub text_expanded: bool,
}

impl TtsAudioSegment {
    pub fn pending(text: impl Into<String>, request_payload: impl Into<String>) -> Self;
    pub fn completed(
        text: impl Into<String>,
        request_payload: impl Into<String>,
        samples: Vec<f32>,
        sample_rate: u32,
    ) -> Self;
}

pub struct ActiveSegmentAssembly {
    pub index: usize,
    pub samples: Vec<f32>,
    pub sample_rate: Option<u32>,
    pub expected_samples: Option<usize>,
}

impl ActiveSegmentAssembly {
    pub fn new(index: usize) -> Self;
    pub fn push_audio(&mut self, samples: &[f32], sample_rate: u32) -> Result<(), SegmentAudioError>;
    pub fn mark_complete(&mut self, sample_count: Option<usize>) -> Result<bool, SegmentAudioError>;
    pub fn is_ready(&self) -> bool;
    pub fn into_samples(self) -> Result<(Vec<f32>, u32), SegmentAudioError>;
}
```

`mark_complete` returns `true` only after a count exists and the buffered audio
length equals it. `push_audio` re-checks that condition so completion-before-
audio ordering is safe. `TtsAudioSegments` owns all offset, duration, merge,
and source-rate validation logic; it contains no Makepad or Dora code.

- [ ] **Step 4: Run the new module tests.**

Run: `cargo test -p moxin-voice tts_segments::tests -- --nocapture`

Expected: PASS.

- [ ] **Step 5: Commit the segment model.**

```bash
git add apps/moxin-voice/src/lib.rs apps/moxin-voice/src/tts_segments.rs
git commit -m "feat: model TTS audio segments"
```

### Task 3: Route initial generation into pending segments safely

**Files:**
- Modify: `apps/moxin-voice/src/screen.rs:403-447,9356-9740,10142-10220,18494-18545,22853-22915`
- Test: `apps/moxin-voice/src/screen.rs`

- [ ] **Step 1: Write failing screen-level tests for request snapshots and delayed finalization.**

```rust
#[test]
fn active_tts_segment_waits_for_audio_count_after_completion() {
    let mut active = ActiveSegmentAssembly::new(0);
    assert!(!active.mark_complete(Some(3)).unwrap());
    active.push_audio(&[0.1, 0.2], 24_000).unwrap();
    assert!(!active.is_ready());
    active.push_audio(&[0.3], 24_000).unwrap();
    assert!(active.is_ready());
}

#[test]
fn retry_payload_is_the_initial_snapshot_not_editor_state() {
    let segment = TtsAudioSegment::pending("原文本", "{\"prompt\":\"VOICE:...\"}");
    assert_eq!(segment.request_payload, "{\"prompt\":\"VOICE:...\"}");
}
```

- [ ] **Step 2: Run the focused tests and verify failure before integration.**

Run: `cargo test -p moxin-voice 'active_tts_segment_waits_for_audio_count_after_completion|retry_payload_is_the_initial_snapshot_not_editor_state'`

Expected: FAIL until the model is wired into `TTSScreen`.

- [ ] **Step 3: Build all initial request payloads before dispatch.**

Replace `TtsSegmentDispatch`'s raw `Vec<String>` with indexed pending segment
requests. At generation start, create `pending_tts_segments` in text order and
store the exact JSON payload currently produced by `send_next_pending_tts_segment`.
Do not clear the prior `stored_audio_samples` or published segment list at this
point. Dispatch exactly one pending payload and instantiate an
`ActiveSegmentAssembly` for its index.

- [ ] **Step 4: Finalize only complete buffered segments in the timer loop.**

```rust
for audio in chunks {
    if let Some(active) = self.active_tts_segment.as_mut() {
        active.push_audio(&audio.samples, audio.sample_rate)?;
    }
}
for event in segment_events {
    self.active_tts_segment.as_mut().unwrap().mark_complete(event.sample_count)?;
}
if self.active_tts_segment.as_ref().is_some_and(ActiveSegmentAssembly::is_ready) {
    self.finalize_initial_tts_segment(cx)?;
}
```

`finalize_initial_tts_segment` moves the assembled PCM into its pending record,
sends the next request, and promotes the pending collection only once all
records are complete. On an event error, missing count, or sample-rate mismatch,
discard pending assembly data, leave the currently published player result
unchanged, and show the existing failure UI.

- [ ] **Step 5: Rebuild compatibility caches only from published records.**

Add `rebuild_audio_from_segments` to concatenate the published segment PCM into
`stored_audio_samples`, set `stored_audio_sample_rate`, call
`rebuild_processed_audio_samples`, and refresh player progress. Use it at
initial promotion instead of appending raw bridge chunks directly.

- [ ] **Step 6: Run focused app tests.**

Run: `cargo test -p moxin-voice 'active_tts_segment_waits_for_audio_count_after_completion|retry_payload_is_the_initial_snapshot_not_editor_state|tts_segment_dispatch_sends_one_segment_at_a_time'`

Expected: PASS.

- [ ] **Step 7: Commit initial-segment capture.**

```bash
git add apps/moxin-voice/src/screen.rs apps/moxin-voice/src/tts_segments.rs
git commit -m "feat: retain generated TTS segments"
```

### Task 4: Add retry, segment playback, and per-segment export

**Files:**
- Modify: `apps/moxin-voice/src/screen.rs:167-170,10142-10220,19480-19645,23004-23455`
- Modify: `apps/moxin-voice/src/tts_segments.rs`
- Test: `apps/moxin-voice/src/screen.rs`

- [ ] **Step 1: Write failing tests for retry replacement and segment seek.**

```rust
#[test]
fn successful_retry_replaces_target_and_seeks_to_new_segment_start() {
    let mut segments = two_completed_segments();
    segments.replace_samples(1, vec![0.9, 1.0], 24_000).unwrap();
    assert_eq!(segments.start_sample(1), Some(2));
    assert_eq!(segments.start_time_secs(1), Some(2.0 / 24_000.0));
}

#[test]
fn failed_retry_keeps_the_old_segment_audio() {
    let mut segments = two_completed_segments();
    let before = segments.merged_samples();
    assert!(!segments.apply_retry(1, RetryCommit::KeepExisting).unwrap());
    assert_eq!(segments.merged_samples(), before);
}
```

- [ ] **Step 2: Run the focused tests and verify they fail before the retry controller exists.**

Run: `cargo test -p moxin-voice 'successful_retry_replaces_target_and_seeks_to_new_segment_start|failed_retry_keeps_the_old_segment_audio'`

Expected: FAIL until retry state and replacement calls exist.

- [ ] **Step 3: Add a single-operation retry controller.**

```rust
enum ActiveTtsOperation {
    Initial { segment_index: usize },
    Retry { segment_index: usize },
}

enum RetryCommit {
    Replace { samples: Vec<f32>, sample_rate: u32 },
    KeepExisting,
}

impl TtsAudioSegments {
    fn apply_retry(&mut self, index: usize, commit: RetryCommit) -> Result<bool, SegmentAudioError> {
        match commit {
            RetryCommit::Replace { samples, sample_rate } => {
                self.replace_samples(index, samples, sample_rate)?;
                Ok(true)
            }
            RetryCommit::KeepExisting => Ok(false),
        }
    }
}

fn retry_tts_segment(&mut self, cx: &mut Cx, index: usize) {
    let payload = self.tts_segments.segment(index).unwrap().request_payload.clone();
    self.stop_main_playback();
    self.active_tts_operation = Some(ActiveTtsOperation::Retry { segment_index: index });
    self.active_tts_segment = Some(ActiveSegmentAssembly::new(index));
    self.dora.as_ref().unwrap().send_prompt(payload);
}
```

Disable generation, retry, current-audio download, and history persistence for
the duration of this operation. On successful completion, call
`apply_retry(index, RetryCommit::Replace { .. })`, rebuild the merged cache, and call
`start_playback_from_time` using the replacement segment's new start time. On
any failure, discard the active assembler only, call
`apply_retry(index, RetryCommit::KeepExisting)`, preserve the former segment,
restore controls, and show a retryable toast.

- [ ] **Step 4: Add source-PCM preview and download helpers.**

Extend `DownloadSource` with `Segment(usize)`. Add
`start_segment_preview`, which creates or resets `preview_player` with the
segment's own samples/rate, and `export_segment_audio`, which selects MP3/WAV
using the existing preference/fallback helpers and writes only those samples.
Name files `tts_segment_<one-based-index>_<timestamp>.<extension>`.

- [ ] **Step 5: Run focused retry/export tests.**

Run: `cargo test -p moxin-voice 'successful_retry_replaces_target_and_seeks_to_new_segment_start|failed_retry_keeps_the_old_segment_audio|tts_download_falls_back_to_wav_when_mp3_encoder_is_unavailable'`

Expected: PASS.

- [ ] **Step 6: Commit retry and export behavior.**

```bash
git add apps/moxin-voice/src/screen.rs apps/moxin-voice/src/tts_segments.rs
git commit -m "feat: retry and export TTS segments"
```

### Task 5: Render and wire the player segment popover

**Files:**
- Modify: `apps/moxin-voice/src/screen.rs:1797-1825,7462-7722,7726-7745,13667-13740,19020-19092,25229-25258,28948-29070`
- Test: `apps/moxin-voice/src/screen.rs`

- [ ] **Step 1: Write failing UI-contract tests.**

```rust
#[test]
fn player_bar_exposes_segment_popover_and_actions() {
    let source = include_str!("screen.rs");
    for marker in [
        "segment_menu_btn = <PlayerSegmentBtn>",
        "player_segment_menu = <PlayerFloatingMenu>",
        "segment_portal_list = <PortalList>",
        "self.retry_tts_segment(cx,",
        "self.start_segment_preview(cx,",
        "DownloadSource::Segment(",
    ] {
        assert!(source.contains(marker), "missing segment player UI: {marker}");
    }
}
```

- [ ] **Step 2: Run the UI-contract test and verify it fails.**

Run: `cargo test -p moxin-voice player_bar_exposes_segment_popover_and_actions -- --exact`

Expected: FAIL because no segment menu exists.

- [ ] **Step 3: Add a player button and upward-anchored floating menu.**

Add `PlayerSegmentBtn` beside `action_menu_btn`, then add
`player_segment_menu = <PlayerFloatingMenu>` with a bounded-height
`segment_portal_list = <PortalList>`. Follow the existing
`player_menu_abs_pos` pattern, using the new button's rectangle and a menu
height clamped to the available screen space. Track
`player_segment_menu_open` and close it with the other player menus.

- [ ] **Step 4: Render rows using the existing PortalList pattern.**

In `draw_walk`, obtain the list UID and draw one visible row per
`tts_segments` entry. Each row shows 1-based ordinal, derived range,
independent play/download/text/retry buttons, and a text view whose height is
zero unless `text_expanded`. Apply a translucent primary-blue row background
and a "Playing" label when `index_at_sample(current_sample)` matches the row.

- [ ] **Step 5: Route row and action events.**

Map row-click actions to `start_playback_from_time` at the derived segment
start. Map explicit action buttons to preview, `open_download_modal` with
`DownloadSource::Segment`, text expansion, and retry. Do not interpret a
button click as a row seek. During retry, disable every retry and download
button and show the active row's generating state.

- [ ] **Step 6: Apply dark-mode styles and test the contracts.**

Add the menu/button/row paths to the existing dark-mode refresh list. Then run:

Run: `cargo test -p moxin-voice 'player_bar_exposes_segment_popover_and_actions|player_bar_exposes_common_audio_controls|player_bar_controls_are_wired_to_actions'`

Expected: PASS.

- [ ] **Step 7: Commit the popover UI.**

```bash
git add apps/moxin-voice/src/screen.rs
git commit -m "feat: inspect TTS generation segments"
```

### Task 6: Regression verification and documentation

**Files:**
- Modify: `docs/superpowers/specs/2026-07-12-tts-segment-retry-design.md`

- [ ] **Step 1: Run formatting and focused package tests.**

Run: `cargo fmt --check && cargo test -p moxin-dora-bridge && cargo test -p dora-qwen3-tts-mlx && cargo test -p moxin-voice`

Expected: all new segment tests pass. The known `voice_persistence::tests::test_voice_id_with_chinese` baseline failure must be rechecked against unchanged `moxin/main` and reported separately if it remains.

- [ ] **Step 2: Build the edited packages.**

Run: `cargo check -p moxin-dora-bridge -p dora-qwen3-tts-mlx -p moxin-voice`

Expected: PASS.

- [ ] **Step 3: Perform a manual local smoke test.**

Generate text that creates at least three segments. Verify list count/text,
row seek/highlighting, individual playback/download, a successful retry that
starts main playback at its replacement offset, and a forced/observed retry
failure that preserves the prior audio.

- [ ] **Step 4: Record actual verification results in the design spec and commit.**

```bash
git add docs/superpowers/specs/2026-07-12-tts-segment-retry-design.md docs/superpowers/plans/2026-07-12-tts-segment-retry.md
git commit -m "docs: verify TTS segment retry"
```
