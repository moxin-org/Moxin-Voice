//! Voice registry – loads voices.json and builds VoiceClonerConfig for each preset.

use anyhow::{Context, Result};
use gpt_sovits_mlx::voice_clone::VoiceClonerConfig;
use serde::Deserialize;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

#[derive(Debug, Deserialize)]
pub struct VoiceEntry {
    pub gpt: String,
    pub sovits: String,
    pub reference: String,
    pub prompt_text: String,
    pub language: String,
    pub speed_factor: f32,
}

pub struct VoiceRegistry {
    voices_dir: PathBuf,
    model_dir: PathBuf,
    voices: HashMap<String, VoiceEntry>,
}

impl VoiceRegistry {
    /// Load the registry from voices.json.
    ///
    /// `model_dir` defaults to `~/.OminiX/models/gpt-sovits-mlx` if not set
    /// via the `GPT_SOVITS_MODEL_DIR` environment variable.
    pub fn load() -> Result<Self> {
        let model_dir = if let Ok(dir) = std::env::var("GPT_SOVITS_MODEL_DIR") {
            PathBuf::from(dir)
        } else {
            let home = dirs::home_dir().context("Cannot determine home directory")?;
            home.join(".OminiX/models/gpt-sovits-mlx")
        };

        let voices_dir = model_dir.join("voices");
        let json_path = voices_dir.join("voices.json");

        let json_str = std::fs::read_to_string(&json_path)
            .with_context(|| format!("Cannot read voices.json at {:?}", json_path))?;

        let voices: HashMap<String, VoiceEntry> =
            serde_json::from_str(&json_str).context("Failed to parse voices.json")?;

        tracing::info!("Loaded {} voices from {:?}", voices.len(), json_path);

        Ok(Self {
            voices_dir,
            model_dir,
            voices,
        })
    }

    /// Normalize a voice name: try as-is first, then with spaces removed.
    /// This bridges the UI IDs ("Yang Mi") to voices.json keys ("YangMi").
    fn resolve_name<'a>(&'a self, name: &'a str) -> Option<&'a str> {
        if self.voices.contains_key(name) {
            return Some(name);
        }
        // Try stripping spaces (and underscores for safety)
        let normalized: String = name
            .chars()
            .filter(|c| !c.is_whitespace() && *c != '_')
            .collect();
        if self.voices.contains_key(normalized.as_str()) {
            // Return a reference to the key stored in the map
            return self
                .voices
                .get_key_value(normalized.as_str())
                .map(|(k, _)| k.as_str());
        }
        None
    }

    /// Build a VoiceClonerConfig for a named preset voice.
    /// Returns (config, ref_wav_path, prompt_text, Option<semantic_codes_path>)
    pub fn config_for_preset(
        &self,
        name: &str,
    ) -> Result<(VoiceClonerConfig, String, String, Option<String>)> {
        let key = self
            .resolve_name(name)
            .with_context(|| format!("Unknown voice: '{}'", name))?;
        let entry = &self.voices[key];

        let sovits_path = self.voices_dir.join(&entry.sovits);
        let config = self.build_config(
            &self.voices_dir.join(&entry.gpt),
            &sovits_path,
            entry.speed_factor,
        );

        let ref_wav = self.voices_dir.join(&entry.reference);

        // Check for pre-computed Python semantic codes (prompt_semantic.npy).
        // Extracted by scripts/extract_all_prompt_semantic.py using Python CNHubert,
        // giving the same T2S conditioning quality as dora-primespeech few-shot mode.
        let semantic_npy = sovits_path
            .parent()
            .map(|d| d.join("prompt_semantic.npy"))
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().to_string());

        Ok((
            config,
            ref_wav.to_string_lossy().to_string(),
            entry.prompt_text.clone(),
            semantic_npy,
        ))
    }

    /// Build a VoiceClonerConfig for a zero-shot custom voice.
    pub fn config_for_custom(&self, speed: f32) -> VoiceClonerConfig {
        // Use default (Doubao) weights – they are overridden later by set_reference_audio_with_text
        self.build_config(
            &self.model_dir.join("voices/Doubao/gpt.safetensors"),
            &self.model_dir.join("voices/Doubao/sovits.safetensors"),
            speed,
        )
    }

    /// Build a VoiceClonerConfig for a trained (Pro-mode) voice.
    pub fn config_for_trained(
        &self,
        gpt_path: &str,
        sovits_path: &str,
        speed: f32,
    ) -> VoiceClonerConfig {
        let mut cfg = self.build_config(Path::new(gpt_path), Path::new(sovits_path), speed);

        // Trained voices in app workflow may be exported as finetuned overlays.
        // Use Doubao SoVITS as stable base, then apply trained weights on top.
        let base = self
            .model_dir
            .join("voices")
            .join("Doubao")
            .join("sovits.safetensors");
        if base.exists() {
            tracing::info!(
                "Using trained voice overlay with pretrained base: {} + {}",
                base.display(),
                sovits_path
            );
            cfg.vits_pretrained_base = Some(base.to_string_lossy().to_string());
        }

        cfg
    }

    fn encoders_dir(&self) -> PathBuf {
        self.model_dir.join("encoders")
    }

    fn bert_tokenizer_path(&self) -> PathBuf {
        self.model_dir.join("bert-tokenizer/tokenizer.json")
    }

    fn bert_weights_path(&self) -> PathBuf {
        // bert.safetensors lives in encoders/ (converted by convert_all_voices.py)
        self.encoders_dir().join("bert.safetensors")
    }

    fn build_config(&self, gpt: &Path, sovits: &Path, speed: f32) -> VoiceClonerConfig {
        // Use ONNX VITS if it exists alongside the sovits weights (batched decode,
        // matches Python quality). Falls back to MLX VITS automatically when absent.
        let onnx_path = sovits.parent().map(|d| d.join("vits.onnx"));
        let vits_onnx_path = onnx_path
            .filter(|p| p.exists())
            .map(|p| p.to_string_lossy().to_string());

        VoiceClonerConfig {
            t2s_weights: gpt.to_string_lossy().to_string(),
            vits_weights: sovits.to_string_lossy().to_string(),
            vits_pretrained_base: None,
            hubert_weights: self
                .encoders_dir()
                .join("hubert.safetensors")
                .to_string_lossy()
                .to_string(),
            bert_weights: self.bert_weights_path().to_string_lossy().to_string(),
            bert_tokenizer: self.bert_tokenizer_path().to_string_lossy().to_string(),
            sample_rate: 32000,
            top_k: 15,
            top_p: 1.0,
            temperature: 1.0,
            repetition_penalty: 1.2,
            noise_scale: 0.5,
            speed,
            use_mlx_vits: vits_onnx_path.is_none(), // use MLX only when no ONNX available
            vits_onnx_path,
            use_gpu_mel: true,
        }
    }

    /// Default speed for preset voices (returned alongside config).
    pub fn speed_for(&self, name: &str) -> f32 {
        self.resolve_name(name)
            .and_then(|k| self.voices.get(k))
            .map(|e| e.speed_factor)
            .unwrap_or(1.0)
    }
}
