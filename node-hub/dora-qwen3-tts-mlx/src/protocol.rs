//! TTS protocol parser.
//!
//! Supports three message formats (same as dora-primespeech):
//!
//!   VOICE:<name>|<text>
//!     Use a preset voice by name.
//!
//!   VOICE:CUSTOM|<ref_wav>|<prompt_text>|<lang>|<text>
//!     Zero-shot voice clone (Express mode).
//!
//!   VOICE:TRAINED|<gpt_path>|<sovits_path>|<ref_wav>|<prompt_text>|<lang>|<text>
//!     Use a Pro-mode trained voice with custom weights.

#[derive(Debug, Clone)]
pub enum TtsRequest {
    /// Use a preset voice from voices.json
    Preset { voice: String, text: String },
    /// Zero-shot voice clone
    Custom {
        ref_wav: String,
        prompt_text: String,
        language: String,
        text: String,
    },
    /// Pro-mode trained voice
    Trained {
        gpt_path: String,
        sovits_path: String,
        ref_wav: String,
        prompt_text: String,
        language: String,
        text: String,
    },
}

impl TtsRequest {
    pub fn parse(input: &str) -> Option<Self> {
        let input = input.trim();

        let body = input.strip_prefix("VOICE:")?;

        if let Some(rest) = body.strip_prefix("CUSTOM|") {
            // VOICE:CUSTOM|ref_wav|prompt_text|lang|text
            let parts: Vec<&str> = rest.splitn(4, '|').collect();
            if parts.len() == 4 {
                return Some(TtsRequest::Custom {
                    ref_wav: parts[0].to_string(),
                    prompt_text: parts[1].to_string(),
                    language: parts[2].to_string(),
                    text: parts[3].to_string(),
                });
            }
            return None;
        }

        if let Some(rest) = body.strip_prefix("TRAINED|") {
            // VOICE:TRAINED|gpt_path|sovits_path|ref_wav|prompt_text|lang|text
            let parts: Vec<&str> = rest.splitn(6, '|').collect();
            if parts.len() == 6 {
                return Some(TtsRequest::Trained {
                    gpt_path: parts[0].to_string(),
                    sovits_path: parts[1].to_string(),
                    ref_wav: parts[2].to_string(),
                    prompt_text: parts[3].to_string(),
                    language: parts[4].to_string(),
                    text: parts[5].to_string(),
                });
            }
            return None;
        }

        // VOICE:<name>|<text>
        if let Some(sep) = body.find('|') {
            let voice = body[..sep].to_string();
            let text = body[sep + 1..].to_string();
            return Some(TtsRequest::Preset { voice, text });
        }

        None
    }
}
