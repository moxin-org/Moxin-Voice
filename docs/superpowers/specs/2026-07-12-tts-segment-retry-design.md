# TTS Segment Retry and Inspection Design

## Goal

Let users inspect and repair the independently synthesized parts of the current
long-text TTS result. A user can open the segment list from the player, listen
to or download an individual segment, inspect its source text, retry that
segment, and navigate the main player by segment. Retries intentionally work
around, rather than attempt to fix, upstream prosody defects.

## Scope

- Add an upward-opening segment-list popover, opened by a distinct segment-stack
  icon beside the current audio player's action menu.
- Store generated segments independently in memory for the current result.
- Preserve the exact original synthesis request for each segment and reuse it
  on retry.
- Replace only the retried segment after a successful retry, rebuild the main
  audio, and begin main playback at that segment's new start time.
- Add individual segment playback and download.
- Highlight the segment that contains the main player's current playback time
  and allow a row click to play from the segment start.

## Non-goals

- Change upstream Qwen sampling or attempt automatic pitch-drift detection.
- Persist per-segment WAV files or make segment retry available after reopening
  a history entry or restarting the app.
- Allow concurrent synthesis or retry requests.
- Change the existing history model beyond saving its normal, current merged
  audio snapshot.

## Alternatives considered

1. **UI-owned in-memory segment records (selected).** The screen stores text,
   original request payload, PCM samples, and UI state for each segment. The
   main audio is derived from these records. This keeps playback, replacement,
   and both download modes consistent.
2. **TTS-node-owned segment list.** The node would own retries and return a
   rebuilt complete audio asset. This makes interactive controls and downloads
   dependent on the inference node and obscures state from the UI.
3. **Temporary WAV per segment.** This simplifies isolated downloads but adds
   lifecycle, cleanup, and history-version management without helping playback
   or retry coordination.

## Data model

`TTSScreen` owns a `Vec<TtsAudioSegment>` for the current completed result.
Each record contains:

- `text`: the exact text group shown in the expanded row;
- `prompt`: the complete serialized request sent for its first synthesis;
- `samples` and `sample_rate`: the segment's source PCM data;
- `text_expanded`: whether the row exposes its text;
- `start_sample` and `end_sample`: derived offsets in the source-rate merged
  audio; and
- `retry_state`: idle or generating, used to render and disable controls.

The initial generation creates records in text order before dispatch. Each
record's `prompt` is created once from the pending generation snapshot and is
never rebuilt from the editor state. A retry therefore keeps the original
voice, cloning/reference information, native instruct, speed, pitch, volume,
and text even if the user changes the editor afterward.

The existing `stored_audio_samples` remains the compatibility cache for the
player, history, and existing export path. It is rebuilt by concatenating the
current segment records after initial generation and after any successful
replacement. `processed_audio_samples` continues to be derived from that cache
when speed processing requires it.

## Segment result protocol

Only one segment request is ever in flight. The existing completion payload is
extended with a request/segment identity and the emitted audio `sample_count`.
The screen collects bridge audio into the active segment buffer. It finalizes
that segment only after it has both a successful completion result and exactly
the reported number of audio samples. This protects the segment boundary from
the bridge's separate audio and completion queues arriving in a different UI
poll.

For initial generation, a completed record unlocks the next segment request.
For retry, a completed record replaces the selected segment only after all its
audio is collected. A failed completion leaves the previous PCM data untouched,
returns the row to idle, logs the error, and presents a retryable error toast.

## Player and popover interaction

The player gets a `Segments` button beside its existing action controls. It
toggles a floating popover anchored directly above the player:

- Header: segment count and current merged duration.
- One row per segment: ordinal, duration range, and icon-only play, download,
  text-expand, and retry controls.
- Expanded text appears below its own row with an explicit high-contrast text
  color in both normal and active-row states.
- The row containing `audio_playing_time` has a translucent primary-blue
  background and a visible, high-contrast “playing” label.
- Clicking a row, excluding an action button, calls the existing seek/play
  helper at that row's derived `start_sample` time.
- Segment preview and main playback are mutually exclusive: starting either
  stops the other before audio is written to its player.

Individual play starts a separate preview player from the segment's source
PCM. Individual download writes only that segment using the selected current
audio format and the segment's source sample rate. Main download keeps using
the rebuilt merged cache, so it includes every successful replacement.

## Retry flow

1. The user presses retry on an idle row.
2. The screen stops main playback, marks that row generating, disables all
   segment retries and all current-audio download actions, and sends the stored
   `prompt` for that row.
3. The screen buffers only this request's returned audio until its successful
   completion event and expected sample count are satisfied.
4. On success, it replaces that row's PCM, recomputes all source offsets,
   rebuilds the merged audio cache, refreshes duration/progress/popover UI,
   and starts main playback from the replacement row's new start time.
5. On failure, it discards buffered retry audio, retains the former row PCM,
   restores the controls, and displays a failure toast. The user may retry
   again.

The original generated result remains available while a retry is running. No
other syntheses, segment retries, main downloads, or history persistence run
until that request finishes.

## Error handling

- A completion event with `complete: false`, malformed result data, a sample
  count mismatch, or a failed prompt dispatch ends only the active operation.
- Initial-generation failure retains the current existing player result and
  does not publish a partially generated segment list.
- Retry failure retains the pre-retry list and audio unchanged.
- If a segment reports a sample rate different from the current result, retry
  fails visibly rather than silently mixing incompatible PCM.

## Testing

- Unit-test segment offset calculation, merged-sample rebuilding, and current
  segment lookup across boundaries.
- Unit-test completion/audio ordering: no segment becomes usable before its
  expected samples arrive.
- Unit-test successful retry replacement changes only the target record and
  begins playback at its recomputed start time.
- Unit-test retry failures preserve old PCM and restore retry availability.
- Unit-test each segment prompt remains unchanged after simulated editor/state
  changes.
- Exercise popover row action routing and row-click seek with focused UI tests.
- Run focused TTS bridge/node/app test suites and the applicable format/check
  commands. Document the already-known upstream `main` baseline test failure
  separately if it remains.

## Verification record

On 2026-07-12, the TTS segment model, player-popover contract, bridge package,
and Qwen node package tests passed. `cargo test -p moxin-voice --lib` ran 84
tests: 83 passed and the sole failure was
`voice_persistence::tests::test_voice_id_with_chinese`. That assertion also
fails on unchanged `moxin/main`; it is not caused by this feature. Repository-
wide `cargo fmt --check` reports pre-existing formatting differences in many
unrelated files, so this change intentionally does not run a bulk formatter.
