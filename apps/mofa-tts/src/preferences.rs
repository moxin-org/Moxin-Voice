//! Application preferences persistence
//!
//! Preferences are stored in: ~/.moxin-tts/preferences.json

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Application preferences
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct AppPreferences {
    /// Preferences version for future compatibility
    pub version: String,
    /// Selected language code (e.g., "en", "zh-CN")
    pub language: String,
    /// Last updated timestamp
    #[serde(skip_serializing_if = "Option::is_none")]
    pub last_updated: Option<String>,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            language: "en".to_string(),
            last_updated: None,
        }
    }
}

/// Get the application preferences directory
pub fn get_preferences_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".moxin-tts")
}

/// Get the preferences file path
pub fn get_preferences_path() -> PathBuf {
    get_preferences_dir().join("preferences.json")
}

/// Ensure preferences directory exists
pub fn ensure_preferences_dir() -> std::io::Result<()> {
    let prefs_dir = get_preferences_dir();
    if !prefs_dir.exists() {
        fs::create_dir_all(&prefs_dir)?;
    }
    Ok(())
}

/// Load application preferences
pub fn load_preferences() -> AppPreferences {
    load_preferences_from_path(&get_preferences_path())
}

fn load_preferences_from_path(prefs_path: &PathBuf) -> AppPreferences {
    if !prefs_path.exists() {
        return AppPreferences::default();
    }

    match fs::read_to_string(&prefs_path) {
        Ok(content) => match serde_json::from_str::<AppPreferences>(&content) {
            Ok(prefs) => prefs,
            Err(e) => {
                log::warn!("Failed to parse preferences, using defaults: {}", e);
                AppPreferences::default()
            }
        },
        Err(e) => {
            log::warn!("Failed to read preferences, using defaults: {}", e);
            AppPreferences::default()
        }
    }
}

/// Save application preferences
pub fn save_preferences(prefs: &AppPreferences) -> Result<(), String> {
    ensure_preferences_dir().map_err(|e| format!("Failed to create preferences directory: {}", e))?;
    save_preferences_to_path(prefs, &get_preferences_path())
}

fn save_preferences_to_path(prefs: &AppPreferences, prefs_path: &PathBuf) -> Result<(), String> {
    let json = serde_json::to_string_pretty(prefs)
        .map_err(|e| format!("Failed to serialize preferences: {}", e))?;

    fs::write(&prefs_path, json)
        .map_err(|e| format!("Failed to write preferences: {}", e))?;

    Ok(())
}

/// Load the saved language preference
pub fn load_language_preference() -> String {
    load_preferences().language
}

/// Save the language preference
pub fn save_language_preference(language: &str) -> Result<(), String> {
    let mut prefs = load_preferences();
    prefs.language = language.to_string();
    prefs.last_updated = Some(chrono::Utc::now().to_rfc3339());
    save_preferences(&prefs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn make_temp_file_path(name: &str) -> PathBuf {
        let mut dir = std::env::temp_dir();
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before epoch")
            .as_nanos();
        dir.push(format!("mofa_prefs_test_{}_{}", name, nanos));
        dir
    }

    fn cleanup_temp_file(path: &PathBuf) {
        if path.exists() {
            let _ = fs::remove_file(path);
        }
    }

    #[test]
    fn test_default_preferences() {
        let prefs = AppPreferences::default();
        assert_eq!(prefs.version, "1.0");
        assert_eq!(prefs.language, "en");
    }

    #[test]
    fn test_load_preferences_missing_file_returns_default() {
        let path = make_temp_file_path("missing");
        cleanup_temp_file(&path);

        let prefs = load_preferences_from_path(&path);
        assert_eq!(prefs.language, "en");
        assert_eq!(prefs.version, "1.0");
    }

    #[test]
    fn test_load_preferences_corrupted_file_returns_default() {
        let path = make_temp_file_path("corrupted");
        fs::write(&path, "not-json").expect("write corrupted preferences");

        let prefs = load_preferences_from_path(&path);
        assert_eq!(prefs.language, "en");
        assert_eq!(prefs.version, "1.0");

        cleanup_temp_file(&path);
    }

    #[test]
    fn test_save_and_load_preferences_roundtrip() {
        let path = make_temp_file_path("roundtrip");
        cleanup_temp_file(&path);

        let prefs = AppPreferences {
            version: "1.0".to_string(),
            language: "zh-CN".to_string(),
            last_updated: Some("2026-03-02T10:30:00Z".to_string()),
        };

        save_preferences_to_path(&prefs, &path).expect("save preferences");
        let loaded = load_preferences_from_path(&path);
        assert_eq!(loaded.language, "zh-CN");
        assert_eq!(loaded.version, "1.0");
        assert_eq!(loaded.last_updated, Some("2026-03-02T10:30:00Z".to_string()));

        cleanup_temp_file(&path);
    }
}
