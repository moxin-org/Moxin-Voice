//! Persistence for TTS generation history.
//!
//! History metadata is stored in:
//! - ~/.dora/primespeech/tts_history.json
//! Audio snapshots are stored in:
//! - ~/.dora/primespeech/tts_history_audio/<entry_id>.wav

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

pub const HISTORY_VERSION: &str = "1.0";
pub const DEFAULT_MAX_HISTORY_ITEMS: usize = 100;

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct TtsHistoryEntry {
    pub id: String,
    pub created_at: u64,
    pub text: String,
    pub text_preview: String,
    pub voice_id: String,
    pub voice_name: String,
    pub model_id: Option<String>,
    pub model_name: Option<String>,
    pub duration_secs: f32,
    pub sample_rate: u32,
    pub sample_count: usize,
    pub speed: f64,
    pub pitch: f64,
    pub volume: f64,
    pub audio_file: String,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
struct TtsHistoryConfig {
    version: String,
    entries: Vec<TtsHistoryEntry>,
}

impl Default for TtsHistoryConfig {
    fn default() -> Self {
        Self {
            version: HISTORY_VERSION.to_string(),
            entries: Vec::new(),
        }
    }
}

fn primespeech_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".dora").join("primespeech")
}

pub fn history_config_path() -> PathBuf {
    primespeech_dir().join("tts_history.json")
}

pub fn history_audio_dir() -> PathBuf {
    primespeech_dir().join("tts_history_audio")
}

pub fn history_audio_path(audio_file: &str) -> PathBuf {
    history_audio_dir().join(audio_file)
}

pub fn ensure_history_storage() -> Result<(), String> {
    let base = primespeech_dir();
    if !base.exists() {
        fs::create_dir_all(&base)
            .map_err(|e| format!("Failed to create history base directory: {}", e))?;
    }

    let audio_dir = history_audio_dir();
    if !audio_dir.exists() {
        fs::create_dir_all(&audio_dir)
            .map_err(|e| format!("Failed to create history audio directory: {}", e))?;
    }

    Ok(())
}

pub fn load_history() -> Vec<TtsHistoryEntry> {
    let path = history_config_path();
    if !path.exists() {
        return Vec::new();
    }

    match fs::read_to_string(&path) {
        Ok(content) => match serde_json::from_str::<TtsHistoryConfig>(&content) {
            Ok(mut config) => {
                config
                    .entries
                    .sort_by(|a, b| b.created_at.cmp(&a.created_at));
                config.entries
            }
            Err(e) => {
                log::error!("Failed to parse TTS history: {}", e);
                Vec::new()
            }
        },
        Err(e) => {
            log::error!("Failed to read TTS history: {}", e);
            Vec::new()
        }
    }
}

pub fn save_history(entries: &[TtsHistoryEntry]) -> Result<(), String> {
    ensure_history_storage()?;

    let config = TtsHistoryConfig {
        version: HISTORY_VERSION.to_string(),
        entries: entries.to_vec(),
    };
    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize TTS history: {}", e))?;
    fs::write(history_config_path(), json)
        .map_err(|e| format!("Failed to write TTS history: {}", e))
}

pub fn delete_audio_file(audio_file: &str) -> Result<(), String> {
    let path = history_audio_path(audio_file);
    if !path.exists() {
        return Ok(());
    }
    fs::remove_file(&path).map_err(|e| format!("Failed to remove history audio {:?}: {}", path, e))
}
