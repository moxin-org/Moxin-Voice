## 1. Setup i18n Infrastructure

- [x] 1.1 Add `rust-i18n` and `sys-locale` dependencies to workspace `Cargo.toml`
- [x] 1.2 Create `locales/` directory structure with `en/` and `zh-CN/` subdirectories
- [x] 1.3 Create initial YAML translation files (common.yml, tts.yml, settings.yml, voice_clone.yml) for English
- [x] 1.4 Initialize `rust-i18n` in the project with proper configuration

## 2. Implement I18n Manager

- [x] 2.1 Create `I18nManager` struct in `mofa-ui/src/i18n_manager.rs`
- [x] 2.2 Implement translation loading and caching logic
- [x] 2.3 Add language switching method with runtime update support
- [x] 2.4 Implement fallback mechanism for missing translations
- [x] 2.5 Add system locale detection on first launch
- [x] 2.6 Integrate `I18nManager` into `MofaAppData` in `mofa-ui/src/app_data.rs`

## 3. Implement Language Preference Persistence

- [x] 3.1 Extend `voice_persistence.rs` to support language preference storage
- [x] 3.2 Create `preferences.json` structure in `~/.moxin-tts/`
- [x] 3.3 Implement save/load methods for language preference
- [x] 3.4 Add preference loading on application startup
- [x] 3.5 Handle migration from non-existent preference file gracefully

## 4. Create Settings Screen Component

- [x] 4.1 Create `apps/mofa-tts/src/settings_screen.rs` file
- [x] 4.2 Define `SettingsScreen` widget using Makepad's `live_design!` macro
- [x] 4.3 Implement language selector UI with radio buttons or dropdown
- [x] 4.4 Add visual indication for currently selected language
- [x] 4.5 Implement language switch handler that updates `I18nManager`
- [x] 4.6 Add navigation back button or close mechanism

## 5. Integrate Settings into Default Layout

- [x] 5.1 Add settings navigation button/tab to `apps/mofa-tts/src/screen.rs`
- [x] 5.2 Implement screen switching logic to show/hide settings screen
- [x] 5.3 Update event handlers to route settings actions
- [ ] 5.4 Test settings access and navigation in default layout

## 6. Integrate Settings into MoYoYo Layout

- [x] 6.1 Add settings sidebar item to `apps/mofa-tts/src/screen_moyoyo.rs`
- [x] 6.2 Implement conditional rendering for settings screen in sidebar navigation
- [x] 6.3 Update event handlers for MoYoYo layout settings navigation
- [ ] 6.4 Test settings access and navigation in MoYoYo layout

## 7. Extract and Translate UI Strings

- [x] 7.1 Audit `screen.rs` and extract all hardcoded English strings to `locales/en/tts.yml`
- [x] 7.2 Audit `screen_moyoyo.rs` and extract strings to `locales/en/tts.yml`
- [x] 7.3 Audit `voice_clone_modal.rs` and extract strings to `locales/en/voice_clone.yml`
- [x] 7.4 Audit `mofa-widgets/` components and extract strings to `locales/en/common.yml`
- [x] 7.5 Create settings page strings in `locales/en/settings.yml`
- [x] 7.6 Generate Chinese translations for all YAML files in `locales/zh-CN/`

## 8. Update Widgets to Use Translations

- [x] 8.1 Update `screen.rs` to use `app_data.i18n().t()` for all UI text
- [x] 8.2 Update `screen_moyoyo.rs` to use translation calls
- [x] 8.3 Update `voice_clone_modal.rs` to use translation calls
- [x] 8.4 Update `voice_selector.rs` to use translation calls
- [x] 8.5 Update `audio_recorder.rs` and other widgets in `mofa-widgets/` to use translations
- [x] 8.6 Ensure all `set_text()` calls use translated strings

## 9. Implement Runtime Language Switching

- [x] 9.1 Add global event for language change notification
- [x] 9.2 Implement widget refresh mechanism when language changes
- [x] 9.3 Update all screens to listen for language change events
- [x] 9.4 Test that all UI text updates immediately on language switch

## 10. Testing and Validation

- [ ] 10.1 Test language switching in default layout
- [ ] 10.2 Test language switching in MoYoYo layout
- [x] 10.3 Verify language preference persists across app restarts
- [x] 10.4 Test system locale detection on first launch
- [x] 10.5 Verify fallback to English for missing translations
- [ ] 10.6 Test that long Chinese strings fit in UI elements
- [ ] 10.7 Verify all screens and modals display correctly in both languages
- [x] 10.8 Test edge cases (missing keys, corrupted preference file)

## 11. Documentation and Polish

- [x] 11.1 Add comments to translation YAML files for context
- [x] 11.2 Document i18n system in project README
- [x] 11.3 Create translation guide for adding new languages
- [x] 11.4 Add logging for translation loading and language switching
- [ ] 11.5 Review and refine Chinese translations with native speaker
