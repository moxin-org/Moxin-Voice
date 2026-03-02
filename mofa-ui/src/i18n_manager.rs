use parking_lot::RwLock;
use std::sync::Arc;

/// Manages internationalization (i18n) for the application.
/// Handles language switching, translation loading, and fallback mechanisms.
pub struct I18nManager {
    current_language: Arc<RwLock<String>>,
}

impl I18nManager {
    /// Create a new I18nManager with the default language
    pub fn new() -> Self {
        log::info!("[i18n] Initializing I18nManager with default language: en");
        Self {
            current_language: Arc::new(RwLock::new("en".to_string())),
        }
    }

    /// Create a new I18nManager with system locale detection
    pub fn with_system_locale() -> Self {
        let detected_lang = Self::detect_system_locale();
        log::info!("[i18n] Creating I18nManager with detected system locale: {}", detected_lang);
        let manager = Self::new();
        manager.set_language(&detected_lang);
        manager
    }

    /// Detect the system locale and return a supported language code
    fn detect_system_locale() -> String {
        let detected = sys_locale::get_locale();
        Self::map_locale_to_supported(detected.as_deref())
    }

    fn map_locale_to_supported(locale: Option<&str>) -> String {
        if let Some(locale) = locale {
            log::debug!("[i18n] System locale detected: {}", locale);
            // Convert locale to language code
            if locale.starts_with("zh") {
                log::info!("[i18n] Mapping locale '{}' to language code: zh-CN", locale);
                return "zh-CN".to_string();
            } else if locale.starts_with("en") {
                log::info!("[i18n] Mapping locale '{}' to language code: en", locale);
                return "en".to_string();
            }
            log::warn!("[i18n] Unsupported locale '{}', falling back to English", locale);
        } else {
            log::warn!("[i18n] Failed to detect system locale, falling back to English");
        }
        // Default to English if detection fails
        "en".to_string()
    }

    /// Get the current language code
    pub fn current_language(&self) -> String {
        self.current_language.read().clone()
    }

    /// Set the current language
    pub fn set_language(&self, lang: &str) {
        let previous_lang = self.current_language.read().clone();
        if previous_lang != lang {
            log::info!("[i18n] Switching language from '{}' to '{}'", previous_lang, lang);
            *self.current_language.write() = lang.to_string();
            rust_i18n::set_locale(lang);
            log::info!("[i18n] Language switch completed successfully");
        } else {
            log::debug!("[i18n] Language already set to '{}', no change needed", lang);
        }
    }

    /// Get a translated string for the given key
    pub fn t(&self, key: &str) -> String {
        let current = self.current_language();
        let mut translation = rust_i18n::t!(key, locale = &current).to_string();

        // Fallback to English when current locale has no translation for an existing key.
        if translation == key && current != "en" {
            let english = rust_i18n::t!(key, locale = "en").to_string();
            if english != key {
                translation = english;
            }
        }

        log::trace!("[i18n] Translation for key '{}': '{}'", key, translation);
        translation
    }

    /// Get a translated string with arguments
    pub fn t_with_args(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut result = self.t(key);
        for (placeholder, value) in args {
            result = result.replace(&format!("{{{}}}", placeholder), value);
        }
        log::trace!("[i18n] Translation with args for key '{}': '{}'", key, result);
        result
    }
}

impl Default for I18nManager {
    fn default() -> Self {
        Self::with_system_locale()
    }
}

#[cfg(test)]
mod tests {
    use super::I18nManager;

    #[test]
    fn language_switch_updates_translations_immediately() {
        let manager = I18nManager::new();
        manager.set_language("en");
        assert_eq!(manager.t("settings.page.title"), "Settings");

        manager.set_language("zh-CN");
        assert_eq!(manager.t("settings.page.title"), "设置");
    }

    #[test]
    fn fallback_to_english_for_unsupported_locale() {
        let manager = I18nManager::new();
        manager.set_language("fr");
        assert_eq!(manager.t("settings.page.back"), "Back");
    }

    #[test]
    fn missing_translation_returns_key() {
        let manager = I18nManager::new();
        manager.set_language("zh-CN");
        assert_eq!(
            manager.t("nonexistent.translation.key"),
            "nonexistent.translation.key"
        );
    }

    #[test]
    fn locale_mapping_prefers_supported_languages() {
        assert_eq!(I18nManager::map_locale_to_supported(Some("zh_CN")), "zh-CN");
        assert_eq!(I18nManager::map_locale_to_supported(Some("en_US")), "en");
        assert_eq!(I18nManager::map_locale_to_supported(Some("fr_FR")), "en");
        assert_eq!(I18nManager::map_locale_to_supported(None), "en");
    }
}
