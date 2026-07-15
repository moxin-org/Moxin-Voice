# Release 前 x-vector 统一与播放器变速不变调加固计划

**目标：** 在下一个 release 前，将产品中的零样本音色克隆统一到 x-vector 路径，并正式支持数千字、最长至少 60 分钟的 TTS 音频；播放器在保留“变速不变调”目标的前提下采用分块 source、后台 time-stretch、分块缓存和过期任务取消，消除 ICL 不完整结果漏检、长音频内存放大和切换倍速卡顿风险。

**总体架构：** `VOICE:CUSTOM` 协议暂时保持兼容，但 Qwen 节点不再根据 `prompt_text` 选择推理模式，所有自定义及捆绑克隆音色都进入 x-vector。旧配置中的参考文本继续允许读取，但不再参与生成。长音频不再合并并复制成多个完整 `Vec<f32>`：以已有 TTS segment 为 canonical source，通过统一的 `PlaybackAudioSource` 按时间读取 5–10 秒 block；播放器只维持有限的播放队列。pitch-preserving time-stretch 在专用 worker 中按 block 执行，以 `(audio_revision, playback_rate, block_index)` 为 key 做有界 LRU 缓存，并用 compute task id 和 playback intent id 取消或拒绝旧的 seek/切速结果。生成参数中的“语速”仍由后端处理，和播放器临时倍速保持独立。当前穷举式算法可以先作为 block engine 使用，随后在不改变 source、worker、cache 和 UI 协议的前提下替换为成熟实时实现。

**已接受的取舍：**

- x-vector 的细粒度音色、语气和韵律还原可能弱于理想状态下的 ICL，但当前实现更稳定，且能够使用现有的提前 EOS 检测与自动重试。
- 保留“播放器变速不变调”的正式产品目标，不接受简单实时重采样造成的变调作为 release 方案。
- 本次 release 允许暂时保留当前 time-stretch 数学实现，但它只能处理有界 block，必须在后台执行、可取消、结果可复用，且不能再分配完整变速音频；算法替换或重构紧随这一最小安全修复进行。
- 60 分钟音频是 release 容量合同：允许 canonical PCM 数据本身随时长线性增长，但禁止额外的完整 merged、processed、stretched 和 player-command 副本。
- 保留 `qwen3-tts-mlx` 底层 ICL API 和模型加载能力，当前计划只关闭产品入口，避免本次 release 扩大删除范围。

**不在本计划范围内：**

- 删除底层 `synthesize_voice_clone_icl*`、Mimi encoder 或 ICL prompt 实现。
- 修改 `VOICE:CUSTOM|ref_wav|prompt_text|language|text` 的线协议字段数量。
- 改变生成阶段的 speed、pitch、volume 处理语义。
- 把“选择并接入新的第三方 DSP 引擎”设为本次 release 的硬门槛；release 先以分块 source、worker、block cache 和取消机制关闭长音频下的 UI 卡顿与内存失控风险，随后在同一抽象后替换算法。

---

## Task 1：在 Qwen 节点中强制所有克隆请求使用 x-vector

**文件：**

- 修改：`node-hub/dora-qwen3-tts-mlx/src/main.rs`
- 验证：`node-hub/dora-qwen3-tts-mlx/patches/qwen3-tts-mlx/src/generate.rs`
- 验证：`node-hub/dora-qwen3-tts-mlx/patches/qwen3-tts-mlx/src/lib.rs`

- [x] **Step 1：记录变更前基线**

运行：

```bash
cargo test -p dora-qwen3-tts-mlx
cargo test -p qwen3-tts-mlx clone_result_is_incomplete_when_eos_precedes_remaining_text -- --exact
```

预期：现有测试通过；若失败，先记录为基线问题，不把无关修复混入本计划。

- [x] **Step 2：简化 `TtsRequest::Custom` 分支**

在 `synthesize_qwen` 中：

1. 将 `prompt_text` 解构为忽略字段，例如 `prompt_text: _`。
2. 删除 `let use_xvector = prompt_text.trim().is_empty()`。
3. 固定使用 `prepare_reference_audio_for_clone(state, &ref_wav, true)`，确保缓存 key 和参考音频上限使用 x-vector 配置。
4. 删除 `synthesize_voice_clone_icl_with_timing_cached` 调用以及“ICL hard failure 后降级”的分支。
5. `synthesize_once` 始终调用 `synthesize_voice_clone_with_timing_cached`。
6. 保留已有的 `timing.incomplete_clone` 判断、不同 seed 的一次重试和第二次失败后的明确错误。

目标结构：

```rust
let (ref_audio_24k, speaker_cache_key) =
    prepare_reference_audio_for_clone(state, &ref_wav, true)?;

let mut synthesize_once = |seed| -> Result<_> {
    let options = SynthesizeOptions {
        language: &lang,
        max_new_tokens,
        seed,
        ..Default::default()
    };

    Ok(synth.synthesize_voice_clone_with_timing_cached(
        &text,
        &ref_audio_24k,
        &lang,
        &speaker_cache_key,
        &options,
    )?)
};
```

- [x] **Step 3：更新日志语义**

将 clone 配置日志明确标记为 `mode=x-vector`。删除任何可能让运维人员误以为运行时还会尝试 ICL 的日志。

- [x] **Step 4：验证产品节点已没有 ICL 调用**

运行：

```bash
rg -n "synthesize_voice_clone_icl" node-hub/dora-qwen3-tts-mlx/src/main.rs
cargo test -p dora-qwen3-tts-mlx
cargo check -p dora-qwen3-tts-mlx
```

预期：`rg` 无匹配；测试和检查通过。底层 patch crate 中仍可保留 ICL 符号。

- [x] **Step 5：确认 x-vector 不完整重试仍然有效**

保留并运行以下已有测试：

```bash
cargo test -p dora-qwen3-tts-mlx retry_seed_is_stable_and_changes_between_attempts -- --exact
cargo test -p dora-qwen3-tts-mlx segment_result_reports_second_incomplete_attempt_as_failure -- --exact
cargo test -p qwen3-tts-mlx clone_result_is_incomplete_when_eos_precedes_remaining_text -- --exact
```

预期：全部通过。

---

## Task 2：让音色创建和请求构造符合“仅 reference audio”语义

**文件：**

- 修改：`apps/moxin-voice/src/voice_clone_modal.rs`
- 修改：`apps/moxin-voice/src/voice_data.rs`
- 修改：`apps/moxin-voice/src/screen.rs`
- 测试：上述文件中的 `#[cfg(test)]` 模块

- [x] **Step 1：先补充请求构造回归测试**

为自定义音色请求构造增加测试，至少覆盖：

1. `VoiceSource::Custom` 只有 `reference_audio_path`、`prompt_text=None` 时仍生成 `VOICE:CUSTOM`，不能退回 `VOICE:Doubao`。
2. 旧配置带有非空 `prompt_text` 时，请求中的 prompt 字段仍被规范化为空字符串。
3. 捆绑克隆音色继续生成 prompt 字段为空的 `VOICE:CUSTOM`。

为了避免测试依赖整个 `TTSScreen`，将“根据 `Voice` 构造 clone payload”的纯字符串逻辑提取为小型 helper；文件路径解析仍留在现有方法中。

运行聚焦测试，确认修改实现前至少前两项失败。

- [x] **Step 2：自定义音色请求不再依赖 `prompt_text`**

在 `build_tts_prompt_for_segment` 中：

- `VoiceSource::Custom` 只要求 `reference_audio_path` 存在。
- 无论旧配置是否保存了参考文本，都发送空 prompt：

```text
VOICE:CUSTOM|<absolute-ref-path>||<language>|<text>
```

- 缺少参考音频时才进入现有错误/回退路径。

这样 UI 和后端同时表达 x-vector-only，且旧客户端即使仍发送非空 prompt，Task 1 的后端也会忽略它。

- [x] **Step 3：简化新音色的数据模型入口**

修改 `Voice::new_custom`：

- 移除 `prompt_text: String` 参数。
- 新音色保存 `prompt_text: None`。
- 保留 `Voice.prompt_text: Option<String>` 字段，以便反序列化旧版 `custom_voices.json`，本 release 不做破坏性数据迁移。

增加序列化兼容测试：旧 JSON 中包含 `prompt_text` 的音色仍能加载。

- [x] **Step 4：移除 Qwen Express 创建流程对参考文本的要求**

在 Qwen zero-shot/Express 路径中：

- 隐藏或移除 “Reference Text” 输入区域。
- 删除保存时的“参考文本不能为空”校验。
- 调用新的 `Voice::new_custom` 签名。
- 录音完成后不再为了填充参考文本而触发 ASR；直接进入音频验证/可保存状态。
- 只删除 Express 克隆专用的转录状态和处理，不能破坏其他仍使用 ASR 的功能。

若 dormant PrimeSpeech/Pro 分支仍需要参考文本，保留其 UI 和校验，但条件必须与 Qwen Express 分开，避免未来恢复时丢失能力。

- [x] **Step 5：让 UI 参考音频限制与 x-vector 后端一致**

Qwen x-vector 后端默认使用最多 6 秒参考音频。将 Qwen Express 的上传和录音提示/校验统一为 3–6 秒，替换当前为 ICL 设置的 3–8 秒说明，避免 UI 接受 8 秒后后端静默裁到 6 秒。

不要修改 PrimeSpeech 的独立限制。

- [x] **Step 6：清理误导性的 Bundled ICL 命名**

将产品数据层的 `VoiceSource::BundledIcl` 重命名为 `VoiceSource::BundledClone`，并使用 serde alias 兼容旧值：

```rust
#[serde(alias = "BundledIcl")]
BundledClone,
```

同步更新：

- `baiyang`、`yangyang` 定义和注释；
- `screen.rs` 中的匹配分支；
- `resolve_bundled_icl_ref_path` 等仅体现旧推理模式的内部命名；
- prompt 文本元数据设为 `None`，不再保存未使用的大段 transcript。

增加 serde 测试，证明旧的 `"BundledIcl"` 仍能反序列化为新 variant。

- [x] **Step 7：运行音色相关测试**

运行：

```bash
cargo test -p moxin-voice voice_data -- --nocapture
cargo test -p moxin-voice voice_persistence -- --nocapture
cargo test -p moxin-voice custom_clone -- --nocapture
cargo check -p moxin-voice
```

预期：新旧自定义音色都走 `VOICE:CUSTOM|...||...`；没有数据迁移错误。

验证记录：新增的 4 个 `voice_data` 回归测试和 `cargo check -p moxin-voice` 通过；`custom_clone` 过滤集无测试。既有 `voice_persistence::tests::test_voice_id_with_chinese` 仍失败，其断言期待中文被替换为下划线，但当前 `generate_voice_id` 会保留 Unicode 字母；该失败与本任务改动无关，留待最终 release gate 统一处理。

---

## Task 3：建立 60 分钟级分块音频 source 和流式播放器

**文件：**

- 新增：`apps/moxin-voice/src/playback_audio_source.rs`
- 修改：`apps/moxin-voice/src/lib.rs`
- 修改：`apps/moxin-voice/src/tts_segments.rs`
- 修改：`apps/moxin-voice/src/audio_player.rs`
- 修改：`apps/moxin-voice/src/screen.rs`
- 按需修改：`apps/moxin-voice/src/tts_history.rs`、下载/分享音频写出路径

- [x] **Step 1：先固定长音频容量合同**

增加不依赖真实模型的 30/60 分钟测试 fixture，约束：

- 24 kHz mono f32；
- canonical PCM 只保留一份；
- 播放、seek、retry 和导出不得调用 `merged_samples()` 创建完整副本；
- 播放器任何一次 command 不携带完整 30/60 分钟 `Vec<f32>`；
- 除 canonical PCM 外，播放器、time-stretch working set 和 cache 的合计预算默认不超过 128 MiB；
- 60 分钟 source 在 0.75x 下不得分配完整约 460.8 MB 的 stretched buffer。

这些测试先暴露当前 `segments.merged_samples()`、`processed_audio_samples` 和 `write_audio(samples.to_vec())` 的放大行为。

- [x] **Step 2：让 segment audio 成为 canonical source**

将 `TtsAudioSegment.samples` 改为可共享、不可变的所有权形式（例如 `Arc<Vec<f32>>`），并为 `TtsAudioSegments` 增加：

- `total_samples()` / `duration_secs()`；
- 预计算的 segment start offsets，避免每次 seek 都线性累加；
- 按全局 sample range 读取最多一个 block 的 API；
- 跨 segment boundary 的 block 读取；
- retry 仅替换目标 segment，并重建 offsets/revision。

播放热路径不得再调用 `merged_samples()`。如测试仍需该 helper，将其限制为测试/小数据用途；生产调用必须清零。

- [x] **Step 3：引入统一的 `PlaybackAudioSource`**

`PlaybackAudioSource` 统一封装：

- 当前生成的 segment-backed source；
- 从历史记录加载的 contiguous source；
- sample rate、总 samples、revision；
- `read_block(start_sample, max_samples)`；
- `block_index_at_time()` 和 block 时间范围。

历史音频可由单一 `Arc<Vec<f32>>` 持有，不再复制到 `stored_audio_samples` 和 `processed_audio_samples`。新生成音频直接引用 segment arcs。

从 `TTSScreen` 删除或淘汰以下完整副本状态：

- `stored_audio_samples`；
- `processed_audio_samples`；
- `rebuild_processed_audio_samples()` 的同采样率 clone 路径。

若遇到非 24 kHz 历史音频，只对当前读取 block 做有状态 resample，不能先重采样整条 60 分钟音频。

- [x] **Step 4：播放器改为有界 block queue**

扩展 `TTSPlayer`：

- 增加 `write_audio_owned(Vec<f32>)`，将 block 所有权发送给音频线程，避免 sender 侧 `samples.to_vec()`；
- 增加 queued/available sample 数或秒数的可读状态；
- 使用固定 20–30 秒 high-water mark 和 5–10 秒 low-water mark；
- feeder 仅在低水位时加入后续 block；
- seek/reset 清空 queue 并从新 block index 重新填充；
- 禁止 `ensure_capacity` 因一次超长 write 扩张到整段音频大小；
- block queue 满时应用 backpressure，不覆盖尚未播放的 samples。

底层 CPAL callback 保持无锁或短锁、无大块分配；UI timer 只驱动轻量 refill，不复制完整音频。

- [x] **Step 5：下载、分享和历史保存改为流式读取**

所有需要完整音频的消费者通过 `PlaybackAudioSource` 顺序读取 block：

- WAV/其他支持的编码器按 block 写出；
- 禁止导出前 `collect::<Vec<f32>>()`；
- 历史元数据使用 `total_samples`/duration，不通过完整 buffer 推导；
- share 如必须创建临时文件，也应流式生成该文件。

- [x] **Step 6：验证 source revision 和 retry**

任何 segment retry、历史加载、新生成或 clear 都创建新的 `audio_revision`。旧 source block、player queue 和后续 stretch cache 都不能跨 revision 复用。

增加竞态测试：旧 source 的 block 在 retry 后晚到，必须因 revision 不匹配被丢弃。

- [x] **Step 7：运行分块 source/player 测试**

```bash
cargo test -p moxin-voice playback_audio_source -- --nocapture
cargo test -p moxin-voice tts_segments::tests -- --nocapture
cargo test -p moxin-voice audio_player::tests -- --nocapture
cargo check -p moxin-voice
```

预期：60 分钟 fixture 不产生完整 merged/processed/player-command 副本，seek 只读取目标 block 附近数据。

验证记录：60 分钟虚拟 source、跨 segment block、全局坐标重采样、revision 隔离、固定播放器容量和流式 WAV 导出测试通过，`cargo check -p moxin-voice` 通过。主界面已没有 `merged_samples()`、`stored_audio_samples` 或 `processed_audio_samples` 生产调用。非 1x block 的同步 DSP 是 Task 3 到 Task 4 之间的临时状态，Task 4 必须在 release 前将其移到可取消 worker。

---

## Task 4：按 block 执行 pitch-preserving time-stretch

**文件：**

- 新增：`apps/moxin-voice/src/playback_time_stretch.rs`
- 修改：`apps/moxin-voice/src/lib.rs`
- 修改：`apps/moxin-voice/src/screen.rs`
- 使用：`apps/moxin-voice/src/playback_audio_source.rs`
- 测试：新模块和 `screen.rs` 中的 `#[cfg(test)]` 模块

- [x] **Step 1：定义 block 和边界上下文合同**

默认使用 10 秒 source block（允许在 benchmark 后调整到 5–10 秒），每个 block 额外读取 100–250 ms 左右上下文。engine 输出后裁掉上下文，并在相邻 block 之间使用连续 engine state 或短 crossfade，避免独立处理造成爆音、断裂和重复音节。

核心 key：

```rust
#[derive(Clone, Copy, Debug, Eq, Hash, PartialEq)]
struct StretchBlockKey {
    audio_revision: u64,
    rate_milli: u16,
    block_index: u64,
}
```

倍率使用规范化整数（0.75x → 750），不能用 `f64` 直接作为 key。

- [x] **Step 2：抽离现有算法为可取消的 block engine**

将 Hann window、overlap 搜索和主循环移出 `TTSScreen`。入口只处理有界 block，并接受取消检查：

```rust
fn stretch_block_preserve_pitch(
    input_with_context: &[f32],
    playback_rate: f64,
    trim: BlockTrim,
    is_cancelled: impl Fn() -> bool,
) -> Result<Vec<f32>, StretchCancelled>;
```

在 synthesis-hop 和候选搜索内定期检查取消；任何调用都不得接受完整 30/60 分钟 source。

- [x] **Step 3：实现单一 worker 和 latest-request-wins**

`PlaybackStretchWorker` 使用一个专用线程：

- request 带 compute `task_id`、`StretchBlockKey` 和 block 数据；
- 新 seek/切速/音频 revision 更新 `latest_task_id`；
- 当前任务在取消检查点退出；
- 开始下一项前 drain 队列，优先处理当前播放 block，再处理预取 block；
- result 带 task id、key 和精确媒体时间范围；
- `Drop` 时 stop 并安全 join。

禁止每个 block 或每次切速创建独立线程。

- [x] **Step 4：实现 64 MiB 有界 block LRU cache**

缓存 key 为 `(audio_revision, rate_milli, block_index)`：

- 默认 byte budget 64 MiB，而不是缓存完整倍率版本；
- 1x 直接读取 source，不进入 stretch cache；
- 优先保留当前 block、后续 2–3 个 block 和最近 seek 区域；
- 超预算按 LRU 淘汰，不能因为 60 分钟总时长增长；
- revision 变化立即使旧 cache 不可见；
- cache hit 不重新计算。

增加 cache hit、revision 隔离、LRU、byte budget 以及“单个 block 也不能超过预算”的测试。

- [x] **Step 5：区分 requested rate、active rate 和 playback intent**

`TTSScreen` 增加：

- `player_requested_rate`；
- `player_active_rate`；
- `player_stretch_pending`；
- `playback_intent_id`；
- 当前 source revision 和 block cursor。

行为：

1. 1x 立即从 source block 播放。
2. 非 1x cache hit 时立即 queue 对应 block。
3. cache miss 时后台计算当前 block；旧 active rate 可继续播放或短暂停留，但 UI 不能阻塞。
4. 当前 block ready 后开始播放，并预取后续 2–3 块。
5. seek 更新 intent、清空 player queue、取消旧优先任务并请求目标 block。
6. 旧 compute result 若 key 仍有效可作为 cache fill，但旧 intent 永远不能改变播放位置、active rate 或 queue。
7. pause 时结果只入 cache，不自动 resume。

- [x] **Step 6：正确维护原始媒体时间**

时间轴始终表示 source 媒体时间：

- 进度 tick 使用 `player_active_rate`；
- 每个 stretched block 携带对应 source start/end time；
- seek 根据 source time 选择 block，而不是在完整 stretched buffer 上换算 index；
- block 切换按 metadata 推进，累计误差不能随 60 分钟时长漂移；
- worker 完成后使用最新 intent 的当前位置，不能回到提交任务时的位置。

底层 `TTSPlayer` 对已经 stretch 的 block 保持 1x 输出，避免二次变调。

- [x] **Step 7：迁移并增强音质/取消测试**

至少覆盖：

- 0.75x、1.25x、1.5x、2x 的 block 输出时长；
- 正弦主周期基本不变；
- 相邻 block boundary 的 RMS/相位连续性；
- 取消延迟；
- 快速连续切速只应用最后 intent；
- 60 分钟虚拟 source 的随机 seek 不分配完整 stretched buffer；
- cache/prefetch 不发生 player underrun。

- [x] **Step 8：写入明确性能门槛**

在 release 参考 Apple Silicon、`--release` 构建下：

- 10 秒、0.75x block 的 p95 处理时间 ≤ 300 ms；
- cache hit 到 queue ready ≤ 50 ms；
- uncached 首播/seek 到首块 ready 的 p95 ≤ 500 ms，目标 100–300 ms；
- 旧任务取消/失效可观察延迟 ≤ 100 ms，目标 50 ms；
- 预取至少维持 20 秒可播放数据且不 underrun；
- UI event handler 不执行超过一个 block copy，不执行 DSP 主循环；
- 除 canonical source 外，player queue + working buffers + stretch cache 默认 ≤ 128 MiB，其中 cache 默认 64 MiB。

- [x] **Step 9：运行 time-stretch/player 测试**

```bash
cargo test -p moxin-voice playback_time_stretch -- --nocapture
cargo test -p moxin-voice playback_audio_source -- --nocapture
cargo test -p moxin-voice player_playback -- --nocapture
cargo test -p moxin-voice player_bar_controls_are_wired_to_actions -- --exact
cargo check -p moxin-voice
```

预期：UI 不执行 DSP；60 分钟 source 仍只处理和缓存播放位置附近 block；变速保持音高目标。

验证记录：10 秒 source block、200 ms 双侧上下文、20 ms 等长边界平滑、单 worker、latest-task-wins、64 MiB LRU、revision/rate/block key、1x 直通、active-rate 媒体时间和三块预取均已接入。worker 自行从共享 source 读取 block，UI 不执行 DSP 或预取音频复制。0.75x/1.25x/1.5x/2x 时长、正弦周期、相邻边界 RMS/相位、取消、旧 task、cache 和 source 坐标测试通过；上述聚焦测试与 `cargo check -p moxin-voice` 通过。

---

## Task 5：在安全异步架构后替换或重构穷举式算法

**阶段说明：** Task 3–4 是本次 release 的最小安全修复和 release blocker。本任务紧随其后执行，但不要求为了赶 release 仓促引入未经验证的新 DSP 依赖。分块 source、worker、cache 和 intent API 必须让算法可以独立替换。

**文件：**

- 修改：`apps/moxin-voice/src/playback_time_stretch.rs`
- 按选择修改：`apps/moxin-voice/Cargo.toml`、`Cargo.lock`
- 新增：time-stretch benchmark 或独立测试工具

- [x] **Step 1：定义可替换的 engine 边界**

将 worker 与算法通过小型 trait 或等价接口隔离：

```rust
trait TimeStretchEngine: Send {
    fn process_block(
        &mut self,
        input_with_context: &[f32],
        rate: f64,
        trim: BlockTrim,
        is_cancelled: &dyn Fn() -> bool,
    ) -> Result<Vec<f32>, StretchError>;
}
```

Task 4 迁移出的实现作为 `LegacyWsolaEngine`，不得让 UI、source 或 cache 依赖其内部细节。

- [x] **Step 2：建立可重复 benchmark 和质量样本**

至少覆盖：

- 5 秒和 10 秒 block 微基准；
- 30 分钟和 60 分钟顺序播放、随机 seek 的系统基准；
- 24 kHz mono；
- 0.75x、1.25x、1.5x、2x；
- 总耗时、峰值内存、取消延迟；
- 输出时长误差、RMS 异常、明显静音尾巴；
- 人工听测音高、断裂、金属音和重复音节。

明确目标：后台处理不能掩盖无限慢算法；block 必须持续快于播放速度，30/60 分钟测试的内存不能随 stretched 总时长增长，取消延迟应控制在用户无感范围。

- [x] **Step 3：评估成熟实时 time-stretch 实现**

候选必须满足：

- macOS/Apple Silicon 可稳定构建；
- 许可证与发布方式兼容；
- 支持 mono f32、24 kHz 和目标倍率；
- 支持分块输入、flush 和状态重置；
- 能在处理循环中响应取消，或允许安全丢弃实例；
- 音质和性能通过 Step 2 基线。

只有满足这些门槛才引入依赖；不能仅根据 crate 名称或简单正弦测试决定。

- [x] **Step 4：替换实现或重构现有算法**

优先顺序：

1. 接入通过评估的成熟分块引擎；
2. 若没有合适依赖，将当前逐候选全量相关搜索重构为有界、分块、可增量输出的实现；
3. 保持 Task 3–4 的分块 source、worker、block cache key、取消和 UI 协议不变。

- [x] **Step 5：使用同一测试矩阵做替换验收**

新 engine 必须通过 Task 4 的功能测试和 Step 2 benchmark。若音质或性能退化，继续使用分块异步化后的 legacy engine，不把未达标替换带入 release。

验证记录：`TimeStretchEngine`/`LegacyWsolaEngine` 边界已建立，可复现 benchmark 位于 `apps/moxin-voice/examples/time_stretch_benchmark.rs`，实测和容量解释记录在 `docs/superpowers/plans/2026-07-15-time-stretch-benchmark-results.md`。Apple M4 release 构建下，10 秒 block 的 p95 为 0.75x 87.096 ms、1.25x 52.734 ms、1.5x 43.881 ms、2x 32.529 ms；cache block queue-prep p95 0.017 ms，取消检查 0.027 ms，全部通过门槛。Signalsmith Stretch、Rubber Band 和 SoundTouch 已按官方许可/API 文档评估；本 release 保留已通过门槛的有界 legacy engine，避免在没有相同矩阵构建、2x 语音听测和许可决策时引入新的 C++ DSP 依赖。

---

## Task 6：更新文档和面向用户的语义

**文件：**

- 修改：`node-hub/dora-qwen3-tts-mlx/patches/qwen3-tts-mlx/README.md`
- 修改：与音色克隆 UI 文案相关的本地化/Makepad 文本
- 按需修改：release notes 或项目 README

- [x] **Step 1：文档明确产品只启用 x-vector**

说明：

- 产品中的自定义音色只需要参考音频；
- 参考文本不再需要；
- 底层 ICL API 是未暴露的实验能力；
- x-vector 提前 EOS 时会自动更换 seed 重试一次。

- [x] **Step 2：区分“生成语速”和“播放器倍速”**

UI 或帮助文案应明确：

- 生成语速：参与生成/后处理，保存和下载的音频包含该效果；
- 播放器倍速：采用变速不变调，仅影响当前预览，不改写音频文件；首次播放或 seek 到未缓存区域时按 block 后台准备，之后从 block cache 复用。

- [x] **Step 3：清理误导性文字**

运行：

```bash
rg -n "BundledIcl|Bundled ICL|uses Base model ICL|Reference Text \(what the audio says\)" apps/moxin-voice node-hub/dora-qwen3-tts-mlx docs
```

预期：产品代码和用户文案无误导性 ICL 描述；计划文档和底层实验文档中的历史/技术说明可以保留。

验证记录：根 README、macOS quickstart 和 Qwen patch README 已明确 Moxin Voice 只暴露 x-vector、Express 使用 3–6 秒参考音频且不需要文本、提前 EOS 自动换 seed 重试一次，底层 ICL 仅为未暴露实验 API。生成设置已标为“生成语速 / Generation speed”，播放器菜单标为“预览倍速 / Preview speed”，README 说明前者改变生成/下载文件、后者只按 block 后台准备本地预览。剩余 `BundledIcl` 仅是 serde 兼容 alias/测试；底层 patch 和历史技术文档中的 ICL 符号按计划保留。

---

## Task 7：完整验证和 release gate

**文件：**

- 验证整个 workspace 中与本计划相关的改动

- [ ] **Step 1：格式和静态检查**

```bash
cargo fmt --all --check
cargo check -p qwen3-tts-mlx
cargo check -p dora-qwen3-tts-mlx
cargo check -p moxin-voice
git diff --check
```

- [ ] **Step 2：运行相关测试集**

```bash
cargo test -p qwen3-tts-mlx
cargo test -p dora-qwen3-tts-mlx
cargo test -p moxin-voice
```

- [ ] **Step 3：执行不依赖模型的静态 release gate**

```bash
rg -n "synthesize_voice_clone_icl" node-hub/dora-qwen3-tts-mlx/src/main.rs
rg -n "fn (stretch_preserve_pitch|best_time_stretch_source_index|hann_window_sample)" apps/moxin-voice/src/screen.rs
rg -n "merged_samples\(" apps/moxin-voice/src/screen.rs
rg -n "stored_audio_samples|processed_audio_samples|rebuild_processed_audio_samples" apps/moxin-voice/src/screen.rs
```

预期：四条命令均无匹配。time-stretch 算法只允许存在于独立 worker/engine 模块；播放热路径只通过 `PlaybackAudioSource` 读取 block。

- [ ] **Step 4：使用真实模型进行手工 smoke test**

至少验证：

1. 新建自定义音色时不要求参考文本，3–6 秒参考音频可保存并生成。
2. release 前已有、带 `prompt_text` 的自定义音色仍能生成，日志显示 `mode=x-vector`。
3. `baiyang`、`yangyang` 正常生成。
4. 一条长文本被分段生成，x-vector 提前 EOS 时日志能看到一次重试；第二次仍失败时不输出残缺音频。
5. 生成或载入至少 30 分钟音频，并以虚拟/fixture source 验证 60 分钟容量；生成完成后没有 merged/processed 完整副本。
6. 30 分钟音频在 0.75x、1x、1.25x、1.5x、2x 之间连续切换，UI 无明显停顿，音高没有随倍率明显升降，播放位置不跳回。
7. 首次访问未缓存 block 时后台处理；再次访问相同 `(audio_revision, rate, block_index)` 时命中缓存。
8. 快速连续选择多个倍率时，只有最后一个 intent 能改变播放状态；旧任务及时取消或其结果只作为仍有效的 cache fill。
9. 在后台处理未完成时跨 10 分钟以上 seek，首块在性能门槛内 ready，旧结果不能把播放位置拉回。
10. 播放中 seek 后继续保持当前已生效的播放器倍速，连续 block 边界无明显断裂或爆音。
11. 30/60 分钟 fixture 的 player queue、working buffers 和 cache 保持在预算内，没有完整 stretched allocation。
12. 修改播放器倍速后流式下载音频，下载时长和内容不变。
13. 修改生成语速后重新生成，下载音频体现生成语速，播放器倍速仍可独立调整。

- [ ] **Step 5：审查最终 diff 范围**

确认：

- 没有删除底层 ICL 实现或模型资产；
- 没有修改 `VOICE:CUSTOM` 字段数量；
- release 最小修复没有仓促引入未经 benchmark、许可证和构建验证的新 DSP 依赖；若 Task 5 已完成，引入项必须有评估记录；
- 没有改动无关的训练、翻译或 ASR 功能；
- 旧音色配置保持可读；
- 所有产品克隆请求最终都进入 x-vector；
- UI 线程不再执行全量 time-stretch；
- canonical audio 没有 merged/processed 完整副本，播放器不接收完整长音频 command；
- cache key 包含 block index 且有 byte 上限，旧任务不能覆盖新音频、seek 或新倍率状态。

---

## 完成标准

只有同时满足以下条件，本计划的 release blocker 才算关闭：

1. Qwen 产品节点中不存在可到达的 ICL clone 调用。
2. 新旧自定义音色以及捆绑克隆音色都使用 x-vector。
3. x-vector 的 incomplete detection、一次自动重试和失败拒绝输出行为保持有效。
4. 30/60 分钟音频通过分块 source 和有界 player queue 播放，不创建完整 merged/processed/player-command 副本。
5. 播放器继续实现变速不变调，UI 线程不再同步处理整段 samples，time-stretch 只接受有界 block。
6. worker 支持过期任务取消/拒收，缓存按音频版本、倍率和 block 隔离，默认 byte budget 明确。
7. seek、快速连续切速、segment retry 和音频替换不会被旧任务结果回滚。
8. 10 秒 block、首播/seek、取消延迟和额外内存达到 Task 4 的 release 性能门槛。
9. 所有自动测试、静态检查和真实模型 smoke test 通过。
