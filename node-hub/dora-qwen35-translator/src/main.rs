//! dora-qwen35-translator: real-time translation Dora node powered by qwen3.5-35B-mlx.
//!
//! The translator treats upstream ASR as a text provider, not as a sentence
//! segmentation authority.
//!
//! Pipeline position:
//!   dora-qwen3-asr + mic bridge
//!      → dora-qwen35-translator
//!      → [source_text, translation]
//!
//! # Inputs
//!   text           – StringArray (single element: latest ASR text chunk)
//!   question_ended – Optional upstream silence marker; ignored by commit logic
//!
//! # Outputs
//!   source_text  – current transcript tail or committed sentence
//!   translation  – translated committed sentence
//!   log          – status / debug messages
//!
//! Internally, the node keeps a continuously growing transcript buffer, merges
//! new text chunks into that buffer, and only commits translations when it can
//! identify a stable, meaningful span.

mod transcript_buffer;

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
use serde::Deserialize;
use std::collections::{BTreeMap, HashSet};
use std::path::PathBuf;
use std::time::{Duration, Instant};
use transcript_buffer::TranscriptBuffer;

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

const MIN_RAW_TAIL_CHARS: usize = 12;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum CommitPromptMode {
    Normal,
    FinalDrain,
}

#[derive(Debug, Default, Deserialize)]
struct StructuredResult {
    #[serde(default)]
    raw_text: String,
    #[serde(default)]
    text: String,
    #[serde(default)]
    translation: String,
}

#[derive(Debug, Deserialize)]
struct StructuredTranslation {
    #[serde(default)]
    result: StructuredResult,
    #[serde(default)]
    remaining: String,
}

fn build_system_prompt(tgt_lang: &str, mode: CommitPromptMode) -> String {
    match mode {
        CommitPromptMode::Normal => format!(
            "/no_think 你会接收到一段没有标点符号的 ASR 文本，这段文本在语义上可能还没有说完。

仅返回 JSON，包含以下字段：
result, remaining

规则：
- 先判断输入前缀中，是否存在“现在就可以独立展示和翻译”的完整语义部分。
- 只有这部分才放进 result，其余全部放进 remaining。
- result 必须包含 raw_text, text, translation 三个字段。
- result.raw_text 和 remaining 都是机器字段，必须直接从输入中逐字符复制。
- 对 result.raw_text 和 remaining：绝对不允许加空格、不允许删字、不允许改字、不允许补标点、不允许改写大小写。
- 只有 result.text 可以整理、补标点、提升可读性。
- result.raw_text + remaining 必须严格等于输入文本。
- result.text 必须是补全自然标点后的源语言句子，适合直接展示。
- result.translation 必须是 result.text 的自然流畅的 {tgt} 翻译。
- 如果后半句还有继续说下去的可能，必须保留在 remaining，绝对不要为了清空 remaining 而强行翻译残句。
- remaining 是正常结果；不确定时，宁可让 remaining 更多，也不要多翻。
- 如果没有任何可以安全翻译的内容，返回：
  result={{}}, remaining=input。",
            tgt = lang_display(tgt_lang),
        ),
        CommitPromptMode::FinalDrain => format!(
            "/no_think 你会接收到一段没有标点符号的 ASR 文本。现在语音已经停止，这是最后一次收尾。

仅返回 JSON，包含以下字段：
result, remaining

规则：
- result 必须包含 raw_text, text, translation 三个字段。
- result.raw_text 和 remaining 都是机器字段，必须直接从输入中逐字符复制。
- 对 result.raw_text 和 remaining：绝对不允许加空格、不允许删字、不允许改字、不允许补标点、不允许改写大小写。
- 只有 result.text 可以整理、补标点、提升可读性。
- result.raw_text + remaining 必须严格等于输入文本。
- result.text 必须是补全自然标点后的源语言句子，适合直接展示。
- result.translation 必须是 result.text 的自然流畅的 {tgt} 翻译。
- 优先提交完整句子。
- 如果尾巴明显没说完，保留在 remaining。
- 只有在收尾场景下，你才可以把最后一个不完全但已经适合展示的尾部片段放进 result。
- 如果没有任何可以安全翻译的内容，返回：
  result={{}}, remaining=input。",
            tgt = lang_display(tgt_lang),
        ),
    }
}

fn build_analysis_user_prompt(raw_tail: &str) -> String {
    format!(
        "Input:\n{raw_tail}\n\nReturn JSON only.",
    )
}

fn format_commit_prompt_debug(system_prompt: &str, user_prompt: &str) -> String {
    format!(
        "system_prompt=\n{system_prompt}\n\nuser_prompt=\n{user_prompt}"
    )
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

fn generate_text_completion(
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
    let _ = send_log(node, &format!(
        "Translated in {:.2}s ({} tokens)",
        elapsed, generated
    ));

    // Release Metal buffer pool accumulated during KV-cache inference.
    // Equivalent to Python's mx.metal.clear_cache(); prevents memory pressure
    // from building up across successive translations.
    unsafe { mlx_sys::mlx_clear_cache(); }

    Ok(full_translation.trim().to_string())
}

fn extract_json_object(raw: &str) -> Result<&str> {
    let start = raw.find('{').ok_or_else(|| anyhow!("No JSON object start found"))?;
    let end = raw.rfind('}').ok_or_else(|| anyhow!("No JSON object end found"))?;
    if end < start {
        return Err(anyhow!("Malformed JSON object boundaries"));
    }
    Ok(&raw[start..=end])
}

fn validate_structured_translation(raw_tail: &str, response: &StructuredTranslation) -> Result<()> {
    if raw_tail != format!("{}{}", response.result.raw_text, response.remaining) {
        return Err(anyhow!("result.raw_text + remaining does not equal raw input"));
    }

    if !response.result.raw_text.trim().is_empty()
        && (response.result.text.trim().is_empty() || response.result.translation.trim().is_empty())
    {
        return Err(anyhow!("result.text or result.translation is empty"));
    }

    Ok(())
}

fn try_commit_once(
    node: &mut DoraNode,
    transcript: &mut TranscriptBuffer,
    tokenizer: &mut Tokenizer,
    model: &mut qwen3_5_35b_mlx::Model,
    chat_template: &str,
    model_id: &str,
    tgt_lang: &str,
    mode: CommitPromptMode,
    force_disable_thinking: bool,
    temperature: f32,
    max_tokens: usize,
    min_raw_tail_chars: usize,
    eos_tokens: &HashSet<u32>,
) -> bool {
    if !transcript.has_pending_raw_text(min_raw_tail_chars) {
        return false;
    }

    let raw_tail = transcript.raw_uncommitted_tail().to_string();
    if raw_tail.is_empty() {
        return false;
    }

    let system_prompt = build_system_prompt(tgt_lang, mode);
    let user_prompt = build_analysis_user_prompt(&raw_tail);
    tracing::info!(
        "Entering commit attempt\n{}\nraw_tail=\n{}\nprompt=\n{}",
        transcript.debug_snapshot(),
        raw_tail,
        format_commit_prompt_debug(&system_prompt, &user_prompt)
    );
    let model_output = match generate_text_completion(
        node,
        tokenizer,
        model,
        chat_template,
        model_id,
        &system_prompt,
        &user_prompt,
        force_disable_thinking,
        temperature,
        max_tokens,
        eos_tokens,
        ) {
        Ok(output) => output,
        Err(e) => {
            tracing::error!("Structured translation generation failed: {e}");
            let _ = send_log(node, &format!("Structured translation generation failed: {e}"));
            return false;
        }
    };
    tracing::info!("Structured translation raw output\n{}", model_output);

    let parsed = match extract_json_object(&model_output)
        .and_then(|json| serde_json::from_str::<StructuredTranslation>(json).map_err(|e| anyhow!(e)))
    {
        Ok(parsed) => parsed,
        Err(e) => {
            tracing::warn!(
                "Failed to parse structured translation output: {e}\nraw_tail=\n{}\nraw_output=\n{}",
                raw_tail,
                model_output
            );
            let _ = send_log(node, &format!("Failed to parse structured translation output: {e}"));
            return false;
        }
    };

    if let Err(e) = validate_structured_translation(&raw_tail, &parsed) {
        tracing::warn!(
            "Structured translation validation failed: {e}\nraw_tail=\n{}\nparsed={:#?}",
            raw_tail,
            parsed
        );
        let _ = send_log(node, &format!("Structured translation validation failed: {e}"));
        return false;
    }

    if parsed.result.raw_text.is_empty() {
        return false;
    }

    tracing::info!(
        "Structured commit: 1 result, {} raw chars",
        parsed.result.raw_text.chars().count()
    );

    tracing::info!(
        "Sentence commit: {} chars\n{}",
        parsed.result.text.chars().count(),
        parsed.result.text
    );
    let _ = send_source(node, &parsed.result.text, "complete", None);
    let _ = send_translation_chunk(node, &parsed.result.translation, "complete", None);

    match transcript.commit_raw_prefix(&parsed.result.raw_text) {
        Ok(()) => {
            tracing::info!(
                "Committed raw prefix\ncommitted_prefix=\n{}\nupdated_committed_raw_pos={}\n{}",
                parsed.result.raw_text,
                transcript.committed_raw_pos(),
                transcript.debug_snapshot()
            );
            true
        }
        Err(e) => {
            tracing::error!("Failed to advance committed raw prefix: {e}");
            let _ = send_log(node, &format!("Failed to advance committed raw prefix: {e}"));
            false
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
    tracing::info!("Translation: {} → {}", src_lang, tgt_lang);

    let model_path = resolve_model_path();
    tracing::info!("Loading Qwen3.5 model from: {}", model_path.display());
    let _ = send_log(
        &mut node,
        &format!("Loading Qwen3.5 model from {}", model_path.display()),
    );

    let tokenizer_file = model_path.join("tokenizer.json");
    let tokenizer_config_file = model_path.join("tokenizer_config.json");

    let mut tokenizer = Tokenizer::from_file(&tokenizer_file)
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

    let mut model =
        load_model(&model_path).map_err(|e| anyhow!("Failed to load Qwen3.5 model: {e}"))?;
    let eos_tokens = load_eos_tokens(&model_path)?;

    tracing::info!("Qwen3.5 model loaded");
    let _ = send_log(&mut node, "Qwen3.5 model loaded — ready to translate");

    let model_id = model_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("qwen3.5")
        .to_string();
    let force_disable_thinking = model_id.to_lowercase().contains("qwen3");
    tracing::info!(
        "Prompt mode: {}, template_switch_supported={}",
        if force_disable_thinking {
            "qwen3-no-think"
        } else {
            "default"
        },
        supports_enable_thinking(&chat_template)
    );
    tracing::info!("Buffer merge mode active: upstream modes are ignored for commit decisions");
    let _ = send_log(
        &mut node,
        "Buffer merge mode active: upstream modes are ignored for commit decisions",
    );

    let mut transcript = TranscriptBuffer::new();
    let mut current_burst_id: Option<i64> = None;
    let mut translate_idle = true;
    let mut last_analyzed_key: Option<String> = None;
    let mut stopping = false;

    loop {
        let event = events.recv_timeout(Duration::from_millis(100));

        match event {
            None => {}
            Some(Event::Input {
                id,
                data,
                metadata,
                ..
            }) => {
                if id.as_str() == "text" {
                    let burst_id = metadata
                        .parameters
                        .get("burst_id")
                        .and_then(|p| match p {
                            dora_node_api::Parameter::Integer(v) => Some(*v),
                            _ => None,
                        })
                        .or_else(|| metadata
                        .parameters
                        .get("question_id")
                        .and_then(|p| match p {
                            dora_node_api::Parameter::Integer(v) => Some(*v),
                            _ => None,
                        }));
                    if burst_id.is_some() {
                        current_burst_id = burst_id;
                    }

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

                    tracing::info!(
                        "Received ASR chunk\nburst_id={:?}\nchunk=\n{}",
                        current_burst_id,
                        chunk
                    );

                    let changed = transcript.update_from_chunk(burst_id, &chunk);
                    tracing::info!(
                        "Transcript state after chunk (changed={})\n{}",
                        changed,
                        transcript.debug_snapshot()
                    );
                    if changed {
                        last_analyzed_key = None;
                    }

                    let tail = transcript.uncommitted_tail();
                    if !tail.is_empty() {
                        let _ = send_source(&mut node, &tail, "streaming", current_burst_id);
                    }
                } else if id.as_str() == "question_ended" {
                    tracing::debug!("Ignoring question_ended in buffer merge mode");
                }
            }
            Some(Event::Stop(_)) => {
                tracing::info!("Stop event received, draining pending translation work");
                stopping = true;
                if transcript.seal_active_burst() {
                    tracing::info!("Active burst sealed on stop\n{}", transcript.debug_snapshot());
                    last_analyzed_key = None;
                }
            }
            _ => {}
        }

        let min_chars = if stopping { 1 } else { MIN_RAW_TAIL_CHARS };
        let raw_tail = transcript.raw_uncommitted_tail().to_string();
        let mode = if stopping {
            CommitPromptMode::FinalDrain
        } else {
            CommitPromptMode::Normal
        };
        let analyze_key = format!("mode={mode:?}\nraw_tail=\n{raw_tail}");
        if translate_idle
            && transcript.has_pending_raw_text(min_chars)
            && last_analyzed_key.as_deref() != Some(analyze_key.as_str())
        {
            translate_idle = false;
            if !translate_idle {
                let did_commit = try_commit_once(
                    &mut node,
                    &mut transcript,
                    &mut tokenizer,
                    &mut model,
                    &chat_template,
                    &model_id,
                    &tgt_lang,
                    mode,
                    force_disable_thinking,
                    temperature,
                    max_tokens,
                    min_chars,
                    &eos_tokens,
                );
                translate_idle = true;
                if did_commit {
                    last_analyzed_key = None;
                } else {
                    last_analyzed_key = Some(analyze_key);
                }

                if did_commit {
                    let tail = transcript.uncommitted_tail();
                    if !tail.is_empty() {
                        let _ = send_source(&mut node, &tail, "streaming", current_burst_id);
                    }
                }
            }
        }

        if stopping {
            let raw_tail = transcript.raw_uncommitted_tail().to_string();
            let final_key = format!("mode={:?}\nraw_tail=\n{}", CommitPromptMode::FinalDrain, raw_tail);
            let drained = raw_tail.is_empty()
                || last_analyzed_key.as_deref() == Some(final_key.as_str());
            if translate_idle && drained {
                break;
            }
        }
    }

    tracing::info!("dora-qwen35-translator stopped");
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        build_system_prompt, extract_json_object, format_commit_prompt_debug,
        validate_structured_translation, CommitPromptMode, StructuredResult, StructuredTranslation,
    };

    #[test]
    fn extract_json_object_ignores_wrapper_text() {
        let raw = "```json\n{\"result\":{\"raw_text\":\"abc\",\"text\":\"甲。\",\"translation\":\"A.\"},\"remaining\":\"def\"}\n```";
        let json = extract_json_object(raw).expect("json object should be extracted");
        assert_eq!(
            json,
            "{\"result\":{\"raw_text\":\"abc\",\"text\":\"甲。\",\"translation\":\"A.\"},\"remaining\":\"def\"}"
        );
    }

    #[test]
    fn validate_structured_translation_accepts_prefix_partition() {
        let response = StructuredTranslation {
            result: StructuredResult {
                raw_text: "大家下午好我叫鲍月然后来自华为".into(),
                text: "大家下午好，我叫鲍月。然后来自华为。".into(),
                translation: "Good afternoon, I'm Bao Yue. I'm from Huawei.".into(),
            },
            remaining: "现在也是CoolEdge".into(),
        };

        validate_structured_translation(
            "大家下午好我叫鲍月然后来自华为现在也是CoolEdge",
            &response,
        )
        .expect("valid structured translation should pass");
    }

    #[test]
    fn validate_structured_translation_rejects_sentence_prefix_mismatch() {
        let response = StructuredTranslation {
            result: StructuredResult {
                raw_text: "大家下午好我叫鲍月".into(),
                text: "大家下午好，我叫鲍月。".into(),
                translation: "Good afternoon, I'm Bao Yue.".into(),
            },
            remaining: "现在也是CoolEdge".into(),
        };

        let err = validate_structured_translation(
            "大家下午好我叫鲍月然后来自华为现在也是CoolEdge",
            &response,
        )
        .expect_err("mismatched raw_text + remaining partition should fail");
        assert!(err
            .to_string()
            .contains("result.raw_text + remaining does not equal raw input"));
    }

    #[test]
    fn validate_structured_translation_accepts_sentence_without_terminal_punctuation() {
        let response = StructuredTranslation {
            result: StructuredResult {
                raw_text: "大家下午好我叫鲍月".into(),
                text: "大家下午好，我叫鲍月".into(),
                translation: "Good afternoon, I'm Bao Yue.".into(),
            },
            remaining: "".into(),
        };

        validate_structured_translation("大家下午好我叫鲍月", &response)
            .expect("content quality should not be rejected by structural validator");
    }

    #[test]
    fn build_system_prompt_is_language_agnostic_and_conservative() {
        let prompt = build_system_prompt("en", CommitPromptMode::Normal);
        assert!(prompt.contains("你会接收到一段没有标点符号的 ASR 文本"));
        assert!(prompt.contains("result, remaining"));
        assert!(prompt.contains("绝对不要为了清空 remaining 而强行翻译残句"));
        assert!(prompt.contains("result.raw_text 和 remaining 都是机器字段"));
        assert!(prompt.contains("绝对不允许加空格、不允许删字、不允许改字"));
    }

    #[test]
    fn build_system_prompt_final_drain_mentions_final_fragment() {
        let prompt = build_system_prompt("zh", CommitPromptMode::FinalDrain);
        assert!(prompt.contains("现在语音已经停止，这是最后一次收尾"));
        assert!(prompt.contains("最后一个不完全但已经适合展示的尾部片段放进 result"));
        assert!(prompt.contains("result.raw_text 和 remaining 都是机器字段"));
    }

    #[test]
    fn format_commit_prompt_debug_keeps_full_prompt_sections() {
        let debug = format_commit_prompt_debug(
            "You are a translator.",
            "Input:\n大家下午好我叫鲍月然后来自华为\n\nReturn JSON only.",
        );
        assert!(debug.contains("system_prompt=\nYou are a translator."));
        assert!(debug.contains("user_prompt=\nInput:\n大家下午好我叫鲍月然后来自华为"));
        assert!(debug.contains("Return JSON only."));
    }
}
