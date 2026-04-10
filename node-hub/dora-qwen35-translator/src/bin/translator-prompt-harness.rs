use anyhow::{anyhow, Result};
use minijinja::{context, Environment};
use minijinja_contrib::pycompat::unknown_method_callback;
use mlx_rs::ops::indexing::{IndexOp, NewAxis};
use mlx_rs::transforms::eval;
use mlx_lm_utils::tokenizer::{
    load_model_chat_template_from_file, ApplyChatTemplateArgs, Conversation, Tokenizer,
};
use qwen3_5_35b_mlx::{load_model, Generate};
use serde::Deserialize;
use std::collections::HashSet;
use std::path::PathBuf;
use std::time::Instant;

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
        _ => "English",
    }
}

fn lang_display(code: &str) -> &'static str {
    normalize_lang(code)
}

fn resolve_model_path() -> PathBuf {
    if let Ok(v) = std::env::var("QWEN35_TRANSLATOR_MODEL_PATH") {
        if !v.trim().is_empty() {
            return PathBuf::from(v);
        }
    }
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

fn build_system_prompt(tgt_lang: &str, mode: CommitPromptMode) -> String {
    // 大家下午好我叫鲍月然后来自华为现在也是CoolEdge社区的Maintainer然后今天CoolEdge社区
    match mode {
//         CommitPromptMode::Normal => format!(
//             "/no_think 你会接收到一段没有标点符号的文本，这段文本在语义上可能还没有说完， 你将会从这段文本中选取语义完整的部分并进行翻译。
// 仅返回 JSON,包含以下字段: result, remaining
// 规则：
// - 先根据语义在这种文本中选取完整的部分, 这个部分将用于翻译工作, 剩下的部分放入 remaining 字段。
// - result 必须包含 raw_text, text, translation 三个字段。
// - result.raw_text 是你从原始文本中选择的部分, result.text 是你对这段文本添加标点符号的结果, result.translation 是你对这段文本的翻译结果。
// - result.raw_text 和 remaining 字段绝对不允许加空格、不允许删字、不允许改字、不允许补标点、不允许改写大小写。
// - remaining 是正常结果；不确定时，宁可让 remaining 更多，也不要多翻。
// - 如果没有任何可以安全翻译的内容，返回：
//   result={{}}, remaining=input。"
//         ),
        CommitPromptMode::Normal => format!(
            "你会接收到一段文本，请按照语义给这段文本添加标点符号并返回。你只能添加逗号和句号，不要对原文做任何修改！不要对原文做任何修改！"
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
    format!("{raw_tail}")
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

    Ok(encodings
        .iter()
        .flat_map(|enc| enc.get_ids().iter().copied())
        .collect::<Vec<u32>>())
}

fn generate_text_completion(
    tokenizer: &mut Tokenizer,
    model: &mut qwen3_5_35b_mlx::Model,
    chat_template: &str,
    model_id: &str,
    system_prompt: &str,
    user_prompt: &str,
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
        user_prompt,
        force_disable_thinking,
    )?;

    let prompt_len = prompt_ids.len();
    let prompt_tokens = mlx_rs::Array::from(&prompt_ids[..]).index(NewAxis);
    eprintln!("prompt_tokens={prompt_len}");
    let t_start = Instant::now();

    let generator = Generate::new(model, temperature, &prompt_tokens);
    let mut token_buf: Vec<mlx_rs::Array> = Vec::new();
    let mut full = String::new();
    let mut generated = 0usize;

    for token_result in generator {
        if t_start.elapsed().as_secs_f32() >= MAX_TRANSLATION_SECS {
            break;
        }
        let token = token_result.map_err(|e| anyhow!("Generation error: {e}"))?;
        let token_id = token.item::<u32>();
        if eos_tokens.contains(&token_id) {
            break;
        }
        token_buf.push(token);
        generated += 1;

        if token_buf.len() >= STREAM_BATCH {
            let _ = eval(&token_buf);
            let ids: Vec<u32> = token_buf.drain(..).map(|t| t.item::<u32>()).collect();
            if let Ok(text) = tokenizer.decode(&ids, true) {
                if !text.is_empty() {
                    full.push_str(&text);
                }
            }
        }

        if generated >= max_tokens {
            break;
        }
    }

    if !token_buf.is_empty() {
        let _ = eval(&token_buf);
        let ids: Vec<u32> = token_buf.drain(..).map(|t| t.item::<u32>()).collect();
        if let Ok(text) = tokenizer.decode(&ids, true) {
            if !text.is_empty() {
                full.push_str(&text);
            }
        }
    }

    unsafe { mlx_sys::mlx_clear_cache(); }
    Ok(full.trim().to_string())
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

#[derive(Debug)]
struct Args {
    mode: CommitPromptMode,
    tgt_lang: String,
    max_tokens: usize,
    temperature: f32,
    model_path: Option<PathBuf>,
    input: String,
}

fn parse_args() -> Result<Args> {
    let mut mode = CommitPromptMode::Normal;
    let mut tgt_lang = "en".to_string();
    let mut max_tokens = 256usize;
    let mut temperature = 0.0f32;
    let mut model_path: Option<PathBuf> = None;
    let mut input: Option<String> = None;

    let mut args = std::env::args().skip(1).peekable();
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--mode" => {
                let v = args.next().ok_or_else(|| anyhow!("missing value for --mode"))?;
                mode = match v.as_str() {
                    "normal" => CommitPromptMode::Normal,
                    "final-drain" => CommitPromptMode::FinalDrain,
                    _ => return Err(anyhow!("unsupported mode: {v}")),
                };
            }
            "--tgt-lang" => {
                tgt_lang = args.next().ok_or_else(|| anyhow!("missing value for --tgt-lang"))?;
            }
            "--max-tokens" => {
                max_tokens = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --max-tokens"))?
                    .parse()?;
            }
            "--temperature" => {
                temperature = args
                    .next()
                    .ok_or_else(|| anyhow!("missing value for --temperature"))?
                    .parse()?;
            }
            "--model-path" => {
                model_path = Some(PathBuf::from(
                    args.next().ok_or_else(|| anyhow!("missing value for --model-path"))?,
                ));
            }
            "--file" => {
                let path = args.next().ok_or_else(|| anyhow!("missing value for --file"))?;
                input = Some(std::fs::read_to_string(path)?.trim().to_string());
            }
            _ => {
                if input.is_none() {
                    input = Some(arg);
                } else {
                    let mut joined = input.take().unwrap();
                    joined.push(' ');
                    joined.push_str(&arg);
                    input = Some(joined);
                }
            }
        }
    }

    let input = match input {
        Some(v) if !v.trim().is_empty() => v,
        _ => {
            use std::io::Read;
            let mut buf = String::new();
            std::io::stdin().read_to_string(&mut buf)?;
            let v = buf.trim().to_string();
            if v.is_empty() {
                return Err(anyhow!(
                    "missing input text\nusage: translator-prompt-harness [--mode normal|final-drain] [--tgt-lang en] [--file path] <text>"
                ));
            }
            v
        }
    };

    Ok(Args {
        mode,
        tgt_lang,
        max_tokens,
        temperature,
        model_path,
        input,
    })
}

fn main() -> Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(
            tracing_subscriber::EnvFilter::from_env("LOG_LEVEL")
                .add_directive(tracing::Level::INFO.into()),
        )
        .init();

    let args = parse_args()?;
    let model_path = args.model_path.unwrap_or_else(resolve_model_path);
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
    let mut model = load_model(&model_path).map_err(|e| anyhow!("Failed to load model: {e}"))?;
    let eos_tokens = load_eos_tokens(&model_path)?;
    let model_id = model_path
        .file_name()
        .and_then(|s| s.to_str())
        .unwrap_or("qwen3.5")
        .to_string();
    let force_disable_thinking = model_id.to_lowercase().contains("qwen3");

    let system_prompt = build_system_prompt(&args.tgt_lang, args.mode);
    let user_prompt = build_analysis_user_prompt(&args.input);
    let output = generate_text_completion(
        &mut tokenizer,
        &mut model,
        &chat_template,
        &model_id,
        &system_prompt,
        &user_prompt,
        force_disable_thinking,
        args.temperature,
        args.max_tokens,
        &eos_tokens,
    )?;

    println!("=== RAW_TAIL ===\n{}\n", args.input);
    println!("=== SYSTEM_PROMPT ===\n{}\n", system_prompt);
    println!("=== USER_PROMPT ===\n{}\n", user_prompt);
    println!("=== MODEL_OUTPUT_RAW ===\n{}\n", output);

    match extract_json_object(&output)
        .and_then(|json| serde_json::from_str::<StructuredTranslation>(json).map_err(|e| anyhow!(e)))
    {
        Ok(parsed) => {
            println!("=== PARSED_JSON ===\n{:#?}\n", parsed);
            match validate_structured_translation(&args.input, &parsed) {
                Ok(()) => println!("=== VALIDATION ===\npass\n"),
                Err(e) => println!("=== VALIDATION ===\nfail: {}\n", e),
            }
        }
        Err(e) => {
            println!("=== PARSE_ERROR ===\n{}\n", e);
        }
    }

    Ok(())
}
