# TTS Long-Text Reliability Design

## Goal

Reduce local Qwen3-TTS long-text pitch drift and prevent silently incomplete custom-voice output by constraining one synthesis request to a short sentence group and making segment completion explicit and observable.

## Scope

- Limit a TTS synthesis segment to 120 Unicode characters.
- Prefer complete sentence boundaries; use clause boundaries only when a sentence exceeds the limit.
- Advance the UI dispatcher only after an explicit `segment_complete` event, not merely after audio arrives.
- Carry a structured segment result from the TTS node with generated frame count and early-EOS metadata.
- Retry a custom-voice segment once with a deterministic alternate seed when the inference layer can prove EOS occurred before all streamed text tokens were consumed.
- Surface a recoverable error when the retry is also incomplete.

## Non-goals

- Change Qwen3-TTS sampling or force EOS suppression through the whole text.
- Guarantee identical prosody across independently synthesized sentence groups.
- Add ASR round-trip validation or change the public UI layout.

## Alternatives considered

1. Only lower the character limit. This reduces risk but still treats an early-EOS audio result as success.
2. Lower the limit and add completion/result metadata. Recommended: it limits blast radius and detects the concrete incomplete-output condition without changing model behavior.
3. Suppress EOS until all text tokens are consumed. Rejected for this change because it diverges from the upstream generation policy and could replace truncation with unnatural tails or hangs.

## Architecture

`TTSScreen` splits 120-character-or-shorter sentence groups and sends one request at a time. The Dora node emits the audio payload, a JSON `segment_result` payload, and then `segment_complete`. The dataflow routes `segment_complete` into the existing audio-player bridge. That bridge places a `TtsSegmentEvent` in a new `SharedDoraState` queue, separate from the audio queue. The screen appends any audio but does not dispatch the next group until it observes the matching completion event.

The Qwen MLX library returns generation metadata. Clone modes report whether EOS occurred before their remaining streamed text tokens were consumed. The Dora node retries only that segment once, using a seed derived from the request content plus retry number. If both attempts are incomplete, it emits an error result; the screen stops and tells the user which segment failed rather than silently omitting text.

## Error handling

- Audio without a matching completion event remains pending; no subsequent segment is sent.
- A failed or twice-incomplete segment terminates the request and retains already generated audio only in memory; it is not marked as a completed TTS result.
- Legacy/unsupported backends keep their existing behavior unless they emit the new explicit event.

## Tests

- 120-character splitting preserves all input characters and prefers sentence boundaries.
- The dispatcher does not advance on audio arrival and advances once on its matching completion event.
- Clone-generation metadata marks EOS before the streamed text is exhausted as incomplete.
- Result parsing and retry selection perform exactly one retry and report the second failure.
