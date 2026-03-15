//! gen-qwen-previews: pre-generate one preview WAV per Qwen3 CustomVoice speaker.
//!
//! Output: ~/.OminiX/models/qwen3-tts-mlx/previews/<speaker_id>.wav
//!
//! Usage:
//!   cargo build -p dora-qwen3-tts-mlx --bin gen-qwen-previews --release
//!   ./target/release/gen-qwen-previews

use anyhow::{Context, Result};
use qwen3_tts_mlx::{normalize_audio, save_wav, SynthesizeOptions, Synthesizer};
use std::path::PathBuf;

fn resolve_qwen_root() -> PathBuf {
    if let Ok(v) = std::env::var("QWEN3_TTS_MODEL_ROOT") {
        return PathBuf::from(v);
    }
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".OminiX").join("models").join("qwen3-tts-mlx")
}

fn resolve_customvoice_model_dir() -> PathBuf {
    if let Ok(v) = std::env::var("QWEN3_TTS_CUSTOMVOICE_MODEL_DIR") {
        return PathBuf::from(v);
    }
    resolve_qwen_root().join("Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit")
}

struct SpeakerSpec {
    id: &'static str,
    language: &'static str,
    text: &'static str,
}

const SPEAKERS: &[SpeakerSpec] = &[
    SpeakerSpec { id: "vivian",   language: "chinese",  text: "你好，欢迎使用 Moxin 语音助手，我是薇薇安。" },
    SpeakerSpec { id: "serena",   language: "chinese",  text: "你好，欢迎使用 Moxin 语音助手，我是赛琳娜。" },
    SpeakerSpec { id: "uncle_fu", language: "chinese",  text: "你好，欢迎使用 Moxin 语音助手，我是傅叔。" },
    SpeakerSpec { id: "dylan",    language: "chinese",  text: "你好，欢迎使用 Moxin 语音助手，我是迪伦。" },
    SpeakerSpec { id: "eric",     language: "chinese",  text: "你好，欢迎使用 Moxin 语音助手，我是埃里克。" },
    SpeakerSpec { id: "ryan",     language: "english",  text: "Hello, welcome to Moxin Voice. I'm Ryan." },
    SpeakerSpec { id: "aiden",    language: "english",  text: "Hello, welcome to Moxin Voice. I'm Aiden." },
    SpeakerSpec { id: "ono_anna", language: "japanese", text: "こんにちは。Moxin ボイスへようこそ。小野安奈です。" },
    SpeakerSpec { id: "sohee",    language: "korean",   text: "안녕하세요. Moxin 보이스에 오신 것을 환영합니다. 저는 소희입니다." },
];

fn main() -> Result<()> {
    let model_dir = resolve_customvoice_model_dir();
    if !model_dir.join("config.json").exists() {
        anyhow::bail!(
            "CustomVoice-8bit model not found at {:?}\nRun scripts/download_qwen3_tts_models.py first.",
            model_dir
        );
    }

    let out_dir = resolve_qwen_root().join("previews");
    std::fs::create_dir_all(&out_dir)
        .with_context(|| format!("Failed to create output dir {:?}", out_dir))?;

    eprintln!("Loading Synthesizer from {:?} ...", model_dir);
    let mut synth = Synthesizer::load(&model_dir).context("Failed to load Synthesizer")?;
    eprintln!("Model loaded. Generating {} preview files...", SPEAKERS.len());

    let mut failed: Vec<&str> = Vec::new();

    for spec in SPEAKERS {
        let out_path = out_dir.join(format!("{}.wav", spec.id));
        eprint!("  [{}] ({}) → {:?} ... ", spec.id, spec.language, out_path);

        let opts = SynthesizeOptions {
            speaker: spec.id,
            language: spec.language,
            temperature: None,
            top_k: None,
            top_p: None,
            max_new_tokens: None,
            seed: Some(42),
            speed_factor: None,
        };

        match synth.synthesize(spec.text, &opts) {
            Ok(samples) if !samples.is_empty() => {
                let samples = normalize_audio(&samples, 0.95);
                match save_wav(&samples, synth.sample_rate, out_path.to_str().unwrap_or("out.wav")) {
                    Ok(_) => eprintln!("OK ({:.1}s)", samples.len() as f32 / synth.sample_rate as f32),
                    Err(e) => {
                        eprintln!("SAVE FAILED: {}", e);
                        failed.push(spec.id);
                    }
                }
            }
            Ok(_) => {
                eprintln!("EMPTY OUTPUT");
                failed.push(spec.id);
            }
            Err(e) => {
                eprintln!("SYNTH FAILED: {}", e);
                failed.push(spec.id);
            }
        }
    }

    if failed.is_empty() {
        eprintln!("\nAll {} preview files generated successfully.", SPEAKERS.len());
        eprintln!("Output directory: {:?}", out_dir);
    } else {
        eprintln!("\nFailed speakers: {:?}", failed);
        std::process::exit(1);
    }

    Ok(())
}
