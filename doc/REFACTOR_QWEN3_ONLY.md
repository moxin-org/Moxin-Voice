# Refactor: Qwen3-TTS-MLX Only Mode

**Date**: 2026-03-18
**Scope**: Remove PrimeSpeech (Python) and PrimeSpeech MLX backends; keep Qwen3-TTS-MLX as the sole inference and voice-cloning backend.

---

## Summary of Changes

| Area | Change |
|------|--------|
| Built-in voices | PrimeSpeech voices (Doubao, 罗翔, etc.) removed from active path; Qwen3 voices only |
| Voice cloning | Pro mode (few-shot GPT-SoVITS training) hidden from UI; Express mode (ICL zero-shot) only |
| Backend selector | Home-page model picker row hidden; settings Zero-shot / Training backend dropdowns hidden |
| Scripts | `run_tts_backend.sh`, `macos_run_tts_backend.sh` → Qwen3-only; `macos_bootstrap.sh` → always download Qwen3 models |
| Preflight | `macos_preflight.sh` → Qwen3 model checks required; PrimeSpeech checks removed |
| Build | `build_macos_app.sh` → no longer builds/bundles `dora-primespeech-mlx` or `moxin-tts-node` |
| Cargo workspace | `node-hub/dora-primespeech-mlx` commented out from `[workspace] members` |
| Dev init | New script: `scripts/init_qwen3_models.sh` |
| Preferences | Default backend changed to `qwen3_tts_mlx`; default voice changed to `vivian` |

---

## Files Modified

### `apps/moxin-voice/src/voice_data.rs`
- `get_builtin_voices()` (PrimeSpeech voices) wrapped in `/* ... */` block comment.
- `get_builtin_voices_for_backend()` simplified: always calls `get_qwen_builtin_voices(locale)`.

### `apps/moxin-voice/src/app_preferences.rs`
- Default `inference_backend` → `"qwen3_tts_mlx"`
- Default `zero_shot_backend` → `"qwen3_tts_mlx"`
- Default `training_backend` → `"option_c"` (Qwen3 ICL mode)
- Default `default_voice_id` → `"vivian"`
- Storage path kept at `~/.dora/primespeech/app_preferences.json` for backward compat.

### `apps/moxin-voice/src/screen.rs`
- `get_project_tts_models()` → only `qwen3_tts_mlx` entry.
- `normalize_inference_backend()` / `normalize_zero_shot_backend()` → always return `"qwen3_tts_mlx"`.
- Startup: inference/zero_shot/training backends forced to qwen3 regardless of stored pref.
- `update_user_settings_page()`: `zero_shot_backend_pick_row` and `backend_pick_row` hidden via `set_visible(false)`.
- Event handlers for `zero_shot_backend_dropdown` and `training_backend_dropdown` removed.
- `advanced_mode_btn` click handler removed (Pro mode unreachable).
- `mode_selector` view hidden (both Quick/Advanced tabs hidden since only Express exists).
- `model_row` in TTS page hidden (no backend choice to display).
- History card label: `"PrimeSpeech (GPT-SoVITS v2)"` → `"Qwen3 TTS MLX"`.

### `apps/moxin-voice/src/voice_clone_modal.rs`
- `CloneMode` enum doc updated; `Pro` variant kept (unreachable via UI).
- `switch_to_mode()` doc updated with Qwen3-only refactor note.
- All Pro mode code preserved unchanged (unreachable since mode_selector is hidden).

### `apps/moxin-voice/dataflow/tts.yml`
- Default `VOICE_NAME` env changed from `"Doubao"` to `"vivian"`.

### `scripts/dataflow/tts.bundle.yml`
- Default `VOICE_NAME` env changed from `"Doubao"` to `"vivian"`.

### `scripts/run_tts_backend.sh`
- Rewritten: always launches `qwen-tts-node`; PrimeSpeech `case` branch removed with comment.

### `scripts/macos_run_tts_backend.sh`
- Rewritten: always launches `qwen-tts-node`; PrimeSpeech resolver removed with comment.

### `scripts/macos_bootstrap.sh`
- `INFERENCE_BACKEND` / `ZERO_SHOT_BACKEND` hardcoded to `qwen3_tts_mlx`.
- `NEED_QWEN_CUSTOM` / `NEED_QWEN_BASE` always set to 1.
- Step 6: PrimeSpeech node install skipped (comment left).
- Step 7: Only FunASR downloaded; PrimeSpeech model download removed.
- Step 8: PrimeSpeech model conversion skipped.

### `scripts/macos_preflight.sh`
- Rewritten: Qwen3 model checks are **required** (not conditional).
- `moxin-tts-node` binary check removed.
- `dora-primespeech` Python package check removed (warning only → removed).
- PrimeSpeech MLX model checks (HuBERT, BERT, voices) removed.

### `scripts/build_macos_app.sh`
- `dora-primespeech-mlx` Cargo build removed (comment left).
- `moxin-tts-node` binary existence check removed.
- `moxin-tts-node` bundle copy + chmod removed.

### `Cargo.toml`
- `"node-hub/dora-primespeech-mlx"` workspace member commented out.

---

## New Files

### `scripts/init_qwen3_models.sh`
Dev-time helper to download Qwen3 CustomVoice and Base models on first setup.

```bash
bash scripts/init_qwen3_models.sh
```

---

## How to Restore PrimeSpeech

To bring back PrimeSpeech Python and/or PrimeSpeech MLX:

1. **`voice_data.rs`** — un-comment `get_builtin_voices()` and restore the `else` branch in `get_builtin_voices_for_backend()`.

2. **`app_preferences.rs`** — revert default `inference_backend` / `zero_shot_backend` / `default_voice_id`.

3. **`screen.rs`**:
   - Restore `get_project_tts_models()` with both entries.
   - Restore `normalize_inference_backend()` / `normalize_zero_shot_backend()` conditionals.
   - Remove forced `"qwen3_tts_mlx"` overrides at startup.
   - Restore `zero_shot_backend_pick_row` / `backend_pick_row` visibility in `update_user_settings_page()`.
   - Restore the two dropdown event handlers.
   - Restore `advanced_mode_btn` click handler.
   - Restore `mode_selector` visibility.
   - Restore `model_row` visibility in `sync_selected_model_ui()`.

4. **`voice_clone_modal.rs`** — No code changes needed; Pro mode code is fully intact.

5. **Scripts** — Restore `run_tts_backend.sh` / `macos_run_tts_backend.sh` / `macos_bootstrap.sh` / `macos_preflight.sh` to multi-backend logic.

6. **`build_macos_app.sh`** — Un-comment primespeech-mlx build and moxin-tts-node bundling.

7. **`Cargo.toml`** — Un-comment `"node-hub/dora-primespeech-mlx"`.

---

## Why Qwen3-Only

- PrimeSpeech Python node requires a Conda env + several GB of PyTorch models.
- PrimeSpeech MLX is still experimental and requires the gpt-sovits-mlx patch.
- Qwen3-TTS-MLX covers both preset voices and ICL zero-shot cloning on Apple Silicon.
- Simpler distribution: no Python bootstrap, no training dependencies, no multi-backend dispatch.
