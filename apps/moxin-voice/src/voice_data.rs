//! Voice data definitions for TTS (Qwen3-TTS-MLX)
// NOTE: PrimeSpeech/GPT-SoVITS voice definitions are preserved below (commented out).
// Restore `get_builtin_voices()` and the old `get_builtin_voices_for_backend()` branch
// when re-enabling the PrimeSpeech backend.  See doc/REFACTOR_QWEN3_ONLY.md.

use serde::{Deserialize, Serialize};

/// Voice filter for category filtering
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum VoiceFilter {
    #[default]
    All,
    Male,
    Female,
    Character,
    Custom,
    Trained,
}

/// Language filter
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum LanguageFilter {
    #[default]
    All,
    Chinese,
    English,
}

/// Voice source - distinguishes between built-in, zero-shot custom, and few-shot trained voices
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub enum VoiceSource {
    #[default]
    Builtin,
    /// Zero-shot voice cloning (uses reference audio only)
    Custom,
    /// Few-shot trained model (requires 3-10 min training)
    Trained,
    /// Built-in clone voice whose reference audio is bundled with the app
    #[serde(alias = "BundledIcl")]
    BundledClone,
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
    /// Preview audio file path (optional)
    pub preview_audio: Option<String>,
    /// Voice source (built-in or custom)
    #[serde(default)]
    pub source: VoiceSource,
    /// Reference audio path for custom voices (relative to custom_voices dir)
    #[serde(default)]
    pub reference_audio_path: Option<String>,
    /// Legacy prompt/reference text retained only for old-config deserialization.
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

// --- PrimeSpeech built-in voices (GPT-SoVITS) — commented out for Qwen3-only mode ---
// Restore by un-commenting and re-enabling the else branch in `get_builtin_voices_for_backend`.
/*
pub fn get_builtin_voices() -> Vec<Voice> {
    vec![
        // Chinese voices
        Voice {
            id: "Doubao".to_string(),
            name: "豆包 (Doubao)".to_string(),
            description: "Chinese - mixed style, natural and expressive".to_string(),
            category: VoiceCategory::Character,
            language: "zh".to_string(),
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
            name: "BYS".to_string(),
            description: "Chinese - casual and friendly".to_string(),
            category: VoiceCategory::Character,
            language: "zh".to_string(),
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
            name: "Trump".to_string(),
            description: "English male - distinctive speaking style".to_string(),
            category: VoiceCategory::Male,
            language: "en".to_string(),
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
*/ // end commented-out get_builtin_voices

/// Localized (name, description) pairs for each Qwen3 preset speaker.
/// Returns English strings when `locale` is "en", Chinese otherwise.
fn qwen_voice_i18n(id: &str, locale: &str) -> (&'static str, &'static str) {
    let en = locale == "en";
    match id {
        "vivian"   => if en { ("Vivian",    "Bright, slightly edgy young female voice") }
                      else  { ("薇薇安",  "活泼灵动、略带个性的年轻女声") },
        "serena"   => if en { ("Serena",    "Warm, gentle young female voice") }
                      else  { ("赛琳娜",  "温柔亲切的年轻女声") },
        "uncle_fu" => if en { ("Uncle Fu",  "Seasoned male voice with low, mellow timbre") }
                      else  { ("傅叔", "低沉醇厚的成熟男声") },
        "dylan"    => if en { ("Dylan",     "Youthful Beijing male voice, clear and natural") }
                      else  { ("迪伦",    "清朗自然的北京青年男声") },
        "eric"     => if en { ("Eric",      "Lively Chengdu male voice with husky brightness") }
                      else  { ("埃里克",   "活泼明亮的成都青年男声") },
        // English-only speakers — same regardless of locale
        "ryan"     => ("Ryan",     "Dynamic male voice with strong rhythmic drive"),
        "aiden"    => ("Aiden",    "Sunny American male voice with clear midrange"),
        // Japanese / Korean — show romaji name with locale-aware description
        "ono_anna" => if en { ("Ono Anna",  "Playful Japanese female voice, light and nimble") }
                      else  { ("小野安奈", "轻快灵动的日本女声") },
        "sohee"    => if en { ("Sohee",     "Warm Korean female voice with rich emotion") }
                      else  { ("素熙",   "情感丰富的韩国女声") },
        "baiyang"  => if en { ("Baiyang",   "Custom trained Chinese female voice") }
                      else  { ("白杨", "自定义训练中文女声") },
        "yangyang" => if en { ("Yangyang",  "Custom trained Chinese male voice") }
                      else  { ("杨阳", "自定义训练中文男声") },
        _          => ("",         ""),
    }
}

/// Get built-in voices for qwen3-tts backend.
/// `locale` should be "en" or "zh" (default zh for anything else).
pub fn get_qwen_builtin_voices(locale: &str) -> Vec<Voice> {
    // (id, voice_language, preview_wav)
    let specs: &[(&str, &str, &str)] = &[
        ("vivian",   "zh", "vivian.wav"),
        ("serena",   "zh", "serena.wav"),
        ("uncle_fu", "zh", "uncle_fu.wav"),
        ("dylan",    "zh", "dylan.wav"),
        ("eric",     "zh", "eric.wav"),
        ("ryan",     "en", "ryan.wav"),
        ("aiden",    "en", "aiden.wav"),
        ("ono_anna", "ja", "ono_anna.wav"),
        ("sohee",    "ko", "sohee.wav"),
    ];
    let mut voices: Vec<Voice> = specs.iter().map(|(id, lang, wav)| {
        let (name, desc) = qwen_voice_i18n(id, locale);
        let cat = match *lang {
            "en" => VoiceCategory::Male,
            _ => if *id == "vivian" || *id == "serena" || *id == "ono_anna" || *id == "sohee" {
                VoiceCategory::Female
            } else {
                VoiceCategory::Male
            },
        };
        Voice {
            id: id.to_string(),
            name: name.to_string(),
            description: desc.to_string(),
            category: cat,
            language: lang.to_string(),
            preview_audio: Some(wav.to_string()),
            source: VoiceSource::Builtin,
            reference_audio_path: None,
            prompt_text: None,
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        }
    }).collect();

    // Bundled clone voices use app-provided reference audio through the x-vector path.
    let (baiyang_name, baiyang_desc) = qwen_voice_i18n("baiyang", locale);
    voices.push(Voice {
        id: "baiyang".to_string(),
        name: baiyang_name.to_string(),
        description: baiyang_desc.to_string(),
        category: VoiceCategory::Female,
        language: "zh".to_string(),
        preview_audio: Some("baiyang.wav".to_string()),
        source: VoiceSource::BundledClone,
        reference_audio_path: Some("ref.wav".to_string()),
        prompt_text: None,
        gpt_weights: None,
        sovits_weights: None,
        created_at: None,
    });

    let (yangyang_name, yangyang_desc) = qwen_voice_i18n("yangyang", locale);
    voices.push(Voice {
        id: "yangyang".to_string(),
        name: yangyang_name.to_string(),
        description: yangyang_desc.to_string(),
        category: VoiceCategory::Male,
        language: "zh".to_string(),
        preview_audio: Some("yangyang.wav".to_string()),
        source: VoiceSource::BundledClone,
        reference_audio_path: Some("ref.wav".to_string()),
        prompt_text: None,
        gpt_weights: None,
        sovits_weights: None,
        created_at: None,
    });

    voices
}

/// Select built-in voices by current inference backend.
/// `locale` is "en" or "zh" and controls display language for Qwen3 voices.
/// Only the Qwen3-TTS-MLX backend is active. PrimeSpeech branch is preserved
/// in comments — see doc/REFACTOR_QWEN3_ONLY.md to restore it.
pub fn get_builtin_voices_for_backend(_inference_backend: &str, locale: &str) -> Vec<Voice> {
    // Qwen3-only: always return Qwen3 voices regardless of backend string.
    get_qwen_builtin_voices(locale)
    // PrimeSpeech restore path (was: else { get_builtin_voices() })
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
    ) -> Self {
        Self {
            id,
            name: name.clone(),
            description: format!("Custom voice - {}", name),
            category: VoiceCategory::Character,
            language,
            preview_audio: Some(reference_audio_path.clone()),
            source: VoiceSource::Custom,
            reference_audio_path: Some(reference_audio_path),
            prompt_text: None,
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

    /// Check if voice matches the given category filter
    pub fn matches_filter(&self, filter: &VoiceFilter) -> bool {
        match filter {
            VoiceFilter::All => true,
            VoiceFilter::Male => self.category == VoiceCategory::Male,
            VoiceFilter::Female => self.category == VoiceCategory::Female,
            VoiceFilter::Character => self.category == VoiceCategory::Character,
            VoiceFilter::Custom => self.source == VoiceSource::Custom,
            VoiceFilter::Trained => self.source == VoiceSource::Trained,
        }
    }

    /// Check if voice matches the given language filter
    pub fn matches_language(&self, filter: &LanguageFilter) -> bool {
        match filter {
            LanguageFilter::All => true,
            LanguageFilter::Chinese => self.language == "zh",
            LanguageFilter::English => self.language == "en",
        }
    }

    /// Check if voice matches search query
    pub fn matches_search(&self, query: &str) -> bool {
        if query.is_empty() {
            return true;
        }
        let query_lower = query.to_lowercase();
        self.name.to_lowercase().contains(&query_lower)
            || self.description.to_lowercase().contains(&query_lower)
    }
}

/// Build the wire payload for a reference-audio-only clone request.
///
/// `prompt_text` is intentionally ignored: the field remains on `Voice` only so
/// configurations written by older releases can still be deserialized.
pub(crate) fn build_custom_clone_prompt(
    voice: &Voice,
    resolved_reference_audio_path: &str,
    text: &str,
) -> Option<String> {
    if !matches!(
        voice.source,
        VoiceSource::Custom | VoiceSource::BundledClone
    ) || voice.reference_audio_path.is_none()
        || resolved_reference_audio_path.is_empty()
    {
        return None;
    }

    Some(format!(
        "VOICE:CUSTOM|{}||{}|{}",
        resolved_reference_audio_path, voice.language, text
    ))
}

#[cfg(test)]
mod tests {
    use super::{build_custom_clone_prompt, Voice, VoiceSource};

    #[test]
    fn new_custom_voice_needs_reference_audio_but_no_prompt_text() {
        let voice = Voice::new_custom(
            "custom-1".to_string(),
            "Custom".to_string(),
            "zh".to_string(),
            "custom-1/ref.wav".to_string(),
        );

        assert_eq!(
            voice.reference_audio_path.as_deref(),
            Some("custom-1/ref.wav")
        );
        assert_eq!(voice.prompt_text, None);
        assert_eq!(
            build_custom_clone_prompt(&voice, "/tmp/ref.wav", "你好").as_deref(),
            Some("VOICE:CUSTOM|/tmp/ref.wav||zh|你好")
        );
    }

    #[test]
    fn legacy_prompt_text_is_read_but_never_sent() {
        let json = r#"{
            "id":"legacy",
            "name":"Legacy",
            "description":"Legacy voice",
            "category":"Character",
            "language":"en",
            "source":"Custom",
            "reference_audio_path":"legacy/ref.wav",
            "prompt_text":"the old reference transcript"
        }"#;
        let voice: Voice = serde_json::from_str(json).unwrap();

        assert_eq!(
            voice.prompt_text.as_deref(),
            Some("the old reference transcript")
        );
        assert_eq!(
            build_custom_clone_prompt(&voice, "/tmp/legacy.wav", "hello").as_deref(),
            Some("VOICE:CUSTOM|/tmp/legacy.wav||en|hello")
        );
    }

    #[test]
    fn legacy_bundled_icl_source_deserializes_as_bundled_clone() {
        let source: VoiceSource = serde_json::from_str(r#""BundledIcl""#).unwrap();

        assert_eq!(source, VoiceSource::BundledClone);
    }

    #[test]
    fn bundled_clone_uses_empty_prompt_field() {
        let voice = Voice {
            id: "baiyang".to_string(),
            name: "Baiyang".to_string(),
            description: String::new(),
            category: super::VoiceCategory::Female,
            language: "zh".to_string(),
            preview_audio: Some("baiyang.wav".to_string()),
            source: VoiceSource::BundledClone,
            reference_audio_path: Some("ref.wav".to_string()),
            prompt_text: Some("legacy transcript".to_string()),
            gpt_weights: None,
            sovits_weights: None,
            created_at: None,
        };

        assert_eq!(
            build_custom_clone_prompt(&voice, "/app/baiyang/ref.wav", "测试").as_deref(),
            Some("VOICE:CUSTOM|/app/baiyang/ref.wav||zh|测试")
        );
    }
}
