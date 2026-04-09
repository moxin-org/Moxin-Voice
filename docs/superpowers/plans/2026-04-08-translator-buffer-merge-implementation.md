# Translator Buffer Merge Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the translator's current event-driven sentence segmentation with a buffer-based text merge model that ignores upstream `mode/question_ended` semantics for commit decisions.

**Architecture:** Extract a focused transcript-buffer core from [`/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/main.rs`](/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/main.rs), then let `main.rs` use that core for chunk merge and span selection while keeping Dora I/O and model invocation in place. The first implementation only commits stable spans ending at hard Chinese sentence boundaries and leaves long-tail/fallback policies for later iterations.

**Tech Stack:** Rust, `cargo test`, Dora node API, existing translator tests in `src/main.rs`

---

## File Structure

- Create: [`/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/transcript_buffer.rs`](/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/transcript_buffer.rs)
  - Own the new `buffer / last_pos / committed_pos` model
  - Implement chunk merge logic
  - Implement committable-span selection logic
  - Hold focused unit tests for merge and span rules
- Modify: [`/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/main.rs`](/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/main.rs)
  - Remove event-driven commit/finalize state machine from the hot path
  - Wire incoming ASR text chunks into `TranscriptBuffer`
  - Translate only spans returned by the new core
  - Keep Dora model loading, prompt generation, and output emission

### Task 1: Create The Buffer Core API Under Test

**Files:**
- Create: [`/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/transcript_buffer.rs`](/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/transcript_buffer.rs)
- Test: [`/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/transcript_buffer.rs`](/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/transcript_buffer.rs)

- [ ] **Step 1: Write failing merge and span-selection tests**

Add tests for:

```rust
#[test]
fn merge_appends_non_overlapping_chunk_after_last_pos() {
    let mut state = TranscriptBuffer::new();
    state.merge_chunk("大家下午好，我叫鲍月。");
    state.merge_chunk("然后来自华为。");
    assert_eq!(state.buffer(), "大家下午好，我叫鲍月。然后来自华为。");
}

#[test]
fn merge_revises_recent_tail_without_touching_old_prefix() {
    let mut state = TranscriptBuffer::new();
    state.merge_chunk("这是我们一八年开。");
    state.merge_chunk("这是我们一八年开源的，呃，面向边缘计算场景。");
    assert_eq!(state.buffer(), "这是我们一八年开源的，呃，面向边缘计算场景。");
}

#[test]
fn last_pos_prevents_matching_against_old_identical_phrase() {
    let mut state = TranscriptBuffer::new();
    state.merge_chunk("我们先讲边缘AI。");
    state.merge_chunk("后面再讲别的内容。");
    state.merge_chunk("我们先讲边缘AI。");
    assert_eq!(state.buffer(), "我们先讲边缘AI。后面再讲别的内容。我们先讲边缘AI。");
}

#[test]
fn find_committable_span_requires_hard_boundary_and_min_length() {
    let mut state = TranscriptBuffer::new();
    state.merge_chunk("嗯。");
    assert!(state.find_committable_span().is_none());
    state.merge_chunk("首先我们可以了解一下为什么我们要去做边缘计算。然后");
    let span = state.find_committable_span().unwrap();
    assert_eq!(span.text, "首先我们可以了解一下为什么我们要去做边缘计算。");
}
```

- [ ] **Step 2: Run targeted tests and verify they fail**

Run:

```bash
cargo test -p dora-qwen35-translator transcript_buffer -- --nocapture
```

Expected:
- compile errors because `TranscriptBuffer` and related APIs do not exist yet

- [ ] **Step 3: Add the minimal `TranscriptBuffer` skeleton**

Create:

```rust
#[derive(Debug, Clone)]
pub struct CommittableSpan {
    pub end_pos: usize,
    pub text: String,
}

#[derive(Debug, Default, Clone)]
pub struct TranscriptBuffer {
    buffer: String,
    last_pos: usize,
    committed_pos: usize,
}

impl TranscriptBuffer {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn buffer(&self) -> &str {
        &self.buffer
    }

    pub fn merge_chunk(&mut self, chunk: &str) {
        todo!()
    }

    pub fn find_committable_span(&self) -> Option<CommittableSpan> {
        todo!()
    }

    pub fn mark_committed(&mut self, end_pos: usize) {
        self.committed_pos = end_pos;
    }
}
```

- [ ] **Step 4: Implement minimal merge and span logic to satisfy tests**

Implement:

```rust
fn find_overlap_start(haystack: &str, needle: &str, search_start: usize) -> Option<usize> {
    haystack[search_start..]
        .find(needle)
        .map(|idx| search_start + idx)
}

fn is_hard_boundary(ch: char) -> bool {
    matches!(ch, '。' | '！' | '？' | '；')
}
```

And use them to:
- prefer matches at or after `last_pos`
- replace the mutable tail when overlap is found
- append when overlap is not found
- search from `committed_pos` for a hard-boundary sentence with minimum effective length

- [ ] **Step 5: Run targeted tests and verify they pass**

Run:

```bash
cargo test -p dora-qwen35-translator transcript_buffer -- --nocapture
```

Expected:
- the new transcript-buffer unit tests pass

### Task 2: Replace The Translator Session Model With Buffer-Driven Commit Selection

**Files:**
- Modify: [`/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/main.rs`](/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/main.rs)
- Modify: [`/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/transcript_buffer.rs`](/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/transcript_buffer.rs)
- Test: [`/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/main.rs`](/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/main.rs)

- [ ] **Step 1: Write failing translator-level tests for mode-independent commit flow**

Add tests such as:

```rust
#[test]
fn final_chunk_does_not_force_commit_without_committable_span() {
    let mut state = TranslatorCoreForTest::new();
    state.on_asr_text("在呃云中心，但是随着我们的边缘设备的增多，比如说呃我们在一九年遇到。");
    assert!(state.take_ready_spans().is_empty());
}

#[test]
fn progressive_and_final_chunks_share_the_same_merge_path() {
    let mut state = TranslatorCoreForTest::new();
    state.on_asr_text("首先我们可以了解一下为什么我们要去做边缘计算。然后");
    let spans = state.take_ready_spans();
    assert_eq!(spans[0], "首先我们可以了解一下为什么我们要去做边缘计算。");
}
```

- [ ] **Step 2: Run the focused test selection and verify failure**

Run:

```bash
cargo test -p dora-qwen35-translator mode_independent -- --nocapture
```

Expected:
- fail because current `main.rs` still drives commits from event-oriented session logic

- [ ] **Step 3: Introduce a small translator-core wrapper around `TranscriptBuffer`**

Add a focused state container in `main.rs` or an extracted helper:

```rust
struct TranslatorBufferState {
    transcript: TranscriptBuffer,
    translation_in_flight: bool,
}
```

And methods like:

```rust
impl TranslatorBufferState {
    fn on_asr_text(&mut self, chunk: &str) {
        self.transcript.merge_chunk(chunk);
    }

    fn next_span_to_translate(&self) -> Option<CommittableSpan> {
        if self.translation_in_flight {
            None
        } else {
            self.transcript.find_committable_span()
        }
    }
}
```

- [ ] **Step 4: Remove upstream-event-driven commit decisions from the main processing path**

Delete or bypass logic centered on:
- `question_ended`-driven finalize decisions
- `commit_sentence!`
- retroactive merge as the main repair path
- special meaning for `progressive` versus `final` in commit selection

Keep:
- text input handling
- model invocation
- output emission helpers

The rule after this step:
- every ASR chunk follows the same merge path
- commit happens only if `TranscriptBuffer` returns a committable span

- [ ] **Step 5: Run focused translator tests and verify they pass**

Run:

```bash
cargo test -p dora-qwen35-translator mode_independent -- --nocapture
```

Expected:
- translator-level tests now pass

### Task 3: Preserve Output Behavior While Advancing `committed_pos`

**Files:**
- Modify: [`/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/main.rs`](/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/main.rs)
- Modify: [`/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/transcript_buffer.rs`](/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/transcript_buffer.rs)

- [ ] **Step 1: Write failing tests for ordered multi-sentence commit**

Add:

```rust
#[test]
fn committed_pos_advances_after_each_translated_sentence() {
    let mut state = TranscriptBuffer::new();
    state.merge_chunk("第一句已经稳定。第二句也已经稳定。然后");
    let first = state.find_committable_span().unwrap();
    assert_eq!(first.text, "第一句已经稳定。");
    state.mark_committed(first.end_pos);
    let second = state.find_committable_span().unwrap();
    assert_eq!(second.text, "第二句也已经稳定。");
}
```

- [ ] **Step 2: Run test and verify it fails**

Run:

```bash
cargo test -p dora-qwen35-translator committed_pos_advances_after_each_translated_sentence -- --nocapture
```

Expected:
- fail because the current span finder or commit accounting does not yet support ordered repeated extraction cleanly

- [ ] **Step 3: Implement committed-prefix advancement**

Ensure:

```rust
pub fn mark_committed(&mut self, end_pos: usize) {
    self.committed_pos = end_pos.min(self.buffer.len());
}
```

And `find_committable_span()` only searches within `buffer[self.committed_pos..]`.

- [ ] **Step 4: Wire translation success to committed advancement in `main.rs`**

After successful translation emission:

```rust
if let Some(span) = state.next_span_to_translate() {
    let source = span.text.clone();
    let translated = run_translation(...)?;
    emit_outputs(...)?;
    state.transcript.mark_committed(span.end_pos);
}
```

- [ ] **Step 5: Run focused ordered-commit tests**

Run:

```bash
cargo test -p dora-qwen35-translator committed_pos_advances_after_each_translated_sentence -- --nocapture
```

Expected:
- ordered commit progression works

### Task 4: Run Full Verification And Check In Progress

**Files:**
- Modify: [`/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/main.rs`](/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/main.rs)
- Modify: [`/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/transcript_buffer.rs`](/Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor/node-hub/dora-qwen35-translator/src/transcript_buffer.rs)

- [ ] **Step 1: Run the crate test suite**

Run:

```bash
cargo test -p dora-qwen35-translator -- --nocapture
```

Expected:
- all translator tests pass

- [ ] **Step 2: Run a clean build**

Run:

```bash
cargo build -p dora-qwen35-translator
```

Expected:
- build succeeds

- [ ] **Step 3: Check git diff for scope discipline**

Run:

```bash
git -C /Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor status --short
git -C /Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor diff -- node-hub/dora-qwen35-translator/src/main.rs node-hub/dora-qwen35-translator/src/transcript_buffer.rs
```

Expected:
- only the planned translator files and plan/spec docs changed

- [ ] **Step 4: Commit the implementation checkpoint**

Run:

```bash
git -C /Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor add \
  node-hub/dora-qwen35-translator/src/main.rs \
  node-hub/dora-qwen35-translator/src/transcript_buffer.rs \
  docs/superpowers/plans/2026-04-08-translator-buffer-merge-implementation.md
git -C /Users/alan0x/Documents/projects/moxin-tts/.worktrees/translator-buffer-refactor commit -m "refactor: add translator buffer merge core"
```

Expected:
- implementation checkpoint committed on `codex/translator-buffer-refactor`
