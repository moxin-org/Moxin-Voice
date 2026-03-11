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
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

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
    resolve_qwen_root().join("Qwen3-TTS-12Hz-1.7B-Base")
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
}

impl QwenState {
    fn new() -> Self {
        Self {
            customvoice: None,
            base: None,
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

fn choose_speaker(synth: &Synthesizer, requested: &str, language: &str) -> String {
    let speakers: Vec<String> = synth.speakers().into_iter().map(|s| s.to_string()).collect();
    if speakers.is_empty() {
        return "vivian".to_string();
    }

    let req = requested.trim().to_lowercase();
    if let Some(hit) = speakers
        .iter()
        .find(|s| s.to_lowercase() == req)
        .cloned()
    {
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
        mapped
    } else {
        speakers[0].clone()
    }
}

fn synthesize_qwen(state: &mut QwenState, request: TtsRequest, params: &TtsParams) -> Result<(Vec<f32>, u32)> {
    match request {
        TtsRequest::Preset { voice, text } => {
            let synth = state.customvoice()?;
            let lang = normalize_language("", &text);
            let speaker = choose_speaker(synth, &voice, &lang);
            let options = SynthesizeOptions {
                speaker: &speaker,
                language: &lang,
                speed_factor: params.speed.map(|s| s.clamp(0.5, 2.0)),
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
            let synth = state.base()?;
            let lang = normalize_language(&language, &text);
            let (ref_audio, ref_sr) = load_wav_mono_f32(&ref_wav)?;
            let ref_audio_24k = resample_linear(&ref_audio, ref_sr, 24000);

            let options = SynthesizeOptions {
                language: &lang,
                speed_factor: params.speed.map(|s| s.clamp(0.5, 2.0)),
                ..Default::default()
            };

            // Prefer ICL when prompt text available; fallback to x-vector mode.
            let samples = if prompt_text.trim().is_empty() {
                synth.synthesize_voice_clone(&text, &ref_audio_24k, &lang, &options)?
            } else {
                match synth.synthesize_voice_clone_icl(
                    &text,
                    &ref_audio_24k,
                    &prompt_text,
                    &lang,
                    &options,
                ) {
                    Ok(samples) => samples,
                    Err(err) => {
                        tracing::warn!(
                            "ICL clone failed, fallback to x-vector mode: {}",
                            err
                        );
                        synth.synthesize_voice_clone(&text, &ref_audio_24k, &lang, &options)?
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
