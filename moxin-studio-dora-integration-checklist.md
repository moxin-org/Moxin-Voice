# Moxin Studio - Dora Integration Checklist

> Consolidated from: roadmap-claude.md, roadmap-m2.md, roadmap-glm.md, moxin-studio-roadmap.m2, moxin-studio-roadmap.claude

---

## Apps Overview

Moxin Studio contains two main applications that use Dora dataflows:

### Moxin FM (Voice Chat with Human)

**Purpose:** Interactive voice assistant where users **talk** to AI tutors using their microphone.

**Key Features:**

- Human voice input via AEC (echo cancellation) + VAD (voice activity detection) + ASR
- Human can interrupt AI speakers (highest priority)
- 3 AI participants: Student1, Student2, Tutor
- Real-time speech-to-text and text-to-speech

**Dataflow:** `apps/moxin-fm/dataflow/voice-chat.yml`

**Architecture:**

```
┌─────────────────┐    ┌─────────────┐    ┌───────────────────┐
│  moxin-mic-input │ -> │     ASR     │ -> │  All 3 Bridges    │
│  (AEC + VAD)    │    │  (FunASR)   │    │  (human input)    │
└─────────────────┘    └─────────────┘    └───────────────────┘
         │                                          │
         ▼                                          ▼
┌─────────────────────────────────────────────────────────────┐
│              Conference Controller                           │
│  Policy: [(human, 0.001), (tutor, *), (student1, 1), ...]   │
│  Human has highest priority - can interrupt anytime          │
└─────────────────────────────────────────────────────────────┘
```

**Config Files:** `study_config_student1.toml`, `study_config_student2.toml`, `study_config_tutor.toml`

---

### Moxin Debate (Autonomous AI Debate)

**Purpose:** Watch AI agents debate each other autonomously. User provides a topic via text prompt.

**Key Features:**

- No human voice input (text prompts only)
- 3 AI participants with distinct roles:
  - **Student1 (PRO)** - Argues in favor of the topic
  - **Student2 (CON)** - Argues against the topic
  - **Tutor (Judge)** - Moderates, stays neutral, summarizes
- Turn-based debate with controller-managed speaking order

**Dataflow:** `apps/moxin-debate/dataflow/voice-chat.yml`

**Architecture:**

```
┌─────────────────────────────────────────────────────────────┐
│              Conference Controller                           │
│  Policy: [(tutor, *), (student2, 1), (student1, 2)]         │
│  Tutor always speaks, then alternating students              │
└─────────────────────────────────────────────────────────────┘
         │                    │                    │
         ▼                    ▼                    ▼
┌───────────────┐    ┌───────────────┐    ┌───────────────┐
│  Student1     │    │  Student2     │    │    Tutor      │
│  (PRO - GPT)  │    │  (CON - GPT)  │    │  (Judge-GPT)  │
└───────────────┘    └───────────────┘    └───────────────┘
```

**Config Files:** `debate_config_pro.toml`, `debate_config_con.toml`, `debate_config_judge.toml`

---

### Key Differences

| Feature                 | Moxin FM                                                                  | Moxin Debate                                                                  |
| ----------------------- | ------------------------------------------------------------------------- | ----------------------------------------------------------------------------- |
| **Human Voice Input**   | Yes (Mic + ASR)                                                           | No (text prompts only)                                                        |
| **Use Case**            | Interactive voice chat                                                    | Autonomous AI debate                                                          |
| **Participants**        | 3 AI + 1 Human                                                            | 3 AI only                                                                     |
| **Human Can Interrupt** | Yes (highest priority)                                                    | N/A                                                                           |
| **Policy Pattern**      | `[(human, 0.001), (tutor, *), ...]`                                       | `[(tutor, *), (student2, 1), (student1, 2)]`                                  |
| **Dynamic Nodes**       | moxin-mic-input, moxin-audio-player, moxin-prompt-input, moxin-system-log | moxin-audio-player-debate, moxin-prompt-input-debate, moxin-system-log-debate |
| **TTS Voices**          | Zhao Daniu, Doubao, Ma Yun                                                | Zhao Daniu, Chen Yifan, Luo Xiang                                             |

### Shared Components

Both apps share:

- **moxin-ui widgets:** MoxinHero, LedMeter, MicButton, AecButton (inline definitions due to Makepad parser limitations)
- **moxin-ui modules:** AudioManager, log_bridge
- **moxin-widgets:** ParticipantPanel, LogPanel, theme
- **moxin-dora-bridge:** SharedDoraState, AudioPlayerBridge, PromptInputBridge, SystemLogBridge

---

## P0: Critical (Do First) - Blocking Production

### P0.1 - Buffer Status Measurement ✅ COMPLETE

**Problem:** Buffer status must be measured from actual circular buffer, not estimated.

**Solution Implemented:**

```rust
// apps/moxin-fm/src/screen.rs:1089-1096
// Send actual buffer fill percentage to dora for backpressure control
// This replaces the bridge's estimation with the real value from AudioPlayer
if let Some(ref player) = self.audio_player {
    let fill_percentage = player.buffer_fill_percentage();
    if let Some(ref dora) = self.dora_integration {
        dora.send_command(DoraCommand::UpdateBufferStatus { fill_percentage });
    }
}
```

**Data Flow:**

1. Audio timer (50ms) triggers in screen.rs
2. Gets real buffer status: `audio_player.buffer_fill_percentage()`
3. Sends to DoraIntegration via `UpdateBufferStatus` command
4. DoraIntegration worker routes to bridge (dora_integration.rs:315-327)
5. Bridge sends to Dora via `send_buffer_status_to_dora()` (audio_player.rs:429-434)
6. Dora outputs `buffer_status` for backpressure control

**Verification:**

- [x] `AudioPlayer::buffer_fill_percentage()` returns real circular buffer fill (audio_player.rs:200)
- [x] Screen sends buffer status every 50ms via audio_timer (screen.rs:1089-1096)
- [x] DoraIntegration forwards to bridge when dataflow running (dora_integration.rs:318)
- [x] Bridge outputs to Dora: `buffer_status` (audio_player.rs:431)
- [x] NO estimation code in bridge (removed, now uses real values)

**Acceptance Criteria:**

- [x] `buffer_status` output reflects actual circular buffer fill (0-100%)
- [x] Bridge receives real values via `buffer_status_receiver` channel
- [x] Dispatcher check ensures status only sent when dataflow running

**Testing Verification:**

```bash
# Run dataflow and check logs
cargo run
# Should see: "Buffer status: XX.X%" in debug logs
# No estimation messages
```

---

### P0.2 - Session Start Deduplication ✅ DONE

**Problem:** `session_start` must be sent exactly ONCE per `question_id` on first audio chunk.

**Solution Implemented:**

```rust
// moxin-dora-bridge/src/widgets/audio_player.rs:222-242
let mut session_start_sent_for: HashSet<String> = HashSet::new();

if let Some(qid) = question_id {
    if !session_start_sent_for.contains(qid) {
        Self::send_session_start(node, input_id, &event_meta)?;
        session_start_sent_for.insert(qid.to_string());

        // Bound set size to last 100 question_ids
        if session_start_sent_for.len() > 100 {
            let to_remove: Vec<_> = session_start_sent_for.iter().take(50).cloned().collect();
            for key in to_remove {
                session_start_sent_for.remove(&key);
            }
        }
    }
}
```

**Verification:**

- [x] HashSet tracks sent question_ids
- [x] Set bounded to prevent memory growth
- [ ] Test 10+ conversation rounds without stopping
- [ ] Verify single `session_start` per question_id in controller logs

---

### P0.3 - Metadata Integer Extraction ✅ DONE

**Problem:** `question_id` is `Parameter::Integer`, but code only extracted `Parameter::String`.

**Solution Implemented:**

```rust
// moxin-dora-bridge/src/widgets/audio_player.rs:189-201
for (key, value) in metadata.parameters.iter() {
    let string_value = match value {
        Parameter::String(s) => s.clone(),
        Parameter::Integer(i) => i.to_string(),  // question_id is Integer!
        Parameter::Float(f) => f.to_string(),
        Parameter::Bool(b) => b.to_string(),
        Parameter::ListInt(l) => format!("{:?}", l),
        Parameter::ListFloat(l) => format!("{:?}", l),
        Parameter::ListString(l) => format!("{:?}", l),
    };
    event_meta.values.insert(key.clone(), string_value);
}
```

**Files Fixed:**

- [x] `moxin-dora-bridge/src/widgets/audio_player.rs`
- [x] `moxin-dora-bridge/src/widgets/participant_panel.rs`
- [x] `moxin-dora-bridge/src/widgets/prompt_input.rs`
- [x] `moxin-dora-bridge/src/widgets/system_log.rs`

---

### P0.4 - Channel Non-Blocking ✅ DONE

**Problem:** `send()` blocks when channel full, stalling the event loop.

**Solution Implemented:**

```rust
// moxin-dora-bridge/src/widgets/audio_player.rs:246-253
// Use try_send() to avoid blocking
if let Err(e) = audio_sender.try_send(audio_data.clone()) {
    warn!("Audio channel full, dropping audio chunk: {}", e);
}
let _ = event_sender.try_send(BridgeEvent::DataReceived { ... });
```

**Changes:**

- [x] Changed `send()` to `try_send()` for audio channel
- [x] Changed `send()` to `try_send()` for event channel
- [x] Increased audio channel buffer from 50 to 500 items

---

### P0.5 - Sample Count Tracking ✅ DONE

**Problem:** `data.0.len()` returns 1 for ListArray, not actual sample count.

**Solution Implemented:**

```rust
// moxin-dora-bridge/src/widgets/audio_player.rs:177-279
fn handle_dora_event(...) -> usize {  // Now returns sample count
    // ...
    if let Some(audio_data) = Self::extract_audio(&data, &event_meta) {
        let sample_count = audio_data.samples.len();
        // ... process audio ...
        return sample_count;  // Return actual samples extracted from ListArray
    }
    0  // Return 0 for non-audio events
}

// In event loop:
let sample_count = Self::handle_dora_event(...);
if sample_count > 0 {
    samples_in_buffer = (samples_in_buffer + sample_count).min(buffer_capacity);
}
```

**Verification:**

- [x] `handle_dora_event` returns `usize` sample count
- [x] Sample count extracted from `audio_data.samples.len()`
- [x] Build verified with `cargo check`

---

### P0.6 - Smart Reset (question_id Filtering) ✅ DONE

**Problem:** After reset, stale audio from previous question plays before new question's audio.

**Root Cause:** When a new question starts, audio chunks from the previous question may still be:

1. In the TTS pipeline (being synthesized)
2. In transit through Dora
3. Buffered in the AudioPlayer's circular buffer

Without filtering, these stale chunks play in order, causing confusing out-of-sync audio.

**Solution:** Track `question_id` with each audio segment and filter on reset.

#### Data Flow

```
TTS Node                    Dora Bridge                  AudioPlayer
─────────────────────────────────────────────────────────────────────
audio + metadata ──────────► extract question_id ──────► store with segment
{question_id: "1"}          from metadata                AudioSegment {
                                                           participant_id,
                                                           question_id: "1",
                                                           samples_remaining
                                                         }
```

#### Implementation Details

**1. AudioSegment with question_id tracking:**

```rust
// apps/moxin-fm/src/audio_player.rs
struct AudioSegment {
    participant_id: Option<String>,
    question_id: Option<String>,  // NEW: tracks which question owns this audio
    samples_remaining: usize,
}
```

**2. Smart reset filters stale audio:**

```rust
// apps/moxin-fm/src/audio_player.rs
fn smart_reset(&mut self, active_question_id: &str) {
    let mut samples_to_discard = 0;
    let mut new_segments = VecDeque::new();

    for segment in &self.segments {
        if let Some(ref qid) = segment.question_id {
            if qid == active_question_id {
                new_segments.push_back(segment.clone());  // KEEP
            } else {
                samples_to_discard += segment.samples_remaining;  // DISCARD
            }
        } else {
            samples_to_discard += segment.samples_remaining;  // No question_id = discard
        }
    }

    // Advance read position past discarded samples
    self.read_pos = (self.read_pos + samples_to_discard) % self.buffer_size;
    self.available_samples = self.available_samples.saturating_sub(samples_to_discard);
    self.segments = new_segments;
}
```

**3. AudioPlayer public API:**

```rust
// apps/moxin-fm/src/audio_player.rs
impl AudioPlayer {
    /// Write audio with question_id for smart reset support
    pub fn write_audio_with_question(
        &self,
        samples: &[f32],
        participant_id: Option<String>,
        question_id: Option<String>
    );

    /// Smart reset - keep only audio for the specified question_id
    pub fn smart_reset(&self, question_id: &str);
}
```

**4. AudioData carries question_id:**

```rust
// moxin-dora-bridge/src/data.rs
pub struct AudioData {
    pub samples: Vec<f32>,
    pub sample_rate: u32,
    pub channels: u16,
    pub participant_id: Option<String>,
    pub question_id: Option<String>,  // NEW
}
```

**5. Bridge extracts question_id from metadata:**

```rust
// moxin-dora-bridge/src/widgets/audio_player.rs
let question_id = metadata.get("question_id").map(|s| s.to_string());
// ... included in AudioData sent to widget
```

**6. Screen uses write_audio_with_question:**

```rust
// apps/moxin-fm/src/screen.rs
DoraEvent::AudioReceived { data } => {
    player.write_audio_with_question(
        &data.samples,
        data.participant_id.clone(),
        data.question_id.clone(),  // Pass question_id
    );
}
```

#### Usage Example

```
Timeline:
─────────────────────────────────────────────────────────────────────
Question #1 audio arrives → stored with question_id="1"
Question #1 audio arrives → stored with question_id="1"
                    ↓
         [RESET: new question starts with id="2"]
                    ↓
         smart_reset("2") called:
           - Segments with question_id="1" → DISCARDED
           - Segments with question_id="2" → KEPT (none yet)
                    ↓
Question #2 audio arrives → stored with question_id="2"
         Only question #2 audio plays ✓
```

#### When to Call smart_reset

The controller should call `audio_player.smart_reset(new_question_id)` when:

- A new question/round starts
- User manually advances to next topic
- Tutor intervenes and changes conversation flow

**Files Modified:**

- [x] `apps/moxin-fm/src/audio_player.rs:14-17` - Added `question_id` to AudioSegment
- [x] `apps/moxin-fm/src/audio_player.rs:149-186` - Added `smart_reset()` to CircularAudioBuffer
- [x] `apps/moxin-fm/src/audio_player.rs:245-250` - Added `write_audio_with_question()` to AudioPlayer
- [x] `apps/moxin-fm/src/audio_player.rs:287-291` - Added `smart_reset()` to AudioPlayer
- [x] `apps/moxin-fm/src/audio_player.rs:421-424` - Handle SmartReset command in audio thread
- [x] `moxin-dora-bridge/src/data.rs:75-76` - Added `question_id` to AudioData
- [x] `moxin-dora-bridge/src/widgets/audio_player.rs:471,478` - Extract and include question_id
- [x] `apps/moxin-fm/src/screen.rs:1836-1840` - Use `write_audio_with_question()`

**Acceptance Criteria:**

- [x] Each audio segment tracks its question_id
- [x] smart_reset() discards segments with non-matching question_id
- [x] Active segments preserved during reset
- [x] No stale audio playback after question change
- [x] Backwards compatible (write_audio() still works with question_id=None)
- [x] Build passes with `cargo check`

---

### P0.7 - Consolidate Participant Panel into Audio Player Bridge ✅ DONE

**Problem:** moxin-fm has TWO separate bridges receiving the same audio:

- `moxin-audio-player` - handles playback, buffer_status, session_start, audio_complete
- `moxin-participant-panel` - handles LED level visualization

This causes:

1. **Duplicate audio processing** (same TTS audio sent to 2 nodes)
2. **Active speaker mismatch** - moxin-participant-panel uses `question_id` tracking, but should use `current_participant` from AudioPlayer (what's actually playing)
3. **More dataflow complexity** (extra dynamic node definition)

**Conference-dashboard approach:** Single `dashboard` node handles BOTH audio playback AND LED visualization.

**Current (moxin-fm):**

```yaml
# voice-chat.yml - TWO nodes receive same audio
moxin-audio-player:
  inputs:
    audio_student1: primespeech-student1/audio
    audio_student2: primespeech-student2/audio
    audio_tutor: primespeech-tutor/audio

moxin-participant-panel: # DUPLICATE - remove this
  inputs:
    audio_student1: primespeech-student1/audio
    audio_student2: primespeech-student2/audio
    audio_tutor: primespeech-tutor/audio
```

**Target (like conference-dashboard):**

```yaml
# Only ONE node receives audio
moxin-audio-player:
  inputs:
    audio_student1: primespeech-student1/audio
    audio_student2: primespeech-student2/audio
    audio_tutor: primespeech-tutor/audio
  # Audio level/bands computed internally, sent to UI via events
```

**Implementation Plan:**

1. **Move audio level calculation into AudioPlayerBridge:**

```rust
// moxin-dora-bridge/src/widgets/audio_player.rs - ADD
fn calculate_audio_level(samples: &[f32]) -> f32 {
    // RMS with peak normalization (same as conference-dashboard)
    let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, |a, b| a.max(b));
    let sum_sq: f32 = samples.iter().map(|s| s * s).sum();
    let rms = (sum_sq / samples.len() as f32).sqrt();
    let norm_factor = if peak > 0.01 { 1.0 / peak } else { 1.0 };
    (rms * norm_factor * 1.5).clamp(0.0, 1.0)
}

fn calculate_bands(samples: &[f32]) -> [f32; 8] {
    // 8-band visualization (same as ParticipantPanelBridge)
}
```

2. **Use AudioPlayer's current_participant for active speaker:**

```rust
// In screen.rs audio_timer handler - get active from AudioPlayer
if let Some(ref player) = self.audio_player {
    let active_participant = player.current_participant(); // What's ACTUALLY playing
    // Update participant panels based on this
}
```

3. **Send ParticipantAudioData from AudioPlayerBridge:**

```rust
// moxin-dora-bridge/src/widgets/audio_player.rs
// After processing audio, emit participant audio data
let audio_data = ParticipantAudioData {
    participant_id: participant.clone(),
    audio_level: Self::calculate_audio_level(&samples),
    bands: Self::calculate_bands(&samples),
    is_active: true, // Active because we just received audio
};
let _ = event_sender.send(BridgeEvent::ParticipantAudio(audio_data));
```

4. **Update dora_integration.rs to handle new event:**

```rust
// dora_integration.rs - handle ParticipantAudio from audio player bridge
BridgeEvent::ParticipantAudio(data) => {
    let _ = event_tx.send(DoraEvent::ParticipantAudioReceived { data });
}
```

5. **Remove moxin-participant-panel from dataflow:**

```yaml
# voice-chat.yml - DELETE this node
# - id: moxin-participant-panel
#   path: dynamic
#   inputs: ...
```

6. **Delete ParticipantPanelBridge (no longer needed):**

- Delete `moxin-dora-bridge/src/widgets/participant_panel.rs`
- Remove from `moxin-dora-bridge/src/widgets/mod.rs`
- Remove from dispatcher bridge creation

**Files Modified:**

- [x] `moxin-dora-bridge/src/widgets/mod.rs` - Removed participant_panel export
- [x] `apps/moxin-fm/dataflow/voice-chat.yml` - Removed moxin-participant-panel node
- [x] Deleted `moxin-dora-bridge/src/widgets/participant_panel.rs`
- [x] LED visualization calculated in screen.rs from output waveform

**Acceptance Criteria:**

- [x] Only ONE dynamic node receives audio (moxin-audio-player)
- [x] LED bars show audio levels correctly (calculated from output waveform)
- [x] No duplicate audio processing
- [x] Build passes without participant_panel bridge

---

### P0.8 - Conference Dashboard Chat Window Format ✅ DONE

**Problem:** moxin-fm chat format differs from conference-dashboard.

**Current (moxin-fm):**

```
**Sender** ⌛: content
```

- No timestamp
- No message separators
- No filtering of "Context" messages
- Streaming indicator: ⌛

**Target (conference-dashboard):**

```
**Sender** (HH:MM:SS):
content

---

**Sender2** (HH:MM:SS):
content2
```

- Timestamp in parentheses
- `---` separator between messages
- Filters out "Context" sender
- Newline after sender line

**Implementation:**

```rust
// apps/moxin-fm/src/screen.rs - update_chat_display()

fn update_chat_display(&mut self, cx: &mut Cx) {
    // Filter out "Context" messages (like conference-dashboard)
    let filtered_messages: Vec<_> = self.chat_messages.iter()
        .filter(|msg| msg.sender != "Context")
        .collect();

    let chat_text = if filtered_messages.is_empty() {
        "Waiting for conversation...".to_string()
    } else {
        filtered_messages.iter()
            .map(|msg| {
                let streaming_indicator = if msg.is_streaming { " ⌛" } else { "" };
                // Format: **Sender** (timestamp) indicator:  \ncontent
                format!("**{}** ({}){}: \n{}",
                    msg.sender,
                    msg.timestamp,  // Need to add timestamp field
                    streaming_indicator,
                    msg.content
                )
            })
            .collect::<Vec<_>>()
            .join("\n\n---\n\n")  // Add --- separator
    };

    self.view.markdown(ids!(...)).set_text(cx, &chat_text);
}
```

**Add timestamp to ChatMessageEntry:**

```rust
// apps/moxin-fm/src/screen.rs

struct ChatMessageEntry {
    sender: String,
    content: String,
    is_streaming: bool,
    timestamp: String,  // ADD THIS
}

impl ChatMessageEntry {
    fn new(sender: &str, content: String) -> Self {
        Self {
            sender: sender.to_string(),
            content,
            is_streaming: false,
            timestamp: chrono::Local::now().format("%H:%M:%S").to_string(),
        }
    }
}
```

**Files Modified:**

- [x] `apps/moxin-fm/src/screen.rs` - ChatMessageEntry has timestamp field (line 1056)
- [x] `apps/moxin-fm/src/screen.rs` - `update_chat_display()` with proper format (line 2007)
- [x] `apps/moxin-fm/src/screen.rs` - `format_timestamp()` for HH:MM:SS (line 2035)
- N/A "Context" filtering - not used in voice-chat dataflow

**Acceptance Criteria:**

- [x] Chat shows timestamp in (HH:MM:SS) format
- [x] Messages separated by `---` (`.join("\n\n---\n\n")`)
- [x] Streaming indicator still works (⌛)
- [x] Format matches conference-dashboard

---

## P0 Summary

**Status:** 8/8 items complete ✅

| Task                               | Status      | Impact                   | Verification                         |
| ---------------------------------- | ----------- | ------------------------ | ------------------------------------ |
| P0.1 Buffer Status Measurement     | ✅ COMPLETE | Accurate backpressure    | ✅ Real values from AudioPlayer      |
| P0.2 Session Start Deduplication   | ✅ DONE     | No duplicate signals     | ✅ HashSet tracking implemented      |
| P0.3 Metadata Integer Extraction   | ✅ DONE     | question_id works        | ✅ All parameter types handled       |
| P0.4 Channel Non-Blocking          | ✅ DONE     | No pipeline stalls       | ✅ try_send() with buffer 500        |
| P0.5 Sample Count Tracking         | ✅ DONE     | Accurate buffer tracking | ✅ Returns actual sample count       |
| P0.6 Smart Reset                   | ✅ DONE     | No stale audio           | ✅ question_id filtering implemented |
| P0.7 Consolidate Participant Panel | ✅ DONE     | No duplicate processing  | ✅ Single bridge, LED from waveform  |
| P0.8 Chat Window Format            | ✅ DONE     | Consistent UX            | ✅ Timestamps, separators, format    |

**All P0 items complete!**

---

## P1: High Priority (Do Second)

### P1.1 - Code Organization: Break Up Large Files ✅ COMPLETE

**Problem:** Monolithic files violate single responsibility principle.

| File                                            | Before     | After                | Status  |
| ----------------------------------------------- | ---------- | -------------------- | ------- |
| `apps/moxin-fm/src/screen.rs`                   | 2314 lines | Extracted to 6 files | ✅ Done |
| `moxin-studio-shell/src/app.rs`                 | 1120 lines | (Makepad constraint) | Skipped |
| `moxin-dora-bridge/src/widgets/audio_player.rs` | ~600 lines | < 400 lines          | TODO    |

**screen.rs Extraction - COMPLETED:**

```
apps/moxin-fm/src/screen/
├── mod.rs              # struct, Widget impl (~590 lines)
├── design.rs           # live_design! DSL block (~1085 lines) - extracted in P2.1
├── audio_controls.rs   # Audio device selection, mic monitoring (~150 lines)
├── chat_panel.rs       # Chat display, prompt input, formatting (~115 lines)
├── log_panel.rs        # Log display, filtering, clipboard (~175 lines)
└── dora_handlers.rs    # Dora event handling, dataflow control (~330 lines)
```

**Implementation Details:**

- Makepad's derive macros (`Live`, `LiveHook`, `Widget`) require struct fields to be private
- Child modules can access private parent fields through `impl` blocks
- The `live_design!` macro can be extracted to a separate file (design.rs) with `use super::MoxinFMScreen;`
- The design module must be public (`pub mod design`) for Makepad path resolution
- Methods are distributed across child modules using `impl MoxinFMScreen` blocks

**Files Modified:**

- [x] Created `apps/moxin-fm/src/screen/` directory
- [x] Created `screen/mod.rs` - core struct, Widget impl, StateChangeListener (~590 lines)
- [x] Created `screen/design.rs` - extracted live_design! DSL block (~1085 lines)
- [x] Created `screen/audio_controls.rs` - init_audio, update_mic_level, device selection
- [x] Created `screen/chat_panel.rs` - send_prompt, update_chat_display, format_timestamp
- [x] Created `screen/log_panel.rs` - toggle_log_panel, update_log_display, poll_rust_logs
- [x] Created `screen/dora_handlers.rs` - init_dora, poll_dora_events, handle_moxin_start/stop
- [x] Deleted old `apps/moxin-fm/src/screen.rs`
- [x] lib.rs unchanged (module path `pub mod screen` works for both file and directory)
- [x] Build verified with `cargo build -p moxin-fm`

---

### P1.2 - Widget Duplication Removal ✅ PHASE 1 DONE

**Problem:** 988 duplicated lines (12% of codebase)

| Component        | Location 1     | Location 2            | Lines | Status                |
| ---------------- | -------------- | --------------------- | ----- | --------------------- |
| ParticipantPanel | shell/widgets/ | moxin-widgets/        | 492   | ✅ Removed from shell |
| LogPanel         | shell/widgets/ | moxin-widgets/        | 134   | ✅ Removed from shell |
| AudioPlayer      | moxin-fm/      | conference-dashboard/ | 724   | 📋 Phase 2 (deferred) |

**Phase 1: Shell Widget Cleanup ✅ DONE**

- [x] Delete `moxin-studio-shell/src/widgets/participant_panel.rs` - DONE
- [x] Delete `moxin-studio-shell/src/widgets/log_panel.rs` - DONE
- [x] Update `moxin-studio-shell/src/widgets/mod.rs` - Has note about moxin_widgets
- [x] All imports use `moxin_widgets::` versions
- [x] Build verified

**Current shell widgets** (no duplicates):

- `dashboard.rs` - Tab system (shell-specific)
- `sidebar.rs` - Navigation sidebar (shell-specific)
- `moxin_hero.rs` - Status bar (shell-specific)
- `tabs.rs` - Tab utilities (shell-specific)

**Phase 2: Audio Player Unification** 📋 DEFERRED

- [ ] Create `moxin-audio/` shared crate in workspace
- [ ] Move `apps/moxin-fm/src/audio_player.rs` to `moxin-audio/src/audio_player.rs`
- [ ] Add smart_reset from conference-dashboard
- [ ] Add streaming timeout from conference-dashboard
- [ ] Update `moxin-fm` and `conference-dashboard` to use shared crate

_Note: Phase 2 deferred as it requires significant refactoring and conference-dashboard integration._

---

### P1.3 - Waveform Visualization

**Problem:** moxin-fm lacks real-time audio visualization that conference-dashboard has.

**Source:** `conference-dashboard/src/widgets/waveform_view.rs`

```rust
// 512-sample rolling buffer for visualization
struct WaveformView {
    samples: VecDeque<f32>,
    // Real-time visualization
}
```

**Files to Modify:**

- [ ] Port `waveform_view.rs` from conference-dashboard to `moxin-widgets/src/`
- [ ] Export from `moxin-widgets/src/lib.rs`
- [ ] Integrate into moxin-fm screen

---

### P1.4 - Font Definition Cleanup ✅ DONE

**Problem:** Same fonts defined in multiple files.

**Solution:** Already completed in CHECKLIST.md P0.2.

**Verification:**

```bash
rg "FONT_REGULAR\s*=|FONT_BOLD\s*=" --type rust
# Only shows moxin-widgets/src/theme.rs - single source of truth ✅
```

**Status:**

- [x] `moxin-studio-shell/src/app.rs` - Imports from theme
- [x] `moxin-studio-shell/src/widgets/sidebar.rs` - Imports from theme
- [x] `moxin-studio-shell/src/widgets/moxin_hero.rs` - Imports from theme
- [x] `moxin-widgets/src/theme.rs` - Single source of truth for FONT_REGULAR, FONT_MEDIUM, FONT_SEMIBOLD, FONT_BOLD

**Note:** This was completed as part of the UI refactoring checklist (CHECKLIST.md P0.2 - Font Consolidation).

---

## P1 Summary

| Task                        | Status          | Impact                                         |
| --------------------------- | --------------- | ---------------------------------------------- |
| P1.1 Break Up Large Files   | ✅ DONE         | screen.rs → 6 files, mod.rs 590 lines          |
| P1.2 Widget Duplication     | ✅ PHASE 1 DONE | Shell duplicates removed (-626 lines)          |
| P1.3 Waveform Visualization | 📋 TODO         | UX improvement                                 |
| P1.4 Font Cleanup           | ✅ DONE         | Single source of truth (see CHECKLIST.md P0.2) |

---

## P2: Medium Priority (Do Third)

### P2.1 - SharedDoraState Architecture (Simplify Dora↔UI Communication)

**Problem:** Current architecture has 4 layers of indirection for Dora data:

```
Bridge → chat_sender channel → dora_integration worker → event_tx channel → screen.poll_dora_events()
```

This causes:

- 4+ channels with different capacities
- Multiple polling loops (10ms, 50ms, 100ms)
- Message consolidation in multiple places
- ~500+ lines of boilerplate

**Solution:** Replace channels with `SharedDoraState` using `Arc<RwLock>` with dirty tracking.

```
Bridge → SharedDoraState (Arc<RwLock>) ← UI reads on single timer
```

#### Architecture Diagram

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         DORA BRIDGES (Worker Threads)                        │
├─────────────────────┬─────────────────────┬─────────────────────────────────┤
│  PromptInputBridge  │  AudioPlayerBridge  │  SystemLogBridge                │
│                     │                     │                                 │
│  state.chat.push()  │  state.audio.push() │  state.logs.push()              │
└─────────┬───────────┴──────────┬──────────┴───────────────┬─────────────────┘
          │         Direct write (no channels)              │
          ▼                      ▼                          ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                     SharedDoraState (Arc<...>)                              │
│                                                                             │
│  chat: ChatState        audio: AudioState       logs: DirtyVec<LogEntry>   │
│  status: DirtyValue<DoraStatus>                                            │
└─────────────────────────────────────────────────────────────────────────────┘
          │          Read on UI timer (single poll)         │
          ▼                      ▼                          ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│                        MoxinFMScreen (UI Thread)                             │
│  poll_dora_state() - single function reads all dirty data                   │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### File Structure

```
moxin-dora-bridge/src/
├── lib.rs                 # Re-exports SharedDoraState
├── data.rs                # ChatMessage, AudioChunk, LogEntry (exists)
├── shared_state.rs        # NEW: SharedDoraState, DirtyVec, DirtyValue
├── dispatcher.rs          # Creates bridges with shared state
└── widgets/
    ├── prompt_input.rs    # Uses state.chat.push()
    ├── audio_player.rs    # Uses state.audio.push()
    └── system_log.rs      # Uses state.logs.push()
```

#### Implementation Steps

**Step 1: Create SharedDoraState** (`moxin-dora-bridge/src/shared_state.rs`) ✅

- [x] Create `DirtyVec<T>` - dirty-trackable collection
- [x] Create `DirtyValue<T>` - dirty-trackable single value
- [x] Create `ChatState` - with streaming consolidation logic
- [x] Create `AudioState` - ring buffer for audio chunks
- [x] Create `SharedDoraState` - unified state container
- [x] Export from `lib.rs`

**Step 2: Update PromptInputBridge** ✅

- [x] Accept `Arc<SharedDoraState>` in constructor
- [x] Replace `chat_sender.send()` with `state.chat.push()`
- [x] Remove channel creation code
- [x] Move streaming consolidation to `ChatState.push()`

**Step 3: Update AudioPlayerBridge** ✅

- [x] Accept `Arc<SharedDoraState>` in constructor
- [x] Replace `audio_sender.send()` with `state.audio.push()`
- [x] Remove channel creation code

**Step 4: Update SystemLogBridge** ✅

- [x] Accept `Arc<SharedDoraState>` in constructor
- [x] Replace `log_sender.send()` with `state.logs.push()`
- [x] Remove channel creation code

**Step 5: Update Dispatcher** ✅

- [x] Create `SharedDoraState` on init
- [x] Pass to all bridges on creation
- [x] Expose `state()` method for UI access

**Step 6: Update DoraIntegration** ✅

- [x] Remove event channels and polling worker
- [x] Expose `state()` -> `Arc<SharedDoraState>`
- [x] Simplify to just manage dataflow lifecycle

**Step 7: Update MoxinFMScreen** ✅

- [x] Replace `poll_dora_events()` with `poll_dora_state()`
- [x] Single timer reads all dirty data
- [x] Remove `pending_streaming_messages` (handled in ChatState)
- [x] Remove multiple poll functions

#### Benefits

| Aspect                | Current         | After           |
| --------------------- | --------------- | --------------- |
| Channels              | 4+              | 0               |
| Polling loops         | 3               | 1               |
| Message consolidation | Multiple places | 1 per data type |
| Code lines            | ~500+           | ~150            |
| Latency               | 10ms + 100ms    | Single timer    |

**Files Modified:** ✅

- [x] `moxin-dora-bridge/src/shared_state.rs` (NEW - 547 lines)
- [x] `moxin-dora-bridge/src/lib.rs`
- [x] `moxin-dora-bridge/src/bridge.rs` (removed dead code: BridgeEvent, InputHandler, BridgeSharedState, BridgeChannel, BridgeBuilder)
- [x] `moxin-dora-bridge/src/widgets/prompt_input.rs`
- [x] `moxin-dora-bridge/src/widgets/audio_player.rs`
- [x] `moxin-dora-bridge/src/widgets/system_log.rs`
- [x] `moxin-dora-bridge/src/dispatcher.rs`
- [x] `apps/moxin-fm/src/dora_integration.rs` (removed DoraState, simplified to Arc<AtomicBool>)
- [x] `apps/moxin-fm/src/screen/mod.rs` (removed pending_streaming_messages)
- [x] `apps/moxin-fm/src/screen/dora_handlers.rs`
- [x] `apps/moxin-fm/src/screen/chat_panel.rs`
- [x] `apps/moxin-fm/src/screen/design.rs` (NEW - extracted live_design! block)

#### Completion Summary

**Dead Code Removed:**

| File                  | Removed                      | Reason                             |
| --------------------- | ---------------------------- | ---------------------------------- |
| `bridge.rs`           | `BridgeEvent` enum           | Replaced by SharedDoraState        |
| `bridge.rs`           | `InputHandler` type          | Unused                             |
| `bridge.rs`           | `BridgeSharedState<T>`       | Replaced by SharedDoraState        |
| `bridge.rs`           | `BridgeChannel<T>`           | Channels removed                   |
| `bridge.rs`           | `BridgeBuilder`              | Unused                             |
| `shared_state.rs`     | `DoraStatus.connected`       | Never used                         |
| `shared_state.rs`     | `set_connected()`            | Never called                       |
| `shared_state.rs`     | `set_dataflow_running()`     | Never called                       |
| `dora_integration.rs` | `DoraState` struct           | Replaced by `Arc<AtomicBool>`      |
| `dora_integration.rs` | `state()` method             | Redundant with shared_dora_state() |
| `mod.rs`              | `pending_streaming_messages` | ChatState handles consolidation    |

**`pending_streaming_messages` Removal Details:**

The field was dead code - never populated (no `.push()` calls), only cleared:

- `mod.rs:1193`: Removed field definition
- `chat_panel.rs:65`: Removed `.clear()` call
- `chat_panel.rs:84`: Removed `.chain()` in update_chat_display()
- `chat_panel.rs:103,110`: Removed `.len()` references
- `dora_handlers.rs:142,338`: Removed `.clear()` calls

**Architecture Simplification:**

| Component          | Before      | After                                       |
| ------------------ | ----------- | ------------------------------------------- |
| DoraEvent variants | 6           | 3 (DataflowStarted, DataflowStopped, Error) |
| Channels           | 4+          | 0 for data (events only for control flow)   |
| DoraState fields   | 3           | 0 (replaced with AtomicBool)                |
| bridge.rs          | 176 lines   | 59 lines                                    |
| mod.rs             | 1,663 lines | 587 lines                                   |

---

### P2.2 - Debug Logging Cleanup

**Problem:** 15+ `println!` statements in production code.

**Files to Clean:**

- [ ] `apps/moxin-fm/src/screen.rs`
- [ ] `moxin-dora-bridge/src/widgets/*.rs`

**Solution:**

```rust
#[cfg(debug_assertions)]
macro_rules! debug_log {
    ($($arg:tt)*) => { println!($($arg)*) };
}

#[cfg(not(debug_assertions))]
macro_rules! debug_log {
    ($($arg:tt)*) => { };
}
```

---

### P2.3 - System Monitoring Integration ✅ DONE

**Problem:** moxin-fm CPU/memory stats update may lag during heavy operations.

**Solution:** Created background system monitor thread that polls sysinfo every 1 second and stores values in atomic shared state. MoxinHero now reads from shared state instead of calling sysinfo on UI thread.

**Implementation:**

1. **New module: `apps/moxin-fm/src/system_monitor.rs`**
   - Uses `OnceLock<Arc<SystemStats>>` for singleton pattern
   - Background thread runs `sysinfo::System` polling
   - Atomic u32 values for lock-free reads (scaled 0-10000 for precision)
   - `start_system_monitor()` - starts background thread (idempotent)
   - `get_cpu_usage()` / `get_memory_usage()` - returns f64 (0.0-1.0)

2. **Modified: `apps/moxin-fm/src/moxin_hero.rs`**
   - Removed `sys: Option<System>` field
   - Added `monitor_started: bool` field
   - `handle_event()` calls `system_monitor::start_system_monitor()` on first event
   - `update_system_stats()` now reads from `system_monitor::get_cpu_usage/get_memory_usage()`

3. **Modified: `apps/moxin-fm/src/lib.rs`**
   - Added `pub mod system_monitor;`

4. **Fixed live_design registration order: `moxin-studio-shell/src/app.rs`**
   - Apps (MoxinFMApp, MoxinSettingsApp) must register BEFORE dashboard
   - Dashboard's `live_design!` references app widgets, so apps must be registered first

5. **Fixed Makepad module path resolution:**
   - `apps/moxin-fm/src/screen/mod.rs` - Made `design` module public (`pub mod design`)
   - `moxin-studio-shell/src/widgets/dashboard.rs` - Updated import to `moxin_fm::screen::design::MoxinFMScreen`

**Files Modified:**

- [x] `apps/moxin-fm/src/system_monitor.rs` - NEW: Background system monitor
- [x] `apps/moxin-fm/src/moxin_hero.rs` - Uses shared state instead of direct sysinfo
- [x] `apps/moxin-fm/src/lib.rs` - Module declaration
- [x] `apps/moxin-fm/src/screen/mod.rs` - Made design module public
- [x] `moxin-studio-shell/src/app.rs` - Fixed live_design registration order
- [x] `moxin-studio-shell/src/widgets/dashboard.rs` - Fixed MoxinFMScreen import path

**Benefits:**

- UI thread no longer blocks on sysinfo polling
- Consistent 1-second update interval regardless of UI load
- Lock-free reads via atomic operations
- Fixed runtime "target class not found" error for MoxinFMScreen

---

### P2.4 - Settings Persistence ✅ DONE

**Completed (2026-01-10):**

All settings now persist correctly to `~/.dora/dashboard/preferences.json`.

**Settings Verified:**

- [x] Dark mode preference saves/loads - `app.rs:588-592` saves, `app.rs:327` loads
- [x] Audio input device saves/loads - Added to Preferences, saved on selection
- [x] Audio output device saves/loads - Added to Preferences, saved on selection
- [x] API keys save/load - Already implemented via Provider struct

**Files Modified:**

- [x] `apps/moxin-settings/src/data/preferences.rs` - Added `audio_input_device`, `audio_output_device` fields
- [x] `apps/moxin-fm/src/screen/audio_controls.rs` - Load saved devices on init, save on selection

**Preferences JSON Structure:**

```json
{
  "providers": [...],
  "dark_mode": true,
  "audio_input_device": "MacBook Pro Microphone",
  "audio_output_device": "MacBook Pro Speakers"
}
```

---

## P2 Summary

| Task                      | Status  | Impact                                             |
| ------------------------- | ------- | -------------------------------------------------- |
| P2.1 Shared State Pattern | ✅ DONE | Cleaner architecture, ~120 lines dead code removed |
| P2.2 Debug Logging        | ✅ DONE | Only 4 legitimate eprintln! remain                 |
| P2.3 System Monitoring    | ✅ DONE | Background thread, lock-free atomic reads          |
| P2.4 Settings Persistence | ✅ DONE | Dark mode + audio devices saved/restored           |

---

## P3: Low Priority (Do Later)

### P3.1 - CLI Interface ✅ DONE

**Completed (2026-01-10):**

Added clap-based CLI argument parsing to moxin-studio-shell.

**Usage:**

```bash
moxin-studio --help              # Show help
moxin-studio --version           # Show version
moxin-studio --dark-mode         # Start in dark mode
moxin-studio --log-level debug   # Enable debug logging
moxin-studio --dataflow path.yml # Custom dataflow
moxin-studio --sample-rate 44100 # Custom sample rate
moxin-studio --width 1600 --height 1000  # Custom window size
```

**Files Created/Modified:**

- [x] `moxin-studio-shell/Cargo.toml` - Added clap 4.4 with derive feature
- [x] `moxin-studio-shell/src/cli.rs` - NEW: Args struct with documentation
- [x] `moxin-studio-shell/src/main.rs` - Parse args, configure logging
- [x] `moxin-studio-shell/src/app.rs` - OnceLock storage, dark mode override

**Available Options:**
| Option | Default | Description |
|--------|---------|-------------|
| `-d, --dataflow` | None | Path to dataflow YAML |
| `--sample-rate` | 32000 | Audio sample rate |
| `--dark-mode` | false | Start in dark mode |
| `--log-level` | info | Log verbosity |
| `--width` | 1400 | Window width |
| `--height` | 900 | Window height |

---

### P3.2 - Track moxin-dora-bridge in Git

**Problem:** `moxin-dora-bridge/` shows as untracked.

```bash
git add moxin-dora-bridge/
git commit -m "Track moxin-dora-bridge crate"
```

---

### P3.3 - Testing Infrastructure

**Target:** 70%+ coverage on testable components.

**Testable:**

- [ ] `CircularAudioBuffer` - fill percentage, smart reset
- [ ] `EventMetadata` - parameter extraction
- [ ] Session start deduplication logic

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_buffer_fill_percentage() {
        let mut buffer = CircularAudioBuffer::new(30.0, 32000);
        buffer.write_samples(&[0.5; 16000], None);
        assert!((buffer.fill_percentage() - 1.67).abs() < 0.1);
    }

    #[test]
    fn test_session_start_deduplication() {
        let mut sent = HashSet::new();
        assert!(should_send("100", &mut sent));
        assert!(!should_send("100", &mut sent)); // Duplicate
        assert!(should_send("200", &mut sent));  // New
    }

    #[test]
    fn test_smart_reset() {
        let mut buffer = CircularAudioBuffer::new(30.0, 32000);
        buffer.write_samples(&[0.5; 16000], Some("100".to_string()));
        buffer.write_samples(&[0.5; 16000], Some("200".to_string()));

        let active = HashSet::from(["200".to_string()]);
        buffer.smart_reset(&active);

        // Only question_id 200 segments remain
        assert_eq!(buffer.segments.len(), 1);
    }
}
```

---

### P3.4 - API Documentation ✅ DONE

**Completed (2026-01-10):**

Comprehensive rustdoc documentation added to `moxin-dora-bridge` crate:

- [x] `moxin-dora-bridge/src/lib.rs` - Crate overview with architecture diagram, usage examples
- [x] `moxin-dora-bridge/src/bridge.rs` - Bridge trait with state machine diagram
- [x] `moxin-dora-bridge/src/shared_state.rs` - DirtyVec, DirtyValue, ChatState, AudioState, SharedDoraState
- [x] `moxin-dora-bridge/src/data.rs` - AudioData, ChatMessage, LogEntry, ControlCommand

**Documentation Features:**

- Architecture diagrams (ASCII art in rustdoc)
- Code examples for all major types
- Design principle explanations
- Thread safety notes
- Streaming consolidation explanation

**Verification:**

```bash
cargo doc --package moxin-dora-bridge --no-deps
# Generated: target/doc/moxin_dora_bridge/index.html
```

---

## P3 Summary

| Task               | Status  | Impact                                          |
| ------------------ | ------- | ----------------------------------------------- |
| P3.1 CLI Interface | ✅ DONE | clap-based args: --dark-mode, --log-level, etc. |
| P3.2 Git Tracking  | ✅ DONE | Already tracked in git                          |
| P3.3 Testing       | 📋 TODO | Reliability                                     |
| P3.4 Documentation | ✅ DONE | Comprehensive rustdoc for moxin-dora-bridge     |

---

## Success Criteria

### After P0

- [ ] Conversation runs 10+ rounds without stopping
- [ ] Buffer status reflects actual fill (measured, not estimated)
- [ ] No buffer overrun warnings in logs
- [ ] No duplicate `session_start` signals
- [ ] Smart reset clears only stale audio
- [ ] Streaming auto-completes after 2s timeout
- [ ] Only ONE bridge receives audio (no duplicate processing)
- [ ] Active speaker based on actual playback (AudioPlayer.current_participant)
- [ ] Chat format matches conference-dashboard (timestamps, separators, filtering)

### After P1

- [ ] No file > 500 lines (except app.rs - Makepad constraint)
- [ ] 0 duplicate widget files
- [ ] Waveform visualization working
- [ ] Single source of truth for fonts

### After P2

- [ ] Shared state pattern implemented
- [ ] 0 debug println statements
- [ ] System stats update in background
- [ ] All settings persist correctly

### After P3

- [ ] CLI arguments working
- [ ] moxin-dora-bridge tracked in git
- [ ] 70%+ test coverage on buffer/signal logic
- [ ] Complete API documentation

---

## Quick Reference: Key Files

### Dora Bridge Layer (Shared)

| File                                            | Purpose                        | Lines |
| ----------------------------------------------- | ------------------------------ | ----- |
| `moxin-dora-bridge/src/widgets/audio_player.rs` | Audio bridge, signals          | ~600  |
| `moxin-dora-bridge/src/widgets/prompt_input.rs` | Chat, control commands         | ~430  |
| `moxin-dora-bridge/src/widgets/system_log.rs`   | Log aggregation                | ~360  |
| `moxin-dora-bridge/src/widgets/aec_input.rs`    | AEC mic input bridge (FM only) | ~550  |
| `moxin-dora-bridge/src/shared_state.rs`         | SharedDoraState, DirtyVec      | ~547  |

_Note: `participant_panel.rs` was deleted in P0.8 - LED visualization now calculated from output waveform_

### Shared UI Infrastructure (moxin-ui)

| File                                 | Purpose                                | Lines |
| ------------------------------------ | -------------------------------------- | ----- |
| `moxin-ui/src/audio.rs`              | AudioManager, device enum, mic monitor | ~233  |
| `moxin-ui/src/log_bridge.rs`         | Rust log capture for UI display        | ~123  |
| `moxin-ui/src/system_monitor.rs`     | Background CPU/memory/GPU monitor      | ~150  |
| `moxin-ui/src/widgets/moxin_hero.rs` | MoxinHero status bar widget            | ~400  |
| `moxin-ui/src/widgets/led_meter.rs`  | LED level meter widget                 | ~245  |
| `moxin-ui/src/widgets/mic_button.rs` | Mic toggle button widget               | ~200  |
| `moxin-ui/src/widgets/aec_button.rs` | AEC toggle button widget               | ~220  |

_Note: LED/Mic/AEC widgets have inline definitions in app design.rs due to Makepad parser limitations with `link::theme::_` imports\*

### Moxin FM (Voice Chat)

| File                                         | Purpose                                 | Lines |
| -------------------------------------------- | --------------------------------------- | ----- |
| `apps/moxin-fm/src/screen/mod.rs`            | Main screen struct, Widget impl         | ~590  |
| `apps/moxin-fm/src/screen/design.rs`         | live_design! UI layout (inline widgets) | ~1250 |
| `apps/moxin-fm/src/screen/audio_controls.rs` | Audio device selection, mic monitoring  | ~150  |
| `apps/moxin-fm/src/screen/chat_panel.rs`     | Chat display, prompt input              | ~115  |
| `apps/moxin-fm/src/screen/log_panel.rs`      | Log display, filtering                  | ~175  |
| `apps/moxin-fm/src/screen/dora_handlers.rs`  | Dora event handling, dataflow control   | ~330  |
| `apps/moxin-fm/src/audio_player.rs`          | Circular buffer, CPAL playback          | ~360  |
| `apps/moxin-fm/src/dora_integration.rs`      | Dora lifecycle management               | ~400  |

### Moxin Debate (AI Debate)

| File                                             | Purpose                                 | Lines |
| ------------------------------------------------ | --------------------------------------- | ----- |
| `apps/moxin-debate/src/screen/mod.rs`            | Main screen struct, Widget impl         | ~590  |
| `apps/moxin-debate/src/screen/design.rs`         | live_design! UI layout (inline widgets) | ~800  |
| `apps/moxin-debate/src/screen/audio_controls.rs` | Audio device selection                  | ~150  |
| `apps/moxin-debate/src/screen/chat_panel.rs`     | Chat display, prompt input              | ~115  |
| `apps/moxin-debate/src/screen/log_panel.rs`      | Log display, filtering                  | ~175  |
| `apps/moxin-debate/src/screen/dora_handlers.rs`  | Dora event handling                     | ~300  |
| `apps/moxin-debate/src/audio_player.rs`          | Circular buffer, CPAL playback          | ~360  |
| `apps/moxin-debate/src/dora_integration.rs`      | Dora lifecycle management               | ~350  |

### Configuration / Dataflows

| File                                              | Purpose                                     |
| ------------------------------------------------- | ------------------------------------------- |
| `apps/moxin-fm/dataflow/voice-chat.yml`           | FM dataflow (with human mic input)          |
| `apps/moxin-fm/dataflow/study_config_*.toml`      | FM role configs (student1, student2, tutor) |
| `apps/moxin-debate/dataflow/voice-chat.yml`       | Debate dataflow (AI-only)                   |
| `apps/moxin-debate/dataflow/debate_config_*.toml` | Debate role configs (pro, con, judge)       |
| `MOFA_DORA_ARCHITECTURE.md`                       | Architecture diagram                        |

---

## Related Documents

| Document                                                 | Description                      |
| -------------------------------------------------------- | -------------------------------- |
| [MOFA_DORA_ARCHITECTURE.md](./MOFA_DORA_ARCHITECTURE.md) | Signal flow diagrams             |
| [CHECKLIST.md](./CHECKLIST.md)                           | UI refactoring checklist         |
| [roadmap-claude.md](./roadmap-claude.md)                 | Architectural analysis           |
| [roadmap-glm.md](./roadmap-glm.md)                       | Strategic planning with grades   |
| [moxin-studio-roadmap.m2](./moxin-studio-roadmap.m2)     | Moxin FM vs Conference Dashboard |

---

_Last Updated: 2026-01-18_
_P0 Progress: 8/8 complete ✅_
_P1 Progress: 3/4 complete_
_P2 Progress: 4/4 complete ✅_
_P3 Progress: 3/4 complete_

**Completed P0 Items:** (All done!)

- ✅ P0.1 Buffer Status Measurement
- ✅ P0.2 Session Start Deduplication
- ✅ P0.3 Metadata Integer Extraction
- ✅ P0.4 Channel Non-Blocking
- ✅ P0.5 Sample Count Tracking
- ✅ P0.6 Smart Reset (question_id filtering)
- ✅ P0.7 Consolidate Participant Panel (LED from output waveform)
- ✅ P0.8 Chat Window Format (timestamps, separators)

**Completed P1 Items:**

- ✅ P1.1 Code Organization (screen.rs → 6 files, live_design! to design.rs)
- ✅ P1.2 Widget Duplication Phase 1 (shell duplicates removed)
- ✅ P1.4 Font Definition Cleanup (see CHECKLIST.md P0.2)

**Completed P2 Items:** (All done!)

- ✅ P2.1 SharedDoraState Architecture (removed ~120 lines dead code)
- ✅ P2.2 Debug Logging (only 4 legitimate eprintln! remain)
- ✅ P2.3 System Monitoring (background thread, atomic reads)
- ✅ P2.4 Settings Persistence (dark mode + audio devices)

**Completed P3 Items:**

- ✅ P3.1 CLI Interface (clap-based args: --dark-mode, --log-level, --dataflow)
- ✅ P3.2 Git Tracking (moxin-dora-bridge already tracked)
- ✅ P3.4 API Documentation (comprehensive rustdoc for moxin-dora-bridge)

**Widget Consolidation Status (2026-01-18):**

- ✅ `moxin-ui/src/audio.rs` - Shared AudioManager (moved from both apps)
- ✅ `moxin-ui/src/log_bridge.rs` - Shared log capture (moved from both apps)
- ✅ `moxin-ui/src/system_monitor.rs` - Shared system monitor
- ✅ `moxin-ui/src/widgets/moxin_hero.rs` - Shared MoxinHero widget
- ⚠️ `moxin-ui/src/widgets/led_meter.rs` - Defined but **inline required** in apps
- ⚠️ `moxin-ui/src/widgets/mic_button.rs` - Defined but **inline required** in apps
- ⚠️ `moxin-ui/src/widgets/aec_button.rs` - Defined but **inline required** in apps

_Note: LED/Mic/AEC widgets must use inline definitions in each app's design.rs due to Makepad live_design parser "Unexpected token #" error when importing `link::theme::_` in shared widget modules. The Rust WidgetExt traits from moxin-ui work correctly; only the live_design! visual definitions need to be inline.\*

**Remaining Items:**

- P1.3 Waveform Visualization
- P3.3 Testing Infrastructure

**Next Action:**

1. P1.3 Waveform Visualization (port from conference-dashboard)
2. P3.3 Testing Infrastructure (unit tests for pure logic)
