# Translator Buffer Merge Design

## Goal

Replace the current event-driven sentence segmentation logic in the realtime translator with a text-driven model.

The new translator should treat upstream ASR/VAD as a plain text provider:

- it receives a sequence of ASR text chunks
- it does not rely on `transcription_mode`
- it does not rely on `question_id`
- it does not treat `final` or `question_ended` as sentence boundaries

Instead, it maintains a continuously growing transcript buffer, merges new ASR chunks into that buffer, and commits translations only when it can identify a stable, meaningful span of text.

## Current Problem

The current translator still inherits too much boundary meaning from upstream ASR and VAD events.

That leads to two recurring failure modes:

- text is cut too early because upstream emits a `final` chunk or a delayed `question_ended` path forces a finalize
- text is cut too late because translator waits for upstream event boundaries instead of deciding from the evolving text itself

This is a structural mismatch:

- upstream segmentation is audio-driven
- downstream translation quality depends on text stability and semantic completeness

Those are not the same boundary.

## Design Direction

Use a single text buffer as the source of truth inside the translator.

The translator should no longer think in terms of:

- ASR bursts
- progressive versus final as commit signals
- upstream utterance boundaries

The translator should instead think in terms of:

- a continuously evolving transcript buffer
- merge anchors for incorporating new ASR observations
- committed prefix position for already translated text
- stable spans that are safe to translate

## Core State

The new translator core maintains three main pieces of state.

### `buffer`

A single continuously growing string that represents the best current transcript for the active realtime session.

This buffer is not divided into upstream chunks or utterances. It is the translator's own view of the current transcript.

### `last_pos`

The last merge anchor.

This marks the approximate position in `buffer` after which the next incoming ASR chunk should be matched and merged.

The point of `last_pos` is to avoid rescanning the entire transcript every time and to restrict merge work to the recently changing tail of the text.

### `committed_pos`

The translation commit anchor.

This marks the position in `buffer` up to which translation has already been committed to the UI/history.

When the translator is idle, it searches for the next committable span starting at `committed_pos`.

## Upstream Contract

The translator consumes only text content from ASR chunks.

For this redesign:

- upstream chunk boundaries are advisory at most
- upstream `progressive/final` metadata is ignored for sentence commit decisions
- upstream `question_id` is ignored for sentence commit decisions
- translator does not implement a finalize event path

Upstream may split chunks in any way:

- by silence
- by max age
- by buffer size
- by arbitrary ASR refresh cadence

That is acceptable because the translator treats every chunk as just another text observation.

## Chunk Merge Model

When a new ASR chunk arrives, the translator merges it into `buffer`.

### Merge Rule

1. Start matching from `last_pos`.
2. Try to align the new chunk with the tail of `buffer`, using the region after `last_pos` as the active merge region.
3. If a match is found, update `buffer` from the matched position onward using the new chunk contents.
4. If no match is found, append the new chunk to the end of `buffer`.
5. Update `last_pos` to the start position used for this merge.

This matches the user's intended model:

- recent text is allowed to be revised
- stable earlier text is left alone
- no upstream chunk is treated as a hard boundary

### Why No Segment Reset

The translator should not infer a new segment or new utterance merely because a new ASR chunk has little or no overlap with the previous one.

That is important because upstream may force a cut due to max segment size. In that case, the next ASR chunk may be semantically continuous even if it has no textual overlap with the previous one.

Therefore:

- no-overlap does not imply reset
- no-overlap does not imply finalize
- no-overlap means append

## Translation Commit Model

Translation commit is independent from chunk arrival boundaries.

The translator runs a commit loop:

1. If translation is busy, do nothing.
2. If translation is idle, inspect `buffer[committed_pos..]`.
3. Find the next committable span.
4. Translate that span.
5. Advance `committed_pos`.
6. Repeat until no committable span remains.

## Definition Of A Committable Span

A committable span is not just any substring ending in punctuation.

It must satisfy all of the following.

### 1. Starts At `committed_pos`

The translator only commits text in order.

The next candidate always begins at `committed_pos`.

### 2. Ends At A Hard Sentence Boundary

The first version should only consider hard Chinese sentence boundaries:

- `。`
- `！`
- `？`
- `；`

Soft boundaries such as commas are out of scope for the first version.

### 3. Has Meaningful Length

The candidate should not be a tiny fragment or filler-only sentence.

The first version should enforce a simple minimum effective length threshold so fragments like `嗯。` or `这个。` do not get translated as standalone history entries.

The exact threshold will be chosen during implementation and covered by tests.

### 4. Is Stable Enough To Trust

The sentence boundary and the text leading up to it must be stable according to the merged transcript state, not just present in a single noisy ASR update.

In practical terms, the translator should only commit a span when the relevant portion of `buffer` has stopped changing under subsequent chunk merges.

The implementation may realize this through merge-derived stability checks rather than upstream modes.

### 5. Has Passed-Through Context

The boundary should not be committed at the exact moment it first appears if the text still looks likely to extend or revise immediately afterward.

The first version should require enough evidence that the sentence has been passed through, for example by observing stable text beyond the boundary or equivalent merge-based confirmation.

This avoids committing obvious false endings such as ASR-inserted period mistakes in the middle of ongoing speech.

## Non-Goals For This Refactor

The first version deliberately does not solve everything.

Out of scope:

- explicit upstream finalize handling
- upstream question or utterance lifecycle tracking
- buffer trimming policy beyond a future simple max-length cap
- advanced semantic segmentation using commas, discourse markers, or syntax heuristics
- retroactive merge as a primary correction mechanism

Those can be added later if needed, but they are not part of the initial refactor.

## Why This Design Is Better

This model aligns the translator with the actual problem it needs to solve.

The translator should answer:

- what does the current transcript look like
- which prefix is already stable enough to translate
- what still looks like a revisable tail

It should not answer:

- whether upstream VAD considers a chunk complete
- whether a forced max-size cut means semantic completion

By moving the ownership to text evolution instead of upstream event semantics, the translator becomes easier to reason about and less sensitive to VAD chunking artifacts.

## Error Handling

### Merge Failure

If a new chunk cannot be aligned meaningfully after `last_pos`, the translator appends it.

This preserves forward progress and avoids destructive reset behavior.

### Translation Backpressure

If the translator is already busy, incoming chunks only update `buffer`.

The commit loop resumes when the translator becomes idle again.

### Extremely Long Tails

The first version may leave a long uncommitted tail if no hard boundary appears.

That is acceptable for the initial refactor. A future iteration may add a max-buffer-size policy or a fallback long-tail policy.

## Validation Strategy

The new implementation should be validated with targeted tests in three groups.

### Merge Tests

- overlapping chunk updates revise only the tail
- non-overlapping chunks append
- repeated chunk updates do not duplicate text unnecessarily
- `last_pos` limits merge work to the tail region

### Commit Selection Tests

- `find_committable_span()` only returns spans starting at `committed_pos`
- hard punctuation candidates are recognized
- very short fragments are rejected
- unstable tail text is not committed
- once a span is committed, the next search starts after `committed_pos`

### End-To-End Translator Tests

- continuous ASR updates produce a growing buffer
- translator commits stable sentences in order
- upstream `final` does not force a commit by itself
- upstream chunk boundaries do not determine translation boundaries

## Success Criteria

- translator sentence commit no longer depends on upstream `progressive/final` semantics
- translator sentence commit no longer depends on upstream `question_ended`
- a single growing transcript buffer becomes the source of truth
- merge behavior is explicit, testable, and independent of VAD segmentation
- translation commit becomes a text-driven process based on stable, meaningful spans
- the implementation becomes structurally simpler than the current event-driven logic
