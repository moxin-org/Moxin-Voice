//! moxin-tts-node: pure-Rust Dora TTS node powered by gpt-sovits-mlx.
//!
//! Inputs:
//!   text  – StringArray, one element per message, format: VOICE:<...>|<text>
//!
//! Outputs:
//!   audio           – Float32Array, 32 kHz f32 PCM
//!   status          – StringArray, human-readable status
//!   segment_complete – Float32Array (empty), signals end of utterance
//!   log             – StringArray, JSON log entry

mod protocol;
mod audio_post;
mod voice_registry;
mod voice_state;

use anyhow::Result;
use arrow::array::{Array, StringArray};
use audio_post::apply_runtime_audio_params;
use dora_node_api::{DoraNode, Event, IntoArrow, Parameter};
use gpt_sovits_mlx::voice_clone::SynthesisOptions;
use protocol::TtsRequest;
use std::collections::BTreeMap;
use voice_registry::VoiceRegistry;
use voice_state::{ActiveVoice, VoiceState};

#[derive(Debug, Clone, Default)]
struct TtsParams {
    speed: Option<f32>,
    pitch: Option<f32>,
    volume: Option<f32>,
}

fn parse_text_and_params(raw: &str) -> (String, TtsParams) {
    let mut text = raw.to_string();
    let mut params = TtsParams::default();

    // Unwrap up to two levels of JSON payload:
    // 1) {"prompt":"VOICE:...","speed":...}
    // 2) {"prompt":"{\"prompt\":\"VOICE:...\",\"speed\":...}"}
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

fn main() -> Result<()> {
    // Initialise tracing
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_env("LOG_LEVEL")
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("moxin-tts-node starting");

    // Load voice registry (voices.json)
    let registry = VoiceRegistry::load()?;
    let mut state = VoiceState::new();

    // Connect to Dora (init_from_env returns eyre::Result, wrap into anyhow)
    let (mut node, mut events) = DoraNode::init_from_env()
        .map_err(|e| anyhow::anyhow!("Failed to init Dora node: {}", e))?;

    tracing::info!("Connected to Dora dataflow");

    // Default voice settings (can be overridden per-message)
    let default_voice = std::env::var("VOICE_NAME").unwrap_or_else(|_| "Doubao".to_string());

    while let Some(event) = events.recv() {
        match event {
            Event::Input {
                id,
                metadata: _,
                data,
            } => {
                if id.as_str() != "text" {
                    continue;
                }

                // Decode Arrow StringArray
                let arr = match data.as_any().downcast_ref::<StringArray>() {
                    Some(a) if a.len() > 0 => a,
                    _ => {
                        tracing::warn!("Received non-string or empty input on 'text'");
                        continue;
                    }
                };
                let raw = arr.value(0);

                let (text_str, params) = parse_text_and_params(raw);
                tracing::info!(
                    "Received params: speed={:?}, pitch={:?}, volume={:?}",
                    params.speed, params.pitch, params.volume
                );

                // Parse protocol
                let request = match TtsRequest::parse(&text_str) {
                    Some(r) => r,
                    None => {
                        // Treat bare text as a request using the default voice
                        TtsRequest::Preset {
                            voice: default_voice.clone(),
                            text: text_str,
                        }
                    }
                };

                tracing::info!("TTS request: {:?}", request);
                send_status(&mut node, "synthesizing")?;

                match synthesize(&registry, &mut state, request, &params) {
                    Ok(samples) => {
                        let samples = apply_runtime_audio_params(samples, &params);
                        send_audio(&mut node, &samples)?;
                        send_segment_complete(&mut node)?;
                        send_status(&mut node, "done")?;
                        tracing::info!(
                            "Synthesis complete: {} samples ({:.1}s)",
                            samples.len(),
                            samples.len() as f32 / 32000.0
                        );
                    }
                    Err(e) => {
                        tracing::error!("Synthesis failed: {:#}", e);
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

    tracing::info!("moxin-tts-node stopped");
    Ok(())
}

// ---------------------------------------------------------------------------
// Synthesis
// ---------------------------------------------------------------------------

fn synthesize(
    registry: &VoiceRegistry,
    state: &mut VoiceState,
    request: TtsRequest,
    params: &TtsParams,
) -> Result<Vec<f32>> {
    let speed_override = params.speed.map(|s| s.clamp(0.5, 2.0));
    let options = SynthesisOptions {
        speed_override,
        ..Default::default()
    };

    match request {
        TtsRequest::Preset { voice, text } => {
            let (mut config, ref_wav, prompt_text, semantic_npy) = registry.config_for_preset(&voice)?;
            let key = ActiveVoice::Preset(voice);
            let cloner = state.get_or_load(config, key)?;
            if let Some(ref npy_path) = semantic_npy {
                // Few-shot mode with Python-extracted semantic codes.
                // Matches dora-primespeech quality: T2S is conditioned on both
                // reference text phonemes and the HuBERT semantic prefix.
                tracing::debug!("Using few-shot mode with precomputed codes: {}", npy_path);
                cloner.set_reference_with_precomputed_codes(&ref_wav, &prompt_text, npy_path)?;
            } else {
                // Fallback: zero-shot mode when .npy not yet extracted.
                // Run scripts/extract_all_prompt_semantic.py to enable few-shot mode.
                tracing::warn!(
                    "prompt_semantic.npy not found for this voice, using zero-shot fallback"
                );
                cloner.set_reference_audio(&ref_wav)?;
            }
            let out = cloner.synthesize_with_options(&text, options)?;
            Ok(out.samples)
        }

        TtsRequest::Custom {
            ref_wav,
            prompt_text,
            language: _,
            text,
        } => {
            let config = registry.config_for_custom(1.0);
            let key = ActiveVoice::Custom {
                ref_wav: ref_wav.clone(),
            };
            let cloner = state.get_or_load(config, key)?;
            cloner.set_reference_audio_with_text(&ref_wav, &prompt_text)?;
            let out = cloner.synthesize_with_options(&text, options)?;
            Ok(out.samples)
        }

        TtsRequest::Trained {
            gpt_path,
            sovits_path,
            ref_wav,
            prompt_text,
            language: _,
            text,
        } => {
            let config = registry.config_for_trained(&gpt_path, &sovits_path, 1.0);
            let key = ActiveVoice::Trained {
                gpt: gpt_path,
                sovits: sovits_path,
            };
            let cloner = state.get_or_load(config, key)?;
            cloner.set_reference_audio_with_text(&ref_wav, &prompt_text)?;
            let out = cloner.synthesize_with_options(&text, options)?;
            Ok(out.samples)
        }
    }
}

// ---------------------------------------------------------------------------
// Dora send helpers
// ---------------------------------------------------------------------------

fn send_audio(node: &mut DoraNode, samples: &[f32]) -> Result<()> {
    let data = samples.to_vec().into_arrow();
    let mut params: BTreeMap<String, Parameter> = BTreeMap::new();
    params.insert("sample_rate".to_string(), Parameter::Integer(32000));
    node.send_output("audio".into(), params, data)
        .map_err(|e| anyhow::anyhow!("send_output(audio) failed: {}", e))
}

fn send_status(node: &mut DoraNode, status: &str) -> Result<()> {
    let data = vec![status.to_string()].into_arrow();
    node.send_output("status".into(), BTreeMap::new(), data)
        .map_err(|e| anyhow::anyhow!("send_output(status) failed: {}", e))
}

fn send_segment_complete(node: &mut DoraNode) -> Result<()> {
    let data = Vec::<f32>::new().into_arrow();
    node.send_output("segment_complete".into(), BTreeMap::new(), data)
        .map_err(|e| anyhow::anyhow!("send_output(segment_complete) failed: {}", e))
}

fn send_log(node: &mut DoraNode, msg: &str) -> Result<()> {
    let data = vec![msg.to_string()].into_arrow();
    node.send_output("log".into(), BTreeMap::new(), data)
        .map_err(|e| anyhow::anyhow!("send_output(log) failed: {}", e))
}
