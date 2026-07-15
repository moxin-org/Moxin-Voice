//! Local user preferences for moxin-voice.
//!
//! Stored at: ~/.dora/primespeech/app_preferences.json
//! (path kept for backward compatibility with existing installations)

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(default)]
pub struct AppPreferences {
    pub app_language: String, // "en" | "zh"
    pub display_name: String,
    pub avatar_letter: String,
    pub last_seen_app_version: Option<String>,
    pub default_voice_id: Option<String>,
    pub default_speed: f64,
    pub default_pitch: f64,
    pub default_volume: f64,
    pub history_retention_days: i64, // -1 = forever
    pub inference_backend: String,   // primespeech_mlx | qwen3_tts_mlx
    pub zero_shot_backend: String,   // primespeech_mlx | qwen3_tts_mlx
    pub training_backend: String,    // option_a | option_b
    pub preferred_output_device: Option<String>,
    pub preferred_input_device: Option<String>,
    pub tts_download_format: String, // "mp3" | "wav"
    pub translation_auto_save_transcript: bool,
    pub translation_periodic_save_transcript: bool,
    pub translation_transcript_file_name: String,
    pub translation_transcript_save_dir: Option<String>,
    pub debug_logs_enabled: bool,
}

impl Default for AppPreferences {
    fn default() -> Self {
        Self {
            app_language: "en".to_string(),
            display_name: "User".to_string(),
            avatar_letter: "U".to_string(),
            last_seen_app_version: None,
            default_voice_id: Some("vivian".to_string()),
            default_speed: 1.0,
            default_pitch: 0.0,
            default_volume: 100.0,
            history_retention_days: -1,
            inference_backend: "qwen3_tts_mlx".to_string(),
            zero_shot_backend: "qwen3_tts_mlx".to_string(),
            training_backend: "option_c".to_string(), // Qwen3 x-vector product mode
            preferred_output_device: None,
            preferred_input_device: None,
            tts_download_format: "mp3".to_string(),
            translation_auto_save_transcript: false,
            translation_periodic_save_transcript: false,
            translation_transcript_file_name: "transcript.md".to_string(),
            translation_transcript_save_dir: None,
            debug_logs_enabled: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AppVersionTransition {
    FirstLaunch,
    SameVersion,
    Updated {
        previous: Option<String>,
        current: String,
    },
}

pub fn record_app_version_transition(
    prefs: &mut AppPreferences,
    current_version: &str,
    had_existing_preferences: bool,
) -> AppVersionTransition {
    let current = current_version.trim().to_string();
    let previous = prefs
        .last_seen_app_version
        .as_deref()
        .map(str::trim)
        .filter(|version| !version.is_empty())
        .map(str::to_string);

    prefs.last_seen_app_version = Some(current.clone());

    match previous {
        Some(previous) if previous == current => AppVersionTransition::SameVersion,
        Some(previous) => AppVersionTransition::Updated {
            previous: Some(previous),
            current,
        },
        None if had_existing_preferences => AppVersionTransition::Updated {
            previous: None,
            current,
        },
        None => AppVersionTransition::FirstLaunch,
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn app_version_transition_marks_first_launch_without_reset() {
        let mut prefs = AppPreferences::default();

        let transition = record_app_version_transition(&mut prefs, "0.0.7", false);

        assert_eq!(transition, AppVersionTransition::FirstLaunch);
        assert_eq!(prefs.last_seen_app_version.as_deref(), Some("0.0.7"));
    }

    #[test]
    fn app_version_transition_ignores_same_version() {
        let mut prefs = AppPreferences {
            last_seen_app_version: Some("0.0.7".to_string()),
            ..AppPreferences::default()
        };

        let transition = record_app_version_transition(&mut prefs, "0.0.7", true);

        assert_eq!(transition, AppVersionTransition::SameVersion);
        assert_eq!(prefs.last_seen_app_version.as_deref(), Some("0.0.7"));
    }

    #[test]
    fn app_version_transition_treats_legacy_preferences_as_update() {
        let mut prefs = AppPreferences::default();

        let transition = record_app_version_transition(&mut prefs, "0.0.7", true);

        assert_eq!(
            transition,
            AppVersionTransition::Updated {
                previous: None,
                current: "0.0.7".to_string()
            }
        );
        assert_eq!(prefs.last_seen_app_version.as_deref(), Some("0.0.7"));
    }

    #[test]
    fn app_version_transition_detects_update_once() {
        let mut prefs = AppPreferences {
            last_seen_app_version: Some("0.0.6".to_string()),
            ..AppPreferences::default()
        };

        let transition = record_app_version_transition(&mut prefs, "0.0.7", true);

        assert_eq!(
            transition,
            AppVersionTransition::Updated {
                previous: Some("0.0.6".to_string()),
                current: "0.0.7".to_string()
            }
        );
        assert_eq!(prefs.last_seen_app_version.as_deref(), Some("0.0.7"));

        let transition = record_app_version_transition(&mut prefs, "0.0.7", true);
        assert_eq!(transition, AppVersionTransition::SameVersion);
    }

    #[test]
    fn legacy_preferences_default_tts_download_format_to_mp3() {
        let prefs: AppPreferences = serde_json::from_str(
            r#"{
                "app_language": "zh",
                "display_name": "Alan",
                "avatar_letter": "A"
            }"#,
        )
        .unwrap();

        assert_eq!(prefs.tts_download_format, "mp3");
    }
}
