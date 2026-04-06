//! dora-qwen35-translator: real-time translation Dora node powered by qwen3.5-35B-mlx.
//!
//! Pipeline position:
//!   dora-qwen3-asr + moxin-mic-input(question_ended)
//!      → dora-qwen35-translator
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
use qwen3_5_35b_mlx::{load_model, Generate};
use std::collections::{BTreeMap, HashSet};
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

/// Find the last boundary character from `boundaries` in `text` that has a non-empty
/// tail after it, with the head having at least `min_head_chars` chars.
/// Returns the byte offset where the tail starts, or None.
fn find_last_boundary_split(text: &str, boundaries: &[char], min_head_chars: usize) -> Option<usize> {
    let chars: Vec<(usize, char)> = text.char_indices().collect();
    for i in (0..chars.len()).rev() {
        let (byte_idx, ch) = chars[i];
        if boundaries.contains(&ch) {
            if i + 1 < min_head_chars {
                break;
            }
            let tail_start = byte_idx + ch.len_utf8();
            if !text[tail_start..].trim().is_empty() {
                return Some(tail_start);
            }
        }
    }
    None
}

const MIN_CHARS_FOR_PUNCT_COMMIT_ZH: usize = 10;
const MIN_CHARS_FOR_PUNCT_COMMIT_NON_ZH: usize = 24;
const DEFERRED_QUESTION_ENDED_ASR_IDLE_MS: u64 = 3000;
const PROGRESSIVE_IMMEDIATE_TAIL_CHARS_ZH: usize = 8;
const PROGRESSIVE_IMMEDIATE_TAIL_CHARS_NON_ZH: usize = 20;

/// Returns the byte offset within `uncommitted` up to which text can be committed.
///
/// Strategy:
/// 1. Hard boundary (。！？.!?；;) with min_chars threshold — always tried first.
/// 2. No soft comma-based boundary commits. Spoken conference content produced too
///    many premature commits when commas were treated as sentence boundaries.
///
/// `require_tail`: true = progressive mode (need non-empty text after boundary to
///   confirm the speaker moved past it); false = final mode (boundary at end is valid).
fn committable_end(
    uncommitted: &str,
    min_chars: usize,
    require_tail: bool,
    allow_terminal_boundary: bool,
) -> Option<usize> {
    const HARD: &[char] = &['。', '！', '？', '.', '!', '?', '；', ';'];

    if require_tail {
        find_last_boundary_split(uncommitted, HARD, min_chars)
    } else {
        let u = uncommitted.trim_end();
        if allow_terminal_boundary
            && u.chars().count() >= min_chars
            && ends_with_hard_sentence_boundary(u)
        {
            Some(u.len())
        } else {
            find_last_boundary_split(uncommitted, HARD, min_chars)
        }
    }
}

fn should_finalize_deferred_question_ended(
    _question_ended_elapsed: Duration,
    idle_since_last_asr_update: Duration,
    has_active_burst: bool,
) -> bool {
    if !has_active_burst {
        return true;
    }

    idle_since_last_asr_update >= Duration::from_millis(DEFERRED_QUESTION_ENDED_ASR_IDLE_MS)
}

#[derive(Clone, Debug, PartialEq, Eq)]
struct ProgressiveCommitCandidate {
    text: String,
    new_committed_chars: usize,
    question_id: Option<i64>,
}

fn should_commit_progressive_candidate(
    previous: Option<&ProgressiveCommitCandidate>,
    current: &ProgressiveCommitCandidate,
) -> bool {
    previous
        .map(|candidate| {
            candidate.text == current.text
                && candidate.new_committed_chars == current.new_committed_chars
                && candidate.question_id == current.question_id
        })
        .unwrap_or(false)
}

fn should_commit_progressive_immediately(
    uncommitted: &str,
    end_byte: usize,
    src_lang: &str,
) -> bool {
    let tail = uncommitted[end_byte..].trim();
    let tail_chars = tail.chars().count();
    let threshold = if normalize_lang(src_lang) == "Chinese" {
        PROGRESSIVE_IMMEDIATE_TAIL_CHARS_ZH
    } else {
        PROGRESSIVE_IMMEDIATE_TAIL_CHARS_NON_ZH
    };

    tail_chars >= threshold
}

fn should_allow_terminal_final_commit(
    previous: Option<&ProgressiveCommitCandidate>,
    current: &ProgressiveCommitCandidate,
) -> bool {
    should_commit_progressive_candidate(previous, current)
}

fn should_force_finalize_by_size(text: &str, src_lang: &str) -> bool {
    let char_count = text.chars().count();
    if normalize_lang(src_lang) == "Chinese" {
        char_count >= 56
    } else {
        char_count >= 180
    }
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
    let eos_value = config
        .get("eos_token_id")
        .or_else(|| config.get("text_config").and_then(|v| v.get("eos_token_id")));

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

fn build_system_prompt(src_lang: &str, tgt_lang: &str) -> String {
    format!(
        "/no_think You are a professional conference interpreter. \
         Translate the following {src} text into {tgt}. \
         Output ONLY in {tgt} — even if the source contains English technical terms or mixed languages. \
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
    // Strip "[...]" even when followed by trailing punctuation (e.g. "not particularly [...].")
    if source_is_complete {
        loop {
            let trimmed = out.trim_end();
            if let Some(marker_pos) = trimmed.rfind("[...]") {
                let after_marker = &trimmed[marker_pos + "[...]".len()..];
                let remainder_is_punct = after_marker
                    .trim_end_matches(|c: char| c.is_ascii_punctuation() || c.is_whitespace())
                    .is_empty();
                if remainder_is_punct {
                    out = trimmed[..marker_pos].trim_end().to_string();
                    continue;
                }
            }
            break;
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
        Conversation { role: "system", content: system_prompt },
        Conversation { role: "user", content: user_text },
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

    // Fallback: single-turn conversation.
    let conversations: Vec<Conversation<&str, &str>> = vec![
        Conversation { role: "system", content: system_prompt },
        Conversation { role: "user", content: user_text },
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
    model: &mut qwen3_5_35b_mlx::Model,
    chat_template: &str,
    model_id: &str,
    system_prompt: &str,
    text_to_translate: &str,
    force_disable_thinking: bool,
    temperature: f32,
    max_tokens: usize,
    question_id: Option<i64>,
    eos_tokens: &HashSet<u32>,
    final_status: &str,
    enable_streaming: bool,
    // When set, embeds the original combined source text into the final event metadata.
    // Used for "replace_last" to let the listener atomically replace history[N-1].
    combined_source_meta: Option<&str>,
) -> Result<String> {
    const STREAM_BATCH: usize = 5;
    const MAX_TRANSLATION_SECS: f32 = 45.0;
    let enable_streaming_output = enable_streaming;

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
                    if enable_streaming_output {
                        let _ = send_translation_chunk(node, &text, "streaming", question_id);
                    }
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
        "Translation done in {:.2}s ({} tokens): {}",
        elapsed,
        generated,
        &full_translation[..full_translation.char_indices().nth(100).map(|(i,_)|i).unwrap_or(full_translation.len())]
    );
    let final_translation = post_process_translation(&full_translation, true);
    let _ = send_log(node, &format!(
        "Translated in {:.2}s ({} tokens)",
        elapsed, generated
    ));
    // For "replace_last": embed combined_source into metadata so the listener
    // can atomically pop N and update N-1 in a single event (no ordering hazard).
    if let Some(src) = combined_source_meta {
        let mut meta = BTreeMap::new();
        meta.insert("session_status".into(), dora_node_api::Parameter::String(final_status.into()));
        meta.insert("combined_source".into(), dora_node_api::Parameter::String(src.into()));
        send_str(node, "translation", &final_translation, meta)?;
    } else {
        send_translation_chunk(node, &final_translation, final_status, question_id)?;
    }

    // Release Metal buffer pool accumulated during KV-cache inference.
    // Equivalent to Python's mx.metal.clear_cache(); prevents memory pressure
    // from building up across successive translations.
    unsafe { mlx_sys::mlx_clear_cache(); }

    Ok(final_translation)
}

/// Full text to translate: finalized bursts + current in-progress burst (if any).
fn effective_text(session: &PendingSession, src_lang: &str) -> String {
    match session.current_burst_text.as_deref() {
        Some(burst) if !burst.is_empty() => {
            let sep = if normalize_lang(src_lang) == "Chinese" { "" } else { " " };
            if session.text.is_empty() {
                burst.to_string()
            } else {
                format!("{}{}{}", session.text, sep, burst)
            }
        }
        _ => session.text.clone(),
    }
}

struct PendingSession {
    question_id: Option<i64>,
    /// Finalized burst text: accumulates across all completed speech bursts
    /// (i.e. all `mode=final` / legacy chunks).  Does NOT include the current
    /// in-progress burst.
    text: String,
    /// Latest progressive snapshot of the CURRENT burst (not yet finalized).
    /// Set on `mode=progressive`, cleared when a `mode=final` for the same
    /// burst arrives.  Never appended to `text` directly.
    current_burst_text: Option<String>,
    /// How many chars of `effective_text()` have already been committed to
    /// translation (sentence-cursor model). Advances as sentences are committed
    /// mid-session; finalize only translates the uncommitted remainder.
    committed_chars: usize,
    started_at: Instant,
    last_chunk_at: Instant,
    last_source_emit_at: Instant,
    last_source_emit_len: usize,
    pending_progressive_commit: Option<ProgressiveCommitCandidate>,
    /// Set when `question_ended` arrives but `current_burst_text` is still active
    /// (ASR is still processing the last audio segment).  Finalization is delayed
    /// until ASR sends `mode=final` for the current burst (clearing
    /// `current_burst_text`), OR until this deadline expires (hard cap).
    question_ended_at: Option<Instant>,
}

#[allow(clippy::too_many_arguments)]
fn finalize_pending_session(
    pending: &mut Option<PendingSession>,
    reason: &str,
    src_lang: &str,
    node: &mut DoraNode,
    tokenizer: &mut Tokenizer,
    model: &mut qwen3_5_35b_mlx::Model,
    chat_template: &str,
    model_id: &str,
    system_prompt: &str,
    force_disable_thinking: bool,
    temperature: f32,
    max_tokens: usize,
    eos_tokens: &HashSet<u32>,
) -> Option<(String, String)> {
    let Some(session) = pending.take() else {
        return None;
    };
    let question_id = session.question_id;
    // Only translate the uncommitted remainder (sentence-cursor model).
    let full = effective_text(&session, src_lang);
    let committed_byte = full.char_indices()
        .nth(session.committed_chars)
        .map(|(i, _)| i)
        .unwrap_or(full.len());
    let text_to_translate = full[committed_byte..].trim().to_string();
    if text_to_translate.is_empty() {
        return None;
    }

    tracing::info!(
        "Finalizing sentence (reason={}): {}",
        reason,
        &text_to_translate[..text_to_translate.char_indices().nth(80).map(|(i,_)|i).unwrap_or(text_to_translate.len())]
    );
    let _ = send_log(
        node,
        &format!(
            "Finalizing sentence: reason={}, question_id={:?}, chars={}",
            reason,
            question_id,
            text_to_translate.chars().count(),
        ),
    );
    eprintln!(
        "[translator-finalize] reason={} qid={:?} text={}",
        reason,
        question_id,
        &text_to_translate[..text_to_translate.char_indices().nth(120).map(|(i,_)|i).unwrap_or(text_to_translate.len())]
    );
    let _ = send_source(node, &text_to_translate, "complete", question_id);

    match translate_and_emit(
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
        eos_tokens,
        "complete",
        false, // no streaming deltas — overlay only shows completed translations
        None,
    ) {
        Ok(translation) => Some((text_to_translate, translation)),
        Err(e) => {
            let msg = format!("{e}");
            tracing::error!("{}", msg);
            let _ = send_log(node, &msg);
            let _ = send_translation_chunk(node, "", "complete", question_id);
            None
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
    // Absolute cap for a single buffered session (safety net only — primary commits
    // happen via sentence-cursor + soft boundary logic before this fires).
    const FORCE_SEND_SECS: u64 = 20;
    const SOURCE_EMIT_INTERVAL_MS: u64 = 280;
    const SOURCE_EMIT_MIN_DELTA_CHARS: usize = 10;
    tracing::info!("Translation: {} → {}", src_lang, tgt_lang);

    let system_prompt = build_system_prompt(&src_lang, &tgt_lang);

    // Load model and tokenizer
    let model_path = resolve_model_path();
    tracing::info!("Loading Qwen3.5 model from: {}", model_path.display());
    let _ = send_log(&mut node, &format!("Loading Qwen3.5 model from {}", model_path.display()));

    let tokenizer_file = model_path.join("tokenizer.json");
    let tokenizer_config_file = model_path.join("tokenizer_config.json");

    let mut tokenizer = Tokenizer::from_file(&tokenizer_file)
        .map_err(|e| anyhow!("Failed to load tokenizer: {e:?}"))?;

    // Try tokenizer_config.json first; fall back to chat_template.jinja (some quantized
    // model distributions ship the template as a separate file instead).
    let chat_template = match load_model_chat_template_from_file(&tokenizer_config_file)? {
        Some(t) => t,
        None => {
            let jinja_path = model_path.join("chat_template.jinja");
            std::fs::read_to_string(&jinja_path)
                .map_err(|_| anyhow!("Chat template not found in tokenizer_config.json or chat_template.jinja"))?
        }
    };

    let mut model = load_model(&model_path)
        .map_err(|e| anyhow!("Failed to load Qwen3.5 model: {e}"))?;

    let eos_tokens = load_eos_tokens(&model_path)?;

    tracing::info!("Qwen3.5 model loaded");
    let _ = send_log(&mut node, "Qwen3.5 model loaded — ready to translate");

    // Use model directory name as identifier for chat template
    let model_id = model_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("qwen3.5")
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
    // remember it so the matching burst can be sealed once the ASR text appears.
    let mut deferred_ended_qid: Option<i64> = None;
    // After question_ended fires and finalizes a session, stale ASR chunks for the
    // same qid may still arrive from the pipeline.  Track the ended qid so we can
    // drop those late-arriving chunks instead of creating a spurious new session.
    let mut already_ended_qid: Option<i64> = None;
    // Helper macro: finalize a session, then attempt retroactive merge with previous segment.
    // Defined as a macro to avoid borrow-checker issues with the mutable captures.
    // Retroactive merge: remember the last completed segment for possible merging.
    struct LastSegment {
        source_text: String,
        completed_at: std::time::Instant,
    }
    let mut last_segment: Option<LastSegment> = None;
    // Merge optimization on/off switch (set from UI before dataflow start).
    let enable_merge: bool = std::env::var("TRANSLATION_MERGE_ENABLED")
        .map(|v| v != "0" && v.to_lowercase() != "false")
        .unwrap_or(true);
    // Merge window: if the next sentence arrives within this many seconds,
    // combine it with the previous one and re-translate as a single unit.
    let merge_window_secs: u64 = std::env::var("TRANSLATION_MERGE_WINDOW_SECS")
        .ok().and_then(|v| v.parse().ok()).unwrap_or(3);
    // Don't merge if combined source would exceed this char count.
    const MAX_MERGE_CHARS: usize = 120;
    tracing::info!("Retroactive merge: enabled={}", enable_merge);

    // finalize! — call finalize_pending_session and attempt retroactive merge with previous segment.
    // Defined as a macro to avoid borrow-checker issues with the mutable captures.
    macro_rules! finalize {
        ($reason:expr) => {{
            let pair_opt = finalize_pending_session(
                &mut pending,
                $reason,
                &src_lang,
                &mut node,
                &mut tokenizer,
                &mut model,
                &chat_template,
                &model_id,
                &system_prompt,
                force_disable_thinking,
                temperature,
                max_tokens,
                &eos_tokens,
            );
            if let Some((src, _tgt)) = pair_opt {
                let should_merge = enable_merge && last_segment.as_ref().map(|prev: &LastSegment| {
                    let sep_len = if normalize_lang(&src_lang) == "Chinese" { 0 } else { 1 };
                    let combined_chars = prev.source_text.chars().count() + sep_len + src.chars().count();
                    prev.completed_at.elapsed() <= Duration::from_secs(merge_window_secs)
                        && combined_chars <= MAX_MERGE_CHARS
                }).unwrap_or(false);

                if should_merge {
                    let prev = last_segment.take().unwrap();
                    let sep = if normalize_lang(&src_lang) == "Chinese" { "" } else { " " };
                    let combined_src = format!("{}{}{}", prev.source_text, sep, src);
                    tracing::info!(
                        "Retroactive merge: combining {} + {} chars -> replace_last",
                        prev.source_text.chars().count(),
                        src.chars().count()
                    );
                    // Use "replace_last" with combined_source in metadata — a single atomic
                    // event so the listener can pop N and update N-1 without ordering hazards.
                    match translate_and_emit(
                        &mut node, &mut tokenizer, &mut model,
                        &chat_template, &model_id, &system_prompt,
                        &combined_src, force_disable_thinking, temperature, max_tokens,
                        None, &eos_tokens, "replace_last", false,
                        Some(&combined_src),
                    ) {
                        Ok(_new_tgt) => {
                            // Do NOT chain further merges: set last_segment = None so the
                            // merged pair is considered final and won't cascade into the next.
                            last_segment = None;
                        }
                        Err(e) => {
                            tracing::error!("Retroactive merge re-translation failed: {e}");
                            last_segment = Some(LastSegment {
                                source_text: src,
                                completed_at: Instant::now(),
                            });
                        }
                    }
                } else {
                    last_segment = Some(LastSegment {
                        source_text: src,
                        completed_at: Instant::now(),
                    });
                }
            }
        }};
    }

    // commit_sentence! — translate the next committable sentence in the pending session
    // without closing the session (sentence-cursor model).
    // `$require_tail`: true = progressive (need text after boundary to confirm it's done),
    //                  false = final (boundary at end of text is also valid).
    macro_rules! commit_sentence {
        ($require_tail:expr) => {{
            let min_chars = if normalize_lang(&src_lang) == "Chinese" {
                MIN_CHARS_FOR_PUNCT_COMMIT_ZH
            } else {
                MIN_CHARS_FOR_PUNCT_COMMIT_NON_ZH
            };
            // Only commit when no in-progress burst (for final mode).
            // For progressive mode, current_burst_text IS the effective content.
            let can_commit = pending.as_ref().map(|s| {
                if !$require_tail { s.current_burst_text.is_none() } else { true }
            }).unwrap_or(false);

            if can_commit {
                let commit_info = pending.as_ref().and_then(|session| {
                    let eff = effective_text(session, &src_lang);
                    let committed_byte = eff.char_indices()
                        .nth(session.committed_chars)
                        .map(|(i, _)| i)
                        .unwrap_or(eff.len());
                    let uncommitted = &eff[committed_byte..];
                    let trimmed_len = uncommitted.trim_end().len();
                    // Also force-commit if uncommitted portion is very long.
                    let force = should_force_finalize_by_size(uncommitted.trim(), &src_lang);
                    let end_byte = if force {
                        Some(trimmed_len)
                    } else {
                        let end_without_terminal =
                            committable_end(uncommitted, min_chars, $require_tail, false);
                        if $require_tail {
                            end_without_terminal
                        } else {
                            let end_with_terminal =
                                committable_end(uncommitted, min_chars, false, true);
                            match end_with_terminal {
                                Some(eb) if eb == trimmed_len => {
                                    let slice = &uncommitted[..eb];
                                    let candidate = ProgressiveCommitCandidate {
                                        text: slice.trim().to_string(),
                                        new_committed_chars: session.committed_chars
                                            + slice.chars().count(),
                                        question_id: session.question_id,
                                    };
                                    let previous =
                                        session.pending_progressive_commit.as_ref();
                                    if should_allow_terminal_final_commit(previous, &candidate) {
                                        Some(eb)
                                    } else {
                                        end_without_terminal
                                    }
                                }
                                other => other,
                            }
                        }
                    };
                    end_byte.map(|eb| {
                        let slice = &uncommitted[..eb];
                        let to_translate = slice.trim().to_string();
                        let new_committed = session.committed_chars + slice.chars().count();
                        (
                            to_translate,
                            new_committed,
                            session.question_id,
                            should_commit_progressive_immediately(
                                uncommitted,
                                eb,
                                &src_lang,
                            ),
                        )
                    })
                });

                let has_commit_info = commit_info.is_some();
                if let Some((to_translate, new_committed, qid, immediate_progressive_commit)) = commit_info {
                    if !to_translate.is_empty() {
                        if $require_tail {
                            let candidate = ProgressiveCommitCandidate {
                                text: to_translate.clone(),
                                new_committed_chars: new_committed,
                                question_id: qid,
                            };
                            if !immediate_progressive_commit {
                                let previous = pending
                                    .as_ref()
                                    .and_then(|s| s.pending_progressive_commit.as_ref());
                                if !should_commit_progressive_candidate(previous, &candidate) {
                                    if let Some(s) = pending.as_mut() {
                                        s.pending_progressive_commit = Some(candidate);
                                    }
                                    tracing::debug!(
                                        "Deferring progressive sentence commit until boundary repeats"
                                    );
                                    continue;
                                }
                            }
                            if let Some(s) = pending.as_mut() {
                                s.pending_progressive_commit = None;
                            }
                        } else if let Some(s) = pending.as_mut() {
                            s.pending_progressive_commit = None;
                        }

                        eprintln!(
                            "[translator-sentence-commit] qid={:?} chars={} text={}",
                            qid,
                            to_translate.chars().count(),
                            &to_translate[..to_translate.char_indices().nth(80).map(|(i,_)|i).unwrap_or(to_translate.len())]
                        );
                        tracing::info!(
                            "Sentence commit: {} chars: {}",
                            to_translate.chars().count(),
                            &to_translate[..to_translate.char_indices().nth(80).map(|(i,_)|i).unwrap_or(to_translate.len())]
                        );
                        let _ = send_source(&mut node, &to_translate, "complete", qid);
                        match translate_and_emit(
                            &mut node, &mut tokenizer, &mut model,
                            &chat_template, &model_id, &system_prompt,
                            &to_translate, force_disable_thinking, temperature, max_tokens,
                            qid, &eos_tokens, "complete", false, None,
                        ) {
                            Ok(_) => {
                                if let Some(s) = pending.as_mut() {
                                    s.committed_chars = new_committed;
                                    s.pending_progressive_commit = None;
                                    // Reset the age clock so max_age_timeout counts from
                                    // the last successful commit, not the session start.
                                    s.started_at = Instant::now();
                                }
                            }
                            Err(e) => {
                                tracing::error!("Sentence commit translation failed: {e}");
                            }
                        }
                    }
                }
                if $require_tail && !has_commit_info {
                    if let Some(s) = pending.as_mut() {
                        s.pending_progressive_commit = None;
                    }
                }
            }
        }};
    }

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
            let has_active_burst = session.current_burst_text.is_some();
            let question_ended_at = session.question_ended_at;
            let session_qid = session.question_id;

            if let Some(qe_at) = question_ended_at {
                // Delayed question_ended: wait for ASR to finish the current burst.
                // Finalize as soon as ASR sends mode=final (burst_done) OR the
                // ASR stream itself has been idle long enough since the last update.
                let burst_done = !has_active_burst;
                let should_finalize = should_finalize_deferred_question_ended(
                    qe_at.elapsed(),
                    idle_elapsed,
                    has_active_burst,
                );
                if should_finalize {
                    let reason = if burst_done {
                        "question_ended_asr_final"
                    } else {
                        "question_ended_asr_idle"
                    };
                    tracing::info!(
                        "Delayed question_ended finalizing (reason={}, burst_done={}, qe_elapsed_ms={}, asr_idle_ms={})",
                        reason,
                        burst_done,
                        qe_at.elapsed().as_millis(),
                        idle_elapsed.as_millis()
                    );
                    finalize!(reason);
                    already_ended_qid = session_qid;
                }
            } else if !has_active_burst && idle_elapsed >= Duration::from_millis(idle_threshold_ms) {
                // While progressive ASR is active, suppress idle timeout — speech is still
                // in progress; only question_ended or a "final" transcript should finalize.
                finalize!("idle_timeout");
            } else if age_elapsed >= Duration::from_secs(FORCE_SEND_SECS) {
                finalize!("max_age_timeout");
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
                    let transcription_mode: Option<String> = metadata
                        .parameters
                        .get("transcription_mode")
                        .and_then(|p| match p {
                            dora_node_api::Parameter::String(s) => Some(s.clone()),
                            _ => None,
                        });
                    let is_progressive = transcription_mode.as_deref() == Some("progressive");
                    let _is_final_mode = transcription_mode.as_deref() == Some("final");

                    let arr = match data.as_any().downcast_ref::<StringArray>() {
                        Some(a) if a.len() > 0 => a,
                        _ => continue,
                    };
                    let raw_chunk = arr.value(0).trim().to_string();
                    if raw_chunk.is_empty() {
                        continue;
                    }
                    if should_drop_low_info_chunk(&raw_chunk, &src_lang) {
                        tracing::debug!("Dropping low-info ASR chunk: {}", raw_chunk);
                        continue;
                    }

                    // Drop stale ASR chunks that arrive after question_ended for the same qid.
                    if let Some(ended_qid) = already_ended_qid {
                        if question_id == Some(ended_qid) {
                            tracing::info!(
                                "Dropping stale chunk for already-ended qid={}: {}",
                                ended_qid,
                                &raw_chunk[..raw_chunk.char_indices().nth(40).map(|(i,_)|i).unwrap_or(raw_chunk.len())]
                            );
                            continue;
                        }
                        // Different qid → clear the marker
                        already_ended_qid = None;
                    }

                    let now = Instant::now();

                    let chunk = raw_chunk;

                    tracing::info!(
                        "Received ASR chunk (mode={:?}): {}",
                        transcription_mode,
                        &chunk[..chunk.char_indices().nth(80).map(|(i,_)|i).unwrap_or(chunk.len())]
                    );
                    eprintln!(
                        "[translator-chunk] qid={:?} mode={:?} chunk={}",
                        question_id,
                        transcription_mode,
                        &chunk[..chunk.char_indices().nth(120).map(|(i,_)|i).unwrap_or(chunk.len())]
                    );

                    match pending.as_mut() {
                        Some(session) => {
                            if session.question_id != question_id {
                                finalize!("question_id_switch");
                                pending = Some(PendingSession {
                                    question_id,
                                    text: if is_progressive { String::new() } else { chunk.clone() },
                                    current_burst_text: if is_progressive { Some(chunk.clone()) } else { None },
                                    committed_chars: 0,
                                    started_at: now,
                                    last_chunk_at: now,
                                    last_source_emit_at: now,
                                    last_source_emit_len: 0,
                                    pending_progressive_commit: None,
                                    question_ended_at: None,
                                });
                            } else if is_progressive {
                                session.current_burst_text = Some(chunk.clone());
                                session.last_chunk_at = now;
                            } else {
                                let sep = if normalize_lang(&src_lang) == "Chinese" {
                                    ""
                                } else {
                                    " "
                                };
                                session.text.push_str(sep);
                                session.text.push_str(&chunk);
                                session.last_chunk_at = now;
                                session.current_burst_text = None;
                                session.pending_progressive_commit = None;
                            }
                        }
                        None => {
                            pending = Some(PendingSession {
                                question_id,
                                text: if is_progressive { String::new() } else { chunk.clone() },
                                current_burst_text: if is_progressive { Some(chunk.clone()) } else { None },
                                committed_chars: 0,
                                started_at: now,
                                last_chunk_at: now,
                                last_source_emit_at: now,
                                last_source_emit_len: 0,
                                pending_progressive_commit: None,
                                question_ended_at: None,
                            });
                        }
                    }

                    // Sentence-cursor model: commit completed sentences on every chunk.
                    // Progressive: need non-empty tail after boundary (confirms sentence is done).
                    // Final: boundary at end of text is also valid (burst is finished).
                    if is_progressive {
                        commit_sentence!(true);

                        if let Some(session) = pending.as_ref() {
                            let eff = effective_text(session, &src_lang);
                            let committed_byte = eff.char_indices()
                                .nth(session.committed_chars)
                                .map(|(i, _)| i)
                                .unwrap_or(eff.len());
                            let tail = eff[committed_byte..].trim();
                            if !tail.is_empty() {
                                let qid = session.question_id;
                                let _ = send_source(&mut node, tail, "streaming", qid);
                                if let Some(s) = pending.as_mut() {
                                    s.last_source_emit_at = now;
                                    s.last_source_emit_len = tail.chars().count();
                                }
                            }
                        }
                        continue;
                    }

                    if let Some(session) = pending.as_ref() {
                        if let Some(deferred_qid) = deferred_ended_qid {
                            if session.question_id == Some(deferred_qid) {
                                tracing::info!(
                                    "Applying deferred question_ended for qid={}",
                                    deferred_qid
                                );
                                eprintln!("[translator-deferred-apply] qid={}", deferred_qid);
                                deferred_ended_qid = None;
                                finalize!("deferred_question_ended");
                                already_ended_qid = Some(deferred_qid);
                                continue;
                            }
                        }
                    }

                    commit_sentence!(false);

                    if let Some(session) = pending.as_ref() {
                        let eff = effective_text(session, &src_lang);
                        let committed_byte = eff.char_indices()
                            .nth(session.committed_chars)
                            .map(|(i, _)| i)
                            .unwrap_or(eff.len());
                        let tail = eff[committed_byte..].trim().to_string();
                        if !tail.is_empty() {
                            let qid = session.question_id;
                            let _ = send_source(&mut node, &tail, "streaming", qid);
                            if let Some(s) = pending.as_mut() {
                                s.last_source_emit_at = now;
                                s.last_source_emit_len = tail.chars().count();
                            }
                        }
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
                    let has_active_burst = pending.as_ref()
                        .map(|s| s.current_burst_text.is_some())
                        .unwrap_or(false);

                    if has_active_burst {
                        // ASR is still processing the last audio segment — its progressive
                        // snapshot is incomplete.  Set a deadline so the timer loop
                        // finalizes once ASR sends mode=final or the ASR stream has
                        // been idle long enough after the last progressive update.
                        // Do NOT set already_ended_qid yet so ASR final can still arrive.
                        if let Some(session) = pending.as_mut() {
                            if session.question_ended_at.is_none() {
                                session.question_ended_at = Some(Instant::now());
                                tracing::info!(
                                    "question_ended deferred: ASR burst still active for qid={:?}, \
                                     will finalize on ASR final or after {}ms of ASR inactivity",
                                    ended_question_id,
                                    DEFERRED_QUESTION_ENDED_ASR_IDLE_MS
                                );
                                eprintln!(
                                    "[translator-qe-defer] qid={:?} waiting for ASR final or idle fallback",
                                    ended_question_id
                                );
                            }
                        }
                    } else {
                        // No active burst — ASR already delivered final for everything.
                        // Finalize immediately for minimum latency.
                        finalize!("question_ended");
                        already_ended_qid = ended_question_id;
                    }
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

    tracing::info!("dora-qwen35-translator stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::Duration;

    #[test]
    fn chinese_progressive_does_not_commit_on_soft_comma_only() {
        let text = "我们在云端统一管理边缘节点和应用发布策略，在边缘侧执行推理任务并持续回传运行指标";

        assert_eq!(
            committable_end(text, MIN_CHARS_FOR_PUNCT_COMMIT_ZH, true, false),
            None
        );
    }

    #[test]
    fn deferred_question_ended_waits_for_recent_asr_activity() {
        assert!(
            !should_finalize_deferred_question_ended(
                Duration::from_secs(4),
                Duration::from_secs(1),
                true,
            )
        );
    }

    #[test]
    fn deferred_question_ended_finalizes_after_asr_has_been_idle_long_enough() {
        assert!(should_finalize_deferred_question_ended(
            Duration::from_secs(4),
            Duration::from_secs(3),
            true,
        ));
    }

    #[test]
    fn deferred_question_ended_finalizes_immediately_when_burst_is_done() {
        assert!(should_finalize_deferred_question_ended(
            Duration::from_millis(200),
            Duration::from_millis(200),
            false,
        ));
    }

    #[test]
    fn progressive_candidate_requires_a_second_matching_snapshot() {
        let candidate = ProgressiveCommitCandidate {
            text: "我们今天主要介绍 KubeEdge。".to_string(),
            new_committed_chars: 18,
            question_id: Some(42),
        };

        assert!(!should_commit_progressive_candidate(None, &candidate));
    }

    #[test]
    fn progressive_candidate_commits_when_same_boundary_reappears() {
        let previous = ProgressiveCommitCandidate {
            text: "我们今天主要介绍 KubeEdge。".to_string(),
            new_committed_chars: 18,
            question_id: Some(42),
        };
        let current = ProgressiveCommitCandidate {
            text: "我们今天主要介绍 KubeEdge。".to_string(),
            new_committed_chars: 18,
            question_id: Some(42),
        };

        assert!(should_commit_progressive_candidate(Some(&previous), &current));
    }

    #[test]
    fn progressive_candidate_resets_when_boundary_text_changes() {
        let previous = ProgressiveCommitCandidate {
            text: "我们今天主要介绍 KubeEdge。".to_string(),
            new_committed_chars: 18,
            question_id: Some(42),
        };
        let current = ProgressiveCommitCandidate {
            text: "我们今天主要介绍 KubeEdge 项目。".to_string(),
            new_committed_chars: 20,
            question_id: Some(42),
        };

        assert!(!should_commit_progressive_candidate(Some(&previous), &current));
    }

    #[test]
    fn progressive_high_confidence_boundary_can_commit_immediately() {
        let text = "我们今天主要介绍 KubeEdge。然后继续讨论它在边缘 AI 场景中的应用价值";
        let end = committable_end(text, MIN_CHARS_FOR_PUNCT_COMMIT_ZH, true, false).unwrap();

        assert!(should_commit_progressive_immediately(text, end, "zh"));
    }

    #[test]
    fn progressive_short_tail_still_requires_confirmation() {
        let text = "我们今天主要介绍 KubeEdge。然后";
        let end = committable_end(text, MIN_CHARS_FOR_PUNCT_COMMIT_ZH, true, false).unwrap();

        assert!(!should_commit_progressive_immediately(text, end, "zh"));
    }

    #[test]
    fn final_terminal_boundary_is_not_trusted_without_progressive_confirmation() {
        let text = "大家下午好，我叫鲍月，然后来自华为，现在也是CoolEdge社区的Maintainer。然后今天CoolEdge社区。";

        assert_eq!(
            committable_end(text, MIN_CHARS_FOR_PUNCT_COMMIT_ZH, false, false),
            text.find("然后今天").map(|idx| idx)
        );
    }

    #[test]
    fn final_terminal_boundary_can_commit_when_progressive_already_confirmed_it() {
        let candidate = ProgressiveCommitCandidate {
            text: "我们今天主要介绍 KubeEdge。".to_string(),
            new_committed_chars: 18,
            question_id: Some(42),
        };

        assert!(should_allow_terminal_final_commit(Some(&candidate), &candidate));
    }
}
