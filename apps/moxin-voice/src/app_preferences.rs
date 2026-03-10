//! Local user preferences for moxin-voice.
//!
//! Stored at: ~/.dora/primespeech/app_preferences.json

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct AppPreferences {
    pub display_name: String,
    pub avatar_letter: String,
    pub default_voice_id: Option<String>,
    pub default_speed: f64,
    pub default_pitch: f64,
    pub default_volume: f64,
    pub history_retention_days: i64, // -1 = forever
    pub training_backend: String,    // option_a | option_b
    pub preferred_output_device: Option<String>,
    pub preferred_input_device: Option<String>,
    pub debug_logs_enabled: bool,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            display_name: "User".to_string(),
            avatar_letter: "U".to_string(),
            default_voice_id: Some("Doubao".to_string()),
            default_speed: 1.0,
            default_pitch: 0.0,
            default_volume: 100.0,
            history_retention_days: -1,
            training_backend: "option_a".to_string(),
            preferred_output_device: None,
            preferred_input_device: None,
            debug_logs_enabled: false,
        }
    }
}

fn primespeech_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".dora").join("primespeech")
}

pub fn preferences_path() -> PathBuf {
    primespeech_dir().join("app_preferences.json")
}

pub fn load_preferences() -> AppPreferences {
    let path = preferences_path();
    if !path.exists() {
        return AppPreferences::default();
    }
    match fs::read_to_string(&path) {
        Ok(content) => serde_json::from_str::<AppPreferences>(&content).unwrap_or_default(),
        Err(_) => AppPreferences::default(),
    }
}

pub fn save_preferences(prefs: &AppPreferences) -> Result<(), String> {
    let dir = primespeech_dir();
    if !dir.exists() {
        fs::create_dir_all(&dir)
            .map_err(|e| format!("Failed to create preferences directory {:?}: {}", dir, e))?;
    }
    let json = serde_json::to_string_pretty(prefs)
        .map_err(|e| format!("Failed to serialize preferences: {}", e))?;
    fs::write(preferences_path(), json).map_err(|e| format!("Failed to write preferences: {}", e))
}
