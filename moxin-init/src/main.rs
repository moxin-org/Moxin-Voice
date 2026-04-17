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
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

// ── State file ────────────────────────────────────────────────────────────────
//
// Format consumed by screen.rs poll_runtime_initialization:
//   "{current}/{total}|{title}|{detail}|{pct}\n"
// where pct is overall download progress as a float 0.0000–1.0000.

// Actual download sizes in bytes (measured 2026-04-17, `du -sk` × 1024)
const BYTES_TTS_CUSTOM: u64 = 5_473_562_624;  // Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit
const BYTES_TTS_BASE: u64   = 3_104_284_672;  // Qwen3-TTS-12Hz-1.7B-Base-8bit
const BYTES_TRANSLATOR: u64 = 1_749_164_032;  // Qwen3.5-2B-MLX-4bit
const BYTES_ASR: u64        = 2_473_308_160;  // Qwen3-ASR-1.7B-8bit
const TOTAL_BYTES: u64      = BYTES_TTS_CUSTOM + BYTES_TTS_BASE + BYTES_TRANSLATOR + BYTES_ASR;
const MODEL_COMPLETION_MARKER: &str = ".moxin-model-complete.json";
const BOOTSTRAP_VERSION: u32 = 1;

fn write_state(
    state_file: Option<&Path>,
    current: usize,
    total: usize,
    title: &str,
    detail: &str,
    bytes_done: u64,
    total_bytes: u64,
) {
    let pct = if total_bytes > 0 {
        (bytes_done as f64 / total_bytes as f64).min(0.99)
    } else {
        0.0
    };
    eprintln!("[moxin-init] {}/{} {} — {} ({:.1}%)", current, total, title, detail, pct * 100.0);
    let Some(path) = state_file else { return };
    if let Some(parent) = path.parent() {
        let _ = fs::create_dir_all(parent);
    }
    let _ = fs::write(path, format!("{}/{}|{}|{}|{:.4}\n", current, total, title, detail, pct));
}

fn file_exists(path: &Path) -> bool {
    path.metadata().map(|m| m.is_file()).unwrap_or(false)
}

fn format_bytes_per_second(bytes_per_sec: f64) -> String {
    if bytes_per_sec >= 1024.0 * 1024.0 * 1024.0 {
        format!("{:.1} GB/s", bytes_per_sec / (1024.0 * 1024.0 * 1024.0))
    } else if bytes_per_sec >= 1024.0 * 1024.0 {
        format!("{:.1} MB/s", bytes_per_sec / (1024.0 * 1024.0))
    } else if bytes_per_sec >= 1024.0 {
        format!("{:.1} KB/s", bytes_per_sec / 1024.0)
    } else {
        format!("{:.0} B/s", bytes_per_sec)
    }
}

#[derive(Serialize, Deserialize)]
struct ModelCompletionMarker {
    repo_id: String,
    bootstrap_version: u32,
}

fn model_completion_marker_path(dir: &Path) -> PathBuf {
    dir.join(MODEL_COMPLETION_MARKER)
}

fn model_completion_marker_valid(dir: &Path, repo_id: &str) -> bool {
    let marker_path = model_completion_marker_path(dir);
    let Ok(contents) = fs::read_to_string(marker_path) else {
        return false;
    };
    let Ok(marker) = serde_json::from_str::<ModelCompletionMarker>(&contents) else {
        return false;
    };
    marker.repo_id == repo_id
}

fn write_model_completion_marker(dir: &Path, repo_id: &str) -> Result<()> {
    fs::create_dir_all(dir).with_context(|| format!("mkdir {:?}", dir))?;
    let marker = ModelCompletionMarker {
        repo_id: repo_id.to_string(),
        bootstrap_version: BOOTSTRAP_VERSION,
    };
    let marker_path = model_completion_marker_path(dir);
    let body = serde_json::to_string_pretty(&marker)?;
    fs::write(&marker_path, body)
        .with_context(|| format!("write model completion marker {:?}", marker_path))
}

fn ensure_model_dir_ready(
    dir: &Path,
    repo_id: &str,
    ready_check: impl Fn(&Path) -> bool,
) -> Result<bool> {
    if ready_check(dir) {
        if !model_completion_marker_valid(dir, repo_id) {
            eprintln!(
                "[moxin-init] complete model found without a valid marker, writing {}",
                dir.display()
            );
            write_model_completion_marker(dir, repo_id)?;
        }
        return Ok(true);
    }

    if model_completion_marker_valid(dir, repo_id) {
        eprintln!(
            "[moxin-init] marker present but model is incomplete, clearing {}",
            dir.display()
        );
        if dir.exists() {
            fs::remove_dir_all(dir).with_context(|| format!("remove incomplete model dir {}", dir.display()))?;
        }
        return Ok(false);
    }

    if dir.exists() {
        eprintln!(
            "[moxin-init] model directory without a valid completion marker, removing {}",
            dir.display()
        );
        fs::remove_dir_all(dir).with_context(|| format!("remove incomplete model dir {}", dir.display()))?;
    }
    Ok(false)
}

// ── Model readiness checks ────────────────────────────────────────────────────

fn tts_model_ready(dir: &Path) -> bool {
    file_exists(&dir.join("config.json"))
        && file_exists(&dir.join("generation_config.json"))
        && file_exists(&dir.join("vocab.json"))
        && file_exists(&dir.join("merges.txt"))
        && (file_exists(&dir.join("model.safetensors"))
            || file_exists(&dir.join("model.safetensors.index.json")))
        && file_exists(&dir.join("speech_tokenizer/config.json"))
        && file_exists(&dir.join("speech_tokenizer/model.safetensors"))
}

fn asr_model_ready(dir: &Path) -> bool {
    file_exists(&dir.join("config.json"))
}

fn qwen35_translation_model_ready(dir: &Path) -> bool {
    file_exists(&dir.join("config.json"))
        && file_exists(&dir.join("tokenizer.json"))
        && file_exists(&dir.join("tokenizer_config.json"))
        && (file_exists(&dir.join("model.safetensors"))
            || file_exists(&dir.join("model.safetensors.index.json")))
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
/// Returns the number of bytes actually written in this call.
fn download_file(
    client: &reqwest::blocking::Client,
    repo_id: &str,
    filename: &str,
    dest: &Path,
    mut on_progress: impl FnMut(u64, f64),
) -> Result<u64> {
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
        return Ok(0);
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

    let mut resp = resp;
    let mut downloaded: u64 = 0;
    let start = Instant::now();
    let mut last_report = Instant::now();
    let mut buf = [0_u8; 256 * 1024];

    loop {
        let n = resp
            .read(&mut buf)
            .with_context(|| format!("read body of {}/{}", repo_id, filename))?;
        if n == 0 {
            break;
        }
        file.write_all(&buf[..n])
            .with_context(|| format!("write {:?}", dest))?;
        downloaded += n as u64;

        let should_report = last_report.elapsed() >= Duration::from_millis(250);
        if should_report {
            let elapsed = start.elapsed().as_secs_f64().max(0.001);
            let speed = downloaded as f64 / elapsed;
            on_progress(downloaded, speed);
            last_report = Instant::now();
        }
    }

    let elapsed = start.elapsed().as_secs_f64().max(0.001);
    let speed = downloaded as f64 / elapsed;
    on_progress(downloaded, speed);
    Ok(downloaded)
}

/// Download all files in a HuggingFace repo to `target_dir`.
///
/// Already-present files are skipped. The `state_file` is updated per-file
/// so the UI progress bar reflects real download activity.
/// `bytes_done` is updated after each file; `total_bytes` is used for pct.
fn download_repo(
    client: &reqwest::blocking::Client,
    repo_id: &str,
    target_dir: &Path,
    state_file: Option<&Path>,
    step: usize,
    total_steps: usize,
    bytes_done: &mut u64,
    total_bytes: u64,
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
            *bytes_done,
            total_bytes,
        );
        let written = download_file(client, repo_id, filename, &dest, |written_so_far, speed_bps| {
            write_state(
                state_file,
                step,
                total_steps,
                &format!("Downloading {}", short_name),
                &format!(
                    "[{}/{}] {} • {}",
                    i + 1,
                    files.len(),
                    filename,
                    format_bytes_per_second(speed_bps),
                ),
                *bytes_done + written_so_far,
                total_bytes,
            );
        })
            .with_context(|| format!("download {}/{}", repo_id, filename))?;
        *bytes_done += written;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::time::{SystemTime, UNIX_EPOCH};

    fn unique_temp_dir(name: &str) -> PathBuf {
        let nanos = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        let dir = std::env::temp_dir().join(format!("moxin-init-{name}-{nanos}"));
        fs::create_dir_all(&dir).unwrap();
        dir
    }

    #[test]
    fn ready_check_requires_expected_translator_files() {
        let dir = unique_temp_dir("translator-ready");
        fs::write(dir.join("config.json"), b"{}").unwrap();
        fs::write(dir.join("tokenizer.json"), b"{}").unwrap();
        fs::write(dir.join("tokenizer_config.json"), b"{}").unwrap();
        File::create(dir.join("model.safetensors")).unwrap();

        assert!(qwen35_translation_model_ready(&dir));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn formats_download_speed_human_readably() {
        assert_eq!(format_bytes_per_second(512.0), "512 B/s");
        assert_eq!(format_bytes_per_second(2048.0), "2.0 KB/s");
        assert_eq!(format_bytes_per_second(5.5 * 1024.0 * 1024.0), "5.5 MB/s");
    }

    #[test]
    fn ensure_model_dir_ready_migrates_complete_unmarked_model_dir() {
        let dir = unique_temp_dir("unmarked-migrate");
        fs::write(dir.join("config.json"), b"{}").unwrap();
        fs::write(dir.join("tokenizer.json"), b"{}").unwrap();
        fs::write(dir.join("tokenizer_config.json"), b"{}").unwrap();
        fs::write(dir.join("model.safetensors"), b"weights").unwrap();

        let ready = ensure_model_dir_ready(
            &dir,
            "mlx-community/Qwen3.5-2B-MLX-4bit",
            qwen35_translation_model_ready,
        )
        .unwrap();

        assert!(ready);
        assert!(model_completion_marker_valid(
            &dir,
            "mlx-community/Qwen3.5-2B-MLX-4bit"
        ));

        fs::remove_dir_all(&dir).unwrap();
    }

    #[test]
    fn ensure_model_dir_ready_removes_incomplete_model_dir() {
        let dir = unique_temp_dir("incomplete-remove");
        fs::write(dir.join("config.json"), b"{}").unwrap();
        File::create(dir.join("model.safetensors")).unwrap();

        let ready = ensure_model_dir_ready(
            &dir,
            "mlx-community/Qwen3.5-2B-MLX-4bit",
            qwen35_translation_model_ready,
        )
        .unwrap();

        assert!(!ready);
        assert!(!dir.exists());
    }
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

    // 4 potential downloads: CustomVoice TTS, Base TTS, Qwen3.5 translator, ASR
    let total: usize = 4;
    let custom_ready = ensure_model_dir_ready(&cfg.tts_custom_dir, &cfg.tts_custom_repo, tts_model_ready)?;
    let base_ready = ensure_model_dir_ready(&cfg.tts_base_dir, &cfg.tts_base_repo, tts_model_ready)?;
    let translator_ready = ensure_model_dir_ready(
        &cfg.qwen35_translator_dir,
        &cfg.qwen35_translator_repo,
        qwen35_translation_model_ready,
    )?;
    let asr_ready = ensure_model_dir_ready(&cfg.asr_dir, &cfg.asr_repo, asr_model_ready)?;

    let mut bytes_done: u64 = 0;
    if custom_ready {
        bytes_done += BYTES_TTS_CUSTOM;
    }
    if base_ready {
        bytes_done += BYTES_TTS_BASE;
    }
    if translator_ready {
        bytes_done += BYTES_TRANSLATOR;
    }
    if asr_ready {
        bytes_done += BYTES_ASR;
    }

    write_state(
        state_file,
        0,
        total,
        "Check Models",
        "Verifying model files",
        bytes_done,
        TOTAL_BYTES,
    );

    let client = reqwest::blocking::Client::builder()
        .timeout(Duration::from_secs(3600))
        .redirect(reqwest::redirect::Policy::limited(10))
        .build()
        .context("Build HTTP client")?;

    // ── Step 1: TTS CustomVoice ───────────────────────────────────────────────
    if custom_ready {
        eprintln!("[moxin-init] TTS CustomVoice already ready, skipping");
        write_state(state_file, 1, total, "TTS CustomVoice", "Already present", bytes_done, TOTAL_BYTES);
    } else {
        write_state(state_file, 1, total, "Downloading TTS CustomVoice", "Starting...", bytes_done, TOTAL_BYTES);
        download_repo(
            &client,
            &cfg.tts_custom_repo,
            &cfg.tts_custom_dir,
            state_file,
            1,
            total,
            &mut bytes_done,
            TOTAL_BYTES,
        )?;
        if !tts_model_ready(&cfg.tts_custom_dir) {
            bail!(
                "TTS CustomVoice model incomplete after download: {}",
                cfg.tts_custom_dir.display()
            );
        }
        write_model_completion_marker(&cfg.tts_custom_dir, &cfg.tts_custom_repo)?;
        eprintln!("[moxin-init] TTS CustomVoice download complete");
    }

    // ── Step 2: TTS Base ──────────────────────────────────────────────────────
    if base_ready {
        eprintln!("[moxin-init] TTS Base already ready, skipping");
        write_state(state_file, 2, total, "TTS Base", "Already present", bytes_done, TOTAL_BYTES);
    } else {
        write_state(state_file, 2, total, "Downloading TTS Base", "Starting...", bytes_done, TOTAL_BYTES);
        download_repo(
            &client,
            &cfg.tts_base_repo,
            &cfg.tts_base_dir,
            state_file,
            2,
            total,
            &mut bytes_done,
            TOTAL_BYTES,
        )?;
        if !tts_model_ready(&cfg.tts_base_dir) {
            bail!(
                "TTS Base model incomplete after download: {}",
                cfg.tts_base_dir.display()
            );
        }
        write_model_completion_marker(&cfg.tts_base_dir, &cfg.tts_base_repo)?;
        eprintln!("[moxin-init] TTS Base download complete");
    }

    // ── Step 3: Qwen3.5 translator (required) ─────────────────────────────────
    if translator_ready {
        eprintln!("[moxin-init] Qwen3.5 translator model already ready, skipping");
        write_state(state_file, 3, total, "Qwen3.5 Translator", "Already present", bytes_done, TOTAL_BYTES);
    } else {
        write_state(state_file, 3, total, "Downloading Qwen3.5 Translator", "Starting...", bytes_done, TOTAL_BYTES);
        download_repo(
            &client,
            &cfg.qwen35_translator_repo,
            &cfg.qwen35_translator_dir,
            state_file,
            3,
            total,
            &mut bytes_done,
            TOTAL_BYTES,
        )
        .with_context(|| "Qwen3.5 translator download failed")?;
        if !qwen35_translation_model_ready(&cfg.qwen35_translator_dir) {
            bail!(
                "Qwen3.5 translator model incomplete after download: {}",
                cfg.qwen35_translator_dir.display()
            );
        }
        write_model_completion_marker(&cfg.qwen35_translator_dir, &cfg.qwen35_translator_repo)?;
        eprintln!("[moxin-init] Qwen3.5 translator download complete");
    }

    // ── Step 4: ASR (required) ─────────────────────────────────────────────────
    if asr_ready {
        eprintln!("[moxin-init] ASR model already ready, skipping");
        write_state(state_file, 4, total, "ASR Model", "Already present", bytes_done, TOTAL_BYTES);
    } else {
        write_state(state_file, 4, total, "Downloading ASR Model", "Starting...", bytes_done, TOTAL_BYTES);
        download_repo(
            &client,
            &cfg.asr_repo,
            &cfg.asr_dir,
            state_file,
            4,
            total,
            &mut bytes_done,
            TOTAL_BYTES,
        )
        .with_context(|| "ASR model download failed")?;
        if !asr_model_ready(&cfg.asr_dir) {
            bail!(
                "ASR model incomplete after download: {}",
                cfg.asr_dir.display()
            );
        }
        write_model_completion_marker(&cfg.asr_dir, &cfg.asr_repo)?;
        eprintln!("[moxin-init] ASR download complete");
    }

    write_state(state_file, total, total, "Done", "All models ready", TOTAL_BYTES, TOTAL_BYTES);
    println!("[moxin-init] initialization complete");
    Ok(())
}
