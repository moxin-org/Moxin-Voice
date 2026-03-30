//! # moxin-init
//!
//! First-run model downloader for Moxin Voice.
//! Replaces the conda/Python bootstrap: downloads Qwen3 TTS and ASR models
//! directly from HuggingFace via HTTP, with resume support.
//!
//! ## Configuration (environment variables)
//!
//! All variables are optional and have sensible defaults:
//!
//! | Variable                          | Default                                              |
//! |-----------------------------------|------------------------------------------------------|
//! | `MOXIN_BOOTSTRAP_STATE_PATH`      | (no state file written)                              |
//! | `QWEN3_TTS_MODEL_ROOT`            | `~/.OminiX/models/qwen3-tts-mlx`                    |
//! | `QWEN3_TTS_CUSTOMVOICE_MODEL_DIR` | `$QWEN3_TTS_MODEL_ROOT/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit` |
//! | `QWEN3_TTS_CUSTOMVOICE_REPO`      | `mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit`|
//! | `QWEN3_TTS_BASE_MODEL_DIR`        | `$QWEN3_TTS_MODEL_ROOT/Qwen3-TTS-12Hz-1.7B-Base-8bit`       |
//! | `QWEN3_TTS_BASE_REPO`             | `mlx-community/Qwen3-TTS-12Hz-1.7B-Base-8bit`       |
//! | `QWEN3_ASR_MODEL_PATH`            | `~/.OminiX/models/qwen3-asr-1.7b`                    |
//! | `QWEN3_ASR_REPO`                  | `mlx-community/Qwen3-ASR-1.7B-8bit`                 |
//! | `QWEN35_TRANSLATOR_MODEL_PATH`    | `~/.OminiX/models/Qwen3.5-2B-MLX-4bit`              |
//! | `QWEN35_TRANSLATOR_REPO`          | `mlx-community/Qwen3.5-2B-MLX-4bit`                 |
//! | `HF_ENDPOINT`                     | `https://huggingface.co` (set for mirror support)   |

use std::env;
use std::fs::{self, File, OpenOptions};
use std::io::Write;
use std::path::{Path, PathBuf};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Deserialize;

// ── State file ────────────────────────────────────────────────────────────────
//
// Format consumed by screen.rs poll_runtime_initialization:
//   "{current}/{total}|{title}|{detail}\n"

fn write_state(state_file: Option<&Path>, current: usize, total: usize, title: &str, detail: &str) {
    eprintln!("[moxin-init] {}/{} {} — {}", current, total, title, detail);
    let Some(path) = state_file else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, format!("{}/{}|{}|{}\n", current, total, title, detail));
}

// ── Model readiness checks ────────────────────────────────────────────────────

fn tts_model_ready(dir: &Path) -> bool {
    dir.join("config.json").exists()
        && dir.join("generation_config.json").exists()
        && dir.join("vocab.json").exists()
        && dir.join("merges.txt").exists()
        && (dir.join("model.safetensors").exists()
            || dir.join("model.safetensors.index.json").exists())
        && dir.join("speech_tokenizer/config.json").exists()
        && dir.join("speech_tokenizer/model.safetensors").exists()
}

fn asr_model_ready(dir: &Path) -> bool {
    dir.join("config.json").exists()
}

fn qwen35_translation_model_ready(dir: &Path) -> bool {
    dir.join("config.json").exists()
        && dir.join("tokenizer.json").exists()
        && dir.join("tokenizer_config.json").exists()
        && (dir.join("model.safetensors").exists()
            || dir.join("model.safetensors.index.json").exists())
}

// ── HuggingFace HTTP helpers ──────────────────────────────────────────────────

#[derive(Deserialize)]
struct Sibling {
    rfilename: String,
}

#[derive(Deserialize)]
struct RepoInfo {
    siblings: Vec<Sibling>,
}

fn hf_base() -> String {
    match env::var("HF_ENDPOINT") {
        Ok(v) if !v.trim().is_empty() => v,
        _ => "https://huggingface.co".to_string(),
    }
}

/// Fetch the list of files in a HuggingFace model repo.
fn list_repo_files(client: &reqwest::blocking::Client, repo_id: &str) -> Result<Vec<String>> {
    let url = format!("{}/api/models/{}", hf_base(), repo_id);
    let info: RepoInfo = client
        .get(&url)
        .send()
        .with_context(|| format!("GET {}", url))?
        .error_for_status()
        .with_context(|| format!("HTTP error listing {}", repo_id))?
        .json()
        .context("Parse repo info JSON")?;
    Ok(info.siblings.into_iter().map(|s| s.rfilename).collect())
}

/// Download a single file from a HuggingFace repo to `dest`.
///
/// Uses `Range` requests for resume: if `dest` already exists and is non-empty,
/// only the remaining bytes are fetched and appended.
fn download_file(
    client: &reqwest::blocking::Client,
    repo_id: &str,
    filename: &str,
    dest: &Path,
) -> Result<()> {
    if let Some(parent) = dest.parent() {
        fs::create_dir_all(parent).with_context(|| format!("mkdir {:?}", dest.parent()))?;
    }

    let existing_bytes = dest.metadata().map(|m| m.len()).unwrap_or(0);
    let url = format!("{}/{}/resolve/main/{}", hf_base(), repo_id, filename);

    let mut req = client.get(&url);
    if existing_bytes > 0 {
        req = req.header("Range", format!("bytes={}-", existing_bytes));
    }

    let resp = req.send().with_context(|| format!("GET {}", url))?;
    let status = resp.status();

    // 416 Range Not Satisfiable = file is already complete
    if status.as_u16() == 416 {
        return Ok(());
    }

    if !status.is_success() {
        bail!("HTTP {} downloading {}/{}", status, repo_id, filename);
    }

    let is_partial = status.as_u16() == 206;
    let mut file = if is_partial {
        OpenOptions::new()
            .append(true)
            .open(dest)
            .with_context(|| format!("open for append {:?}", dest))?
    } else {
        File::create(dest).with_context(|| format!("create {:?}", dest))?
    };

    let bytes = resp
        .bytes()
        .with_context(|| format!("read body of {}/{}", repo_id, filename))?;
    file.write_all(&bytes)
        .with_context(|| format!("write {:?}", dest))?;
    Ok(())
}

/// Download all files in a HuggingFace repo to `target_dir`.
///
/// Already-present files are skipped. The `state_file` is updated per-file
/// so the UI progress bar reflects real download activity.
fn download_repo(
    client: &reqwest::blocking::Client,
    repo_id: &str,
    target_dir: &Path,
    state_file: Option<&Path>,
    step: usize,
    total_steps: usize,
) -> Result<()> {
    fs::create_dir_all(target_dir)
        .with_context(|| format!("mkdir {:?}", target_dir))?;

    let short_name = repo_id.split('/').last().unwrap_or(repo_id);
    eprintln!("[moxin-init] listing files for {}", repo_id);

    let files = list_repo_files(client, repo_id)
        .with_context(|| format!("list files for {}", repo_id))?;

    eprintln!("[moxin-init] {} file(s) in {}", files.len(), repo_id);

    for (i, filename) in files.iter().enumerate() {
        let dest = target_dir.join(filename);
        if dest.exists() && dest.metadata().map(|m| m.len()).unwrap_or(0) > 0 {
            eprintln!("[moxin-init] skip (exists): {}", filename);
            continue;
        }
        eprintln!("[moxin-init] downloading [{}/{}]: {}", i + 1, files.len(), filename);
        write_state(
            state_file,
            step,
            total_steps,
            &format!("Downloading {}", short_name),
            &format!("[{}/{}] {}", i + 1, files.len(), filename),
        );
        download_file(client, repo_id, filename, &dest)
            .with_context(|| format!("download {}/{}", repo_id, filename))?;
    }
    Ok(())
}

// ── Configuration ─────────────────────────────────────────────────────────────

struct Config {
    state_file: Option<PathBuf>,
    tts_custom_dir: PathBuf,
    tts_custom_repo: String,
    tts_base_dir: PathBuf,
    tts_base_repo: String,
    asr_dir: PathBuf,
    asr_repo: String,
    qwen35_translator_dir: PathBuf,
    qwen35_translator_repo: String,
}

fn resolve_config() -> Config {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    let qwen_root = env::var("QWEN3_TTS_MODEL_ROOT")
        .map(PathBuf::from)
        .unwrap_or_else(|_| home.join(".OminiX/models/qwen3-tts-mlx"));

    Config {
        state_file: env::var("MOXIN_BOOTSTRAP_STATE_PATH").ok().map(PathBuf::from),
        tts_custom_dir: env::var("QWEN3_TTS_CUSTOMVOICE_MODEL_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| qwen_root.join("Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit")),
        tts_custom_repo: env::var("QWEN3_TTS_CUSTOMVOICE_REPO")
            .unwrap_or_else(|_| "mlx-community/Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit".to_string()),
        tts_base_dir: env::var("QWEN3_TTS_BASE_MODEL_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| qwen_root.join("Qwen3-TTS-12Hz-1.7B-Base-8bit")),
        tts_base_repo: env::var("QWEN3_TTS_BASE_REPO")
            .unwrap_or_else(|_| "mlx-community/Qwen3-TTS-12Hz-1.7B-Base-8bit".to_string()),
        asr_dir: env::var("QWEN3_ASR_MODEL_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".OminiX/models/qwen3-asr-1.7b")),
        asr_repo: env::var("QWEN3_ASR_REPO")
            .unwrap_or_else(|_| "mlx-community/Qwen3-ASR-1.7B-8bit".to_string()),
        qwen35_translator_dir: env::var("QWEN35_TRANSLATOR_MODEL_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| home.join(".OminiX/models/Qwen3.5-2B-MLX-4bit")),
        qwen35_translator_repo: env::var("QWEN35_TRANSLATOR_REPO")
            .unwrap_or_else(|_| "mlx-community/Qwen3.5-2B-MLX-4bit".to_string()),
    }
}

// ── Entry point ───────────────────────────────────────────────────────────────

fn main() -> Result<()> {
    let cfg = resolve_config();
    let state_file = cfg.state_file.as_deref();

    // 4 potential downloads: CustomVoice TTS, Base TTS, Qwen3.5 translator, ASR (optional)
    let total: usize = 4;

    write_state(state_file, 0, total, "Check Models", "Verifying model files");

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3600))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .context("Build HTTP client")?;

    // ── Step 1: TTS CustomVoice ───────────────────────────────────────────────
    if tts_model_ready(&cfg.tts_custom_dir) {
        eprintln!("[moxin-init] TTS CustomVoice already ready, skipping");
        write_state(state_file, 1, total, "TTS CustomVoice", "Already present");
    } else {
        write_state(state_file, 1, total, "Downloading TTS CustomVoice", "Starting...");
        download_repo(
            &client,
            &cfg.tts_custom_repo,
            &cfg.tts_custom_dir,
            state_file,
            1,
            total,
        )?;
        if !tts_model_ready(&cfg.tts_custom_dir) {
            bail!(
                "TTS CustomVoice model incomplete after download: {}",
                cfg.tts_custom_dir.display()
            );
        }
        eprintln!("[moxin-init] TTS CustomVoice download complete");
    }

    // ── Step 2: TTS Base ──────────────────────────────────────────────────────
    if tts_model_ready(&cfg.tts_base_dir) {
        eprintln!("[moxin-init] TTS Base already ready, skipping");
        write_state(state_file, 2, total, "TTS Base", "Already present");
    } else {
        write_state(state_file, 2, total, "Downloading TTS Base", "Starting...");
        download_repo(
            &client,
            &cfg.tts_base_repo,
            &cfg.tts_base_dir,
            state_file,
            2,
            total,
        )?;
        if !tts_model_ready(&cfg.tts_base_dir) {
            bail!(
                "TTS Base model incomplete after download: {}",
                cfg.tts_base_dir.display()
            );
        }
        eprintln!("[moxin-init] TTS Base download complete");
    }

    // ── Step 3: Qwen3.5 translator (required) ─────────────────────────────────
    if qwen35_translation_model_ready(&cfg.qwen35_translator_dir) {
        eprintln!("[moxin-init] Qwen3.5 translator model already ready, skipping");
        write_state(state_file, 3, total, "Qwen3.5 Translator", "Already present");
    } else {
        write_state(
            state_file,
            3,
            total,
            "Downloading Qwen3.5 Translator",
            "Starting...",
        );
        download_repo(
            &client,
            &cfg.qwen35_translator_repo,
            &cfg.qwen35_translator_dir,
            state_file,
            3,
            total,
        )
        .with_context(|| "Qwen3.5 translator download failed")?;
        if !qwen35_translation_model_ready(&cfg.qwen35_translator_dir) {
            bail!(
                "Qwen3.5 translator model incomplete after download: {}",
                cfg.qwen35_translator_dir.display()
            );
        }
        eprintln!("[moxin-init] Qwen3.5 translator download complete");
    }

    // ── Step 4: ASR (required) ─────────────────────────────────────────────────
    if asr_model_ready(&cfg.asr_dir) {
        eprintln!("[moxin-init] ASR model already ready, skipping");
        write_state(state_file, 4, total, "ASR Model", "Already present");
    } else {
        write_state(state_file, 4, total, "Downloading ASR Model", "Starting...");
        download_repo(&client, &cfg.asr_repo, &cfg.asr_dir, state_file, 4, total)
            .with_context(|| "ASR model download failed")?;
        if !asr_model_ready(&cfg.asr_dir) {
            bail!(
                "ASR model incomplete after download: {}",
                cfg.asr_dir.display()
            );
        }
        eprintln!("[moxin-init] ASR download complete");
    }

    write_state(state_file, total, total, "Done", "All models ready");
    println!("[moxin-init] initialization complete");
    Ok(())
}
