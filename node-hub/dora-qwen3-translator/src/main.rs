//! dora-qwen3-translator: real-time translation Dora node powered by qwen3-mlx.
//!
//! Pipeline position:
//!   dora-qwen3-asr → [transcription] → dora-qwen3-translator → [source_text, translation]
//!
//! # Inputs
//!   text  – StringArray (single element: ASR transcription)
//!           metadata: (none required; src/tgt lang come from env)
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
//! # Sentence completeness & buffering (module 3 / VAD logic)
//!
//! Chinese speakers often pause mid-clause (inside long pre-nominal modifier
//! chains), causing VAD to cut before the head noun arrives.  The translator
//! buffers incomplete chunks and waits for the next ASR output before
//! translating, using lightweight heuristics:
//!
//!   - Text ending with '的', '地', '得' → likely inside a modifier chain
//!   - Text ending with subordinating conjunctions (但是, 虽然, 因为 …) → incomplete
//!   - Maximum buffer duration: FORCE_SEND_SECS (default 8 s elapsed since
//!     first chunk in the current buffer)
//!
//! For EN/FR input the risk is much lower; the default is to translate every
//! chunk as-is (the LLM handles minor fragments gracefully).

use anyhow::{anyhow, Result};
use arrow::array::{Array, StringArray};
use dora_node_api::{DoraNode, Event, IntoArrow, Metadata};
use mlx_rs::ops::indexing::{IndexOp, NewAxis};
use mlx_rs::transforms::eval;
use mlx_lm_utils::tokenizer::{
    load_model_chat_template_from_file, ApplyChatTemplateArgs, Conversation, Role, Tokenizer,
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

fn send_source(node: &mut DoraNode, text: &str) -> Result<()> {
    let mut meta = BTreeMap::new();
    meta.insert("session_status".into(), dora_node_api::Parameter::String("complete".into()));
    send_str(node, "source_text", text, meta)
}

fn send_translation_chunk(node: &mut DoraNode, chunk: &str, status: &str) -> Result<()> {
    let mut meta = BTreeMap::new();
    meta.insert("session_status".into(), dora_node_api::Parameter::String(status.into()));
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

// ── Sentence completeness check (module 3) ───────────────────────────────────

/// Returns true if `text` looks like a syntactically complete sentence for
/// the given source language.  False positives (accepting an incomplete
/// fragment) are acceptable — the LLM will handle them gracefully.  False
/// negatives (rejecting a complete sentence) cause unnecessary buffering.
fn is_syntactically_complete(text: &str, src_lang: &str) -> bool {
    let t = text.trim();
    if t.is_empty() {
        return false;
    }

    // For English and French, accept every chunk — the risk of mid-clause VAD
    // cuts is low and the LLM handles fragments well.
    let lang = normalize_lang(src_lang);
    if lang != "Chinese" {
        return true;
    }

    // Chinese-specific checks
    // 1. Ends with a definite sentence-final punctuation → complete.
    if t.ends_with('。') || t.ends_with('！') || t.ends_with('？')
        || t.ends_with('…') || t.ends_with('；')
    {
        return true;
    }

    // 2. Ends with a structural particle → likely inside a modifier chain.
    //    的/地/得 signal attributive, adverbial, or resultative modifiers.
    if t.ends_with('的') || t.ends_with('地') || t.ends_with('得') {
        return false;
    }

    // 3. Ends with a subordinating conjunction → clause not yet complete.
    let incomplete_endings: &[&str] = &[
        "但是", "然而", "不过", "而且", "并且", "虽然", "尽管", "即使",
        "因为", "由于", "所以", "因此", "如果", "假如", "假设", "只要",
        "虽", "但", "若", "如",
    ];
    for ending in incomplete_endings {
        if t.ends_with(ending) {
            return false;
        }
    }

    // 4. Default: accept.  The LLM adds "[...]" if it thinks the input is
    //    a fragment (see system prompt).
    true
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
         If the input appears to be a sentence fragment (ends abruptly mid-clause), \
         translate what is available and append \" [...]\" to indicate continuation is expected.",
        src = lang_display(src_lang),
        tgt = lang_display(tgt_lang),
    )
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
    // How many tokens to batch before sending a streaming update to the UI
    const STREAM_BATCH: usize = 5;
    // Force-send buffered text after this many seconds even if incomplete
    const FORCE_SEND_SECS: u64 = 8;

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

    // ── Text buffer for incomplete sentences ─────────────────────────────────
    let mut pending_text = String::new();
    let mut buffer_start: Option<Instant> = None;

    while let Some(event) = events.recv() {
        match event {
            Event::Input { id, data, .. } => {
                if id.as_str() != "text" {
                    continue;
                }

                // Extract the transcription string
                let arr = match data.as_any().downcast_ref::<StringArray>() {
                    Some(a) if a.len() > 0 => a,
                    _ => continue,
                };
                let chunk = arr.value(0).trim().to_string();
                if chunk.is_empty() {
                    continue;
                }

                tracing::info!("Received ASR chunk: {}", &chunk[..chunk.len().min(80)]);

                // Append to buffer
                if pending_text.is_empty() {
                    pending_text = chunk;
                    buffer_start = Some(Instant::now());
                } else {
                    // Join Chinese without space, others with space
                    let sep = if normalize_lang(&src_lang) == "Chinese" { "" } else { " " };
                    pending_text.push_str(sep);
                    pending_text.push_str(&chunk);
                }

                // Decide whether to translate now or continue buffering
                let timed_out = buffer_start
                    .map(|t| t.elapsed() >= Duration::from_secs(FORCE_SEND_SECS))
                    .unwrap_or(false);

                let complete = is_syntactically_complete(&pending_text, &src_lang);

                if !complete && !timed_out {
                    tracing::debug!("Buffering (incomplete sentence): {}", &pending_text[..pending_text.len().min(60)]);
                    continue;
                }

                // Take the buffered text and reset
                let text_to_translate = std::mem::take(&mut pending_text);
                buffer_start = None;

                if timed_out && !complete {
                    tracing::warn!("Force-sending after {}s timeout: {}", FORCE_SEND_SECS, &text_to_translate[..text_to_translate.len().min(60)]);
                }

                // Pass original text through so the overlay can show it
                let _ = send_source(&mut node, &text_to_translate);

                // Build chat messages: system + user (the source text)
                let conversations: Vec<Conversation<Role, &str>> = vec![
                    Conversation { role: Role::User, content: text_to_translate.as_str() },
                ];

                let args = ApplyChatTemplateArgs {
                    conversations: vec![conversations.into()],
                    documents: None,
                    model_id: &model_id,
                    chat_template_id: None,
                    add_generation_prompt: None,
                    continue_final_message: None,
                };

                let system_conversations: Vec<Conversation<Role, &str>> = vec![
                    Conversation { role: Role::System, content: system_prompt.as_str() },
                    Conversation { role: Role::User, content: text_to_translate.as_str() },
                ];

                let args_with_system = ApplyChatTemplateArgs {
                    conversations: vec![system_conversations.into()],
                    documents: None,
                    model_id: &model_id,
                    chat_template_id: None,
                    add_generation_prompt: None,
                    continue_final_message: None,
                };

                let encodings = match tokenizer.apply_chat_template_and_encode(
                    chat_template.clone(),
                    args_with_system,
                ) {
                    Ok(e) => e,
                    Err(e) => {
                        let msg = format!("Tokenization failed: {e:?}");
                        tracing::error!("{}", msg);
                        let _ = send_log(&mut node, &msg);
                        continue;
                    }
                };

                let prompt_ids: Vec<u32> = encodings
                    .iter()
                    .flat_map(|enc| enc.get_ids().iter().copied())
                    .collect();

                let prompt_len = prompt_ids.len();
                let prompt_tokens = mlx_rs::Array::from(&prompt_ids[..]).index(NewAxis);

                tracing::info!("Translating {} chars ({} prompt tokens)…", text_to_translate.len(), prompt_len);
                let t_start = Instant::now();

                let mut cache = Vec::new();
                let generator = Generate::<KVCache>::new(&mut model, &mut cache, temperature, &prompt_tokens);

                let mut token_buf: Vec<mlx_rs::Array> = Vec::new();
                let mut full_translation = String::new();
                let mut generated = 0usize;

                for token_result in generator {
                    let token = match token_result {
                        Ok(t) => t,
                        Err(e) => {
                            let msg = format!("Generation error: {e}");
                            tracing::error!("{}", msg);
                            let _ = send_log(&mut node, &msg);
                            break;
                        }
                    };

                    let token_id = token.item::<u32>();

                    // Qwen3 EOS tokens
                    if token_id == 151643 || token_id == 151645 {
                        break;
                    }

                    token_buf.push(token);
                    generated += 1;

                    // Stream every STREAM_BATCH tokens
                    if token_buf.len() >= STREAM_BATCH {
                        if let Err(e) = eval(&token_buf) {
                            tracing::warn!("eval failed: {e}");
                        }
                        let ids: Vec<u32> = token_buf.drain(..).map(|t| t.item::<u32>()).collect();
                        if let Ok(text) = tokenizer.decode(&ids, true) {
                            if !text.is_empty() {
                                let _ = send_translation_chunk(&mut node, &text, "streaming");
                                full_translation.push_str(&text);
                            }
                        }
                    }

                    if generated >= max_tokens {
                        break;
                    }
                }

                // Flush remaining tokens
                if !token_buf.is_empty() {
                    let _ = eval(&token_buf);
                    let ids: Vec<u32> = token_buf.drain(..).map(|t| t.item::<u32>()).collect();
                    if let Ok(text) = tokenizer.decode(&ids, true) {
                        if !text.is_empty() {
                            full_translation.push_str(&text);
                        }
                    }
                }

                // Send "complete" with the full translation so the UI can do a clean final update
                let elapsed = t_start.elapsed().as_secs_f32();
                tracing::info!(
                    "Translation done in {:.2}s ({} tokens): {}",
                    elapsed,
                    generated,
                    &full_translation[..full_translation.len().min(100)]
                );
                let _ = send_log(&mut node, &format!("Translated in {:.2}s ({} tokens)", elapsed, generated));
                let _ = send_translation_chunk(&mut node, &full_translation, "complete");
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
