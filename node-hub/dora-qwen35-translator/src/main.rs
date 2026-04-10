//! dora-qwen35-translator: real-time translation Dora node powered by qwen3.5-35B-mlx.
//!
//! The translator treats upstream ASR as a text provider.
//!
//! Pipeline position:
//!   dora-qwen3-asr + mic bridge
//!      → dora-qwen35-translator
//!      → [source_text, translation]
//!
//! # Inputs
//!   text – StringArray (single element: latest ASR text chunk)
//!
//! # Outputs
//!   source_text  – current transcript tail or committed sentence
//!   translation  – translated committed sentence
//!   log          – status / debug messages
//!
//! Internally, the node maintains a continuously growing transcript buffer.
//! Same-burst chunks replace the active burst, new bursts seal the previous
//! one, and only sealed text participates in periodic translation commits.

mod transcript_buffer;

use anyhow::{anyhow, Result};
use arrow::array::{Array, StringArray};
use dora_node_api::{DoraNode, Event, IntoArrow};
use minijinja::{context, Environment};
use minijinja_contrib::pycompat::unknown_method_callback;
use mlx_lm_utils::tokenizer::{
    load_model_chat_template_from_file, ApplyChatTemplateArgs, Conversation, Tokenizer,
};
use mlx_rs::ops::indexing::{IndexOp, NewAxis};
use mlx_rs::transforms::eval;
use qwen3_5_35b_mlx::{load_model, Generate};
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::sync::mpsc;
use std::thread;
use std::time::{Duration, Instant};
use transcript_buffer::TranscriptBuffer;

// ── Dora output helper ───────────────────────────────────────────────────────

fn send_str(
    node: &mut DoraNode,
    output: &str,
    value: &str,
    meta: BTreeMap<String, dora_node_api::Parameter>,
) -> Result<()> {
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
    meta.insert(
        "session_status".into(),
        dora_node_api::Parameter::String(status.into()),
    );
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
    meta.insert(
        "session_status".into(),
        dora_node_api::Parameter::String(status.into()),
    );
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
        "en" | "en-us" | "english" => "English",
        "fr" | "french" => "French",
        "ja" | "jp" | "japanese" => "Japanese",
        "ko" | "korean" => "Korean",
        "de" | "german" => "German",
        "es" | "spanish" => "Spanish",
        "ru" | "russian" => "Russian",
        _ => "English", // safe fallback
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
            "嗯", "啊", "呃", "额", "唔", "哦", "噢", "哎", "哈", "嗯嗯", "啊啊", "呃呃",
        ];
        if FILLERS.contains(&t) {
            return true;
        }
    }

    false
}

// ── Model path resolution ────────────────────────────────────────────────────

fn resolve_model_path() -> PathBuf {
    // 1. Explicit env override
    if let Ok(v) = std::env::var("QWEN35_TRANSLATOR_MODEL_PATH") {
        if !v.trim().is_empty() {
            return PathBuf::from(v);
        }
    }
    // 2. Default: ~/.OminiX/models/Qwen3.5-2B-MLX-4bit
    let base = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    base.join(".OminiX")
        .join("models")
        .join("Qwen3.5-2B-MLX-4bit")
}

fn load_eos_tokens(model_path: &std::path::Path) -> Result<HashSet<u32>> {
    let config_path = model_path.join("config.json");
    let config: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(&config_path)?)?;
    let eos_value = config.get("eos_token_id").or_else(|| {
        config
            .get("text_config")
            .and_then(|v| v.get("eos_token_id"))
    });

    let eos_tokens = match eos_value {
        Some(serde_json::Value::Array(ids)) => ids
            .iter()
            .filter_map(|v| v.as_u64().map(|n| n as u32))
            .collect(),
        Some(serde_json::Value::Number(n)) => {
            let mut set = HashSet::new();
            set.insert(n.as_u64().unwrap_or(248044) as u32);
            set
        }
        _ => {
            let mut set = HashSet::new();
            set.insert(248044);
            set
        }
    };

    Ok(eos_tokens)
}

// ── Translation system prompt ─────────────────────────────────────────────────

const COMMIT_THRESHOLD_CHARS: usize = 10;
const COMMIT_TICK_MS: u64 = 500;
const IDLE_FLUSH_MS_DEFAULT: u64 = 9000;

#[derive(Debug)]
struct TranslationTask {
    source_text: String,
    system_prompt: String,
    user_prompt: String,
}

#[derive(Debug)]
struct TranslationResponse {
    source_text: String,
    output: Result<String, String>,
}

fn build_system_prompt(tgt_lang: &str) -> String {
    format!(
        "/no_think 将用户提供的文本翻译成{tgt}。只输出译文，不要解释，不要重复原文。",
        tgt = lang_display(tgt_lang)
    )
}

fn build_translation_user_prompt(source_text: &str) -> String {
    format!("Input:\n{source_text}")
}

fn format_commit_prompt_debug(system_prompt: &str, user_prompt: &str) -> String {
    format!("system_prompt=\n{system_prompt}\n\nuser_prompt=\n{user_prompt}")
}

fn strip_hard_cut_terminal_punctuation(chunk: &str) -> String {
    let trimmed = chunk.trim_end();
    let mut out = trimmed.to_string();
    if matches!(out.chars().last(), Some('。' | '.' | '！' | '!' | '？' | '?')) {
        out.pop();
    }
    out
}

fn find_commit_boundary_from_tail(text: &str) -> Option<usize> {
    text.char_indices()
        .rev()
        .find_map(|(idx, ch)| match ch {
            '，' | ',' | '。' | '.' | '！' | '!' | '？' | '?' | '；' | ';' => {
                Some(idx + ch.len_utf8())
            }
            _ => None,
        })
}

fn should_trigger_idle_flush(
    elapsed_since_last_chunk: Option<Duration>,
    has_buffered_text: bool,
    translation_pending: bool,
    flush_requested: bool,
    idle_flush_ms: u64,
) -> bool {
    !translation_pending
        && !flush_requested
        && has_buffered_text
        && elapsed_since_last_chunk
            .map(|elapsed| elapsed >= Duration::from_millis(idle_flush_ms))
            .unwrap_or(false)
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
                tracing::warn!(
                    "No-think template render failed, fallback to default template path: {e}"
                );
            }
        }
    }

    // Fallback: single-turn conversation.
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

fn generate_text_completion(
    tokenizer: &mut Tokenizer,
    model: &mut qwen3_5_35b_mlx::Model,
    chat_template: &str,
    model_id: &str,
    system_prompt: &str,
    text_to_translate: &str,
    force_disable_thinking: bool,
    temperature: f32,
    max_tokens: usize,
    eos_tokens: &HashSet<u32>,
) -> Result<String> {
    const MAX_TRANSLATION_SECS: f32 = 45.0;
    const STREAM_BATCH: usize = 5;

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

    let generator = Generate::new(model, temperature, &prompt_tokens);

    let mut token_buf: Vec<mlx_rs::Array> = Vec::new();
    let mut full_translation = String::new();
    let mut generated = 0usize;

    let token_budget = max_tokens;
    for token_result in generator {
        if t_start.elapsed().as_secs_f32() >= MAX_TRANSLATION_SECS {
            tracing::warn!(
                "Generation timeout after {:.2}s, forcing finalize",
                t_start.elapsed().as_secs_f32()
            );
            break;
        }

        let token = match token_result {
            Ok(t) => t,
            Err(e) => {
                return Err(anyhow!("Generation error: {e}"));
            }
        };

        let token_id = token.item::<u32>();
        if eos_tokens.contains(&token_id) {
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
                    full_translation.push_str(&text);
                }
            }
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
        "Translation done in {:.2}s ({} tokens)\n{}",
        elapsed,
        generated,
        full_translation
    );

    // Release Metal buffer pool accumulated during KV-cache inference.
    // Equivalent to Python's mx.metal.clear_cache(); prevents memory pressure
    // from building up across successive translations.
    unsafe {
        mlx_sys::mlx_clear_cache();
    }

    Ok(full_translation.trim().to_string())
}

fn submit_translation_task(
    request_tx: &mpsc::Sender<TranslationTask>,
    source_text: &str,
    tgt_lang: &str,
    transcript: &TranscriptBuffer,
) -> Result<()> {
    let source_text = source_text.to_string();
    let system_prompt = build_system_prompt(tgt_lang);
    let user_prompt = build_translation_user_prompt(&source_text);

    tracing::info!(
        "Entering commit attempt\n{}\nsource_text=\n{}\nprompt=\n{}",
        transcript.debug_snapshot(),
        source_text,
        format_commit_prompt_debug(&system_prompt, &user_prompt)
    );

    request_tx
        .send(TranslationTask {
            source_text,
            system_prompt,
            user_prompt,
        })
        .map_err(|e| anyhow!("failed to send translation task to worker: {e}"))?;

    Ok(())
}

fn handle_translation_response(
    node: &mut DoraNode,
    transcript: &mut TranscriptBuffer,
    response: TranslationResponse,
) -> bool {
    let TranslationResponse {
        source_text,
        output,
    } = response;

    if source_text.is_empty() {
        return false;
    }

    let translation = match output {
        Ok(output) => output,
        Err(e) => {
            tracing::error!("Translation generation failed: {e}");
            let _ = send_log(
                node,
                &format!("Translation generation failed: {e}"),
            );
            return false;
        }
    };
    if translation.trim().is_empty() {
        return false;
    }

    match transcript.consume_stable_prefix(&source_text) {
        Ok(()) => {
            tracing::info!(
                "Committed stable prefix\nsource_text=\n{}\n{}",
                source_text,
                transcript.debug_snapshot()
            );
            let _ = send_source(node, &source_text, "complete", None);
            let _ = send_translation_chunk(node, &translation, "complete", None);
            true
        }
        Err(e) => {
            tracing::error!("Failed to consume committed stable prefix: {e}");
            let _ = send_log(node, &format!("Failed to consume committed stable prefix: {e}"));
            false
        }
    }
}

fn translation_worker_loop(
    model_path: PathBuf,
    temperature: f32,
    max_tokens: usize,
    ready_tx: mpsc::Sender<Result<(), String>>,
    request_rx: mpsc::Receiver<TranslationTask>,
    response_tx: mpsc::Sender<TranslationResponse>,
) {
    let init = || -> Result<(
        Tokenizer,
        qwen3_5_35b_mlx::Model,
        String,
        String,
        bool,
        HashSet<u32>,
    )> {
        let tokenizer_file = model_path.join("tokenizer.json");
        let tokenizer_config_file = model_path.join("tokenizer_config.json");

        let tokenizer = Tokenizer::from_file(&tokenizer_file)
            .map_err(|e| anyhow!("Failed to load tokenizer: {e:?}"))?;

        let chat_template = match load_model_chat_template_from_file(&tokenizer_config_file)? {
            Some(t) => t,
            None => {
                let jinja_path = model_path.join("chat_template.jinja");
                std::fs::read_to_string(&jinja_path).map_err(|_| {
                    anyhow!("Chat template not found in tokenizer_config.json or chat_template.jinja")
                })?
            }
        };

        let model =
            load_model(&model_path).map_err(|e| anyhow!("Failed to load Qwen3.5 model: {e}"))?;
        let eos_tokens = load_eos_tokens(&model_path)?;
        let model_id = model_path
            .file_name()
            .and_then(|s| s.to_str())
            .unwrap_or("qwen3.5")
            .to_string();
        let force_disable_thinking = model_id.to_lowercase().contains("qwen3");

        Ok((
            tokenizer,
            model,
            chat_template,
            model_id,
            force_disable_thinking,
            eos_tokens,
        ))
    };

    let (mut tokenizer, mut model, chat_template, model_id, force_disable_thinking, eos_tokens) =
        match init() {
            Ok(state) => {
                let _ = ready_tx.send(Ok(()));
                state
            }
            Err(e) => {
                let _ = ready_tx.send(Err(e.to_string()));
                return;
            }
        };

    while let Ok(task) = request_rx.recv() {
        let output = generate_text_completion(
            &mut tokenizer,
            &mut model,
            &chat_template,
            &model_id,
            &task.system_prompt,
            &task.user_prompt,
            force_disable_thinking,
            temperature,
            max_tokens,
            &eos_tokens,
        )
        .map_err(|e| e.to_string());

        if response_tx
            .send(TranslationResponse {
                source_text: task.source_text,
                output,
            })
            .is_err()
        {
            break;
        }
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

    tracing::info!("dora-qwen35-translator starting");

    let (mut node, mut events) =
        DoraNode::init_from_env().map_err(|e| anyhow!("Failed to init Dora node: {e}"))?;

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
    let idle_flush_ms: u64 = std::env::var("TRANSLATOR_IDLE_FLUSH_MS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(IDLE_FLUSH_MS_DEFAULT);
    tracing::info!("Translation: {} → {}", src_lang, tgt_lang);

    let model_path = resolve_model_path();
    tracing::info!("Loading Qwen3.5 model from: {}", model_path.display());
    let _ = send_log(
        &mut node,
        &format!("Loading Qwen3.5 model from {}", model_path.display()),
    );

    let (request_tx, request_rx) = mpsc::channel();
    let (response_tx, response_rx) = mpsc::channel();
    let (ready_tx, ready_rx) = mpsc::channel();
    let worker_model_path = model_path.clone();
    let worker_handle = thread::spawn(move || {
        translation_worker_loop(
            worker_model_path,
            temperature,
            max_tokens,
            ready_tx,
            request_rx,
            response_tx,
        )
    });

    match ready_rx.recv() {
        Ok(Ok(())) => {
            tracing::info!("Qwen3.5 model loaded");
            let _ = send_log(&mut node, "Qwen3.5 model loaded - ready to translate");
        }
        Ok(Err(e)) => {
            let _ = worker_handle.join();
            return Err(anyhow!("Failed to initialize translation worker: {e}"));
        }
        Err(e) => {
            let _ = worker_handle.join();
            return Err(anyhow!("Translation worker did not report readiness: {e}"));
        }
    }

    tracing::info!("Buffer merge mode active");
    let _ = send_log(&mut node, "Buffer merge mode active");

    let mut transcript = TranscriptBuffer::new();
    let mut current_burst_id: Option<i64> = None;
    let mut translation_pending = false;
    let mut flush_requested = false;
    let mut stopping = false;
    let mut last_asr_chunk_at: Option<Instant> = None;

    loop {
        while let Ok(response) = response_rx.try_recv() {
            let did_commit = handle_translation_response(&mut node, &mut transcript, response);
            translation_pending = false;

            if did_commit {
                let tail = transcript.uncommitted_tail();
                if !tail.is_empty() {
                    let _ = send_source(&mut node, &tail, "streaming", current_burst_id);
                }
            }

            if flush_requested && transcript.stable_buffer().trim().is_empty() {
                flush_requested = false;
            }
        }

        let event = events.recv_timeout(Duration::from_millis(COMMIT_TICK_MS));

        match event {
            None => {}
            Some(Event::Input {
                id, data, metadata, ..
            }) => {
                if id.as_str() == "text" {
                    let burst_id = metadata
                        .parameters
                        .get("burst_id")
                        .and_then(|p| match p {
                            dora_node_api::Parameter::Integer(v) => Some(*v),
                            _ => None,
                        })
                        .or_else(|| {
                            metadata
                                .parameters
                                .get("question_id")
                                .and_then(|p| match p {
                                    dora_node_api::Parameter::Integer(v) => Some(*v),
                                    _ => None,
                                })
                        });
                    if burst_id.is_some() {
                        current_burst_id = burst_id;
                    }
                    let transcription_mode = metadata
                        .parameters
                        .get("transcription_mode")
                        .and_then(|p| match p {
                            dora_node_api::Parameter::String(v) => Some(v.as_str()),
                            _ => None,
                        })
                        .unwrap_or("progressive");
                    let segment_reason = metadata
                        .parameters
                        .get("segment_reason")
                        .and_then(|p| match p {
                            dora_node_api::Parameter::String(v) => Some(v.as_str()),
                            _ => None,
                        });

                    let arr = match data.as_any().downcast_ref::<StringArray>() {
                        Some(a) if a.len() > 0 => a,
                        _ => continue,
                    };
                    let chunk = arr.value(0).trim().to_string();
                    let chunk_is_usable = !chunk.is_empty()
                        && !should_drop_low_info_chunk(&chunk, &src_lang);

                    if chunk_is_usable {
                        tracing::info!(
                            "Received ASR chunk\nburst_id={:?}\nmode={}\nsegment_reason={:?}\nchunk=\n{}",
                            current_burst_id,
                            transcription_mode,
                            segment_reason,
                            chunk
                        );
                    } else {
                        tracing::info!(
                            "Ignoring non-usable ASR chunk\nburst_id={:?}\nmode={}\nsegment_reason={:?}\nchunk_is_empty={}",
                            current_burst_id,
                            transcription_mode,
                            segment_reason,
                            chunk.is_empty()
                        );
                    }

                    if !chunk_is_usable {
                        continue;
                    }

                    last_asr_chunk_at = Some(Instant::now());
                    let chunk_for_buffer = if segment_reason == Some("max_segment") {
                        strip_hard_cut_terminal_punctuation(&chunk)
                    } else {
                        chunk
                    };

                    let mut changed = transcript.update_from_chunk(burst_id, &chunk_for_buffer);
                    if transcription_mode == "final" && transcript.seal_active_burst() {
                        changed = true;
                    }

                    tracing::info!(
                        "Transcript state after chunk (changed={})\n{}",
                        changed,
                        transcript.debug_snapshot()
                    );

                    let tail = transcript.uncommitted_tail();
                    if !tail.is_empty() {
                        let _ = send_source(&mut node, &tail, "streaming", current_burst_id);
                    }
                }
            }
            Some(Event::Stop(_)) => {
                tracing::info!("Stop event received, draining pending translation work");
                stopping = true;
                flush_requested = true;
                if transcript.seal_active_burst() {
                    tracing::info!(
                        "Active burst sealed on stop\n{}",
                        transcript.debug_snapshot()
                    );
                }
            }
            _ => {}
        }

        if should_trigger_idle_flush(
            last_asr_chunk_at.map(|instant| instant.elapsed()),
            !transcript.buffer().trim().is_empty(),
            translation_pending,
            flush_requested,
            idle_flush_ms,
        ) {
            tracing::info!(
                "Idle flush triggered after {}ms without new ASR chunk",
                idle_flush_ms
            );
            flush_requested = true;
            if transcript.seal_active_burst() {
                tracing::info!(
                    "Active burst sealed on idle flush\n{}",
                    transcript.debug_snapshot()
                );
            }
        }

        if !translation_pending {
            let next_source = if flush_requested {
                let stable = transcript.stable_buffer().trim();
                if stable.is_empty() {
                    None
                } else {
                    Some(stable.to_string())
                }
            } else if transcript.has_stable_text(COMMIT_THRESHOLD_CHARS) {
                let stable = transcript.stable_buffer();
                find_commit_boundary_from_tail(stable).map(|end| stable[..end].to_string())
            } else {
                None
            };

            if let Some(source_text) = next_source {
                match submit_translation_task(&request_tx, &source_text, &tgt_lang, &transcript) {
                    Ok(()) => {
                        translation_pending = true;
                    }
                    Err(e) => {
                        let _ = worker_handle.join();
                        return Err(anyhow!("Failed to submit translation task: {e}"));
                    }
                }
            }
        }

        if stopping
            && !translation_pending
            && transcript.stable_buffer().trim().is_empty()
            && transcript.active_burst_text().is_empty()
        {
            break;
        }
    }

    drop(request_tx);
    worker_handle
        .join()
        .map_err(|_| anyhow!("Translation worker thread panicked"))?;

    tracing::info!("dora-qwen35-translator stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_system_prompt, find_commit_boundary_from_tail, format_commit_prompt_debug,
        should_trigger_idle_flush, strip_hard_cut_terminal_punctuation,
    };
    use std::time::Duration;

    #[test]
    fn strip_hard_cut_terminal_punctuation_removes_single_sentence_mark() {
        assert_eq!(
            strip_hard_cut_terminal_punctuation("我们一八年开源的。"),
            "我们一八年开源的"
        );
        assert_eq!(
            strip_hard_cut_terminal_punctuation("我们一八年开源的！"),
            "我们一八年开源的"
        );
        assert_eq!(
            strip_hard_cut_terminal_punctuation("我们一八年开源的"),
            "我们一八年开源的"
        );
    }

    #[test]
    fn find_commit_boundary_from_tail_prefers_last_comma() {
        let text = "大家下午好，我叫鲍月，然后来自华为，现在也是CoolEdge";
        let end = find_commit_boundary_from_tail(text).expect("boundary should exist");
        assert_eq!(&text[..end], "大家下午好，我叫鲍月，然后来自华为，");
    }

    #[test]
    fn find_commit_boundary_from_tail_falls_back_to_sentence_end() {
        let text = "这是我们一八年开源的。然后继续";
        let end = find_commit_boundary_from_tail(text).expect("boundary should exist");
        assert_eq!(&text[..end], "这是我们一八年开源的。");
    }

    #[test]
    fn find_commit_boundary_from_tail_returns_none_without_supported_separator() {
        assert!(find_commit_boundary_from_tail("大家下午好我叫鲍月然后来自华为").is_none());
    }

    #[test]
    fn build_system_prompt_is_plain_translation_only() {
        let prompt = build_system_prompt("en");
        assert!(prompt.contains("只输出译文"));
        assert!(prompt.contains("English"));
    }

    #[test]
    fn format_commit_prompt_debug_keeps_full_prompt_sections() {
        let debug = format_commit_prompt_debug(
            "Translate to English. Output translation only.",
            "Input:\n大家下午好，我叫鲍月，然后来自华为，",
        );
        assert!(debug.contains("system_prompt=\nTranslate to English. Output translation only."));
        assert!(debug.contains("user_prompt=\nInput:\n大家下午好，我叫鲍月，然后来自华为，"));
    }

    #[test]
    fn should_trigger_idle_flush_only_after_threshold_with_buffered_text() {
        assert!(!should_trigger_idle_flush(
            Some(Duration::from_millis(800)),
            true,
            false,
            false,
            1100
        ));
        assert!(should_trigger_idle_flush(
            Some(Duration::from_millis(1200)),
            true,
            false,
            false,
            1100
        ));
        assert!(!should_trigger_idle_flush(
            Some(Duration::from_millis(1200)),
            false,
            false,
            false,
            1100
        ));
        assert!(!should_trigger_idle_flush(
            Some(Duration::from_millis(1200)),
            true,
            true,
            false,
            1100
        ));
        assert!(!should_trigger_idle_flush(
            Some(Duration::from_millis(1200)),
            true,
            false,
            true,
            1100
        ));
    }
}
