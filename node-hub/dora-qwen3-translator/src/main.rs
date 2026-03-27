//! dora-qwen3-translator: real-time translation Dora node powered by qwen3-mlx.
//!
//! Pipeline position:
//!   dora-qwen3-asr + moxin-mic-input(question_ended)
//!      → dora-qwen3-translator
//!      → [source_text, translation]
//!
//! # Inputs
//!   text           – StringArray (single element: ASR transcription chunk)
//!   question_ended – Float64Array (silence timeout marker from mic bridge)
//!
//! # Outputs
//!   source_text  – StringArray (pass-through of the original ASR text,
//!                               possibly buffered across multiple ASR chunks)
//!                  metadata: session_status = "complete"
//!
//!   translation  – StringArray (translated text)
//!                  metadata: session_status = "streaming" | "complete"
//!                  "streaming": partial token batch, UI shows in tentative style
//!                  "complete":  full sentence done, UI switches to final style
//!
//!   log          – StringArray (status / debug messages)
//!
//! # Session finalization model
//!
//! Translator now runs as an explicit session state machine:
//! - `text` chunks continuously update source_text(streaming)
//! - sentence finalization happens on `question_ended` OR inactivity timeout
//! - final translation emits `source_text(complete)` + `translation(complete)`

use anyhow::{anyhow, Result};
use arrow::array::{Array, StringArray};
use dora_node_api::{DoraNode, Event, IntoArrow};
use minijinja::{context, Environment};
use minijinja_contrib::pycompat::unknown_method_callback;
use mlx_rs::ops::indexing::{IndexOp, NewAxis};
use mlx_rs::transforms::eval;
use mlx_lm_utils::tokenizer::{
    load_model_chat_template_from_file, ApplyChatTemplateArgs, Conversation, Tokenizer,
};
use qwen3_mlx::{load_model, Generate, KVCache};
use std::collections::BTreeMap;
use std::path::PathBuf;
use std::time::{Duration, Instant};

// ── Dora output helper ───────────────────────────────────────────────────────

fn send_str(node: &mut DoraNode, output: &str, value: &str, meta: BTreeMap<String, dora_node_api::Parameter>) -> Result<()> {
    let arr = vec![value.to_string()].into_arrow();
    node.send_output(output.into(), meta, arr)
        .map_err(|e| anyhow!("send_output({output}) failed: {e}"))
}

fn send_log(node: &mut DoraNode, msg: &str) -> Result<()> {
    send_str(node, "log", msg, BTreeMap::new())
}

fn send_source(
    node: &mut DoraNode,
    text: &str,
    status: &str,
    question_id: Option<i64>,
) -> Result<()> {
    let mut meta = BTreeMap::new();
    meta.insert("session_status".into(), dora_node_api::Parameter::String(status.into()));
    if let Some(qid) = question_id {
        meta.insert(
            "question_id".to_string(),
            dora_node_api::Parameter::Integer(qid),
        );
    }
    send_str(node, "source_text", text, meta)
}

fn send_translation_chunk(
    node: &mut DoraNode,
    chunk: &str,
    status: &str,
    question_id: Option<i64>,
) -> Result<()> {
    let mut meta = BTreeMap::new();
    meta.insert("session_status".into(), dora_node_api::Parameter::String(status.into()));
    if let Some(qid) = question_id {
        meta.insert(
            "question_id".to_string(),
            dora_node_api::Parameter::Integer(qid),
        );
    }
    send_str(node, "translation", chunk, meta)
}

// ── Language helpers ─────────────────────────────────────────────────────────

fn normalize_lang(raw: &str) -> &'static str {
    match raw.trim().to_lowercase().as_str() {
        "zh" | "zh-cn" | "chinese" | "cn" => "Chinese",
        "en" | "en-us" | "english"         => "English",
        "fr" | "french"                    => "French",
        "ja" | "jp" | "japanese"           => "Japanese",
        "ko" | "korean"                    => "Korean",
        "de" | "german"                    => "German",
        "es" | "spanish"                   => "Spanish",
        "ru" | "russian"                   => "Russian",
        _                                  => "English",   // safe fallback
    }
}

/// Full language name used in the translation prompt.
fn lang_display(code: &str) -> &'static str {
    normalize_lang(code)
}

fn should_drop_low_info_chunk(chunk: &str, src_lang: &str) -> bool {
    let t = chunk.trim().trim_matches(|c: char| {
        c.is_ascii_punctuation() || c.is_whitespace() || "，。！？；：、“”‘’（）()…".contains(c)
    });
    if t.is_empty() {
        return true;
    }

    // Chinese filler words / non-lexical short utterances that are commonly
    // triggered by breath/noise and should not be translated as standalone text.
    if normalize_lang(src_lang) == "Chinese" {
        const FILLERS: &[&str] = &[
            "嗯", "啊", "呃", "额", "唔", "哦", "噢", "哎", "哈",
            "嗯嗯", "啊啊", "呃呃",
        ];
        if FILLERS.contains(&t) {
            return true;
        }
    }

    false
}

fn ends_with_hard_sentence_boundary(text: &str) -> bool {
    let trimmed = text.trim_end();
    trimmed.ends_with('。')
        || trimmed.ends_with('！')
        || trimmed.ends_with('？')
        || trimmed.ends_with('.')
        || trimmed.ends_with('!')
        || trimmed.ends_with('?')
        || trimmed.ends_with('；')
        || trimmed.ends_with(';')
}

fn should_force_finalize_by_size(text: &str, src_lang: &str) -> bool {
    let char_count = text.chars().count();
    if normalize_lang(src_lang) == "Chinese" {
        char_count >= 56
    } else {
        char_count >= 180
    }
}

fn translation_looks_complete(text: &str, generated_tokens: usize) -> bool {
    if generated_tokens < 8 {
        return false;
    }
    let t = text.trim_end();
    t.ends_with('.')
        || t.ends_with('!')
        || t.ends_with('?')
        || t.ends_with('。')
        || t.ends_with('！')
        || t.ends_with('？')
}

// ── Model path resolution ────────────────────────────────────────────────────

fn resolve_model_path() -> PathBuf {
    // 1. Explicit env override
    if let Ok(v) = std::env::var("QWEN3_TRANSLATOR_MODEL_PATH") {
        if !v.trim().is_empty() {
            return PathBuf::from(v);
        }
    }
    // 2. Default: ~/.OminiX/models/qwen3-8b-4bit
    let base = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(".OminiX").join("models").join("qwen3-8b-4bit")
}

// ── Translation system prompt ─────────────────────────────────────────────────

fn build_system_prompt(src_lang: &str, tgt_lang: &str) -> String {
    format!(
        "/no_think You are a professional conference interpreter. \
         Translate the following {src} text into {tgt}. \
         Output only the translation — no explanations, no annotations, no parentheses. \
         Append \" [...]\" ONLY when the input is clearly an unfinished fragment; \
         for complete sentences, NEVER append \"[...]\" and NEVER repeat the translation.",
        src = lang_display(src_lang),
        tgt = lang_display(tgt_lang),
    )
}

fn post_process_translation(raw: &str, source_is_complete: bool) -> String {
    let mut out = raw.trim().to_string();

    // For syntactically complete source text, fragment markers are usually noise.
    if source_is_complete {
        loop {
            let trimmed = out.trim_end();
            if let Some(prefix) = trimmed.strip_suffix("[...]") {
                out = prefix.trim_end().to_string();
            } else {
                break;
            }
        }
    }

    out
}

fn supports_enable_thinking(chat_template: &str) -> bool {
    chat_template.contains("enable_thinking")
}

fn render_prompt_with_enable_thinking(
    chat_template: &str,
    system_prompt: &str,
    user_text: &str,
    enable_thinking: bool,
) -> Result<String> {
    let mut env = Environment::new();
    env.set_unknown_method_callback(unknown_method_callback);
    env.add_template_owned("chat".to_string(), chat_template.to_string())
        .map_err(|e| anyhow!("Failed to compile chat_template: {e}"))?;
    let template = env
        .get_template("chat")
        .map_err(|e| anyhow!("Failed to load compiled chat_template: {e}"))?;

    let messages = vec![
        Conversation {
            role: "system",
            content: system_prompt,
        },
        Conversation {
            role: "user",
            content: user_text,
        },
    ];

    let rendered = template
        .render(context! {
            messages => messages,
            add_generation_prompt => true,
            enable_thinking => enable_thinking,
        })
        .map_err(|e| anyhow!("Failed to render chat_template: {e}"))?;
    Ok(rendered)
}

fn build_prompt_token_ids(
    tokenizer: &mut Tokenizer,
    chat_template: &str,
    model_id: &str,
    system_prompt: &str,
    user_text: &str,
    force_disable_thinking: bool,
) -> Result<Vec<u32>> {
    // Qwen3 template supports a real switch: enable_thinking=false
    if force_disable_thinking && supports_enable_thinking(chat_template) {
        match render_prompt_with_enable_thinking(chat_template, system_prompt, user_text, false) {
            Ok(rendered) => {
                let encoding = tokenizer
                    .encode(rendered.as_str(), false)
                    .map_err(|e| anyhow!("Tokenization failed (manual template render): {e:?}"))?;
                return Ok(encoding.get_ids().to_vec());
            }
            Err(e) => {
                tracing::warn!("No-think template render failed, fallback to default template path: {e}");
            }
        }
    }

    // Fallback to existing path for non-Qwen3 or templates without the switch.
    let conversations: Vec<Conversation<&str, &str>> = vec![
        Conversation {
            role: "system",
            content: system_prompt,
        },
        Conversation {
            role: "user",
            content: user_text,
        },
    ];
    let args = ApplyChatTemplateArgs {
        conversations: vec![conversations.into()],
        documents: None,
        model_id,
        chat_template_id: None,
        add_generation_prompt: Some(true),
        continue_final_message: None,
    };
    let encodings = tokenizer
        .apply_chat_template_and_encode(chat_template.to_string(), args)
        .map_err(|e| anyhow!("Tokenization failed: {e:?}"))?;

    let prompt_ids = encodings
        .iter()
        .flat_map(|enc| enc.get_ids().iter().copied())
        .collect::<Vec<u32>>();
    Ok(prompt_ids)
}

fn translate_and_emit(
    node: &mut DoraNode,
    tokenizer: &mut Tokenizer,
    model: &mut qwen3_mlx::Model,
    chat_template: &str,
    model_id: &str,
    system_prompt: &str,
    text_to_translate: &str,
    force_disable_thinking: bool,
    temperature: f32,
    max_tokens: usize,
    question_id: Option<i64>,
) -> Result<()> {
    const STREAM_BATCH: usize = 5;
    const MAX_TRANSLATION_SECS: f32 = 45.0;
    const ENABLE_STREAMING_OUTPUT: bool = true;

    let prompt_ids = build_prompt_token_ids(
        tokenizer,
        chat_template,
        model_id,
        system_prompt,
        text_to_translate,
        force_disable_thinking,
    )?;

    let prompt_len = prompt_ids.len();
    let prompt_tokens = mlx_rs::Array::from(&prompt_ids[..]).index(NewAxis);
    tracing::info!(
        "Translating {} chars ({} prompt tokens)…",
        text_to_translate.len(),
        prompt_len
    );
    let t_start = Instant::now();

    let mut cache = Vec::new();
    let generator = Generate::<KVCache>::new(model, &mut cache, temperature, &prompt_tokens);

    let mut token_buf: Vec<mlx_rs::Array> = Vec::new();
    let mut full_translation = String::new();
    let mut generated = 0usize;

    let token_budget = max_tokens;
    for token_result in generator {
        if t_start.elapsed().as_secs_f32() >= MAX_TRANSLATION_SECS {
            let msg = format!(
                "Generation timeout after {:.2}s, forcing finalize",
                t_start.elapsed().as_secs_f32()
            );
            tracing::warn!("{}", msg);
            let _ = send_log(node, &msg);
            break;
        }

        let token = match token_result {
            Ok(t) => t,
            Err(e) => {
                let msg = format!("Generation error: {e}");
                tracing::error!("{}", msg);
                let _ = send_log(node, &msg);
                break;
            }
        };

        let token_id = token.item::<u32>();
        if token_id == 151643 || token_id == 151645 {
            break;
        }

        token_buf.push(token);
        generated += 1;

        if token_buf.len() >= STREAM_BATCH {
            if let Err(e) = eval(&token_buf) {
                tracing::warn!("eval failed: {e}");
            }
            let ids: Vec<u32> = token_buf.drain(..).map(|t| t.item::<u32>()).collect();
            if let Ok(text) = tokenizer.decode(&ids, true) {
                if !text.is_empty() {
                    if ENABLE_STREAMING_OUTPUT {
                        let _ = send_translation_chunk(node, &text, "streaming", question_id);
                    }
                    full_translation.push_str(&text);
                }
            }
        }

        if translation_looks_complete(&full_translation, generated) {
            break;
        }

        if generated >= token_budget {
            break;
        }
    }

    if !token_buf.is_empty() {
        let _ = eval(&token_buf);
        let ids: Vec<u32> = token_buf.drain(..).map(|t| t.item::<u32>()).collect();
        if let Ok(text) = tokenizer.decode(&ids, true) {
            if !text.is_empty() {
                full_translation.push_str(&text);
            }
        }
    }

    let elapsed = t_start.elapsed().as_secs_f32();
    tracing::info!(
        "Translation done in {:.2}s ({} tokens): {}",
        elapsed,
        generated,
        &full_translation[..full_translation.len().min(100)]
    );
    let final_translation = post_process_translation(&full_translation, true);
    let _ = send_log(node, &format!(
        "Translated in {:.2}s ({} tokens)",
        elapsed, generated
    ));
    send_translation_chunk(node, &final_translation, "complete", question_id)?;

    // Release Metal buffer pool accumulated during KV-cache inference.
    // Equivalent to Python's mx.metal.clear_cache(); prevents memory pressure
    // from building up across successive translations.
    unsafe { mlx_sys::mlx_clear_cache(); }

    Ok(())
}

struct PendingSession {
    question_id: Option<i64>,
    text: String,
    started_at: Instant,
    last_chunk_at: Instant,
    last_source_emit_at: Instant,
    last_source_emit_len: usize,
}

#[allow(clippy::too_many_arguments)]
fn finalize_pending_session(
    pending: &mut Option<PendingSession>,
    reason: &str,
    node: &mut DoraNode,
    tokenizer: &mut Tokenizer,
    model: &mut qwen3_mlx::Model,
    chat_template: &str,
    model_id: &str,
    system_prompt: &str,
    force_disable_thinking: bool,
    temperature: f32,
    max_tokens: usize,
) {
    let Some(session) = pending.take() else {
        return;
    };
    let text_to_translate = session.text;
    let question_id = session.question_id;
    if text_to_translate.trim().is_empty() {
        return;
    }

    tracing::info!(
        "Finalizing sentence (reason={}): {}",
        reason,
        &text_to_translate[..text_to_translate.len().min(80)]
    );
    let _ = send_log(
        node,
        &format!(
            "Finalizing sentence: reason={}, question_id={:?}, chars={}",
            reason,
            question_id,
            text_to_translate.chars().count()
        ),
    );
    eprintln!(
        "[translator-finalize] reason={} qid={:?} text={}",
        reason,
        question_id,
        &text_to_translate[..text_to_translate.len().min(120)]
    );
    let _ = send_source(node, &text_to_translate, "complete", question_id);
    if let Err(e) = translate_and_emit(
        node,
        tokenizer,
        model,
        chat_template,
        model_id,
        system_prompt,
        &text_to_translate,
        force_disable_thinking,
        temperature,
        max_tokens,
        question_id,
    ) {
        let msg = format!("{e}");
        tracing::error!("{}", msg);
        let _ = send_log(node, &msg);
        // Never leave UI/session in perpetual "translating" due to a failed send/generation.
        let _ = send_translation_chunk(node, "", "complete", question_id);
    }
}

// ── Main ─────────────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_env("LOG_LEVEL")
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    tracing::info!("dora-qwen3-translator starting");

    let (mut node, mut events) = DoraNode::init_from_env()
        .map_err(|e| anyhow!("Failed to init Dora node: {e}"))?;

    // Configuration from environment
    let src_lang = std::env::var("SRC_LANG").unwrap_or_else(|_| "zh".into());
    let tgt_lang = std::env::var("TGT_LANG").unwrap_or_else(|_| "en".into());
    let temperature: f32 = std::env::var("TEMPERATURE")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(0.0);
    let max_tokens: usize = std::env::var("MAX_TOKENS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(256);
    // Idle timeout controls sentence finalization when question_ended is missed.
    // With question_id present, use longer timeout to avoid premature split.
    const IDLE_FINALIZE_WITH_QID_MS: u64 = 3200;
    const IDLE_FINALIZE_NO_QID_MS: u64 = 1400;
    // Absolute cap for a single buffered session.
    const FORCE_SEND_SECS: u64 = 8;
    const SOURCE_EMIT_INTERVAL_MS: u64 = 280;
    const SOURCE_EMIT_MIN_DELTA_CHARS: usize = 10;
    // Proactive commit to prevent endless backlog during fast continuous speech.
    const MIN_CHARS_FOR_PUNCT_COMMIT_ZH: usize = 10;
    const MIN_CHARS_FOR_PUNCT_COMMIT_NON_ZH: usize = 24;

    tracing::info!("Translation: {} → {}", src_lang, tgt_lang);

    let system_prompt = build_system_prompt(&src_lang, &tgt_lang);

    // Load model and tokenizer
    let model_path = resolve_model_path();
    tracing::info!("Loading Qwen3 model from: {}", model_path.display());
    let _ = send_log(&mut node, &format!("Loading Qwen3 model from {}", model_path.display()));

    let tokenizer_file = model_path.join("tokenizer.json");
    let tokenizer_config_file = model_path.join("tokenizer_config.json");

    let mut tokenizer = Tokenizer::from_file(&tokenizer_file)
        .map_err(|e| anyhow!("Failed to load tokenizer: {e:?}"))?;

    let chat_template = load_model_chat_template_from_file(&tokenizer_config_file)?
        .ok_or_else(|| anyhow!("Chat template not found in tokenizer_config.json"))?;

    let mut model = load_model(&model_path)
        .map_err(|e| anyhow!("Failed to load Qwen3 model: {e}"))?;

    tracing::info!("Qwen3 model loaded");
    let _ = send_log(&mut node, "Qwen3 model loaded — ready to translate");

    // Use model directory name as identifier for chat template
    let model_id = model_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("qwen3")
        .to_string();
    let force_disable_thinking = model_id.to_lowercase().contains("qwen3");
    tracing::info!(
        "Prompt mode: {}, template_switch_supported={}",
        if force_disable_thinking { "qwen3-no-think" } else { "default" },
        supports_enable_thinking(&chat_template)
    );
    tracing::info!(
        "Stability mode: stream_translation_chunks=false, source_emit_interval_ms=280, source_emit_min_delta_chars=10"
    );
    let _ = send_log(
        &mut node,
        "Stability mode active: stream_chunks=false, source_emit_interval_ms=280, source_emit_min_delta_chars=10",
    );

    let mut pending: Option<PendingSession> = None;
    // Deferred question_ended: when question_ended arrives before the ASR text,
    // remember it so we can finalize immediately when the matching text arrives.
    let mut deferred_ended_qid: Option<i64> = None;

    loop {
        // Timer-driven convergence: even with no new inputs, session can finalize.
        if let Some(session) = pending.as_ref() {
            let now = Instant::now();
            let idle_elapsed = now.duration_since(session.last_chunk_at);
            let age_elapsed = now.duration_since(session.started_at);
            let idle_threshold_ms = if session.question_id.is_some() {
                IDLE_FINALIZE_WITH_QID_MS
            } else {
                IDLE_FINALIZE_NO_QID_MS
            };
            if idle_elapsed >= Duration::from_millis(idle_threshold_ms) {
                finalize_pending_session(
                    &mut pending,
                    "idle_timeout",
                    &mut node,
                    &mut tokenizer,
                    &mut model,
                    &chat_template,
                    &model_id,
                    &system_prompt,
                    force_disable_thinking,
                    temperature,
                    max_tokens,
                );
            } else if age_elapsed >= Duration::from_secs(FORCE_SEND_SECS) {
                finalize_pending_session(
                    &mut pending,
                    "max_age_timeout",
                    &mut node,
                    &mut tokenizer,
                    &mut model,
                    &chat_template,
                    &model_id,
                    &system_prompt,
                    force_disable_thinking,
                    temperature,
                    max_tokens,
                );
            }
        }

        let event = events.recv_timeout(Duration::from_millis(100));
        let Some(event) = event else {
            continue;
        };

        match event {
            Event::Input {
                id,
                data,
                metadata,
                ..
            } => {
                if id.as_str() == "text" {
                    let question_id = metadata
                        .parameters
                        .get("question_id")
                        .and_then(|p| match p {
                            dora_node_api::Parameter::Integer(v) => Some(*v),
                            _ => None,
                        });
                    let arr = match data.as_any().downcast_ref::<StringArray>() {
                        Some(a) if a.len() > 0 => a,
                        _ => continue,
                    };
                    let chunk = arr.value(0).trim().to_string();
                    if chunk.is_empty() {
                        continue;
                    }
                    if should_drop_low_info_chunk(&chunk, &src_lang) {
                        tracing::debug!("Dropping low-info ASR chunk: {}", chunk);
                        continue;
                    }

                    tracing::info!("Received ASR chunk: {}", &chunk[..chunk.len().min(80)]);
                    eprintln!(
                        "[translator-chunk] qid={:?} chunk={}",
                        question_id,
                        &chunk[..chunk.len().min(120)]
                    );
                    let now = Instant::now();
                    match pending.as_mut() {
                        Some(session) => {
                            if session.question_id != question_id {
                                finalize_pending_session(
                                    &mut pending,
                                    "question_id_switch",
                                    &mut node,
                                    &mut tokenizer,
                                    &mut model,
                                    &chat_template,
                                    &model_id,
                                    &system_prompt,
                                    force_disable_thinking,
                                    temperature,
                                    max_tokens,
                                );
                                pending = Some(PendingSession {
                                    question_id,
                                    text: chunk,
                                    started_at: now,
                                    last_chunk_at: now,
                                    last_source_emit_at: now,
                                    last_source_emit_len: 0,
                                });
                            } else {
                                let sep = if normalize_lang(&src_lang) == "Chinese" {
                                    ""
                                } else {
                                    " "
                                };
                                session.text.push_str(sep);
                                session.text.push_str(&chunk);
                                session.last_chunk_at = now;
                            }
                        }
                        None => {
                            pending = Some(PendingSession {
                                question_id,
                                text: chunk,
                                started_at: now,
                                last_chunk_at: now,
                                last_source_emit_at: now,
                                last_source_emit_len: 0,
                            });
                        }
                    }

                    if let Some(session) = pending.as_mut() {
                        let current_len = session.text.chars().count();
                        let delta = current_len.saturating_sub(session.last_source_emit_len);
                        let due_by_time = now.duration_since(session.last_source_emit_at)
                            >= Duration::from_millis(SOURCE_EMIT_INTERVAL_MS);
                        let due_by_delta = delta >= SOURCE_EMIT_MIN_DELTA_CHARS;
                        let due_by_boundary = ends_with_hard_sentence_boundary(&session.text);
                        if due_by_time || due_by_delta || due_by_boundary {
                            let _ = send_source(&mut node, &session.text, "streaming", session.question_id);
                            session.last_source_emit_at = now;
                            session.last_source_emit_len = current_len;
                        }

                        // Check deferred question_ended: if ASR text arrived after
                        // question_ended, finalize immediately instead of waiting for idle timeout.
                        if let Some(deferred_qid) = deferred_ended_qid {
                            if session.question_id == Some(deferred_qid) {
                                tracing::info!(
                                    "Applying deferred question_ended for qid={}",
                                    deferred_qid
                                );
                                eprintln!("[translator-deferred-apply] qid={}", deferred_qid);
                                deferred_ended_qid = None;
                                finalize_pending_session(
                                    &mut pending,
                                    "deferred_question_ended",
                                    &mut node,
                                    &mut tokenizer,
                                    &mut model,
                                    &chat_template,
                                    &model_id,
                                    &system_prompt,
                                    force_disable_thinking,
                                    temperature,
                                    max_tokens,
                                );
                            }
                        }
                    }

                    let should_finalize_now = pending.as_ref().map(|session| {
                        let txt = session.text.trim();
                        let min_chars = if normalize_lang(&src_lang) == "Chinese" {
                            MIN_CHARS_FOR_PUNCT_COMMIT_ZH
                        } else {
                            MIN_CHARS_FOR_PUNCT_COMMIT_NON_ZH
                        };
                        (txt.chars().count() >= min_chars && ends_with_hard_sentence_boundary(txt))
                            || should_force_finalize_by_size(txt, &src_lang)
                    }).unwrap_or(false);

                    if should_finalize_now {
                        finalize_pending_session(
                            &mut pending,
                            "proactive_boundary_commit",
                            &mut node,
                            &mut tokenizer,
                            &mut model,
                            &chat_template,
                            &model_id,
                            &system_prompt,
                            force_disable_thinking,
                            temperature,
                            max_tokens,
                        );
                    }
                    continue;
                }

                if id.as_str() == "question_ended" {
                    let ended_question_id = metadata
                        .parameters
                        .get("question_id")
                        .and_then(|p| match p {
                            dora_node_api::Parameter::Integer(v) => Some(*v),
                            _ => None,
                        });
                    if let Some(session) = pending.as_ref() {
                        if ended_question_id.is_some() && session.question_id != ended_question_id {
                            tracing::info!(
                                "Ignoring question_ended mismatch: pending={:?}, ended={:?}",
                                session.question_id, ended_question_id
                            );
                            eprintln!(
                                "[translator-mismatch] pending_qid={:?} ended_qid={:?}",
                                session.question_id, ended_question_id
                            );
                            continue;
                        }
                    }
                    if pending.is_none() {
                        // ASR hasn't delivered the text yet; remember for later.
                        if let Some(qid) = ended_question_id {
                            tracing::info!(
                                "question_ended arrived before ASR text, deferring qid={}",
                                qid
                            );
                            eprintln!("[translator-defer] qid={}", qid);
                            deferred_ended_qid = Some(qid);
                        }
                        continue;
                    }
                    finalize_pending_session(
                        &mut pending,
                        "question_ended",
                        &mut node,
                        &mut tokenizer,
                        &mut model,
                        &chat_template,
                        &model_id,
                        &system_prompt,
                        force_disable_thinking,
                        temperature,
                        max_tokens,
                    );
                    deferred_ended_qid = None;
                    continue;
                }
            }
            Event::Stop(_) => {
                tracing::info!("Stop event received, shutting down");
                break;
            }
            _ => {}
        }
    }

    tracing::info!("dora-qwen3-translator stopped");
    Ok(())
}
