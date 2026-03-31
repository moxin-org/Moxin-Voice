//! qwen-tts-node: Dora TTS node powered by qwen3-tts-mlx.
//!
//! Inputs:
//!   text  – StringArray, accepts VOICE:* protocol (same envelope as moxin-tts-node)
//!
//! Outputs:
//!   audio            – Float32Array (24kHz PCM)
//!   status           – StringArray
//!   segment_complete – Float32Array (empty)
//!   log              – StringArray

mod protocol;
mod audio_post;

use anyhow::{anyhow, Context, Result};
use arrow::array::{Array, StringArray};
use audio_post::apply_runtime_audio_params;
use dora_node_api::{DoraNode, Event, IntoArrow, Parameter};
use protocol::TtsRequest;
use qwen3_tts_mlx::{SynthesizeOptions, Synthesizer};
use std::collections::{BTreeMap, HashMap};
use std::path::{Path, PathBuf};
use std::time::UNIX_EPOCH;

#[derive(Debug, Clone, Default)]
struct TtsParams {
    speed: Option<f32>,
    pitch: Option<f32>,
    volume: Option<f32>,
}

fn parse_text_and_params(raw: &str) -> (String, TtsParams) {
    let mut text = raw.to_string();
    let mut params = TtsParams::default();

    for _ in 0..6 {
        let trimmed = text.trim_start();
        if !trimmed.starts_with('{') {
            break;
        }
        let Ok(v) = serde_json::from_str::<serde_json::Value>(&text) else {
            break;
        };

        if params.speed.is_none() {
            params.speed = v
                .get("speed")
                .and_then(|x| x.as_f64().map(|n| n as f32).or_else(|| x.as_str()?.parse::<f32>().ok()));
        }
        if params.pitch.is_none() {
            params.pitch = v
                .get("pitch")
                .and_then(|x| x.as_f64().map(|n| n as f32).or_else(|| x.as_str()?.parse::<f32>().ok()));
        }
        if params.volume.is_none() {
            params.volume = v
                .get("volume")
                .and_then(|x| x.as_f64().map(|n| n as f32).or_else(|| x.as_str()?.parse::<f32>().ok()));
        }

        if let Some(prompt) = v.get("prompt").and_then(|p| p.as_str()) {
            text = prompt.to_string();
        } else {
            break;
        }
    }

    (text, params)
}

fn contains_cjk(text: &str) -> bool {
    text.chars().any(|c| ('\u{4E00}'..='\u{9FFF}').contains(&c))
}

fn normalize_language(lang: &str, text_hint: &str) -> String {
    let raw = lang.trim().to_lowercase();
    let mapped = match raw.as_str() {
        "zh" | "zh-cn" | "zh-hans" | "cn" | "chinese" => "chinese",
        "en" | "en-us" | "en-gb" | "english" => "english",
        "ja" | "jp" | "japanese" => "japanese",
        "ko" | "korean" => "korean",
        "de" | "german" => "german",
        "fr" | "french" => "french",
        "es" | "spanish" => "spanish",
        "ru" | "russian" => "russian",
        "ar" | "arabic" => "arabic",
        "vi" | "vietnamese" => "vietnamese",
        "pt" | "portuguese" => "portuguese",
        "th" | "thai" => "thai",
        _ => {
            if contains_cjk(text_hint) {
                "chinese"
            } else {
                "english"
            }
        }
    };
    mapped.to_string()
}

fn resolve_qwen_root() -> PathBuf {
    if let Ok(v) = std::env::var("QWEN3_TTS_MODEL_ROOT") {
        return PathBuf::from(v);
    }
    dirs::home_dir()
        .unwrap_or_else(|| PathBuf::from("."))
        .join(".OminiX/models/qwen3-tts-mlx")
}

fn resolve_customvoice_model_dir() -> PathBuf {
    if let Ok(v) = std::env::var("QWEN3_TTS_CUSTOMVOICE_MODEL_DIR") {
        return PathBuf::from(v);
    }
    resolve_qwen_root().join("Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit")
}

fn resolve_base_model_dir() -> PathBuf {
    if let Ok(v) = std::env::var("QWEN3_TTS_BASE_MODEL_DIR") {
        return PathBuf::from(v);
    }
    resolve_qwen_root().join("Qwen3-TTS-12Hz-1.7B-Base-8bit")
}

fn model_dir_ready(model_dir: &Path) -> bool {
    model_dir.join("config.json").exists()
        && model_dir.join("generation_config.json").exists()
        && model_dir.join("vocab.json").exists()
        && model_dir.join("merges.txt").exists()
        && (model_dir.join("model.safetensors").exists()
            || model_dir.join("model.safetensors.index.json").exists())
        && model_dir.join("speech_tokenizer/config.json").exists()
        && model_dir.join("speech_tokenizer/model.safetensors").exists()
}

fn resample_linear(input: &[f32], in_sr: u32, out_sr: u32) -> Vec<f32> {
    if in_sr == out_sr || input.is_empty() {
        return input.to_vec();
    }
    let ratio = out_sr as f64 / in_sr as f64;
    let out_len = ((input.len() as f64) * ratio).round().max(1.0) as usize;
    if input.len() == 1 {
        return vec![input[0]; out_len];
    }

    let in_last = (input.len() - 1) as f32;
    let out_last = (out_len - 1).max(1) as f32;
    let mut out = Vec::with_capacity(out_len);
    for i in 0..out_len {
        let pos = (i as f32) * in_last / out_last;
        let idx = pos.floor() as usize;
        let frac = pos - idx as f32;
        let a = input[idx];
        let b = input[(idx + 1).min(input.len() - 1)];
        out.push(a + (b - a) * frac);
    }
    out
}

fn load_wav_mono_f32(path: &str) -> Result<(Vec<f32>, u32)> {
    let mut reader = hound::WavReader::open(path)
        .with_context(|| format!("failed to open wav: {}", path))?;
    let spec = reader.spec();
    let channels = spec.channels.max(1) as usize;

    let mut interleaved: Vec<f32> = Vec::new();
    match spec.sample_format {
        hound::SampleFormat::Float => {
            for s in reader.samples::<f32>() {
                interleaved.push(s.context("invalid float wav sample")?);
            }
        }
        hound::SampleFormat::Int => {
            let bits = spec.bits_per_sample;
            if bits <= 16 {
                let scale = i16::MAX as f32;
                for s in reader.samples::<i16>() {
                    interleaved.push((s.context("invalid i16 wav sample")? as f32) / scale);
                }
            } else {
                // 24/32-bit PCM
                let scale = ((1_i64 << (bits.saturating_sub(1) as u32)) - 1) as f32;
                for s in reader.samples::<i32>() {
                    interleaved.push((s.context("invalid i32 wav sample")? as f32) / scale.max(1.0));
                }
            }
        }
    }

    if interleaved.is_empty() {
        return Err(anyhow!("reference audio is empty"));
    }

    let mono = if channels == 1 {
        interleaved
    } else {
        let frames = interleaved.len() / channels;
        let mut out = Vec::with_capacity(frames);
        for i in 0..frames {
            let mut sum = 0.0f32;
            for c in 0..channels {
                sum += interleaved[i * channels + c];
            }
            out.push(sum / channels as f32);
        }
        out
    };

    Ok((mono, spec.sample_rate))
}

struct QwenState {
    customvoice: Option<Synthesizer>,
    base: Option<Synthesizer>,
    ref_audio_cache: HashMap<String, Vec<f32>>,
}

impl QwenState {
    fn new() -> Self {
        Self {
            customvoice: None,
            base: None,
            ref_audio_cache: HashMap::new(),
        }
    }

    fn customvoice(&mut self) -> Result<&mut Synthesizer> {
        if self.customvoice.is_none() {
            let path = resolve_customvoice_model_dir();
            if !model_dir_ready(&path) {
                return Err(anyhow!(
                    "Qwen CustomVoice model not ready: {}",
                    path.display()
                ));
            }
            tracing::info!("Loading Qwen CustomVoice model: {}", path.display());
            self.customvoice = Some(Synthesizer::load(&path)?);
        }
        Ok(self.customvoice.as_mut().unwrap())
    }

    fn base(&mut self) -> Result<&mut Synthesizer> {
        if self.base.is_none() {
            let path = resolve_base_model_dir();
            if !model_dir_ready(&path) {
                return Err(anyhow!("Qwen Base model not ready: {}", path.display()));
            }
            tracing::info!("Loading Qwen Base model: {}", path.display());
            self.base = Some(Synthesizer::load(&path)?);
        }
        Ok(self.base.as_mut().unwrap())
    }
}

fn parse_positive_usize_env(name: &str, default_value: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default_value)
}

fn clone_ref_max_sec(xvector_mode: bool) -> usize {
    if xvector_mode {
        parse_positive_usize_env("MOXIN_QWEN_XVECTOR_REF_MAX_SEC", 6)
    } else {
        parse_positive_usize_env("MOXIN_QWEN_ICL_REF_MAX_SEC", 8)
    }
}

fn file_stamp(path: &str) -> Option<String> {
    let meta = std::fs::metadata(path).ok()?;
    let len = meta.len();
    let modified = meta.modified().ok()?;
    let ts = modified
        .duration_since(UNIX_EPOCH)
        .ok()
        .map(|d| d.as_secs())
        .unwrap_or(0);
    Some(format!("{len}:{ts}"))
}

fn build_ref_audio_cache_key(ref_wav: &str, xvector_mode: bool, max_sec: usize) -> String {
    let mode = if xvector_mode { "xvector" } else { "icl" };
    let stamp = file_stamp(ref_wav).unwrap_or_else(|| "nostamp".to_string());
    format!("{mode}|{max_sec}|{ref_wav}|{stamp}")
}

fn trim_reference_audio_in_place(samples_24k: &mut Vec<f32>, max_sec: usize) {
    let max_samples = max_sec.saturating_mul(24_000);
    if samples_24k.len() > max_samples {
        samples_24k.truncate(max_samples);
    }
}

fn prepare_reference_audio_for_clone(
    state: &mut QwenState,
    ref_wav: &str,
    xvector_mode: bool,
) -> Result<(Vec<f32>, String)> {
    let max_sec = clone_ref_max_sec(xvector_mode);
    let cache_key = build_ref_audio_cache_key(ref_wav, xvector_mode, max_sec);
    if let Some(cached) = state.ref_audio_cache.get(&cache_key) {
        tracing::info!(
            "Using cached clone reference audio (mode={}, {} samples @24k): {}",
            if xvector_mode { "xvector" } else { "icl" },
            cached.len(),
            ref_wav
        );
        return Ok((cached.clone(), cache_key));
    }
    let (ref_audio, ref_sr) = load_wav_mono_f32(ref_wav)?;
    let mut ref_audio_24k = resample_linear(&ref_audio, ref_sr, 24000);
    let original_len = ref_audio_24k.len();
    trim_reference_audio_in_place(&mut ref_audio_24k, max_sec);
    if ref_audio_24k.len() != original_len {
        tracing::info!(
            "Clone fast path (mode={}): trimmed reference audio to {} samples (~{:.2}s): {}",
            if xvector_mode { "xvector" } else { "icl" },
            ref_audio_24k.len(),
            ref_audio_24k.len() as f32 / 24_000.0,
            ref_wav
        );
    } else {
        tracing::info!(
            "Clone fast path (mode={}): reference audio kept at {} samples (~{:.2}s): {}",
            if xvector_mode { "xvector" } else { "icl" },
            ref_audio_24k.len(),
            ref_audio_24k.len() as f32 / 24_000.0,
            ref_wav
        );
    }
    // Bound cache size to avoid unbounded growth for many custom voices.
    if state.ref_audio_cache.len() >= 32 {
        state.ref_audio_cache.clear();
    }
    state
        .ref_audio_cache
        .insert(cache_key.clone(), ref_audio_24k.clone());
    Ok((ref_audio_24k, cache_key))
}

fn choose_speaker(synth: &Synthesizer, requested: &str, language: &str) -> String {
    let speakers: Vec<String> = synth.speakers().into_iter().map(|s| s.to_string()).collect();
    if speakers.is_empty() {
        tracing::warn!(
            "Qwen speaker list empty, fallback speaker='vivian' (requested='{}', language='{}')",
            requested,
            language
        );
        return "vivian".to_string();
    }

    let req = requested.trim().to_lowercase();
    if let Some(hit) = speakers
        .iter()
        .find(|s| s.to_lowercase() == req)
        .cloned()
    {
        tracing::info!(
            "Qwen speaker resolved: requested='{}' -> speaker='{}' (language='{}', mode=direct_match)",
            requested,
            hit,
            language
        );
        return hit;
    }

    let mapped = match req.as_str() {
        // Existing built-ins in this app mapped to sensible qwen defaults.
        "doubao" | "luo xiang" | "yang mi" | "zhou jielun" | "ma yun" | "shen yi"
        | "chen yifan" => {
            std::env::var("MOXIN_QWEN_ZH_SPEAKER").unwrap_or_else(|_| "serena".to_string())
        }
        _ => {
            if language == "chinese" {
                std::env::var("MOXIN_QWEN_ZH_SPEAKER").unwrap_or_else(|_| "serena".to_string())
            } else {
                std::env::var("MOXIN_QWEN_EN_SPEAKER").unwrap_or_else(|_| "vivian".to_string())
            }
        }
    };

    if speakers.iter().any(|s| s == &mapped) {
        tracing::info!(
            "Qwen speaker resolved: requested='{}' -> speaker='{}' (language='{}', mode=fallback_map)",
            requested,
            mapped,
            language
        );
        mapped
    } else {
        let fallback = speakers[0].clone();
        tracing::warn!(
            "Qwen speaker fallback to first entry: requested='{}', mapped='{}', chosen='{}', language='{}'",
            requested,
            mapped,
            fallback,
            language
        );
        fallback
    }
}

fn resolve_max_new_tokens() -> i32 {
    std::env::var("MOXIN_QWEN_MAX_NEW_TOKENS")
        .ok()
        .and_then(|v| v.parse::<i32>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(4096)
}

fn synthesize_qwen(state: &mut QwenState, request: TtsRequest, _params: &TtsParams) -> Result<(Vec<f32>, u32)> {
    match request {
        TtsRequest::Preset { voice, text } => {
            let synth = state.customvoice()?;
            let lang = normalize_language("", &text);
            let speaker = choose_speaker(synth, &voice, &lang);
            let max_new_tokens = resolve_max_new_tokens();
            tracing::info!(
                "Qwen generation config: speaker='{}', language='{}', max_new_tokens={}",
                speaker,
                lang,
                max_new_tokens
            );
            let options = SynthesizeOptions {
                speaker: &speaker,
                language: &lang,
                max_new_tokens: Some(max_new_tokens),
                ..Default::default()
            };
            let samples = synth.synthesize(&text, &options)?;
            Ok((samples, synth.sample_rate))
        }
        TtsRequest::Custom {
            ref_wav,
            prompt_text,
            language,
            text,
        } => {
            let lang = normalize_language(&language, &text);
            let use_xvector = prompt_text.trim().is_empty();
            let (ref_audio_24k, speaker_cache_key) =
                prepare_reference_audio_for_clone(state, &ref_wav, use_xvector)?;
            let synth = state.base()?;
            let max_new_tokens = resolve_max_new_tokens();
            tracing::info!(
                "Qwen clone generation config: language='{}', max_new_tokens={}",
                lang,
                max_new_tokens
            );

            let options = SynthesizeOptions {
                language: &lang,
                max_new_tokens: Some(max_new_tokens),
                ..Default::default()
            };

            // Prefer ICL when prompt text available; fallback to x-vector mode on hard failure.
            let samples = if use_xvector {
                synth.synthesize_voice_clone_cached(
                    &text,
                    &ref_audio_24k,
                    &lang,
                    &speaker_cache_key,
                    &options,
                )?
            } else {
                match synth.synthesize_voice_clone_icl_cached(
                    &text,
                    &ref_audio_24k,
                    &prompt_text,
                    &lang,
                    &speaker_cache_key,
                    &options,
                ) {
                    Ok(samples) => samples,
                    Err(err) => {
                        tracing::warn!(
                            "ICL clone failed, fallback to x-vector mode: {}",
                            err
                        );
                        synth.synthesize_voice_clone_cached(
                            &text,
                            &ref_audio_24k,
                            &lang,
                            &speaker_cache_key,
                            &options,
                        )?
                    }
                }
            };
            Ok((samples, synth.sample_rate))
        }
        TtsRequest::Trained { .. } => Err(anyhow!(
            "Qwen backend does not support VOICE:TRAINED custom-weight inference yet"
        )),
    }
}

fn send_audio(node: &mut DoraNode, samples: &[f32], sample_rate: u32) -> Result<()> {
    let data = samples.to_vec().into_arrow();
    let mut params: BTreeMap<String, Parameter> = BTreeMap::new();
    params.insert("sample_rate".to_string(), Parameter::Integer(sample_rate as i64));
    node.send_output("audio".into(), params, data)
        .map_err(|e| anyhow!("send_output(audio) failed: {}", e))
}

fn send_status(node: &mut DoraNode, s: &str) -> Result<()> {
    let arr = vec![s.to_string()].into_arrow();
    node.send_output("status".into(), Default::default(), arr)
        .map_err(|e| anyhow!("send_output(status) failed: {}", e))
}

fn send_log(node: &mut DoraNode, s: &str) -> Result<()> {
    let arr = vec![s.to_string()].into_arrow();
    node.send_output("log".into(), Default::default(), arr)
        .map_err(|e| anyhow!("send_output(log) failed: {}", e))
}

fn send_segment_complete(node: &mut DoraNode) -> Result<()> {
    let empty: Vec<f32> = Vec::new();
    let arr = empty.into_arrow();
    node.send_output("segment_complete".into(), Default::default(), arr)
        .map_err(|e| anyhow!("send_output(segment_complete) failed: {}", e))
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_env("LOG_LEVEL")
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("qwen-tts-node starting");

    let (mut node, mut events) = DoraNode::init_from_env()
        .map_err(|e| anyhow!("Failed to init Dora node: {}", e))?;

    tracing::info!("Connected to Dora dataflow");

    let default_voice = std::env::var("VOICE_NAME").unwrap_or_else(|_| "Doubao".to_string());
    let mut qwen_state = QwenState::new();

    while let Some(event) = events.recv() {
        match event {
            Event::Input { id, data, .. } => {
                if id.as_str() != "text" {
                    continue;
                }

                let arr = match data.as_any().downcast_ref::<StringArray>() {
                    Some(a) if a.len() > 0 => a,
                    _ => {
                        tracing::warn!("Received non-string or empty input on 'text'");
                        continue;
                    }
                };

                let raw = arr.value(0);
                let (text_str, params) = parse_text_and_params(raw);
                let request = match TtsRequest::parse(&text_str) {
                    Some(r) => r,
                    None => TtsRequest::Preset {
                        voice: default_voice.clone(),
                        text: text_str,
                    },
                };

                tracing::info!(
                    "Qwen request: speed={:?}, pitch={:?}, volume={:?}",
                    params.speed,
                    params.pitch,
                    params.volume
                );
                send_status(&mut node, "synthesizing")?;

                match synthesize_qwen(&mut qwen_state, request, &params) {
                    Ok((samples, sample_rate)) => {
                        let samples = apply_runtime_audio_params(samples, &params);
                        send_audio(&mut node, &samples, sample_rate)?;
                        send_segment_complete(&mut node)?;
                        send_status(&mut node, "done")?;
                        tracing::info!(
                            "Qwen synthesis complete: {} samples ({:.1}s @ {}Hz)",
                            samples.len(),
                            samples.len() as f32 / sample_rate as f32,
                            sample_rate
                        );
                    }
                    Err(e) => {
                        tracing::error!("Qwen synthesis failed: {:#}", e);
                        send_status(&mut node, &format!("error: {}", e))?;
                        send_log(&mut node, &format!("{{\"error\": \"{}\"}}", e))?;
                    }
                }
            }
            Event::Stop(_) => {
                tracing::info!("Stop event received, shutting down");
                break;
            }
            _ => {}
        }
    }

    tracing::info!("qwen-tts-node stopped");
    Ok(())
}
