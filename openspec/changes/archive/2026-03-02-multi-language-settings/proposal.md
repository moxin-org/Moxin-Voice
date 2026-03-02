## Why

Moxin TTS currently only supports a single language interface, limiting its accessibility to non-English speaking users. As the application targets a global audience for voice synthesis and cloning, providing multi-language support is essential for user adoption and usability across different regions.

## What Changes

- Add internationalization (i18n) infrastructure to support multiple UI languages
- Create a settings page with language selection interface
- Implement language switching mechanism that persists user preferences
- Translate all UI strings into multiple languages (starting with English and Chinese)
- Add language detection and fallback mechanisms

## Capabilities

### New Capabilities
- `language-settings`: Settings page UI component for language selection and preferences management
- `i18n-system`: Internationalization infrastructure including translation loading, language switching, and string localization

### Modified Capabilities
<!-- No existing capabilities are being modified at the requirements level -->

## Impact

**Affected Code:**
- `apps/mofa-tts/src/screen.rs` - Main TTS screen will need i18n integration
- `apps/mofa-tts/src/screen_moyoyo.rs` - MoYoYo UI screen will need i18n integration
- `mofa-widgets/` - UI components will need localized strings
- New settings screen component will be added

**New Dependencies:**
- i18n library for Rust (e.g., `fluent`, `rust-i18n`, or custom solution)
- Translation files storage structure

**User Impact:**
- Users can select their preferred language from settings
- All UI text will be displayed in the selected language
- Language preference persists across sessions
