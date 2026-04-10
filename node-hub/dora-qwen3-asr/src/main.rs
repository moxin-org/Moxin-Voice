//! dora-qwen3-asr: Dora ASR node powered by qwen3-asr-mlx.
//!
//! Inputs:
//!   audio          – Float32Array (16kHz mono PCM samples)
//!                    metadata: sample_rate (u32, default 16000), language (str, default "Chinese")
//!   question_ended – marker from upstream VAD; forwarded as a transcription metadata flag
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

fn build_transcription_meta(
    question_id: Option<i64>,
    burst_id: Option<i64>,
    transcription_mode: Option<&str>,
    segment_reason: Option<&str>,
    question_ended: bool,
    question_ended_only: bool,
) -> BTreeMap<String, dora_node_api::Parameter> {
    let mut meta = BTreeMap::new();
    if let Some(qid) = question_id {
        meta.insert(
            "question_id".to_string(),
            dora_node_api::Parameter::Integer(qid),
        );
    }
    if let Some(bid) = burst_id {
        meta.insert(
            "burst_id".to_string(),
            dora_node_api::Parameter::Integer(bid),
        );
    }
    if let Some(mode) = transcription_mode {
        meta.insert(
            "transcription_mode".to_string(),
            dora_node_api::Parameter::String(mode.to_string()),
        );
    }
    if let Some(reason) = segment_reason {
        meta.insert(
            "segment_reason".to_string(),
            dora_node_api::Parameter::String(reason.to_string()),
        );
    }
    if question_ended {
        meta.insert(
            "question_ended".to_string(),
            dora_node_api::Parameter::Bool(true),
        );
    }
    if question_ended_only {
        meta.insert(
            "question_ended_only".to_string(),
            dora_node_api::Parameter::Bool(true),
        );
    }
    meta
}

fn send_transcription(
    node: &mut DoraNode,
    text: &str,
    question_id: Option<i64>,
    burst_id: Option<i64>,
    transcription_mode: Option<&str>,
    segment_reason: Option<&str>,
    question_ended: bool,
    question_ended_only: bool,
) -> Result<()> {
    let arr = vec![text.to_string()].into_arrow();
    let meta = build_transcription_meta(
        question_id,
        burst_id,
        transcription_mode,
        segment_reason,
        question_ended,
        question_ended_only,
    );
    node.send_output("transcription".into(), meta, arr)
        .map_err(|e| anyhow!("send_output(transcription) failed: {}", e))
}

fn send_question_ended_marker(
    node: &mut DoraNode,
    question_id: Option<i64>,
    burst_id: Option<i64>,
) -> Result<()> {
    send_transcription(
        node,
        "",
        question_id,
        burst_id,
        Some("marker"),
        None,
        true,
        true,
    )
}

fn should_emit_question_ended_marker_before_question(
    pending_question_ended_qid: Option<i64>,
    last_emitted_question_id: Option<i64>,
    next_question_id: Option<i64>,
) -> bool {
    match pending_question_ended_qid {
        Some(pending_qid) => {
            Some(pending_qid) == last_emitted_question_id && Some(pending_qid) != next_question_id
        }
        None => false,
    }
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
    use super::{
        build_transcription_meta, preview_text_for_log,
        should_emit_question_ended_marker_before_question,
    };
    use dora_node_api::Parameter;

    #[test]
    fn preview_text_for_log_handles_multibyte_without_panic() {
        let s = "现在已经可以重新启动，看看是否还出现那条“apply”错误。如果还在，我会。";
        let out = preview_text_for_log(s, 100);
        assert!(!out.is_empty());
        assert!(s.starts_with(out));
        assert!(out.is_char_boundary(out.len()));
    }

    #[test]
    fn build_transcription_meta_includes_question_ended_flag_only_when_requested() {
        let meta = build_transcription_meta(
            Some(123),
            Some(456),
            Some("final"),
            Some("speech_end"),
            true,
            true,
        );
        assert!(matches!(meta.get("question_id"), Some(Parameter::Integer(123))));
        assert!(matches!(meta.get("burst_id"), Some(Parameter::Integer(456))));
        assert!(matches!(
            meta.get("transcription_mode"),
            Some(Parameter::String(mode)) if mode == "final"
        ));
        assert!(matches!(
            meta.get("segment_reason"),
            Some(Parameter::String(reason)) if reason == "speech_end"
        ));
        assert!(matches!(meta.get("question_ended"), Some(Parameter::Bool(true))));
        assert!(matches!(
            meta.get("question_ended_only"),
            Some(Parameter::Bool(true))
        ));

        let meta = build_transcription_meta(Some(123), None, None, None, false, false);
        assert!(!meta.contains_key("question_ended"));
        assert!(!meta.contains_key("question_ended_only"));
    }

    #[test]
    fn pending_question_ended_marker_emits_only_before_next_question() {
        assert!(!should_emit_question_ended_marker_before_question(
            None,
            Some(123),
            Some(456)
        ));
        assert!(!should_emit_question_ended_marker_before_question(
            Some(123),
            Some(123),
            Some(123)
        ));
        assert!(should_emit_question_ended_marker_before_question(
            Some(123),
            Some(123),
            Some(456)
        ));
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

    let mut pending_question_ended_qid: Option<i64> = None;
    let mut last_emitted_question_id: Option<i64> = None;
    let mut last_emitted_burst_id: Option<i64> = None;

    while let Some(event) = events.recv() {
        match event {
            Event::Input { id, data, metadata } => {
                if id.as_str() == "question_ended" {
                    let question_id = metadata
                        .parameters
                        .get("question_id")
                        .and_then(|p| match p {
                            dora_node_api::Parameter::Integer(v) => Some(*v),
                            _ => None,
                        });
                    tracing::info!(
                        "Received question_ended marker from upstream (question_id={:?})",
                        question_id
                    );
                    pending_question_ended_qid = question_id;
                    let _ = send_log(
                        &mut node,
                        &format!(
                            "Queued question_ended marker until next question transition (question_id={question_id:?})"
                        ),
                    );
                    continue;
                }

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
                let burst_id: Option<i64> = metadata
                    .parameters
                    .get("burst_id")
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
                let segment_reason: Option<String> = metadata
                    .parameters
                    .get("segment_reason")
                    .and_then(|p| match p {
                        dora_node_api::Parameter::String(s) => Some(s.clone()),
                        _ => None,
                    });

                if should_emit_question_ended_marker_before_question(
                    pending_question_ended_qid,
                    last_emitted_question_id,
                    question_id,
                ) {
                    let pending_qid = pending_question_ended_qid.expect("checked above");
                    tracing::info!(
                        "Forwarding deferred question_ended via marker before new transcription (pending_qid={:?}, next_qid={:?}, burst_id={:?})",
                        pending_qid,
                        question_id,
                        last_emitted_burst_id
                    );
                    let _ = send_log(
                        &mut node,
                        &format!(
                            "Forwarding deferred question_ended marker before new transcription (pending_qid={pending_qid}, next_qid={question_id:?}, burst_id={last_emitted_burst_id:?})"
                        ),
                    );
                    send_question_ended_marker(
                        &mut node,
                        Some(pending_qid),
                        last_emitted_burst_id,
                    )?;
                    pending_question_ended_qid = None;
                }

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
                            let _ = send_transcription(
                                &mut node,
                                "",
                                question_id,
                                burst_id,
                                transcription_mode.as_deref(),
                                segment_reason.as_deref(),
                                false,
                                false,
                            );
                            last_emitted_question_id = question_id;
                            last_emitted_burst_id = burst_id;
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

                        send_transcription(
                            &mut node,
                            &text,
                            question_id,
                            burst_id,
                            transcription_mode.as_deref(),
                            segment_reason.as_deref(),
                            false,
                            false,
                        )?;
                        last_emitted_question_id = question_id;
                        last_emitted_burst_id = burst_id;
                    }
                    Err(e) => {
                        let msg = format!("Transcription failed: {:#}", e);
                        tracing::error!("{}", msg);
                        let _ = send_log(&mut node, &msg);
                        // Send empty transcription on error (matching dora-asr behavior)
                        let _ = send_transcription(
                            &mut node,
                            "",
                            question_id,
                            burst_id,
                            transcription_mode.as_deref(),
                            segment_reason.as_deref(),
                            false,
                            false,
                        );
                        last_emitted_question_id = question_id;
                        last_emitted_burst_id = burst_id;
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
