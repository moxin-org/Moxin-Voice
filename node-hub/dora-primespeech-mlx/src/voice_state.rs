//! Manages VoiceCloner lifecycle – lazy loading and voice switching.

use anyhow::{Context, Result};
use gpt_sovits_mlx::voice_clone::{VoiceCloner, VoiceClonerConfig};

/// Identifies which voice the cloner is currently configured for,
/// so we avoid reloading weights unnecessarily.
#[derive(Debug, Clone, PartialEq)]
pub enum ActiveVoice {
    None,
    Preset(String),
    Custom { ref_wav: String },
    Trained { gpt: String, sovits: String },
}

pub struct VoiceState {
    cloner: Option<VoiceCloner>,
    active: ActiveVoice,
}

impl VoiceState {
    pub fn new() -> Self {
        Self {
            cloner: None,
            active: ActiveVoice::None,
        }
    }

    /// Get (or create) a cloner loaded with the given config.
    /// Weights are reloaded only when `voice_key` changes.
    pub fn get_or_load(
        &mut self,
        config: VoiceClonerConfig,
        voice_key: ActiveVoice,
    ) -> Result<&mut VoiceCloner> {
        let need_reload = self.cloner.is_none() || self.active != voice_key;

        if need_reload {
            tracing::info!("Loading VoiceCloner for {:?}", voice_key);
            let cloner = VoiceCloner::new(config).context("Failed to create VoiceCloner")?;
            self.cloner = Some(cloner);
            self.active = voice_key;
        }

        Ok(self.cloner.as_mut().unwrap())
    }
}
