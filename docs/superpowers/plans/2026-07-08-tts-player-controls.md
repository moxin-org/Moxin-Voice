# TTS Player Controls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Upgrade the TTS bottom player with common audio-player controls while preserving the existing play button becoming pause during playback.

**Architecture:** Keep the bottom bar inside `apps/moxin-voice/src/screen.rs` and reuse the existing `TTSPlayer` in `apps/moxin-voice/src/audio_player.rs`. Add small pure helper functions first so seek, volume, mute, and playback-rate behavior can be tested without running Makepad or CPAL. Extend the audio thread only after tests define the command/state contract.

**Tech Stack:** Rust, Makepad live design, CPAL, Cargo tests.

---

### Task 1: Player State Helpers

**Files:**
- Modify: `apps/moxin-voice/src/screen.rs`

- [ ] Add tests for skip time clamping, volume display, mute toggling, and playback-rate cycling.
- [ ] Add pure helper functions and state fields for `player_volume`, `player_muted`, and `player_playback_rate`.
- [ ] Run focused unit tests.
- [ ] Commit.

### Task 2: Audio Thread Playback Settings

**Files:**
- Modify: `apps/moxin-voice/src/audio_player.rs`
- Modify: `apps/moxin-voice/src/screen.rs`

- [ ] Add failing tests for playback resampler rate updates and volume scaling.
- [ ] Add `SetVolume` and `SetPlaybackRate` commands to `TTSPlayer`.
- [ ] Apply volume/mute and user playback rate in the CPAL output path.
- [ ] Run focused audio player tests.
- [ ] Commit.

### Task 3: Bottom Bar UI Controls

**Files:**
- Modify: `apps/moxin-voice/src/screen.rs`

- [ ] Add rewind/forward buttons around the existing play/pause button.
- [ ] Add compact mute/volume and speed controls to the bottom bar.
- [ ] Wire UI actions to the tested helper functions and `TTSPlayer` settings.
- [ ] Run focused screen tests and `cargo check -p moxin-voice`.
- [ ] Commit.

### Task 4: Final Verification

**Files:**
- Verify: `apps/moxin-voice/src/screen.rs`
- Verify: `apps/moxin-voice/src/audio_player.rs`

- [ ] Run `cargo test -p moxin-voice`.
- [ ] Run `cargo check -p moxin-voice-shell`.
- [ ] Run `git diff --check`.
- [ ] Report results and any manual UI verification still needed.
