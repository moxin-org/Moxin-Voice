## Context

Moxin TTS is a Rust-based desktop application using the Makepad UI framework. The application currently has all UI strings hardcoded in English, limiting its accessibility to international users. The codebase supports two UI layouts (default MoFA style and MoYoYo style) through feature flags, and both need to support multi-language.

The application uses:
- Makepad for GPU-accelerated UI rendering with `live_design!` macros
- Rust 2021 edition
- Persistent storage for user preferences (voice data, etc.)
- Two UI layout modes switchable via Cargo features

## Goals / Non-Goals

**Goals:**
- Implement a flexible i18n system that works with Makepad's live_design! macro system
- Create a settings page accessible from both UI layouts
- Support English and Chinese (Simplified) initially
- Persist language preferences across sessions
- Enable runtime language switching without application restart
- Provide a foundation for adding more languages in the future

**Non-Goals:**
- Right-to-left (RTL) language support in initial implementation
- Automatic translation of user-generated content (voice names, custom text)
- Translation of Python backend logs or error messages
- Dynamic language pack downloading (all translations bundled with app)

## Decisions

### Decision 1: i18n Library Choice

**Choice:** Use `rust-i18n` crate

**Rationale:**
- Lightweight and simple API suitable for desktop apps
- Supports YAML translation files with nested keys
- Compile-time translation loading (no runtime file I/O)
- Macro-based API (`t!("key")`) works well with Rust
- Good support for pluralization and interpolation

**Alternatives Considered:**
- **Fluent**: More powerful but complex, designed for Mozilla's needs, heavier dependency
- **Custom solution**: Would require significant development time and maintenance
- **gettext**: C-based, less idiomatic for Rust, more complex tooling

### Decision 2: Translation File Structure

**Choice:** YAML files organized by feature area

```
locales/
  en/
    common.yml       # Shared strings (buttons, labels)
    tts.yml          # TTS screen strings
    settings.yml     # Settings page strings
    voice_clone.yml  # Voice cloning modal strings
  zh-CN/
    common.yml
    tts.yml
    settings.yml
    voice_clone.yml
```

**Rationale:**
- YAML is human-readable and easy to edit
- Feature-based organization makes translations manageable
- Nested keys support logical grouping (e.g., `settings.language.title`)
- Separate files prevent merge conflicts when multiple translators work in parallel

### Decision 3: Settings Page Integration

**Choice:** Create new `SettingsScreen` component with conditional rendering for both UI layouts

**Rationale:**
- Maintains consistency with existing architecture (separate screen components)
- Can reuse existing navigation patterns from both layouts
- Allows settings to grow beyond just language (future: audio settings, model paths, etc.)
- Clean separation of concerns

**Implementation:**
- Add `settings_screen.rs` in `apps/mofa-tts/src/`
- Integrate into `screen.rs` (default layout) via tab or button
- Integrate into `screen_moyoyo.rs` via sidebar navigation item

### Decision 4: Language Preference Storage

**Choice:** Extend existing `voice_persistence.rs` mechanism to store language preference

**Rationale:**
- Reuses proven persistence infrastructure
- Consistent with how app already stores user data
- Simple JSON-based storage in user's home directory
- Easy to migrate or extend in future

**Storage location:** `~/.moxin-tts/preferences.json`
```json
{
  "language": "zh-CN",
  "last_updated": "2026-03-02T10:30:00Z"
}
```

### Decision 5: Makepad Integration Strategy

**Choice:** Create a global `I18nState` in `MofaAppData` and use accessor methods in widgets

**Rationale:**
- Makepad's `live_design!` macros don't support dynamic string interpolation
- Need to pass translated strings as widget properties
- Global state ensures all widgets access same language setting
- Accessor pattern: `app_data.i18n().t("key")` provides clean API

**Implementation approach:**
```rust
// In mofa-ui/src/app_data.rs
pub struct MofaAppData {
    // ... existing fields
    i18n: I18nManager,
}

// In widgets
let title = app_data.i18n().t("tts.title");
self.label(id!(title_label)).set_text(&title);
```

### Decision 6: Language Detection

**Choice:** Use system locale as default, with explicit fallback to English

**Rationale:**
- Better UX: users see their language immediately on first launch
- `sys-locale` crate provides reliable cross-platform locale detection
- Graceful degradation: if system locale not supported, use English
- User can always override via settings

## Risks / Trade-offs

**Risk:** Makepad's live_design! macros may not support dynamic text well
→ **Mitigation:** Use programmatic text setting via `set_text()` methods after widget creation. This is already used in the codebase for dynamic content.

**Risk:** Translation quality for Chinese may be inconsistent
→ **Mitigation:** Start with machine translation, then iterate with native speaker review. Mark strings needing review with `# TODO: review` comments in YAML.

**Risk:** Adding i18n may increase binary size
→ **Mitigation:** `rust-i18n` compiles translations into binary, but YAML is compact. Estimated increase: <100KB for 2 languages. Acceptable for desktop app.

**Risk:** Existing UI layouts may not accommodate longer translated strings
→ **Mitigation:** Test with longest expected translations (German often longest). Use ellipsis truncation where needed. Settings page has flexible layout.

**Trade-off:** Compile-time vs runtime translation loading
- **Chosen:** Compile-time (translations bundled in binary)
- **Benefit:** No file I/O, faster startup, no missing file errors
- **Cost:** Need recompile to update translations, larger binary
- **Justification:** Desktop app context makes this acceptable; reliability > flexibility

## Migration Plan

### Phase 1: Infrastructure (Week 1)
1. Add `rust-i18n` dependency to `Cargo.toml`
2. Create `locales/` directory structure
3. Add `I18nManager` to `MofaAppData`
4. Implement language preference persistence

### Phase 2: Settings Page (Week 1-2)
1. Create `SettingsScreen` component
2. Implement language selector UI
3. Integrate into both UI layouts (default and MoYoYo)
4. Add navigation to settings from main screens

### Phase 3: Translation (Week 2-3)
1. Extract all hardcoded English strings from codebase
2. Create English YAML files (baseline)
3. Generate Chinese translations
4. Update all widgets to use `t!()` macro or `i18n().t()` calls

### Phase 4: Testing & Polish (Week 3-4)
1. Test language switching in both UI layouts
2. Verify text fits in all UI elements
3. Test persistence across app restarts
4. Handle edge cases (missing keys, fallbacks)

### Rollback Strategy
- Feature can be disabled by reverting to hardcoded English strings
- Language preference file is non-critical; app works without it
- No database migrations or breaking changes to existing features

## Open Questions

1. **Should we support language switching per-session or system-wide?**
   - Current design: per-application preference
   - Alternative: respect system locale changes at runtime
   - **Decision needed:** Clarify with user testing

2. **How to handle Python backend messages (Dora nodes)?**
   - Current design: Python logs remain in English
   - Future: Could add i18n to Python nodes if needed
   - **Decision:** Defer to future iteration

3. **Should settings page include other preferences?**
   - Current design: Language only initially
   - Future: Audio device selection, model paths, theme preferences
   - **Decision:** Start minimal, expand based on user feedback
