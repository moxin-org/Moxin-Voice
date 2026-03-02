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
        Self {
            current_language: Arc::new(RwLock::new("en".to_string())),
        }
    }

    /// Create a new I18nManager with system locale detection
    pub fn with_system_locale() -> Self {
        let detected_lang = Self::detect_system_locale();
        let manager = Self::new();
        manager.set_language(&detected_lang);
        manager
    }

    /// Detect the system locale and return a supported language code
    fn detect_system_locale() -> String {
        if let Some(locale) = sys_locale::get_locale() {
            // Convert locale to language code
            if locale.starts_with("zh") {
                return "zh-CN".to_string();
            } else if locale.starts_with("en") {
                return "en".to_string();
            }
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
        *self.current_language.write() = lang.to_string();
        rust_i18n::set_locale(lang);
    }

    /// Get a translated string for the given key
    pub fn t(&self, key: &str) -> String {
        rust_i18n::t!(key).to_string()
    }

    /// Get a translated string with arguments
    pub fn t_with_args(&self, key: &str, args: &[(&str, &str)]) -> String {
        let mut result = rust_i18n::t!(key).to_string();
        for (placeholder, value) in args {
            result = result.replace(&format!("{{{}}}", placeholder), value);
        }
        result
    }
}

impl Default for I18nManager {
    fn default() -> Self {
        Self::with_system_locale()
    }
}
