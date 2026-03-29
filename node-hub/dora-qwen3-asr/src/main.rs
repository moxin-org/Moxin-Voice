//! dora-qwen3-asr: Dora ASR node powered by qwen3-asr-mlx.
//!
//! Inputs:
//!   audio  – Float32Array (16kHz mono PCM samples)
//!            metadata: sample_rate (u32, default 16000), language (str, default "Chinese")
//!
//! Outputs:
//!   transcription – StringArray (single element: transcribed text)
//!   log           – StringArray (status/debug messages)

use anyhow::{anyhow, Result};
use arrow::array::{Array, Float32Array};
use dora_node_api::{DoraNode, Event, IntoArrow};
use qwen3_asr_mlx::{default_model_path, Qwen3ASR};
use std::collections::BTreeMap;
use std::path::PathBuf;

fn resolve_model_path() -> PathBuf {
    // 1. Explicit env override (ignore empty string)
    if let Ok(v) = std::env::var("QWEN3_ASR_MODEL_PATH") {
        if !v.trim().is_empty() {
            return PathBuf::from(v);
        }
    }
    // 2. Delegate to the crate's own resolver (~/.OminiX/models/qwen3-asr-1.7b)
    default_model_path()
}

fn normalize_language(lang: &str) -> String {
    let raw = lang.trim().to_lowercase();
    match raw.as_str() {
        "zh" | "zh-cn" | "zh-hans" | "cn" | "chinese" => "Chinese".to_string(),
        "en" | "en-us" | "en-gb" | "english" => "English".to_string(),
        "ja" | "jp" | "japanese" => "Japanese".to_string(),
        "ko" | "korean" => "Korean".to_string(),
        "de" | "german" => "German".to_string(),
        "fr" | "french" => "French".to_string(),
        "es" | "spanish" => "Spanish".to_string(),
        "ru" | "russian" => "Russian".to_string(),
        "ar" | "arabic" => "Arabic".to_string(),
        // Default: pass through the original (capitalized for the model)
        other => {
            let mut s = other.to_string();
            if let Some(first) = s.get_mut(0..1) {
                first.make_ascii_uppercase();
            }
            s
        }
    }
}

fn send_transcription(
    node: &mut DoraNode,
    text: &str,
    question_id: Option<i64>,
    transcription_mode: Option<&str>,
) -> Result<()> {
    let arr = vec![text.to_string()].into_arrow();
    let mut meta = BTreeMap::new();
    if let Some(qid) = question_id {
        meta.insert(
            "question_id".to_string(),
            dora_node_api::Parameter::Integer(qid),
        );
    }
    if let Some(mode) = transcription_mode {
        meta.insert(
            "transcription_mode".to_string(),
            dora_node_api::Parameter::String(mode.to_string()),
        );
    }
    node.send_output("transcription".into(), meta, arr)
        .map_err(|e| anyhow!("send_output(transcription) failed: {}", e))
}

fn send_log(node: &mut DoraNode, msg: &str) -> Result<()> {
    let arr = vec![msg.to_string()].into_arrow();
    node.send_output("log".into(), BTreeMap::new(), arr)
        .map_err(|e| anyhow!("send_output(log) failed: {}", e))
}

fn preview_text_for_log(text: &str, max_chars: usize) -> &str {
    if max_chars == 0 {
        return "";
    }
    match text.char_indices().nth(max_chars) {
        Some((idx, _)) => &text[..idx],
        None => text,
    }
}

#[cfg(test)]
mod tests {
    use super::preview_text_for_log;

    #[test]
    fn preview_text_for_log_handles_multibyte_without_panic() {
        let s = "现在已经可以重新启动，看看是否还出现那条“apply”错误。如果还在，我会。";
        let out = preview_text_for_log(s, 100);
        assert!(!out.is_empty());
        assert!(s.starts_with(out));
        assert!(out.is_char_boundary(out.len()));
    }
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_env("LOG_LEVEL")
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("dora-qwen3-asr starting");

    let (mut node, mut events) = DoraNode::init_from_env()
        .map_err(|e| anyhow!("Failed to init Dora node: {}", e))?;

    tracing::info!("Connected to Dora dataflow");

    // Default language from env (e.g. LANGUAGE=zh or LANGUAGE=Chinese)
    let default_language = std::env::var("LANGUAGE")
        .map(|v| normalize_language(&v))
        .unwrap_or_else(|_| "Chinese".to_string());

    tracing::info!("Default language: {}", default_language);

    // Load model
    let model_path = resolve_model_path();
    tracing::info!("Loading Qwen3-ASR model from: {}", model_path.display());

    let _ = send_log(
        &mut node,
        &format!("Loading Qwen3-ASR model from {}", model_path.display()),
    );

    let mut model = match Qwen3ASR::load(&model_path) {
        Ok(m) => {
            tracing::info!("Qwen3-ASR model loaded successfully");
            let _ = send_log(&mut node, "Qwen3-ASR model loaded");
            m
        }
        Err(e) => {
            let msg = format!("Failed to load Qwen3-ASR model: {:#}", e);
            tracing::error!("{}", msg);
            let _ = send_log(&mut node, &msg);
            return Err(anyhow!(msg));
        }
    };

    while let Some(event) = events.recv() {
        match event {
            Event::Input { id, data, metadata } => {
                if id.as_str() != "audio" {
                    continue;
                }

                // Extract audio samples (Float32Array)
                let samples_arr = match data.as_any().downcast_ref::<Float32Array>() {
                    Some(arr) if arr.len() > 0 => arr,
                    Some(_) => {
                        tracing::warn!("Received empty audio array, skipping");
                        continue;
                    }
                    None => {
                        tracing::warn!("Received non-Float32 data on 'audio', skipping");
                        continue;
                    }
                };

                let samples: Vec<f32> = (0..samples_arr.len())
                    .map(|i| samples_arr.value(i))
                    .collect();

                // Read sample_rate from metadata (default 16000)
                let sample_rate: u32 = metadata
                    .parameters
                    .get("sample_rate")
                    .and_then(|p| match p {
                        dora_node_api::Parameter::Integer(v) => Some(*v as u32),
                        _ => None,
                    })
                    .unwrap_or(16000);

                // Read language from metadata, fallback to env default
                let language: String = metadata
                    .parameters
                    .get("language")
                    .and_then(|p| match p {
                        dora_node_api::Parameter::String(s) => Some(normalize_language(s)),
                        _ => None,
                    })
                    .unwrap_or_else(|| default_language.clone());
                let question_id: Option<i64> = metadata
                    .parameters
                    .get("question_id")
                    .and_then(|p| match p {
                        dora_node_api::Parameter::Integer(v) => Some(*v),
                        _ => None,
                    });
                let transcription_mode: Option<String> = metadata
                    .parameters
                    .get("transcription_mode")
                    .and_then(|p| match p {
                        dora_node_api::Parameter::String(s) => Some(s.clone()),
                        _ => None,
                    });

                let duration_secs = samples.len() as f32 / sample_rate.max(1) as f32;
                tracing::info!(
                    "Received audio: {} samples @ {}Hz ({:.2}s), language={}",
                    samples.len(),
                    sample_rate,
                    duration_secs,
                    language
                );

                let _ = send_log(
                    &mut node,
                    &format!(
                        "Transcribing {:.2}s of audio ({}Hz, lang={})",
                        duration_secs, sample_rate, language
                    ),
                );

                // Resample to 16kHz if needed
                let samples_16k = if sample_rate != 16000 {
                    tracing::info!("Resampling {}Hz -> 16000Hz", sample_rate);
                    match qwen3_asr_mlx::audio::resample(&samples, sample_rate, 16000) {
                        Ok(resampled) => resampled,
                        Err(e) => {
                            let msg = format!("Resample failed: {}", e);
                            tracing::error!("{}", msg);
                            let _ = send_log(&mut node, &msg);
                            let _ = send_transcription(&mut node, "", question_id, transcription_mode.as_deref());
                            continue;
                        }
                    }
                } else {
                    samples
                };

                // Transcribe
                let start = std::time::Instant::now();
                match model.transcribe_samples(&samples_16k, &language) {
                    Ok(text) => {
                        let elapsed = start.elapsed().as_secs_f32();
                        tracing::info!(
                            "Transcription complete in {:.2}s ({:.1}x realtime): {}",
                            elapsed,
                            duration_secs / elapsed.max(0.001),
                            preview_text_for_log(&text, 100)
                        );

                        let _ = send_log(
                            &mut node,
                            &format!(
                                "Transcribed in {:.2}s ({:.1}x realtime)",
                                elapsed,
                                duration_secs / elapsed.max(0.001)
                            ),
                        );

                        send_transcription(&mut node, &text, question_id, transcription_mode.as_deref())?;
                    }
                    Err(e) => {
                        let msg = format!("Transcription failed: {:#}", e);
                        tracing::error!("{}", msg);
                        let _ = send_log(&mut node, &msg);
                        // Send empty transcription on error (matching dora-asr behavior)
                        let _ = send_transcription(&mut node, "", question_id, transcription_mode.as_deref());
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

    tracing::info!("dora-qwen3-asr stopped");
    Ok(())
}
