# Translation Guide

This guide explains how to add new language translations to Moxin TTS.

## Overview

Moxin TTS uses the `rust-i18n` crate for internationalization. Translations are stored in YAML files organized by language and feature area.

## Translation File Structure

```
locales/
├── en/                    # English (default)
│   ├── common.yml         # Shared UI strings
│   ├── tts.yml            # TTS screen strings
│   ├── settings.yml       # Settings page strings
│   └── voice_clone.yml    # Voice cloning modal strings
├── zh-CN/                 # Simplified Chinese
│   ├── common.yml
│   ├── tts.yml
│   ├── settings.yml
│   └── voice_clone.yml
└── [new-language]/        # Your new language
    ├── common.yml
    ├── tts.yml
    ├── settings.yml
    └── voice_clone.yml
```

## Adding a New Language

### Step 1: Create Language Directory

Create a new directory under `locales/` using the appropriate language code:

```bash
mkdir -p locales/[language-code]/
```

**Common language codes:**
- `en` - English
- `zh-CN` - Simplified Chinese
- `zh-TW` - Traditional Chinese
- `ja` - Japanese
- `ko` - Korean
- `es` - Spanish
- `fr` - French
- `de` - German
- `pt-BR` - Brazilian Portuguese
- `ru` - Russian

### Step 2: Copy Template Files

Copy the English translation files as templates:

```bash
cp locales/en/*.yml locales/[language-code]/
```

### Step 3: Translate Strings

Edit each YAML file and translate the strings. Keep the structure and keys unchanged, only translate the values.

**Example:**

```yaml
# English (locales/en/common.yml)
buttons:
  ok: "OK"
  cancel: "Cancel"
  save: "Save"

# Spanish (locales/es/common.yml)
buttons:
  ok: "Aceptar"
  cancel: "Cancelar"
  save: "Guardar"
```

### Step 4: Update Language Settings

Add your language to the settings configuration:

1. Open `locales/en/settings.yml`
2. Add your language to the `languages` section:

```yaml
languages:
  en: "English"
  zh_cn: "中文 (简体)"
  es: "Español"  # Your new language
```

3. Do the same in all other language files, including your new one.

### Step 5: Update I18n Manager

Add your language to the I18n Manager:

1. Open `mofa-ui/src/i18n_manager.rs`
2. Add your language code to the supported languages list (if needed)

### Step 6: Test Your Translation

1. Build the application:
   ```bash
   cargo build -p moxin-tts
   ```

2. Run the application:
   ```bash
   cargo run -p moxin-tts
   ```

3. Navigate to Settings and select your new language
4. Verify all UI text displays correctly
5. Check that long strings fit in UI elements
6. Test language switching back and forth

## Translation Guidelines

### General Principles

1. **Consistency**: Use consistent terminology throughout
2. **Context**: Consider the UI context when translating
3. **Length**: Try to keep translations similar in length to avoid UI overflow
4. **Native Feel**: Translate for meaning, not word-for-word
5. **Technical Terms**: Keep technical terms recognizable (e.g., "TTS", "GPU", "ASR")

### Specific Guidelines

#### Button Labels
- Keep button labels short and action-oriented
- Use imperative mood (e.g., "Save", "Cancel", not "Saving", "Canceling")

#### Placeholders
- Make placeholders helpful and concise
- Use ellipsis (...) to indicate expected input

#### Error Messages
- Be clear and specific about what went wrong
- Suggest solutions when possible

#### Status Messages
- Use present continuous for ongoing actions (e.g., "Loading...", "Processing...")
- Use past tense for completed actions (e.g., "Completed", "Failed")

### Special Considerations

#### Emoji and Icons
- Keep emoji in translations (🎙, ⚙, 📝, etc.)
- They provide visual consistency across languages

#### Interpolation
- Preserve interpolation variables: `{current}`, `{max}`, `{name}`, etc.
- Example: `"{current} / {max} characters"` → `"{current} / {max} 字符"`

#### Language-Specific Text
- Some keys have language-specific variants (e.g., `placeholder` vs `placeholder_zh`)
- Translate the appropriate variant for your language

## File-by-File Guide

### common.yml
Contains shared UI strings used across the application:
- Button labels (OK, Cancel, Save, etc.)
- Common labels (Loading, Error, Success, etc.)
- Status indicators (Ready, Processing, Completed, etc.)

### tts.yml
Contains strings for the main TTS interface:
- Screen titles and navigation
- Text input placeholders
- Voice selection labels
- Control buttons (Generate, Play, Export, etc.)

### settings.yml
Contains strings for the settings page:
- Page title and navigation
- Language selection labels
- Feedback messages

### voice_clone.yml
Contains strings for the voice cloning modal:
- Modal title and mode descriptions
- Audio upload/recording instructions
- Training progress messages
- Warning messages

## Testing Checklist

- [ ] All UI text displays in the new language
- [ ] No missing translations (no English fallbacks)
- [ ] Long strings fit in UI elements without overflow
- [ ] Language switching works correctly
- [ ] Language preference persists across app restarts
- [ ] Special characters display correctly
- [ ] Pluralization works correctly (if applicable)
- [ ] Date/time formats are appropriate (if applicable)

## Common Issues

### Issue: Strings not updating after translation
**Solution**: Rebuild the application. Translations are compiled into the binary.

### Issue: Some strings still in English
**Solution**: Check that the translation key exists in your YAML file and matches exactly.

### Issue: UI layout breaks with long translations
**Solution**: Try to shorten the translation or report the issue for UI adjustment.

### Issue: Special characters not displaying
**Solution**: Ensure your YAML file is saved with UTF-8 encoding.

## Getting Help

If you encounter issues or have questions:

1. Check existing translations in `locales/en/` and `locales/zh-CN/` for examples
2. Review the comments in YAML files for context
3. Open an issue on GitHub with the `translation` label
4. Join the community discussion for translation help

## Contributing Your Translation

Once your translation is complete and tested:

1. Commit your changes:
   ```bash
   git add locales/[language-code]/
   git commit -m "feat: add [Language Name] translation"
   ```

2. Push to your fork and create a pull request

3. In the PR description, include:
   - Language name and code
   - Native speaker verification status
   - Any UI issues encountered
   - Screenshots of the translated UI (optional but helpful)

Thank you for contributing to Moxin TTS! 🎉
