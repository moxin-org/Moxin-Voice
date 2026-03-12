use anyhow::{anyhow, Context, Result};
use gpt_sovits_mlx::audio::{load_audio_for_hubert, load_wav, resample, save_wav};
use gpt_sovits_mlx::inference::preprocess_text_with_lang;
use gpt_sovits_mlx::models::hubert::load_hubert_model;
use gpt_sovits_mlx::text::Language;
use gpt_sovits_mlx::training::{VITSDataset, VITSTrainer, VITSTrainingConfig};
use mlx_rs::module::{Module, ModuleParameters};
use mlx_rs::transforms::eval;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::fs;
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};
use std::time::{SystemTime, UNIX_EPOCH};

#[derive(Debug, Deserialize)]
struct TrainingRequest {
    voice_id: String,
    #[allow(dead_code)]
    voice_name: String,
    audio_file: String,
    language: String,
    workspace_dir: String,
    reference_text: String,
    training_params: TrainingParams,
}

#[derive(Debug, Deserialize)]
struct TrainingParams {
    #[allow(dead_code)]
    gpt_epochs: u32,
    sovits_epochs: u32,
    #[allow(dead_code)]
    batch_size: u32,
}

#[derive(Debug, Serialize)]
struct Event {
    #[serde(rename = "type")]
    event_type: String,
    message: String,
    timestamp: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    data: Option<Value>,
}

#[derive(Debug, Serialize)]
struct VitsSampleMeta {
    id: String,
    ssl_len: usize,
    audio_len: usize,
    phoneme_len: usize,
}

#[derive(Debug, Serialize)]
struct VitsDatasetMeta {
    num_samples: usize,
    sample_rate: u32,
    ssl_dim: usize,
    samples: Vec<VitsSampleMeta>,
}

fn emit(event_type: &str, message: impl Into<String>, data: Option<Value>) {
    let ts = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64())
        .unwrap_or(0.0);
    let event = Event {
        event_type: event_type.to_string(),
        message: message.into(),
        timestamp: ts,
        data,
    };
    if let Ok(line) = serde_json::to_string(&event) {
        println!("{line}");
        let _ = std::io::stdout().flush();
    }
}

fn map_language(lang: &str) -> Language {
    match lang {
        "en" => Language::English,
        "mixed" | "auto" => Language::Mixed,
        _ => Language::Chinese,
    }
}

fn model_dir() -> Result<PathBuf> {
    if let Ok(dir) = std::env::var("GPT_SOVITS_MODEL_DIR") {
        return Ok(PathBuf::from(dir));
    }
    let home = dirs::home_dir().ok_or_else(|| anyhow!("Cannot determine home directory"))?;
    Ok(home.join(".OminiX/models/gpt-sovits-mlx"))
}

fn find_base_gpt(model_dir: &Path) -> Result<PathBuf> {
    let doubao = model_dir
        .join("voices")
        .join("Doubao")
        .join("gpt.safetensors");
    if doubao.exists() {
        return Ok(doubao);
    }

    let voices_dir = model_dir.join("voices");
    let entries = fs::read_dir(&voices_dir)
        .with_context(|| format!("Failed to read voices dir: {}", voices_dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path().join("gpt.safetensors");
        if path.exists() {
            return Ok(path);
        }
    }
    Err(anyhow!(
        "No base GPT weights found under {}",
        voices_dir.display()
    ))
}

fn find_base_sovits(model_dir: &Path) -> Result<PathBuf> {
    let doubao = model_dir
        .join("voices")
        .join("Doubao")
        .join("sovits.safetensors");
    if doubao.exists() {
        return Ok(doubao);
    }

    let voices_dir = model_dir.join("voices");
    let entries = fs::read_dir(&voices_dir)
        .with_context(|| format!("Failed to read voices dir: {}", voices_dir.display()))?;
    for entry in entries.flatten() {
        let path = entry.path().join("sovits.safetensors");
        if path.exists() {
            return Ok(path);
        }
    }
    Err(anyhow!(
        "No base SoVITS weights found under {}",
        voices_dir.display()
    ))
}

fn write_npy_i32_1d(path: &Path, values: &[i32]) -> Result<()> {
    write_npy(path, "<i4", &[values.len()], |out| {
        for v in values {
            out.extend_from_slice(&v.to_le_bytes());
        }
    })
}

fn write_npy_f32_1d(path: &Path, values: &[f32]) -> Result<()> {
    write_npy(path, "<f4", &[values.len()], |out| {
        for v in values {
            out.extend_from_slice(&v.to_le_bytes());
        }
    })
}

fn write_npy_f32_2d(path: &Path, rows: usize, cols: usize, values: &[f32]) -> Result<()> {
    if values.len() != rows * cols {
        return Err(anyhow!(
            "2D npy shape mismatch: {}x{} vs {} values",
            rows,
            cols,
            values.len()
        ));
    }
    write_npy(path, "<f4", &[rows, cols], |out| {
        for v in values {
            out.extend_from_slice(&v.to_le_bytes());
        }
    })
}

fn write_npy<F>(path: &Path, descr: &str, shape: &[usize], mut write_data: F) -> Result<()>
where
    F: FnMut(&mut Vec<u8>),
{
    let mut out = Vec::new();
    out.extend_from_slice(b"\x93NUMPY");
    out.push(1);
    out.push(0);

    let shape_str = if shape.len() == 1 {
        format!("({},)", shape[0])
    } else {
        let joined = shape
            .iter()
            .map(|d| d.to_string())
            .collect::<Vec<_>>()
            .join(", ");
        format!("({joined})")
    };
    let mut header = format!(
        "{{'descr': '{}', 'fortran_order': False, 'shape': {}, }}",
        descr, shape_str
    );

    let header_len_base = header.len() + 1;
    let pad = (16 - ((10 + 2 + header_len_base) % 16)) % 16;
    header.push_str(&" ".repeat(pad));
    header.push('\n');

    let hlen: u16 = header
        .len()
        .try_into()
        .map_err(|_| anyhow!("NPY header too long"))?;
    out.extend_from_slice(&hlen.to_le_bytes());
    out.extend_from_slice(header.as_bytes());

    write_data(&mut out);
    fs::write(path, out).with_context(|| format!("Failed to write {}", path.display()))?;
    Ok(())
}

fn read_request() -> Result<TrainingRequest> {
    let mut line = String::new();
    let mut reader = BufReader::new(std::io::stdin());
    let bytes = reader
        .read_line(&mut line)
        .context("Failed to read request from stdin")?;
    if bytes == 0 {
        return Err(anyhow!("No request data received"));
    }
    let req: TrainingRequest =
        serde_json::from_str(line.trim()).context("Failed to parse training request JSON")?;
    Ok(req)
}

fn prepare_reference_audio(input_audio: &Path, output_wav: &Path) -> Result<Vec<f32>> {
    let (mut samples, sr) = load_wav(input_audio)
        .map_err(|e| anyhow!("Failed to load audio {}: {}", input_audio.display(), e))?;
    if sr != 32000 {
        samples = resample(&samples, sr, 32000);
    }
    save_wav(&samples, 32000, output_wav)
        .map_err(|e| anyhow!("Failed to write reference wav: {}", e))?;
    Ok(samples)
}

fn run(req: &TrainingRequest) -> Result<()> {
    if req.reference_text.trim().is_empty() {
        return Err(anyhow!(
            "reference_text is required for Rust few-shot training"
        ));
    }

    emit(
        "STAGE",
        "Preparing workspace",
        Some(json!({ "current": 1, "total": 7 })),
    );

    let workspace = PathBuf::from(&req.workspace_dir);
    let dataset_dir = workspace.join("rust_vits_dataset");
    let dataset_ssl_dir = dataset_dir.join("ssl");
    let dataset_audio_dir = dataset_dir.join("audio");
    let dataset_phoneme_dir = dataset_dir.join("phonemes");
    let models_dir = workspace.join("models");
    fs::create_dir_all(&dataset_ssl_dir)?;
    fs::create_dir_all(&dataset_audio_dir)?;
    fs::create_dir_all(&dataset_phoneme_dir)?;
    fs::create_dir_all(&models_dir)?;

    emit(
        "STAGE",
        "Validating and normalizing audio",
        Some(json!({ "current": 2, "total": 7 })),
    );
    let input_audio = PathBuf::from(&req.audio_file);
    if !input_audio.exists() {
        return Err(anyhow!(
            "Training audio not found: {}",
            input_audio.display()
        ));
    }
    let reference_wav = workspace.join("reference_32k.wav");
    let audio_32k = prepare_reference_audio(&input_audio, &reference_wav)?;
    let duration = audio_32k.len() as f32 / 32000.0;
    if duration < 10.0 {
        return Err(anyhow!("Audio too short: {:.1}s (minimum 10s)", duration));
    }
    emit(
        "INFO",
        format!("Reference audio ready: {:.1}s", duration),
        None,
    );

    emit(
        "STAGE",
        "Extracting HuBERT features and phonemes",
        Some(json!({ "current": 3, "total": 7 })),
    );
    let model_dir = model_dir()?;
    let hubert_path = model_dir.join("encoders").join("hubert.safetensors");
    if !hubert_path.exists() {
        return Err(anyhow!("HuBERT model not found: {}", hubert_path.display()));
    }
    let mut hubert = load_hubert_model(&hubert_path)
        .map_err(|e| anyhow!("Failed to load HuBERT model: {}", e))?;
    let audio_16k = load_audio_for_hubert(&reference_wav)
        .map_err(|e| anyhow!("Failed to prepare HuBERT audio: {}", e))?;
    eval([&audio_16k]).map_err(|e| anyhow!("MLX eval audio failed: {}", e))?;

    let ssl_nlc = hubert
        .forward(&audio_16k)
        .map_err(|e| anyhow!("HuBERT forward failed: {}", e))?;
    eval([&ssl_nlc]).map_err(|e| anyhow!("MLX eval HuBERT failed: {}", e))?;
    let shape = ssl_nlc.shape().to_vec();
    if shape.len() != 3 || shape[0] != 1 || shape[2] != 768 {
        return Err(anyhow!("Unexpected HuBERT output shape: {:?}", shape));
    }
    let t = shape[1] as usize;
    let ssl_raw = ssl_nlc.as_slice();
    let mut ssl_ncl = vec![0.0f32; 768 * t];
    for frame in 0..t {
        for chan in 0..768usize {
            ssl_ncl[chan * t + frame] = ssl_raw[frame * 768 + chan];
        }
    }

    let lang = map_language(req.language.as_str());
    let (phoneme_ids_arr, _ph, _w2ph, normalized_text) =
        preprocess_text_with_lang(req.reference_text.as_str(), Some(lang));
    eval([&phoneme_ids_arr]).map_err(|e| anyhow!("MLX eval phonemes failed: {}", e))?;
    let phoneme_shape = phoneme_ids_arr.shape().to_vec();
    if phoneme_shape.len() != 2 || phoneme_shape[0] != 1 {
        return Err(anyhow!(
            "Unexpected phoneme tensor shape: {:?}",
            phoneme_shape
        ));
    }
    let phoneme_ids = phoneme_ids_arr.as_slice().to_vec();
    if phoneme_ids.is_empty() {
        return Err(anyhow!(
            "No phonemes generated from reference_text: {}",
            req.reference_text
        ));
    }

    emit(
        "STAGE",
        "Building training dataset",
        Some(json!({ "current": 4, "total": 7 })),
    );
    let sample_id = "sample0000";
    write_npy_f32_2d(
        &dataset_ssl_dir.join(format!("{sample_id}.npy")),
        768,
        t,
        &ssl_ncl,
    )?;
    write_npy_f32_1d(
        &dataset_audio_dir.join(format!("{sample_id}.npy")),
        &audio_32k,
    )?;
    write_npy_i32_1d(
        &dataset_phoneme_dir.join(format!("{sample_id}.npy")),
        &phoneme_ids,
    )?;

    let meta = VitsDatasetMeta {
        num_samples: 1,
        sample_rate: 32000,
        ssl_dim: 768,
        samples: vec![VitsSampleMeta {
            id: sample_id.to_string(),
            ssl_len: t,
            audio_len: audio_32k.len(),
            phoneme_len: phoneme_ids.len(),
        }],
    };
    fs::write(
        dataset_dir.join("metadata.json"),
        serde_json::to_string_pretty(&meta)?,
    )?;
    emit(
        "INFO",
        format!(
            "Dataset prepared: ssl={} frames, phonemes={}, text='{}'",
            t,
            phoneme_ids.len(),
            normalized_text
        ),
        None,
    );

    emit(
        "STAGE",
        "Training SoVITS (Rust/MLX)",
        Some(json!({ "current": 5, "total": 7 })),
    );
    let pretrained_sovits = find_base_sovits(&model_dir)?;
    let mut dataset = VITSDataset::load(&dataset_dir)?;
    let epochs = req.training_params.sovits_epochs.max(1) as usize;

    let mut trainer = VITSTrainer::new(VITSTrainingConfig {
        learning_rate_g: 1e-5,
        learning_rate_d: 1e-5,
        batch_size: 1,
        segment_size: 20480,
        log_every: 1,
        save_every: 999999,
        ..Default::default()
    })?;
    trainer.load_generator_weights_with_regularization(&pretrained_sovits)?;
    trainer.freeze_non_decoder_layers();

    let hop_length = 640;
    let steps_per_epoch = 1usize;
    for epoch in 0..epochs {
        emit(
            "PROGRESS",
            format!("SoVITS epoch {}/{}", epoch + 1, epochs),
            Some(json!({ "epoch": epoch + 1, "total_epochs": epochs })),
        );
        dataset.shuffle(Some((epoch as u64 + 1) * 17));
        for batch in dataset.iter_batches(1, 20480, hop_length) {
            let batch = batch?;
            let losses = trainer.train_step(&batch)?;
            emit(
                "INFO",
                format!(
                    "Epoch {}/{} step {}: D={:.4} G={:.4} Mel={:.4}",
                    epoch + 1,
                    epochs,
                    steps_per_epoch,
                    losses.loss_d,
                    losses.loss_gen,
                    losses.loss_mel
                ),
                None,
            );
        }
    }

    emit(
        "STAGE",
        "Saving trained weights",
        Some(json!({ "current": 6, "total": 7 })),
    );
    // save_generator() exports trainable parameters. We freeze most layers during
    // few-shot training, so unfreeze before saving to export a full loadable model.
    trainer.generator.unfreeze_parameters(true);
    let sovits_output = models_dir.join("sovits_final.safetensors");
    trainer.save_generator(&sovits_output)?;

    let gpt_base = find_base_gpt(&model_dir)?;

    emit(
        "STAGE",
        "Finalizing",
        Some(json!({ "current": 7, "total": 7 })),
    );
    emit(
        "COMPLETE",
        "Rust few-shot training completed",
        Some(json!({
            "voice_id": req.voice_id,
            "gpt_weights": gpt_base.to_string_lossy().to_string(),
            "sovits_weights": sovits_output.to_string_lossy().to_string(),
            "reference_audio": reference_wav.to_string_lossy().to_string(),
            "reference_text": req.reference_text,
        })),
    );

    Ok(())
}

fn main() {
    match read_request().and_then(|req| run(&req)) {
        Ok(_) => {}
        Err(e) => {
            emit("ERROR", e.to_string(), None);
        }
    }
}
