//! Qwen3-TTS: Text-to-speech on Apple Silicon using MLX.
//!
//! Supports the `mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit` model
//! with 9 preset speakers and multilingual support.

pub mod config;
pub mod error;
pub mod generate;
pub mod mrope;
pub mod sampling;
pub mod speaker_encoder;
pub mod speech_encoder;
pub mod speech_tokenizer;
pub mod talker;

use std::collections::HashMap;
use std::path::Path;
use std::time::Instant;

use mlx_rs::Array;
use tracing::info;

use config::{GenerationConfig, Qwen3TtsConfig, SpeechTokenizerConfig};
use error::{Error, Result};
use generate::{
    build_codec_prefix, build_codec_prefix_voice_design, generate_custom_voice,
    generate_voice_clone, generate_voice_clone_icl, generate_voice_design, GenerationState,
};
use speech_tokenizer::SpeechTokenizerDecoder;
use talker::Talker;

// Re-exports
pub use config::GenerationConfig as GenConfig;
pub use config::ModelType;
pub use error::Error as TtsError;
pub use generate::GenerationTiming;

/// Default chunk size for streaming (10 frames = ~833ms at 12Hz)
pub const DEFAULT_CHUNK_FRAMES: usize = 10;

/// High-level text-to-speech synthesizer.
pub struct Synthesizer {
    pub talker: Talker,
    pub decoder: SpeechTokenizerDecoder,
    pub tts_config: Qwen3TtsConfig,
    pub gen_config: GenerationConfig,
    pub tokenizer: tokenizers::Tokenizer,
    pub sample_rate: u32,
    /// Optional speaker encoder for voice cloning (Base model only)
    pub speaker_encoder: Option<speaker_encoder::SpeakerEncoder>,
    /// Optional speech encoder for ICL voice cloning (Base model only)
    pub speech_encoder: Option<speech_encoder::SpeechEncoder>,
    /// Optional in-memory cache for speaker embeddings keyed by caller-provided cache keys.
    pub speaker_embedding_cache: HashMap<String, Array>,
}

/// Configuration for synthesis.
pub struct SynthesizeOptions<'a> {
    pub speaker: &'a str,
    pub language: &'a str,
    pub instruct: Option<&'a str>,
    pub temperature: Option<f32>,
    pub top_k: Option<i32>,
    pub top_p: Option<f32>,
    pub max_new_tokens: Option<i32>,
    pub seed: Option<u64>,
    /// Speed factor: > 1.0 = faster, < 1.0 = slower. Default 1.0.
    pub speed_factor: Option<f32>,
}

impl Default for SynthesizeOptions<'_> {
    fn default() -> Self {
        Self {
            speaker: "vivian",
            language: "english",
            instruct: None,
            temperature: None,
            top_k: None,
            top_p: None,
            max_new_tokens: None,
            seed: None,
            speed_factor: None,
        }
    }
}

/// Build the assistant ChatML text used by the official Qwen3-TTS Python wrapper.
pub fn build_assistant_text(text: &str) -> String {
    format!(
        "<|im_start|>assistant\n{}<|im_end|>\n<|im_start|>assistant\n",
        text
    )
}

/// Build the user ChatML instruction used by the official Qwen3-TTS Python wrapper.
pub fn build_instruct_text(instruct: &str) -> String {
    format!("<|im_start|>user\n{}<|im_end|>\n", instruct)
}

fn instruct_if_present(instruct: Option<&str>) -> Option<&str> {
    instruct.filter(|value| !value.is_empty())
}

/// Timing breakdown for synthesis.
#[derive(Debug, Clone)]
pub struct SynthesisTiming {
    pub prefill_ms: f64,
    pub generation_ms: f64,
    pub generation_frames: usize,
    pub incomplete_clone: bool,
    pub decode_ms: f64,
    pub total_ms: f64,
}

impl Synthesizer {
    /// Load models from a directory.
    /// The directory should contain:
    /// - config.json, generation_config.json
    /// - model.safetensors (or with index)
    /// - vocab.json, merges.txt (BPE tokenizer)
    /// - speech_tokenizer/ subdirectory with its own model.safetensors and config.json
    pub fn load(model_dir: impl AsRef<Path>) -> Result<Self> {
        let model_dir = model_dir.as_ref();

        info!("Loading TTS config...");
        let tts_config = Qwen3TtsConfig::load(model_dir)?;
        let gen_config = GenerationConfig::load(model_dir)?;
        let st_config = SpeechTokenizerConfig::load(model_dir)?;

        let quant = tts_config.quant_config().cloned();

        info!("Loading text tokenizer...");
        let tokenizer = load_bpe_tokenizer(model_dir)?;

        if let Some(ref q) = quant {
            info!("Loading talker model ({}-bit)...", q.bits);
        } else {
            info!("Loading talker model (float)...");
        }
        let talker = talker::load_talker(
            model_dir,
            &tts_config.talker_config,
            quant.as_ref(),
            tts_config.tts_pad_token_id,
        )?;

        info!("Loading speech tokenizer decoder...");
        let decoder =
            speech_tokenizer::load_speech_tokenizer(model_dir, &st_config.decoder_config)?;

        // Load speaker encoder if present (Base model only)
        let model_type = tts_config.model_type();
        let (spk_encoder, spch_encoder) = if model_type == config::ModelType::Base {
            info!("Loading speaker encoder (ECAPA-TDNN)...");
            let weights = talker::load_all_weights(model_dir)?;

            let spk = if speaker_encoder::has_speaker_encoder_weights(&weights) {
                let enc_dim = tts_config
                    .speaker_encoder_config
                    .as_ref()
                    .map(|c| c.enc_dim)
                    .unwrap_or(tts_config.talker_config.hidden_size);
                let se_config = speaker_encoder::SpeakerEncoderConfig::from_enc_dim(enc_dim);
                match speaker_encoder::load_speaker_encoder(&weights, &se_config) {
                    Ok(enc) => {
                        info!("Speaker encoder loaded (enc_dim={})", enc_dim);
                        Some(enc)
                    }
                    Err(e) => {
                        tracing::warn!("Failed to load speaker encoder: {}", e);
                        None
                    }
                }
            } else {
                tracing::warn!("Base model but no speaker_encoder.* weights found");
                None
            };

            // Load speech encoder (Mimi) for ICL voice cloning
            let st_model_path = model_dir.join("speech_tokenizer").join("model.safetensors");
            let spch = if st_model_path.exists() {
                info!("Loading speech encoder (Mimi) for ICL voice cloning...");
                let st_weights = mlx_rs::Array::load_safetensors(&st_model_path)?;
                if speech_encoder::has_encoder_weights(&st_weights) {
                    match speech_encoder::load_speech_encoder(&st_weights) {
                        Ok(enc) => {
                            info!("Speech encoder (Mimi) loaded");
                            Some(enc)
                        }
                        Err(e) => {
                            tracing::warn!("Failed to load speech encoder: {}", e);
                            None
                        }
                    }
                } else {
                    tracing::info!(
                        "No encoder.* weights in speech_tokenizer — ICL mode unavailable"
                    );
                    None
                }
            } else {
                None
            };

            (spk, spch)
        } else {
            (None, None)
        };

        info!("Models loaded successfully (type: {})", model_type);

        Ok(Self {
            talker,
            decoder,
            tts_config,
            gen_config,
            tokenizer,
            sample_rate: st_config.output_sample_rate,
            speaker_encoder: spk_encoder,
            speech_encoder: spch_encoder,
            speaker_embedding_cache: HashMap::new(),
        })
    }

    fn get_or_compute_speaker_embedding(
        &mut self,
        reference_audio: &[f32],
        cache_key: Option<&str>,
    ) -> Result<Array> {
        if let Some(key) = cache_key {
            if let Some(cached) = self.speaker_embedding_cache.get(key) {
                info!("Using cached speaker embedding: key='{}'", key);
                return Ok(cached.clone());
            }
        }

        let spk_encoder = self.speaker_encoder.as_mut().ok_or_else(|| {
            Error::Model("Voice cloning requires a Base model with speaker encoder".into())
        })?;

        info!(
            "Computing speaker embedding from reference audio ({} samples)...",
            reference_audio.len()
        );
        let mel_config = speaker_encoder::SpeakerMelConfig::default();
        let mel = speaker_encoder::compute_speaker_mel(reference_audio, &mel_config)?;
        let speaker_embedding = spk_encoder.forward(&mel)?;
        mlx_rs::transforms::eval(std::iter::once(&speaker_embedding))?;

        info!(
            "Speaker embedding computed: {:?}",
            speaker_embedding.shape()
        );

        if let Some(key) = cache_key {
            if self.speaker_embedding_cache.len() >= 64 {
                self.speaker_embedding_cache.clear();
            }
            self.speaker_embedding_cache
                .insert(key.to_string(), speaker_embedding.clone());
            info!("Cached speaker embedding: key='{}'", key);
        }

        Ok(speaker_embedding)
    }

    /// Detected model type (Base, CustomVoice, VoiceDesign).
    pub fn model_type(&self) -> config::ModelType {
        self.tts_config.model_type()
    }

    /// Whether this model supports preset speakers.
    pub fn supports_preset_speakers(&self) -> bool {
        self.model_type().supports_preset_speakers()
    }

    /// Whether this model supports voice cloning.
    pub fn supports_voice_cloning(&self) -> bool {
        self.model_type().supports_voice_cloning()
    }

    /// Whether this model supports voice design via text instructions.
    pub fn supports_voice_design(&self) -> bool {
        self.model_type().supports_voice_design()
    }

    /// Synthesize speech from text.
    /// Returns audio samples as f32 in [-1, 1] at 24kHz.
    pub fn synthesize(&mut self, text: &str, opts: &SynthesizeOptions) -> Result<Vec<f32>> {
        let (samples, _timing) = self.synthesize_with_timing(text, opts)?;
        Ok(samples)
    }

    /// Synthesize speech from text with detailed timing breakdown.
    /// Returns (audio samples, timing info).
    pub fn synthesize_with_timing(
        &mut self,
        text: &str,
        opts: &SynthesizeOptions,
    ) -> Result<(Vec<f32>, SynthesisTiming)> {
        let total_start = Instant::now();

        // Apply optional overrides
        let mut gen_config = self.gen_config.clone();
        if let Some(temp) = opts.temperature {
            gen_config.temperature = temp;
        }
        if let Some(k) = opts.top_k {
            gen_config.top_k = k;
        }
        if let Some(p) = opts.top_p {
            gen_config.top_p = p;
        }
        if let Some(n) = opts.max_new_tokens {
            gen_config.max_new_tokens = n;
        }
        if let Some(s) = opts.speed_factor {
            gen_config.speed_factor = s;
        }

        // Official CustomVoice wrapper tokenizes assistant ChatML and passes
        // optional user ChatML as a separate instruct input.
        let assistant_text = build_assistant_text(text);
        let encoding = self
            .tokenizer
            .encode(assistant_text.as_str(), false)
            .map_err(|e| Error::Model(format!("Tokenizer error: {e}")))?;
        let assistant_input_ids: Vec<u32> = encoding.get_ids().to_vec();

        let instruct_token_ids = instruct_if_present(opts.instruct)
            .map(|instruct| {
                let instruct_text = build_instruct_text(instruct);
                self.tokenizer
                    .encode(instruct_text.as_str(), false)
                    .map(|encoding| encoding.get_ids().to_vec())
                    .map_err(|e| Error::Model(format!("Tokenizer error (instruct): {e}")))
            })
            .transpose()?;

        info!(
            "CustomVoice tokenized: {} assistant tokens, {} instruct tokens",
            assistant_input_ids.len(),
            instruct_token_ids.as_ref().map_or(0, Vec::len)
        );

        // Build codec prefix
        let codec_prefix =
            build_codec_prefix(&self.tts_config.talker_config, opts.language, opts.speaker)?;

        // Generate codec frames
        let (codes, gen_timing) = generate_custom_voice(
            &mut self.talker,
            &assistant_input_ids,
            instruct_token_ids.as_deref().unwrap_or(&[]),
            &codec_prefix,
            &gen_config,
            &self.tts_config,
            opts.seed,
        )?;

        if codes.is_empty() {
            let timing = SynthesisTiming {
                prefill_ms: gen_timing.prefill_ms,
                generation_ms: gen_timing.generation_ms,
                generation_frames: 0,
                incomplete_clone: gen_timing.is_incomplete_clone(),
                decode_ms: 0.0,
                total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
            };
            return Ok((vec![], timing));
        }

        info!("Decoding {} codec frames to audio...", codes.len());

        // Decode to waveform
        let decode_start = Instant::now();
        let samples = self.decoder.decode(&codes)?;
        mlx_rs::transforms::eval(std::iter::empty::<&mlx_rs::Array>())?;
        let decode_ms = decode_start.elapsed().as_secs_f64() * 1000.0;

        let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;

        info!(
            "Generated {:.2}s of audio ({} samples at {}Hz)",
            samples.len() as f32 / self.sample_rate as f32,
            samples.len(),
            self.sample_rate
        );

        let timing = SynthesisTiming {
            prefill_ms: gen_timing.prefill_ms,
            generation_ms: gen_timing.generation_ms,
            generation_frames: gen_timing.generation_frames,
            incomplete_clone: gen_timing.is_incomplete_clone(),
            decode_ms,
            total_ms,
        };

        Ok((samples, timing))
    }

    /// Synthesize speech using VoiceDesign mode (voice described by text instruction).
    /// Requires a VoiceDesign model variant.
    /// `instruct` describes the desired voice characteristics (e.g., "A young woman with a warm, gentle voice").
    /// Returns audio samples as f32 in [-1, 1] at 24kHz.
    pub fn synthesize_voice_design(
        &mut self,
        text: &str,
        instruct: &str,
        language: &str,
        opts: &SynthesizeOptions,
    ) -> Result<Vec<f32>> {
        let (samples, _timing) =
            self.synthesize_voice_design_with_timing(text, instruct, language, opts)?;
        Ok(samples)
    }

    /// Synthesize speech using VoiceDesign mode with timing breakdown.
    pub fn synthesize_voice_design_with_timing(
        &mut self,
        text: &str,
        instruct: &str,
        language: &str,
        opts: &SynthesizeOptions,
    ) -> Result<(Vec<f32>, SynthesisTiming)> {
        let total_start = Instant::now();

        let mut gen_config = self.gen_config.clone();
        if let Some(temp) = opts.temperature {
            gen_config.temperature = temp;
        }
        if let Some(k) = opts.top_k {
            gen_config.top_k = k;
        }
        if let Some(p) = opts.top_p {
            gen_config.top_p = p;
        }
        if let Some(n) = opts.max_new_tokens {
            gen_config.max_new_tokens = n;
        }
        if let Some(s) = opts.speed_factor {
            gen_config.speed_factor = s;
        }

        // Tokenize text
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| Error::Model(format!("Tokenizer error: {e}")))?;
        let text_token_ids: Vec<u32> = encoding.get_ids().to_vec();

        // Tokenize instruct text with ChatML wrapping
        let chatml_instruct = format!("<|im_start|>user\n{}<|im_end|>\n", instruct);
        let instruct_encoding = self
            .tokenizer
            .encode(chatml_instruct.as_str(), false)
            .map_err(|e| Error::Model(format!("Tokenizer error (instruct): {e}")))?;
        let instruct_token_ids: Vec<u32> = instruct_encoding.get_ids().to_vec();

        info!(
            "VoiceDesign: {} text tokens, {} instruct tokens",
            text_token_ids.len(),
            instruct_token_ids.len()
        );

        // Build codec prefix (no speaker for VoiceDesign)
        let codec_prefix =
            build_codec_prefix_voice_design(&self.tts_config.talker_config, language)?;

        // Generate codec frames
        let (codes, gen_timing) = generate_voice_design(
            &mut self.talker,
            &text_token_ids,
            &instruct_token_ids,
            &codec_prefix,
            &gen_config,
            &self.tts_config,
            opts.seed,
        )?;

        if codes.is_empty() {
            let timing = SynthesisTiming {
                prefill_ms: gen_timing.prefill_ms,
                generation_ms: gen_timing.generation_ms,
                generation_frames: 0,
                incomplete_clone: gen_timing.is_incomplete_clone(),
                decode_ms: 0.0,
                total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
            };
            return Ok((vec![], timing));
        }

        info!("Decoding {} codec frames to audio...", codes.len());

        let decode_start = Instant::now();
        let samples = self.decoder.decode(&codes)?;
        mlx_rs::transforms::eval(std::iter::empty::<&mlx_rs::Array>())?;
        let decode_ms = decode_start.elapsed().as_secs_f64() * 1000.0;

        let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;

        info!(
            "Generated {:.2}s of audio ({} samples at {}Hz)",
            samples.len() as f32 / self.sample_rate as f32,
            samples.len(),
            self.sample_rate
        );

        let timing = SynthesisTiming {
            prefill_ms: gen_timing.prefill_ms,
            generation_ms: gen_timing.generation_ms,
            generation_frames: gen_timing.generation_frames,
            incomplete_clone: gen_timing.is_incomplete_clone(),
            decode_ms,
            total_ms,
        };

        Ok((samples, timing))
    }

    /// Synthesize speech using voice cloning (x_vector_only mode).
    /// Requires a Base model with speaker encoder.
    /// `reference_audio` is the reference audio samples (f32 at 24kHz).
    /// Returns audio samples as f32 in [-1, 1] at 24kHz.
    pub fn synthesize_voice_clone(
        &mut self,
        text: &str,
        reference_audio: &[f32],
        language: &str,
        opts: &SynthesizeOptions,
    ) -> Result<Vec<f32>> {
        let (samples, _timing) =
            self.synthesize_voice_clone_with_timing(text, reference_audio, language, opts)?;
        Ok(samples)
    }

    /// Synthesize speech using voice cloning and reuse speaker embedding by cache key.
    pub fn synthesize_voice_clone_cached(
        &mut self,
        text: &str,
        reference_audio: &[f32],
        language: &str,
        speaker_cache_key: &str,
        opts: &SynthesizeOptions,
    ) -> Result<Vec<f32>> {
        let (samples, _timing) = self.synthesize_voice_clone_with_timing_cached(
            text,
            reference_audio,
            language,
            speaker_cache_key,
            opts,
        )?;
        Ok(samples)
    }

    /// Synthesize speech using voice cloning with timing breakdown.
    pub fn synthesize_voice_clone_with_timing(
        &mut self,
        text: &str,
        reference_audio: &[f32],
        language: &str,
        opts: &SynthesizeOptions,
    ) -> Result<(Vec<f32>, SynthesisTiming)> {
        self.synthesize_voice_clone_with_timing_internal(
            text,
            reference_audio,
            language,
            None,
            opts,
        )
    }

    /// Synthesize speech using voice cloning with timing breakdown and embedding cache key.
    pub fn synthesize_voice_clone_with_timing_cached(
        &mut self,
        text: &str,
        reference_audio: &[f32],
        language: &str,
        speaker_cache_key: &str,
        opts: &SynthesizeOptions,
    ) -> Result<(Vec<f32>, SynthesisTiming)> {
        self.synthesize_voice_clone_with_timing_internal(
            text,
            reference_audio,
            language,
            Some(speaker_cache_key),
            opts,
        )
    }

    fn synthesize_voice_clone_with_timing_internal(
        &mut self,
        text: &str,
        reference_audio: &[f32],
        language: &str,
        speaker_cache_key: Option<&str>,
        opts: &SynthesizeOptions,
    ) -> Result<(Vec<f32>, SynthesisTiming)> {
        let total_start = Instant::now();

        let mut gen_config = self.gen_config.clone();
        if let Some(temp) = opts.temperature {
            gen_config.temperature = temp;
        }
        if let Some(k) = opts.top_k {
            gen_config.top_k = k;
        }
        if let Some(p) = opts.top_p {
            gen_config.top_p = p;
        }
        if let Some(n) = opts.max_new_tokens {
            gen_config.max_new_tokens = n;
        }
        if let Some(s) = opts.speed_factor {
            gen_config.speed_factor = s;
        }

        // Tokenize text
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| Error::Model(format!("Tokenizer error: {e}")))?;
        let text_token_ids: Vec<u32> = encoding.get_ids().to_vec();

        let speaker_embedding =
            self.get_or_compute_speaker_embedding(reference_audio, speaker_cache_key)?;

        // Build codec prefix for voice clone (think + explicit language)
        let codec_prefix =
            generate::build_codec_prefix_voice_design(&self.tts_config.talker_config, language)?;

        // Generate codec frames
        let (codes, gen_timing) = generate_voice_clone(
            &mut self.talker,
            &text_token_ids,
            &codec_prefix,
            &speaker_embedding,
            &gen_config,
            &self.tts_config,
            opts.seed,
        )?;

        if codes.is_empty() {
            let timing = SynthesisTiming {
                prefill_ms: gen_timing.prefill_ms,
                generation_ms: gen_timing.generation_ms,
                generation_frames: 0,
                incomplete_clone: gen_timing.is_incomplete_clone(),
                decode_ms: 0.0,
                total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
            };
            return Ok((vec![], timing));
        }

        info!("Decoding {} codec frames to audio...", codes.len());

        let decode_start = Instant::now();
        let samples = self.decoder.decode(&codes)?;
        mlx_rs::transforms::eval(std::iter::empty::<&mlx_rs::Array>())?;
        let decode_ms = decode_start.elapsed().as_secs_f64() * 1000.0;

        let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;

        info!(
            "Generated {:.2}s of audio ({} samples at {}Hz)",
            samples.len() as f32 / self.sample_rate as f32,
            samples.len(),
            self.sample_rate
        );

        let timing = SynthesisTiming {
            prefill_ms: gen_timing.prefill_ms,
            generation_ms: gen_timing.generation_ms,
            generation_frames: gen_timing.generation_frames,
            incomplete_clone: gen_timing.is_incomplete_clone(),
            decode_ms,
            total_ms,
        };

        Ok((samples, timing))
    }

    /// Synthesize speech using ICL voice cloning (full quality).
    /// Requires a Base model with both speaker encoder and speech encoder.
    /// Uses both speaker embedding AND reference audio codes for conditioning.
    /// `reference_audio` is the reference audio samples (f32 at 24kHz).
    /// `reference_text` is the transcript of the reference audio.
    pub fn synthesize_voice_clone_icl(
        &mut self,
        text: &str,
        reference_audio: &[f32],
        reference_text: &str,
        language: &str,
        opts: &SynthesizeOptions,
    ) -> Result<Vec<f32>> {
        let (samples, _timing) = self.synthesize_voice_clone_icl_with_timing(
            text,
            reference_audio,
            reference_text,
            language,
            opts,
        )?;
        Ok(samples)
    }

    /// Synthesize speech using ICL voice cloning and reuse speaker embedding by cache key.
    pub fn synthesize_voice_clone_icl_cached(
        &mut self,
        text: &str,
        reference_audio: &[f32],
        reference_text: &str,
        language: &str,
        speaker_cache_key: &str,
        opts: &SynthesizeOptions,
    ) -> Result<Vec<f32>> {
        let (samples, _timing) = self.synthesize_voice_clone_icl_with_timing_cached(
            text,
            reference_audio,
            reference_text,
            language,
            speaker_cache_key,
            opts,
        )?;
        Ok(samples)
    }

    /// Synthesize speech using ICL voice cloning with timing breakdown.
    pub fn synthesize_voice_clone_icl_with_timing(
        &mut self,
        text: &str,
        reference_audio: &[f32],
        reference_text: &str,
        language: &str,
        opts: &SynthesizeOptions,
    ) -> Result<(Vec<f32>, SynthesisTiming)> {
        self.synthesize_voice_clone_icl_with_timing_internal(
            text,
            reference_audio,
            reference_text,
            language,
            None,
            opts,
        )
    }

    /// Synthesize speech using ICL voice cloning with timing breakdown and embedding cache key.
    pub fn synthesize_voice_clone_icl_with_timing_cached(
        &mut self,
        text: &str,
        reference_audio: &[f32],
        reference_text: &str,
        language: &str,
        speaker_cache_key: &str,
        opts: &SynthesizeOptions,
    ) -> Result<(Vec<f32>, SynthesisTiming)> {
        self.synthesize_voice_clone_icl_with_timing_internal(
            text,
            reference_audio,
            reference_text,
            language,
            Some(speaker_cache_key),
            opts,
        )
    }

    fn synthesize_voice_clone_icl_with_timing_internal(
        &mut self,
        text: &str,
        reference_audio: &[f32],
        reference_text: &str,
        language: &str,
        speaker_cache_key: Option<&str>,
        opts: &SynthesizeOptions,
    ) -> Result<(Vec<f32>, SynthesisTiming)> {
        let total_start = Instant::now();

        if self.speech_encoder.is_none() {
            return Err(Error::Model(
                "ICL voice cloning requires a Base model with speech encoder (Mimi)".into(),
            ));
        }

        let speaker_embedding =
            self.get_or_compute_speaker_embedding(reference_audio, speaker_cache_key)?;

        info!("Speaker embedding: {:?}", speaker_embedding.shape());

        // Encode reference audio to codec frames via Mimi
        let spch_encoder = self.speech_encoder.as_mut().ok_or_else(|| {
            Error::Model(
                "ICL voice cloning requires a Base model with speech encoder (Mimi)".into(),
            )
        })?;
        let ref_codes = spch_encoder.encode(reference_audio)?;
        info!("Reference audio encoded: {} frames", ref_codes.len());

        let mut gen_config = self.gen_config.clone();
        if let Some(temp) = opts.temperature {
            gen_config.temperature = temp;
        }
        if let Some(k) = opts.top_k {
            gen_config.top_k = k;
        }
        if let Some(p) = opts.top_p {
            gen_config.top_p = p;
        }
        if let Some(n) = opts.max_new_tokens {
            gen_config.max_new_tokens = n;
        }
        if let Some(s) = opts.speed_factor {
            gen_config.speed_factor = s;
        }

        // Tokenize target text
        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| Error::Model(format!("Tokenizer error: {e}")))?;
        let text_token_ids: Vec<u32> = encoding.get_ids().to_vec();

        // Tokenize reference text
        let ref_encoding = self
            .tokenizer
            .encode(reference_text, false)
            .map_err(|e| Error::Model(format!("Tokenizer error (ref text): {e}")))?;
        let ref_text_ids: Vec<u32> = ref_encoding.get_ids().to_vec();

        info!(
            "ICL text tokens: target={:?} ({}), ref={:?} ({})",
            text_token_ids,
            text_token_ids.len(),
            ref_text_ids,
            ref_text_ids.len(),
        );

        // Build codec prefix for ICL: think + explicit language
        // TrevorS's working implementation uses CODEC_THINK with explicit language
        let codec_prefix =
            generate::build_codec_prefix_voice_design(&self.tts_config.talker_config, language)?;

        // Generate codec frames
        let (gen_codes, ref_code_frames, _ref_text_len, gen_timing) = generate_voice_clone_icl(
            &mut self.talker,
            &text_token_ids,
            &ref_text_ids,
            &ref_codes,
            &codec_prefix,
            &speaker_embedding,
            &gen_config,
            &self.tts_config,
            opts.seed,
        )?;

        if gen_codes.is_empty() {
            let timing = SynthesisTiming {
                prefill_ms: gen_timing.prefill_ms,
                generation_ms: gen_timing.generation_ms,
                generation_frames: 0,
                incomplete_clone: gen_timing.is_incomplete_clone(),
                decode_ms: 0.0,
                total_ms: total_start.elapsed().as_secs_f64() * 1000.0,
            };
            return Ok((vec![], timing));
        }

        // The decoder warmup path below already prepends `ref_code_frames` and then
        // removes the corresponding decoded reference samples from the waveform.
        //
        // Trimming generated frames again using a text-token ratio heuristic is too
        // aggressive for some samples and can remove valid target speech from the
        // beginning of the utterance. Keep the full generated codec sequence here.
        let trim_frames = 0usize;
        let target_codes = &gen_codes[..];

        info!(
            "ICL: {} ref frames, {} gen frames, trim={} frames, {} target frames",
            ref_code_frames.len(),
            gen_codes.len(),
            trim_frames,
            target_codes.len(),
        );

        // Prepend ref_codes as decoder warmup context (prevents cold-start artifacts),
        // then decode, then cut the ref portion from the audio output.
        let decode_start = Instant::now();
        let ref_len = ref_code_frames.len();
        let mut codes_for_decode = Vec::with_capacity(ref_len + target_codes.len());
        codes_for_decode.extend_from_slice(&ref_code_frames);
        codes_for_decode.extend_from_slice(target_codes);
        let total_decode_len = codes_for_decode.len();

        let all_samples = self.decoder.decode(&codes_for_decode)?;
        mlx_rs::transforms::eval(std::iter::empty::<&mlx_rs::Array>())?;

        // Cut ref portion from decoded audio (proportional to ref frames in decode input)
        let ref_audio_cut = if total_decode_len > 0 {
            (ref_len as f64 / total_decode_len as f64 * all_samples.len() as f64) as usize
        } else {
            0
        };
        let mut samples = all_samples[ref_audio_cut..].to_vec();

        // 50ms fade-in to eliminate residual decoder cold-start blip
        let fade_len = (0.05 * self.sample_rate as f64) as usize;
        if samples.len() > fade_len {
            for i in 0..fade_len {
                samples[i] *= i as f32 / fade_len as f32;
            }
        }

        let decode_ms = decode_start.elapsed().as_secs_f64() * 1000.0;
        let total_ms = total_start.elapsed().as_secs_f64() * 1000.0;

        info!(
            "Generated {:.2}s of audio ({} samples, trimmed {} gen frames, cut {} ref samples, 50ms fade-in)",
            samples.len() as f32 / self.sample_rate as f32,
            samples.len(),
            trim_frames,
            ref_audio_cut,
        );

        let timing = SynthesisTiming {
            prefill_ms: gen_timing.prefill_ms,
            generation_ms: gen_timing.generation_ms,
            generation_frames: gen_timing.generation_frames,
            incomplete_clone: gen_timing.is_incomplete_clone(),
            decode_ms,
            total_ms,
        };

        Ok((samples, timing))
    }

    /// Available speakers for CustomVoice model.
    pub fn speakers(&self) -> Vec<&str> {
        self.tts_config
            .talker_config
            .spk_id
            .keys()
            .map(|s| s.as_str())
            .collect()
    }

    /// Available languages.
    pub fn languages(&self) -> Vec<&str> {
        self.tts_config
            .talker_config
            .codec_language_id
            .keys()
            .map(|s| s.as_str())
            .collect()
    }

    /// Start a streaming synthesis session.
    /// Returns a `StreamingSession` that yields audio chunks via `next_chunk()`.
    /// Each chunk contains `chunk_frames` frames of decoded audio (~80ms per frame at 12Hz).
    pub fn start_streaming(
        &mut self,
        text: &str,
        opts: &SynthesizeOptions,
        chunk_frames: usize,
    ) -> Result<StreamingSession<'_>> {
        let mut gen_config = self.gen_config.clone();
        if let Some(temp) = opts.temperature {
            gen_config.temperature = temp;
        }
        if let Some(k) = opts.top_k {
            gen_config.top_k = k;
        }
        if let Some(p) = opts.top_p {
            gen_config.top_p = p;
        }
        if let Some(n) = opts.max_new_tokens {
            gen_config.max_new_tokens = n;
        }
        if let Some(s) = opts.speed_factor {
            gen_config.speed_factor = s;
        }

        let encoding = self
            .tokenizer
            .encode(text, false)
            .map_err(|e| Error::Model(format!("Tokenizer error: {e}")))?;
        let text_token_ids: Vec<u32> = encoding.get_ids().to_vec();

        info!(
            "Streaming: {} text tokens, chunk_frames={}",
            text_token_ids.len(),
            chunk_frames
        );

        let codec_prefix =
            build_codec_prefix(&self.tts_config.talker_config, opts.language, opts.speaker)?;

        let state = GenerationState::new(
            &mut self.talker,
            &text_token_ids,
            &codec_prefix,
            &gen_config,
            &self.tts_config,
            opts.seed,
        )?;

        Ok(StreamingSession {
            state,
            talker: &mut self.talker,
            decoder: &mut self.decoder,
            chunk_frames,
            total_samples: 0,
            sample_rate: self.sample_rate,
        })
    }
}

/// A streaming synthesis session that yields decoded audio chunks.
pub struct StreamingSession<'a> {
    state: GenerationState,
    talker: &'a mut Talker,
    decoder: &'a mut SpeechTokenizerDecoder,
    chunk_frames: usize,
    total_samples: usize,
    sample_rate: u32,
}

impl StreamingSession<'_> {
    /// Generate the next chunk of audio samples.
    /// Returns `Some(samples)` with decoded f32 audio, or `None` when generation is done.
    pub fn next_chunk(&mut self) -> Result<Option<Vec<f32>>> {
        let frames = self.state.next_chunk(self.talker, self.chunk_frames)?;
        match frames {
            None => Ok(None),
            Some(frames) => {
                let samples = self.decoder.decode(&frames)?;
                self.total_samples += samples.len();
                Ok(Some(samples))
            }
        }
    }

    /// Returns true if generation has finished.
    pub fn is_finished(&self) -> bool {
        self.state.is_finished()
    }

    /// Total audio frames generated so far.
    pub fn total_frames(&self) -> usize {
        self.state.total_frames()
    }

    /// Total audio samples decoded so far.
    pub fn total_samples(&self) -> usize {
        self.total_samples
    }

    /// Duration of audio generated so far in seconds.
    pub fn duration_secs(&self) -> f32 {
        self.total_samples as f32 / self.sample_rate as f32
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::time::{SystemTime, UNIX_EPOCH};

    #[test]
    fn assistant_text_matches_official_qwen_wrapper() {
        assert_eq!(
            build_assistant_text("你好"),
            "<|im_start|>assistant\n你好<|im_end|>\n<|im_start|>assistant\n"
        );
    }

    #[test]
    fn instruct_text_matches_official_qwen_wrapper() {
        assert_eq!(
            build_instruct_text("用特别愤怒的语气说"),
            "<|im_start|>user\n用特别愤怒的语气说<|im_end|>\n"
        );
    }

    #[test]
    fn instruct_presence_matches_official_wrapper() {
        assert_eq!(instruct_if_present(None), None);
        assert_eq!(instruct_if_present(Some("")), None);
        assert_eq!(instruct_if_present(Some("  ")), Some("  "));
        assert_eq!(instruct_if_present(Some(" angry ")), Some(" angry "));
    }

    #[test]
    fn tokenizer_registers_qwen_added_special_tokens() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let model_dir =
            std::env::temp_dir().join(format!("qwen3-tts-tokenizer-test-{}-{unique}", std::process::id()));
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(
            model_dir.join("vocab.json"),
            r#"{"<":0,"|":1,"i":2,"m":3,"_":4,"s":5,"t":6,"a":7,"r":8,">":9}"#,
        )
        .unwrap();
        fs::write(model_dir.join("merges.txt"), "#version: 0.2\n").unwrap();
        fs::write(
            model_dir.join("tokenizer_config.json"),
            r#"{
                "added_tokens_decoder": {
                    "10": {
                        "content": "<|im_start|>",
                        "lstrip": false,
                        "normalized": false,
                        "rstrip": false,
                        "single_word": false,
                        "special": true
                    }
                }
            }"#,
        )
        .unwrap();

        let tokenizer = load_bpe_tokenizer(&model_dir).unwrap();
        let encoding = tokenizer.encode("<|im_start|>", false).unwrap();

        fs::remove_dir_all(&model_dir).unwrap();
        assert_eq!(encoding.get_ids(), &[10]);
    }

    #[test]
    fn tokenizer_uses_qwen_byte_level_regex() {
        let unique = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let model_dir =
            std::env::temp_dir().join(format!("qwen3-tts-regex-test-{}-{unique}", std::process::id()));
        fs::create_dir_all(&model_dir).unwrap();
        fs::write(
            model_dir.join("vocab.json"),
            r#"{"a":0,"Ġ":1,"ĠĠ":2,"Ġa":3}"#,
        )
        .unwrap();
        fs::write(
            model_dir.join("merges.txt"),
            "#version: 0.2\nĠ Ġ\nĠ a\n",
        )
        .unwrap();

        let tokenizer = load_bpe_tokenizer(&model_dir).unwrap();
        let encoding = tokenizer.encode("a  a", false).unwrap();

        fs::remove_dir_all(&model_dir).unwrap();
        assert_eq!(encoding.get_ids(), &[0, 1, 3]);
    }

}

/// Load a BPE tokenizer from vocab.json + merges.txt (Qwen2 format).
fn load_bpe_tokenizer(model_dir: &Path) -> Result<tokenizers::Tokenizer> {
    use tokenizers::models::bpe::BPE;
    use tokenizers::{AddedToken, Tokenizer};

    #[derive(serde::Deserialize)]
    struct TokenizerConfig {
        #[serde(default)]
        added_tokens_decoder: std::collections::BTreeMap<u32, AddedTokenConfig>,
    }

    #[derive(serde::Deserialize)]
    struct AddedTokenConfig {
        content: String,
        #[serde(default)]
        single_word: bool,
        #[serde(default)]
        lstrip: bool,
        #[serde(default)]
        rstrip: bool,
        #[serde(default = "default_added_token_normalized")]
        normalized: bool,
        #[serde(default)]
        special: bool,
    }

    fn default_added_token_normalized() -> bool {
        true
    }

    let vocab_path = model_dir.join("vocab.json");
    let merges_path = model_dir.join("merges.txt");

    if !vocab_path.exists() || !merges_path.exists() {
        return Err(Error::Config(
            "vocab.json and merges.txt required for BPE tokenizer".to_string(),
        ));
    }

    let bpe = BPE::from_file(vocab_path.to_str().unwrap(), merges_path.to_str().unwrap())
        .build()
        .map_err(|e| Error::Model(format!("BPE build error: {e}")))?;

    let mut tokenizer = Tokenizer::new(bpe);

    let tokenizer_config_path = model_dir.join("tokenizer_config.json");
    if tokenizer_config_path.exists() {
        let file = std::fs::File::open(&tokenizer_config_path)?;
        let config: TokenizerConfig = serde_json::from_reader(file)?;
        let declared_tokens: Vec<(u32, AddedToken)> = config
            .added_tokens_decoder
            .into_iter()
            .map(|(id, token)| {
                let added = AddedToken::from(token.content, token.special)
                    .single_word(token.single_word)
                    .lstrip(token.lstrip)
                    .rstrip(token.rstrip)
                    .normalized(token.normalized);
                (id, added)
            })
            .collect();
        let added_tokens: Vec<AddedToken> = declared_tokens
            .iter()
            .map(|(_, token)| token.clone())
            .collect();
        tokenizer.add_tokens(&added_tokens);

        for (declared_id, token) in declared_tokens {
            let actual_id = tokenizer.token_to_id(&token.content);
            if actual_id != Some(declared_id) {
                return Err(Error::Config(format!(
                    "Tokenizer added token '{}' expected id {}, got {:?}",
                    token.content, declared_id, actual_id
                )));
            }
        }
    }

    // Add byte-level pre-tokenizer (matches Qwen2 tokenizer)
    use tokenizers::pre_tokenizers::byte_level::ByteLevel;
    tokenizer.with_pre_tokenizer(Some(ByteLevel::new(false, true, true)));

    // Add byte-level decoder
    use tokenizers::decoders::byte_level::ByteLevel as ByteLevelDecoder;
    tokenizer.with_decoder(Some(ByteLevelDecoder::new(false, true, false)));

    Ok(tokenizer)
}

/// Save audio samples as a WAV file (16-bit PCM, mono, 24kHz).
pub fn save_wav(samples: &[f32], sample_rate: u32, path: impl AsRef<Path>) -> Result<()> {
    mlx_rs_core::audio::save_wav(samples, sample_rate, path)?;
    Ok(())
}

/// Normalize audio to target peak amplitude.
/// Returns a new Vec with samples scaled so the peak equals `target_peak`.
pub fn normalize_audio(samples: &[f32], target_peak: f32) -> Vec<f32> {
    let peak = samples.iter().map(|s| s.abs()).fold(0.0f32, f32::max);
    if peak < 1e-8 {
        return samples.to_vec();
    }
    let gain = target_peak / peak;
    samples.iter().map(|s| s * gain).collect()
}
