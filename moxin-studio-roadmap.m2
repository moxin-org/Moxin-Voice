# Moxin Studio Roadmap: Moxin FM vs Conference Dashboard Analysis

**Date**: 2026-01-05
**Author**: Claude Code Analysis
**Branch**: cloud-model-mcp

## Executive Summary

This document analyzes the relationship between the Moxin FM application (in `moxin-studio/apps/moxin-fm/`) and the Conference Dashboard (in `conference-dashboard/`). Key finding: **significant code duplication with divergent architectural patterns**. Recommendation: converge on a unified architecture using `moxin-dora-bridge`.

---

## 1. Architecture Comparison

### 1.1 Directory Structure

```
moxin-studio/
├── apps/
│   └── moxin-fm/
│       ├── src/
│       │   ├── lib.rs           # App entry, exports
│       │   ├── screen.rs        # Main UI (2065 lines)
│       │   ├── moxin_hero.rs     # Status bar widget (740 lines)
│       │   ├── audio.rs         # Audio device management
│       │   ├── audio_player.rs  # TTS audio playback
│       │   ├── dora_integration.rs  # Dora bridge abstraction
│       │   └── log_bridge.rs    # Rust log capture
│       ├── dataflow/            # Configuration files
│       └── Cargo.toml
└── Cargo.toml                   # Workspace (moxin-studio)

conference-dashboard/
├── src/
│   ├── main.rs                  # Entry point, CLI args
│   ├── lib.rs                   # Shared state, audio enumeration
│   ├── app.rs                   # Main UI (108KB)
│   ├── audio_player.rs          # Audio playback
│   ├── dora_bridge.rs           # Inline Dora integration (61KB)
│   └── widgets/                 # Custom widgets
├── dataflow-conference.yml      # Dataflow configs
└── Cargo.toml                   # Standalone crate
```

### 1.2 Key Architectural Differences

| Aspect | Moxin FM (moxin-studio) | Conference Dashboard |
|--------|----------------------|---------------------|
| **Workspace** | Part of moxin-studio workspace | Standalone crate |
| **Dora Integration** | `moxin-dora-bridge` crate | Inline `dora-node-api` |
| **State Management** | `Arc<RwLock<DoraState>>` | `Arc<Mutex<SharedState>>` |
| **UI Pattern** | `moxin-widgets` library | Direct Makepad widgets |
| **Lifecycle** | Timer management (`start_timers`/`stop_timers`) | Thread-based, no pause |
| **Dark Mode** | `StateChangeListener` trait | Inline handling |
| **Log Bridge** | `log_bridge.rs` | Inline |
| **CLI Args** | Basic | Comprehensive (`--dataflow`, `--name`, etc.) |

---

## 2. Moxin FM (moxin-studio) Analysis

### 2.1 Strengths

1. **Clean Abstraction Layer** (`dora_integration.rs`)
   - `DoraCommand` enum for UI → dora commands
   - `DoraEvent` enum for dora → UI events
   - Worker thread with bounded channels (100 capacity)
   - Graceful shutdown via `Drop` trait

2. **Widget Reuse**
   - Uses shared `moxin-widgets` crate
   - `ParticipantPanel` for participant status
   - `LogPanel` for log display
   - Consistent theming via `StateChangeListener`

3. **Lifecycle Awareness** (`screen.rs:1942-1963`)
   ```rust
   pub fn stop_timers(&self, cx: &mut Cx) {
       cx.stop_timer(inner.audio_timer);
       cx.stop_timer(inner.dora_timer);
   }
   pub fn start_timers(&self, cx: &mut Cx) {
       inner.audio_timer = cx.start_interval(0.05);
       inner.dora_timer = cx.start_interval(0.1);
   }
   ```

4. **Shader-Based Animations** (`moxin_hero.rs:276-289`)
   - AEC blink uses shader time, no timer overhead
   - Connected state blink: `sin(self.time * 2.0)`

5. **API Key Management** (`screen.rs:1904-1939`)
   - Loads from `moxin_settings::Preferences`
   - Supports OpenAI, DeepSeek, Alibaba Cloud

### 2.2 Weaknesses

1. **Untracked Dependency**: `moxin-dora-bridge/` is in git untracked state (`?? moxin-dora-bridge/`)

2. **Missing Audio Player Export** (`lib.rs:6`)
   ```rust
   pub mod audio_player;  // Declared but may not build
   ```

3. **Over-Abstraction**
   - More layers than necessary for simple use cases
   - Some duplication between `audio.rs` and `conference-dashboard`'s enumeration

4. **No CLI Interface**
   - No way to specify dataflow path, node name, sample rate

---

## 3. Conference Dashboard Analysis

### 3.1 Strengths

1. **Self-Contained**
   - No external crate dependencies besides `dora-node-api`
   - All code in one crate

2. **Direct Dora Access**
   - Uses `dora-node-api` directly
   - Lower-level control over node lifecycle

3. **Comprehensive CLI** (`main.rs:21-106`)
   ```rust
   --dataflow, -d <PATH>    # Dataflow file
   --name, -n <NAME>        # Node name
   --sample-rate, -s <RATE> # Audio sample rate
   ```

4. **AEC Mode Support** (`lib.rs:123-143`)
   - Configurable participant names via env vars
   - Single user + assistant mode

5. **System Monitor Thread** (`main.rs:216-252`)
   - CPU and memory monitoring in background

### 3.2 Weaknesses

1. **Monolithic dora_bridge.rs** (61KB)
   - All Dora integration logic in single file
   - No separation of concerns

2. **No Lifecycle Management**
   - Threads continue running even when app hidden
   - Potential memory leak in paused state

3. **Code Duplication**
   - Audio device enumeration (similar to `audio.rs`)
   - Mic monitoring logic

4. **Older Widget Patterns**
   - Doesn't benefit from `moxin-widgets` improvements
   - Manual dark mode handling

---

## 4. Code Duplication Analysis

### 4.1 Audio Device Enumeration

**Moxin FM** (`audio.rs:48-88`):
```rust
pub fn get_input_devices(&self) -> Vec<AudioDeviceInfo> {
    let default_name = self.host.default_input_device()...
    // Enumerate and sort with default first
}
```

**Conference Dashboard** (`lib.rs:278-318`):
```rust
pub fn enumerate_audio_devices() -> (Vec<String>, Vec<String>) {
    let host = cpal::default_host();
    // Nearly identical logic
}
```

**Verdict**: ~80% identical code, should be unified.

### 4.2 Mic Level Monitoring

**Moxin FM** (`audio.rs:131-186`):
```rust
// Exponential smoothing: level * 0.7 + max * 0.3
state.level = state.level * 0.7 + max * 0.3;
```

**Conference Dashboard** (`lib.rs:232-250`):
```rust
// RMS calculation with faster smoothing
state.mic_input_level = state.mic_input_level * 0.5 + level * 0.5;
```

**Verdict**: Different approaches, both valid. Could share algorithm choice.

### 4.3 System Monitoring

**Moxin FM** (`moxin_hero.rs:518-542`):
```rust
fn update_system_stats(&mut self, cx: &mut Cx) {
    sys.refresh_cpu_usage();
    sys.refresh_memory();
    // Update UI immediately
}
```

**Conference Dashboard** (`main.rs:216-252`):
```rust
fn start_system_monitor(shared_state: SharedStateRef) {
    // Updates every 1 second in background thread
}
```

**Verdict**: Different threading models. Conference Dashboard's approach better for shared state.

---

## 5. Integration Points

### 5.1 Dataflow Configuration

**Moxin FM** (`dataflow/`):
- `voice-chat.yml`
- `maas_config.toml`
- `study_config_student{1,2}.toml`
- `study_config_tutor.toml`

**Conference Dashboard**:
- `dataflow-conference.yml`
- `dataflow-conference-multi.yml`
- `dataflow-with-aec.yml`
- `dataflow-study-*.yml`

### 5.2 Bridge Names

| Purpose | Moxin FM | Conference Dashboard |
|---------|---------|---------------------|
| Audio Output | `moxin-audio-player` | `moxin-audio-player` |
| System Log | `moxin-system-log` | `moxin-system-log` |
| Prompt Input | `moxin-prompt-input` | `moxin-prompt-input` |
| Participant Panel | `moxin-participant-panel` | N/A |

---

## 6. Recommendations

### 6.1 Short Term (P0)

1. **Track moxin-dora-bridge**
   ```bash
   git add moxin-dora-bridge/
   git commit -m "Track moxin-dora-bridge as proper Rust crate"
   ```

2. **Fix audio_player export** in `moxin-fm/src/lib.rs`
   - Add proper conditional compilation
   - Ensure it builds standalone

### 6.2 Medium Term (P1)

1. **Adopt moxin-dora-bridge in Conference Dashboard**
   - Replace `dora_bridge.rs` with `DoraIntegration`
   - Remove ~61KB of inline code

2. **Unify Audio Enumeration**
   - Move to shared location (moxin-studio root or moxin-widgets)
   - Or simply copy Moxin FM's implementation

3. **Add Lifecycle Management to Conference Dashboard**
   - Pause threads when app hidden
   - Prevent memory leaks

### 6.3 Long Term (P2)

1. **Widget Convergence**
   - Conference Dashboard widgets → `moxin-widgets`
   - Waveform display is unique feature worth sharing
   - Participant panels already shared

2. **Configuration Unification**
   - Single dataflow format
   - Shared study config structure

3. **Build System**
   - Consider merging Conference Dashboard into workspace
   - Or extract shared components to separate crate

---

## 7. Action Items

| Priority | Task | Owner | Status |
|----------|------|-------|--------|
| P0 | Track moxin-dora-bridge | Claude | TODO |
| P0 | Fix audio_player build | Claude | TODO |
| P1 | Port Conference Dashboard to moxin-dora-bridge | Claude | TODO |
| P1 | Unify audio enumeration code | Claude | TODO |
| P1 | Add lifecycle management | Claude | TODO |
| P2 | Extract waveform widget | Claude | TODO |
| P2 | Merge build systems | Claude | TODO |

---

## 8. Appendix: Key File References

### Moxin FM
- `apps/moxin-fm/src/lib.rs` - App entry point
- `apps/moxin-fm/src/screen.rs` - Main UI (2065 lines)
- `apps/moxin-fm/src/moxin_hero.rs` - Status widget (740 lines)
- `apps/moxin-fm/src/dora_integration.rs` - Bridge abstraction (469 lines)
- `apps/moxin-fm/src/audio.rs` - Audio management (229 lines)

### Conference Dashboard
- `src/main.rs` - Entry point with CLI
- `src/lib.rs` - Shared state (319 lines)
- `src/dora_bridge.rs` - Inline integration (61KB)
- `src/app.rs` - Main UI (108KB)

---

*End of Document*
