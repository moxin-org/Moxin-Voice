//! Voice data definitions for TTS (GPT-SoVITS)

use serde::{Deserialize, Serialize};
use std::fmt;

/// Voice source - distinguishes between built-in, zero-shot custom, and few-shot trained voices
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum VoiceSource {
    #[default]
    Builtin,
    /// Zero-shot voice cloning (uses reference audio only)
    Custom,
    /// Few-shot trained model (requires 3-10 min training)
    Trained,
}

/// Voice gender-age taxonomy for timbre management
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum VoiceGenderAge {
    Male,
    Female,
    Child,
}

impl VoiceGenderAge {
    pub fn key(self) -> &'static str {
        match self {
            VoiceGenderAge::Male => "male",
            VoiceGenderAge::Female => "female",
            VoiceGenderAge::Child => "child",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            VoiceGenderAge::Male => "男声",
            VoiceGenderAge::Female => "女声",
            VoiceGenderAge::Child => "童声",
        }
    }

    pub fn from_key(value: &str) -> Result<Self, String> {
        match value {
            "male" => Ok(VoiceGenderAge::Male),
            "female" => Ok(VoiceGenderAge::Female),
            "child" => Ok(VoiceGenderAge::Child),
            _ => Err(format!(
                "Invalid gender_age '{}', expected one of: male/female/child",
                value
            )),
        }
    }
}

impl fmt::Display for VoiceGenderAge {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// Voice style taxonomy for timbre management
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum VoiceStyle {
    Sweet,
    Magnetic,
    Youth,
}

impl VoiceStyle {
    pub fn key(self) -> &'static str {
        match self {
            VoiceStyle::Sweet => "sweet",
            VoiceStyle::Magnetic => "magnetic",
            VoiceStyle::Youth => "youth",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            VoiceStyle::Sweet => "甜美",
            VoiceStyle::Magnetic => "磁性",
            VoiceStyle::Youth => "青年音",
        }
    }

    pub fn from_key(value: &str) -> Result<Self, String> {
        match value {
            "sweet" => Ok(VoiceStyle::Sweet),
            "magnetic" => Ok(VoiceStyle::Magnetic),
            "youth" => Ok(VoiceStyle::Youth),
            _ => Err(format!(
                "Invalid style '{}', expected one of: sweet/magnetic/youth",
                value
            )),
        }
    }
}

impl fmt::Display for VoiceStyle {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

/// Voice trait categories shown in Select Voice panel.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum VoiceTraitCategory {
    ProfessionalBroadcast,
    FeaturedCharacter,
}

impl VoiceTraitCategory {
    pub fn key(self) -> &'static str {
        match self {
            VoiceTraitCategory::ProfessionalBroadcast => "professional_broadcast",
            VoiceTraitCategory::FeaturedCharacter => "featured_character",
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            VoiceTraitCategory::ProfessionalBroadcast => "专业播音",
            VoiceTraitCategory::FeaturedCharacter => "特色人物",
        }
    }

    pub fn from_key(value: &str) -> Result<Self, String> {
        match value {
            "professional_broadcast" => Ok(VoiceTraitCategory::ProfessionalBroadcast),
            "featured_character" => Ok(VoiceTraitCategory::FeaturedCharacter),
            _ => Err(format!(
                "Invalid trait category '{}', expected one of: professional_broadcast/featured_character",
                value
            )),
        }
    }
}

impl fmt::Display for VoiceTraitCategory {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.key())
    }
}

impl Default for VoiceTraitCategory {
    fn default() -> Self {
        SELECT_VOICE_DEFAULT_TRAIT_CATEGORY
    }
}

pub const SELECT_VOICE_DEFAULT_TRAIT_CATEGORY: VoiceTraitCategory =
    VoiceTraitCategory::ProfessionalBroadcast;

const SELECT_VOICE_TRAIT_CATEGORIES: [VoiceTraitCategory; 2] = [
    VoiceTraitCategory::ProfessionalBroadcast,
    VoiceTraitCategory::FeaturedCharacter,
];

const PROFESSIONAL_BROADCAST_VOICE_IDS: [&str; 3] = ["BYS", "Luo Xiang", "Shen Yi"];
const FEATURED_CHARACTER_VOICE_IDS: [&str; 4] = ["Ma Yun", "Yang Mi", "Zhou Jielun", "Trump"];

pub fn select_voice_trait_categories() -> &'static [VoiceTraitCategory] {
    &SELECT_VOICE_TRAIT_CATEGORIES
}

pub fn select_voice_trait_voice_ids(category: VoiceTraitCategory) -> &'static [&'static str] {
    match category {
        VoiceTraitCategory::ProfessionalBroadcast => &PROFESSIONAL_BROADCAST_VOICE_IDS,
        VoiceTraitCategory::FeaturedCharacter => &FEATURED_CHARACTER_VOICE_IDS,
    }
}

pub fn matches_select_voice_trait_category(voice: &Voice, category: VoiceTraitCategory) -> bool {
    select_voice_trait_voice_ids(category)
        .iter()
        .any(|id| voice.id == *id)
}

pub fn restore_select_voice_trait_category(saved_key: Option<&str>) -> VoiceTraitCategory {
    match saved_key {
        Some("child") => SELECT_VOICE_DEFAULT_TRAIT_CATEGORY,
        Some(value) => {
            VoiceTraitCategory::from_key(value).unwrap_or(SELECT_VOICE_DEFAULT_TRAIT_CATEGORY)
        }
        None => SELECT_VOICE_DEFAULT_TRAIT_CATEGORY,
    }
}

/// Voice information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Voice {
    /// Unique voice ID (matches VOICE_NAME in PrimeSpeech config)
    pub id: String,
    /// Display name
    pub name: String,
    /// Description
    pub description: String,
    /// Voice style/category
    pub category: VoiceCategory,
    /// Language (zh, en)
    pub language: String,
    /// Gender-age taxonomy used by voice library filtering
    #[serde(default)]
    pub gender_age: Option<VoiceGenderAge>,
    /// Style taxonomy used by voice library filtering
    #[serde(default)]
    pub style: Option<VoiceStyle>,
    /// Preview audio file path (optional)
    pub preview_audio: Option<String>,
    /// Voice source (built-in or custom)
    #[serde(default)]
    pub source: VoiceSource,
    /// Reference audio path for custom voices (relative to custom_voices dir)
    #[serde(default)]
    pub reference_audio_path: Option<String>,
    /// Prompt/reference text for zero-shot cloning
    #[serde(default)]
    pub prompt_text: Option<String>,
    /// GPT model weights path (optional, uses default if not set)
    #[serde(default)]
    pub gpt_weights: Option<String>,
    /// SoVITS model weights path (optional, uses default if not set)
    #[serde(default)]
    pub sovits_weights: Option<String>,
    /// Creation timestamp (Unix epoch seconds)
    #[serde(default)]
    pub created_at: Option<u64>,
}

/// Voice category
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub enum VoiceCategory {
    Male,
    Female,
    Character,
}

impl VoiceCategory {
    pub fn as_str(&self) -> &'static str {
        match self {
            VoiceCategory::Male => "Male",
            VoiceCategory::Female => "Female",
            VoiceCategory::Character => "Character",
        }
    }
}

/// Get built-in voices for PrimeSpeech (GPT-SoVITS)
/// These match the VOICE_CONFIGS in dora-primespeech/config.py
pub fn get_builtin_voices() -> Vec<Voice> {
    vec![
        // Chinese voices
        Voice {
            id: "Doubao".to_string(),
            name: "豆包 (Doubao)".to_string(),
            description: "Chinese - mixed style, natural and expressive".to_string(),
            category: VoiceCategory::Character,
            language: "zh".to_string(),
            gender_age: Some(VoiceGenderAge::Child),
            style: Some(VoiceStyle::Youth),
            preview_audio: Some("doubao_ref_mix_new.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
        Voice {
            id: "Luo Xiang".to_string(),
            name: "罗翔 (Luo Xiang)".to_string(),
            description: "Chinese male - law professor, articulate and thoughtful".to_string(),
            category: VoiceCategory::Male,
            language: "zh".to_string(),
            gender_age: Some(VoiceGenderAge::Male),
            style: Some(VoiceStyle::Magnetic),
            preview_audio: Some("luoxiang_ref.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
        Voice {
            id: "Yang Mi".to_string(),
            name: "杨幂 (Yang Mi)".to_string(),
            description: "Chinese female - actress, sweet and charming".to_string(),
            category: VoiceCategory::Female,
            language: "zh".to_string(),
            gender_age: Some(VoiceGenderAge::Female),
            style: Some(VoiceStyle::Sweet),
            preview_audio: Some("yangmi_ref.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
        Voice {
            id: "Zhou Jielun".to_string(),
            name: "周杰伦 (Zhou Jielun)".to_string(),
            description: "Chinese male - singer, unique and distinctive".to_string(),
            category: VoiceCategory::Male,
            language: "zh".to_string(),
            gender_age: Some(VoiceGenderAge::Male),
            style: Some(VoiceStyle::Youth),
            preview_audio: Some("zhoujielun_ref.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
        Voice {
            id: "Ma Yun".to_string(),
            name: "马云 (Ma Yun)".to_string(),
            description: "Chinese male - entrepreneur, confident speaker".to_string(),
            category: VoiceCategory::Male,
            language: "zh".to_string(),
            gender_age: Some(VoiceGenderAge::Male),
            style: Some(VoiceStyle::Magnetic),
            preview_audio: Some("mayun_ref.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
        Voice {
            id: "Chen Yifan".to_string(),
            name: "陈一凡 (Chen Yifan)".to_string(),
            description: "Chinese male - analyst, professional tone".to_string(),
            category: VoiceCategory::Male,
            language: "zh".to_string(),
            gender_age: Some(VoiceGenderAge::Male),
            style: Some(VoiceStyle::Magnetic),
            preview_audio: Some("yfc_ref.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
        Voice {
            id: "Zhao Daniu".to_string(),
            name: "赵大牛 (Zhao Daniu)".to_string(),
            description: "Chinese male - podcast host, engaging narrator".to_string(),
            category: VoiceCategory::Male,
            language: "zh".to_string(),
            gender_age: Some(VoiceGenderAge::Male),
            style: Some(VoiceStyle::Youth),
            preview_audio: Some("dnz_ref.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
        Voice {
            id: "BYS".to_string(),
            name: "白岩松 (Bai Yansong)".to_string(),
            description: "Chinese male - news anchor, steady and professional".to_string(),
            category: VoiceCategory::Character,
            language: "zh".to_string(),
            gender_age: Some(VoiceGenderAge::Child),
            style: Some(VoiceStyle::Sweet),
            preview_audio: Some("bys_ref.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
        Voice {
            id: "Ma Baoguo".to_string(),
            name: "马保国 (Ma Baoguo)".to_string(),
            description: "Chinese male - martial arts master, distinctive style".to_string(),
            category: VoiceCategory::Male,
            language: "zh".to_string(),
            gender_age: Some(VoiceGenderAge::Male),
            style: Some(VoiceStyle::Magnetic),
            preview_audio: Some("mabaoguo_ref.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
        Voice {
            id: "Shen Yi".to_string(),
            name: "沈逸 (Shen Yi)".to_string(),
            description: "Chinese male - professor, analytical tone".to_string(),
            category: VoiceCategory::Male,
            language: "zh".to_string(),
            gender_age: Some(VoiceGenderAge::Male),
            style: Some(VoiceStyle::Magnetic),
            preview_audio: Some("shenyi_ref.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
        // English voices
        Voice {
            id: "Maple".to_string(),
            name: "Maple".to_string(),
            description: "English female - storyteller, warm and gentle".to_string(),
            category: VoiceCategory::Female,
            language: "en".to_string(),
            gender_age: Some(VoiceGenderAge::Female),
            style: Some(VoiceStyle::Sweet),
            preview_audio: Some("maple_ref.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
        Voice {
            id: "Cove".to_string(),
            name: "Cove".to_string(),
            description: "English male - commentator, clear and professional".to_string(),
            category: VoiceCategory::Male,
            language: "en".to_string(),
            gender_age: Some(VoiceGenderAge::Male),
            style: Some(VoiceStyle::Magnetic),
            preview_audio: Some("cove_ref.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
        Voice {
            id: "Ellen".to_string(),
            name: "Ellen".to_string(),
            description: "English female - talk show host, energetic".to_string(),
            category: VoiceCategory::Female,
            language: "en".to_string(),
            gender_age: Some(VoiceGenderAge::Female),
            style: Some(VoiceStyle::Youth),
            preview_audio: Some("ellen_ref.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
        Voice {
            id: "Juniper".to_string(),
            name: "Juniper".to_string(),
            description: "English female - narrator, calm and soothing".to_string(),
            category: VoiceCategory::Female,
            language: "en".to_string(),
            gender_age: Some(VoiceGenderAge::Female),
            style: Some(VoiceStyle::Sweet),
            preview_audio: Some("juniper_ref.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
        Voice {
            id: "Trump".to_string(),
            name: "trump罗翔 (Trump Luo Xiang)".to_string(),
            description: "English male - distinctive speaking style".to_string(),
            category: VoiceCategory::Male,
            language: "en".to_string(),
            gender_age: Some(VoiceGenderAge::Male),
            style: Some(VoiceStyle::Magnetic),
            preview_audio: Some("trump_ref.wav".to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        },
    ]
}

/// TTS generation status
#[derive(Clone, Debug, PartialEq)]
pub enum TTSStatus {
    Idle,
    Generating,
    Ready,
    Playing,
    Error(String),
}

impl Default for TTSStatus {
    fn default() -> Self {
        TTSStatus::Idle
    }
}

/// Voice cloning status
#[derive(Clone, Debug, PartialEq)]
pub enum CloningStatus {
    Idle,
    ValidatingAudio,
    CopyingFiles,
    SavingConfig,
    Completed,
    Error(String),
}

impl Default for CloningStatus {
    fn default() -> Self {
        CloningStatus::Idle
    }
}

impl Voice {
    /// Create a new custom voice
    pub fn new_custom(
        id: String,
        name: String,
        language: String,
        reference_audio_path: String,
        prompt_text: String,
    ) -> Self {
        Self {
            id,
            name: name.clone(),
            description: format!("Custom voice - {}", name),
            category: VoiceCategory::Character,
            language,
            gender_age: Some(VoiceGenderAge::Child),
            style: Some(VoiceStyle::Youth),
            preview_audio: Some(reference_audio_path.clone()),
            source: VoiceSource::Custom,
            reference_audio_path: Some(reference_audio_path),
            prompt_text: Some(prompt_text),
            gpt_weights: None,
            sovits_weights: None,
            created_at: Some(
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_secs())
                    .unwrap_or(0),
            ),
        }
    }

    /// Check if this is a custom voice
    pub fn is_custom(&self) -> bool {
        self.source == VoiceSource::Custom
    }

    /// Check if this is a trained voice (few-shot)
    pub fn is_trained(&self) -> bool {
        self.source == VoiceSource::Trained
    }

    /// Check if this voice uses custom models (either zero-shot or trained)
    pub fn has_custom_models(&self) -> bool {
        self.gpt_weights.is_some() || self.sovits_weights.is_some()
    }

    /// Validate timbre tag values exposed by create/update interfaces.
    pub fn validate_timbre_tags(&self) -> Result<(), String> {
        if self.gender_age.is_none() {
            return Err("Missing required field: gender_age".to_string());
        }
        if self.style.is_none() {
            return Err("Missing required field: style".to_string());
        }
        Ok(())
    }

    /// Backward compatibility for legacy records without timbre tags.
    pub fn with_backfilled_timbre_tags(mut self) -> Self {
        if self.gender_age.is_none() {
            self.gender_age = Some(self.default_gender_age());
        }
        if self.style.is_none() {
            self.style = Some(self.default_style());
        }
        self
    }

    pub fn default_gender_age(&self) -> VoiceGenderAge {
        match self.category {
            VoiceCategory::Male => VoiceGenderAge::Male,
            VoiceCategory::Female => VoiceGenderAge::Female,
            VoiceCategory::Character => VoiceGenderAge::Child,
        }
    }

    pub fn default_style(&self) -> VoiceStyle {
        VoiceStyle::Youth
    }

    pub fn effective_gender_age(&self) -> VoiceGenderAge {
        self.gender_age.unwrap_or_else(|| self.default_gender_age())
    }

    pub fn effective_style(&self) -> VoiceStyle {
        self.style.unwrap_or_else(|| self.default_style())
    }
}

/// OR within each dimension, AND across dimensions.
pub fn matches_timbre_filters(
    voice: &Voice,
    gender_filters: &[VoiceGenderAge],
    style_filters: &[VoiceStyle],
) -> bool {
    let gender_match =
        gender_filters.is_empty() || gender_filters.contains(&voice.effective_gender_age());
    let style_match = style_filters.is_empty() || style_filters.contains(&voice.effective_style());
    gender_match && style_match
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_timbre_parse_validation() {
        assert_eq!(
            VoiceGenderAge::from_key("male").unwrap(),
            VoiceGenderAge::Male
        );
        assert!(VoiceGenderAge::from_key("elderly_male").is_err());
        assert_eq!(VoiceStyle::from_key("sweet").unwrap(), VoiceStyle::Sweet);
        assert!(VoiceStyle::from_key("warm").is_err());
        assert_eq!(
            VoiceTraitCategory::from_key("professional_broadcast").unwrap(),
            VoiceTraitCategory::ProfessionalBroadcast
        );
        assert!(VoiceTraitCategory::from_key("child").is_err());
    }

    #[test]
    fn test_backfill_legacy_voice_tags() {
        let legacy_voice = Voice {
            id: "legacy".to_string(),
            name: "Legacy Voice".to_string(),
            description: "legacy".to_string(),
            category: VoiceCategory::Male,
            language: "zh".to_string(),
            gender_age: None,
            style: None,
            preview_audio: None,
            source: VoiceSource::Custom,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        };

        let normalized = legacy_voice.with_backfilled_timbre_tags();
        assert_eq!(normalized.gender_age, Some(VoiceGenderAge::Male));
        assert_eq!(normalized.style, Some(VoiceStyle::Youth));
    }

    #[test]
    fn test_combined_timbre_filter_logic() {
        let voice = Voice {
            id: "v1".to_string(),
            name: "Voice 1".to_string(),
            description: "voice".to_string(),
            category: VoiceCategory::Male,
            language: "zh".to_string(),
            gender_age: Some(VoiceGenderAge::Male),
            style: Some(VoiceStyle::Magnetic),
            preview_audio: None,
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        };

        assert!(matches_timbre_filters(&voice, &[], &[]));
        assert!(matches_timbre_filters(
            &voice,
            &[VoiceGenderAge::Male],
            &[VoiceStyle::Magnetic]
        ));
        assert!(matches_timbre_filters(
            &voice,
            &[VoiceGenderAge::Male, VoiceGenderAge::Child],
            &[]
        ));
        assert!(!matches_timbre_filters(
            &voice,
            &[VoiceGenderAge::Female],
            &[VoiceStyle::Magnetic]
        ));
    }

    #[test]
    fn test_select_voice_trait_category_mapping_is_exact() {
        let voices = get_builtin_voices();

        let professional_ids: Vec<String> = voices
            .iter()
            .filter(|voice| {
                matches_select_voice_trait_category(
                    voice,
                    VoiceTraitCategory::ProfessionalBroadcast,
                )
            })
            .map(|voice| voice.id.clone())
            .collect();
        assert_eq!(
            professional_ids,
            vec![
                "Luo Xiang".to_string(),
                "BYS".to_string(),
                "Shen Yi".to_string()
            ]
        );

        let featured_ids: Vec<String> = voices
            .iter()
            .filter(|voice| {
                matches_select_voice_trait_category(voice, VoiceTraitCategory::FeaturedCharacter)
            })
            .map(|voice| voice.id.clone())
            .collect();
        assert_eq!(
            featured_ids,
            vec![
                "Yang Mi".to_string(),
                "Zhou Jielun".to_string(),
                "Ma Yun".to_string(),
                "Trump".to_string()
            ]
        );
    }

    #[test]
    fn test_restore_select_voice_trait_category_fallback() {
        assert_eq!(
            restore_select_voice_trait_category(Some("professional_broadcast")),
            VoiceTraitCategory::ProfessionalBroadcast
        );
        assert_eq!(
            restore_select_voice_trait_category(Some("featured_character")),
            VoiceTraitCategory::FeaturedCharacter
        );
        assert_eq!(
            restore_select_voice_trait_category(Some("child")),
            SELECT_VOICE_DEFAULT_TRAIT_CATEGORY
        );
        assert_eq!(
            restore_select_voice_trait_category(Some("unknown_category")),
            SELECT_VOICE_DEFAULT_TRAIT_CATEGORY
        );
        assert_eq!(
            restore_select_voice_trait_category(None),
            SELECT_VOICE_DEFAULT_TRAIT_CATEGORY
        );
    }

    #[test]
    fn test_select_voice_trait_category_labels() {
        let labels: Vec<&'static str> = select_voice_trait_categories()
            .iter()
            .map(|category| category.label())
            .collect();
        assert_eq!(labels, vec!["专业播音", "特色人物"]);
        assert!(!labels.contains(&"童声"));
    }
}
