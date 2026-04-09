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

The point of `last_pos` is to prevent a new chunk from matching against an older identical phrase much earlier in the transcript.

That matters because a speaker may repeat the same words after some time. Without `last_pos`, the merge logic might incorrectly attach the new chunk to an earlier occurrence of the same phrase instead of the recent tail that is actually being updated.

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

## 中文版本

### 目标

将当前实时翻译中“事件驱动”的断句逻辑替换为“文本驱动”的模型。

新的 translator 应将上游 ASR/VAD 视为纯文本提供者：

- 它接收一串连续到来的 ASR 文本 chunk
- 它不依赖 `transcription_mode`
- 它不依赖 `question_id`
- 它不把 `final` 或 `question_ended` 当作句子边界

translator 改为维护一条持续增长的 transcript buffer，把新的 ASR chunk merge 进这条 buffer，然后只在能够识别出“稳定且有意义”的文本 span 时，才提交翻译。

### 当前问题

当前 translator 仍然继承了过多来自上游 ASR/VAD 事件的边界语义。

这会导致两类反复出现的问题：

- 文本过早被切断，因为上游发出了 `final` chunk，或 `question_ended` 的延迟收尾路径强行触发 finalize
- 文本过晚被切断，因为 translator 在等待上游事件边界，而不是根据不断演化的文本本身来判断

这是一个结构性错位：

- 上游分段是音频驱动的
- 下游翻译质量依赖的是文本稳定性和语义完整性

这两者不是同一种边界。

### 设计方向

在 translator 内部使用单一文本 buffer 作为唯一事实来源。

translator 不再围绕下面这些概念思考：

- ASR burst
- progressive 与 final 作为提交信号
- 上游 utterance 边界

translator 改为围绕下面这些概念思考：

- 一条持续演化的 transcript buffer
- 用于合并新 ASR 观察值的 merge anchor
- 表示已翻译前缀位置的 committed prefix
- 可以安全翻译的稳定 span

### 核心状态

新的 translator 核心维护三份主要状态。

#### `buffer`

一条持续增长的字符串，表示当前实时发言的最佳 transcript 视图。

这条 buffer 不按上游 chunk 或 utterance 划分，而是 translator 自己对当前发言文本的统一视图。

#### `last_pos`

上一次 merge 的锚点。

它表示在 `buffer` 中的大致位置，下一次新 ASR chunk 到来时，应从这个位置之后开始匹配和合并。

`last_pos` 的目的，是避免每次都全量扫描整个 transcript，只把 merge 的工作限制在最近仍可能发生变化的尾部区域。
`last_pos` 的目的，是避免新的 chunk 去错误匹配到 transcript 中更早出现过、但其实已经属于历史内容的相同文本。

这是必要的，因为说话人完全可能隔一段时间后重复一句相同的话。如果没有 `last_pos` 作为约束，merge 逻辑就可能把新的 chunk 错误地合并到更早以前的同一句话上，而不是合并到最近正在演化的尾部文本。

#### `committed_pos`

翻译提交锚点。

它表示 `buffer` 中已经完成翻译提交、已经进入 UI/history 的位置。

当 translator 空闲时，它从 `committed_pos` 开始寻找下一个可提交的 span。

### 上游契约

translator 只消费 ASR chunk 中的文本内容。

在这次重构中：

- 上游 chunk 边界最多只是参考信息
- 上游 `progressive/final` metadata 不参与句子提交决策
- 上游 `question_id` 不参与句子提交决策
- translator 不实现 finalize 事件路径

上游可以用任何方式切 chunk：

- 静音切分
- max age 强制切分
- buffer 大小上限切分
- 任意 ASR 刷新节奏

这些都可以接受，因为 translator 会把每个 chunk 都视为“当前 transcript 的一次新观察值”。

### Chunk Merge 模型

每当一个新的 ASR chunk 到来时，translator 将它 merge 进 `buffer`。

#### Merge 规则

1. 从 `last_pos` 开始匹配。
2. 尝试在 `buffer` 的尾部与新的 chunk 对齐，并把 `last_pos` 之后的区域作为活跃 merge 区域。
3. 如果找到匹配，则从匹配位置开始，用新 chunk 的内容更新 `buffer` 的尾部。
4. 如果找不到匹配，则直接把新 chunk append 到 `buffer` 末尾。
5. 将 `last_pos` 更新为这次 merge 所采用的起始位置。

这和当前确认的设计一致：

- 最近的文本允许被修正
- 较早、较稳定的文本保持不变
- 任何上游 chunk 都不被当作硬边界

#### 为什么不做 segment reset

translator 不应仅仅因为一个新 ASR chunk 与上一拍文本重叠很少或没有重叠，就推断“这是一个新的 segment”或“这是一个新的 utterance”。

这是因为上游可能由于 max segment size 被强制切断。在这种情况下，下一块 ASR chunk 即使和上一块没有文本重叠，语义上也可能仍然属于同一段连续发言。

因此：

- 无重叠不代表 reset
- 无重叠不代表 finalize
- 无重叠时，直接 append

### 翻译提交模型

翻译提交与 chunk 到来的边界解耦。

translator 运行一条提交循环：

1. 如果翻译仍在忙，则不做任何事。
2. 如果翻译空闲，则检查 `buffer[committed_pos..]`。
3. 寻找下一个可提交的 span。
4. 翻译这个 span。
5. 推进 `committed_pos`。
6. 重复上述过程，直到找不到可提交 span。

### 可提交 Span 的定义

可提交 span 不是“任意一个以标点结尾的子串”。

它必须同时满足下面所有条件。

#### 1. 从 `committed_pos` 开始

translator 只按顺序提交文本。

下一个候选 span 必须从 `committed_pos` 开始。

#### 2. 以硬句子边界结束

第一版只考虑中文硬终止符：

- `。`
- `！`
- `？`
- `；`

像逗号这类软边界，不在第一版范围内。

#### 3. 具有足够语义长度

候选 span 不应只是很短的小碎片，或只有语气词的句子。

第一版应设置一个简单的最小有效长度门槛，避免像 `嗯。`、`这个。` 这样的短片段被单独翻译并进入历史。

具体阈值在实现阶段确定，并由测试覆盖。

#### 4. 足够稳定，可以信任

句子终点以及终点之前的文本，必须在 merged transcript 状态下已经足够稳定，而不是仅仅在某一次噪声较大的 ASR 更新里出现过。

更实际地说，translator 只应在相关文本区间在后续 chunk merge 中不再继续变化时，才提交这个 span。

实现上可以通过 merge 导出的稳定性判断来完成，而不是依赖上游 mode。

#### 5. 具备“已经越过该句”的上下文

边界不应在首次出现的瞬间就立刻被提交，因为文本很可能马上继续延伸或被修订。

第一版应要求足够证据表明说话已经越过该句，比如句号之后已经出现了一段稳定尾巴，或有等价的 merge 级确认信号。

这样可以避免把 ASR 中途误插入的句号直接当作真实句尾。

### 本次重构的非目标

第一版不会试图一次解决所有问题。

本次不包含：

- 显式的上游 finalize 处理路径
- 上游 question 或 utterance 生命周期跟踪
- 超出未来简单长度上限之外的 buffer trimming 策略
- 使用逗号、话语标记或句法启发式进行高级语义切分
- 将 retroactive merge 作为主修正机制

这些都可以在后续版本中再考虑，但不属于这次首轮重构的范围。

### 为什么这个设计更好

这套模型更贴近 translator 真正要解决的问题。

translator 应该回答的是：

- 当前 transcript 长什么样
- 哪一段前缀已经稳定到可以翻译
- 哪一段尾巴仍然可能继续变化

translator 不应该回答的是：

- 上游 VAD 是否认为这块 chunk 已经结束
- 一次 max-size 强切是否意味着语义完成

当所有权从“上游事件语义”转到“文本自身的演化过程”后，translator 会更容易理解、更容易测试，也会更少受到 VAD chunking 伪边界的干扰。

### 错误处理

#### Merge 失败

如果一个新的 chunk 无法在 `last_pos` 之后与 `buffer` 合理对齐，则直接 append。

这样可以保证系统持续向前推进，并避免破坏性的 reset 行为。

#### 翻译背压

如果 translator 当前正忙，新的 chunk 只更新 `buffer`。

等 translator 空闲后，再恢复提交循环。

#### 极长未提交尾巴

如果长时间没有出现硬句子边界，第一版可能会保留一段较长的未提交尾巴。

这是可以接受的。后续版本可以再加入 max-buffer-size 策略，或长尾兜底策略。

### 验证策略

新的实现应至少覆盖三组测试。

#### Merge 测试

- 有重叠的 chunk 更新只修正尾部
- 无重叠的 chunk 会 append
- 重复 chunk 更新不会不必要地制造重复文本
- `last_pos` 只限制 merge 在尾部区域内发生

#### Commit Selection 测试

- `find_committable_span()` 只返回从 `committed_pos` 开始的 span
- 能正确识别硬标点候选
- 很短的小碎片会被拒绝
- 不稳定尾巴不会被提交
- 一旦某个 span 已提交，下一次搜索会从新的 `committed_pos` 开始

#### Translator 端到端测试

- 连续 ASR 更新会生成一条持续增长的 buffer
- translator 会按顺序提交稳定句子
- 上游 `final` 本身不会强制触发提交
- 上游 chunk 边界不会决定翻译边界

### 成功标准

- translator 的句子提交不再依赖上游 `progressive/final` 语义
- translator 的句子提交不再依赖上游 `question_ended`
- 单一持续增长的 transcript buffer 成为唯一事实来源
- merge 行为明确、可测试，并且独立于 VAD 分段
- 翻译提交变成一个基于“稳定且有意义的 span”的文本驱动过程
- 这套实现的结构比当前事件驱动逻辑更简单、更清晰

## Success Criteria

- translator sentence commit no longer depends on upstream `progressive/final` semantics
- translator sentence commit no longer depends on upstream `question_ended`
- a single growing transcript buffer becomes the source of truth
- merge behavior is explicit, testable, and independent of VAD segmentation
- translation commit becomes a text-driven process based on stable, meaningful spans
- the implementation becomes structurally simpler than the current event-driven logic
