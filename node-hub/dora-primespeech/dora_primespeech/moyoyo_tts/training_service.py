"""
Few-Shot Voice Cloning Training Service

Orchestrates the complete GPT-SoVITS training pipeline:
1. Slice long audio into segments
2. Denoise audio segments
3. ASR transcription
4. Feature extraction (semantic + phoneme)
5. Train GPT model (semantic)
6. Train SoVITS model (acoustic)

Communication: JSON-RPC over stdin/stdout
- Reads training request from stdin as JSON
- Emits progress events to stdout as JSON lines
"""

import json
import logging
import os
import shutil
import sys
import time
import traceback
from pathlib import Path
from typing import Dict, List, Optional, Tuple

import numpy as np
import torch
from scipy.io import wavfile

# Setup logging to stderr (stdout is reserved for JSON events)
logging.basicConfig(
    level=logging.INFO,
    format='%(asctime)s - %(levelname)s - %(message)s',
    stream=sys.stderr
)
logger = logging.getLogger(__name__)


class DictToAttrRecursive(dict):
    def __init__(self, input_dict):
        super().__init__(input_dict)
        for key, value in input_dict.items():
            if isinstance(value, dict):
                value = DictToAttrRecursive(value)
            self[key] = value
            setattr(self, key, value)

    def __getattr__(self, item):
        try:
            return self[item]
        except KeyError:
            raise AttributeError(f"Attribute {item} not found")

    def __setattr__(self, key, value):
        if isinstance(value, dict):
            value = DictToAttrRecursive(value)
        super(DictToAttrRecursive, self).__setitem__(key, value)
        super().__setattr__(key, value)

    def __delattr__(self, item):
        try:
            del self[item]
        except KeyError:
            raise AttributeError(f"Attribute {item} not found")


def get_pretrained_models_dir() -> str:
    """Resolve the pretrained models base directory.

    Checks (in order):
    1. PRIMESPEECH_MODEL_DIR env var → <dir>/moyoyo
    2. Fallback to ~/.dora/models/primespeech/moyoyo
    """
    env_dir = os.environ.get("PRIMESPEECH_MODEL_DIR")
    if env_dir:
        return os.path.join(os.path.expanduser(env_dir), "moyoyo")
    return os.path.join(os.path.expanduser("~"), ".dora", "models", "primespeech", "moyoyo")


def emit_progress(event_type: str, message: str, data: Optional[Dict] = None):
    """Emit JSON event to stdout for Rust to parse"""
    event = {
        "type": event_type,
        "message": message,
        "timestamp": time.time()
    }
    if data:
        event["data"] = data

    print(json.dumps(event), flush=True)
    logger.info(f"[{event_type}] {message}")


def get_audio_duration(file_path: str) -> float:
    """Get audio duration in seconds"""
    try:
        from moyoyo_tts.tools.my_utils import load_audio
        audio = load_audio(file_path, 32000)
        return len(audio) / 32000.0
    except Exception as e:
        logger.warning(f"Failed to get duration for {file_path}: {e}")
        return 0.0


def select_best_reference_audio(asr_list_path: str) -> Tuple[str, str]:
    """
    Select best reference audio from ASR transcription results.
    Prefer segments around 5-7 seconds for optimal reference quality.

    Returns: (audio_path, text)
    """
    emit_progress("INFO", "Selecting best reference audio from ASR results")

    if not os.path.exists(asr_list_path):
        raise FileNotFoundError(f"ASR list file not found: {asr_list_path}")

    with open(asr_list_path, 'r', encoding='utf-8') as f:
        lines = f.readlines()

    candidates = []
    for line in lines:
        parts = line.strip().split('|')
        if len(parts) == 4:
            audio_path, speaker, lang, text = parts

            # Skip if text is too short
            if len(text) < 10:
                continue

            duration = get_audio_duration(audio_path)

            # Filter for 3-10 second segments
            if 3.0 <= duration <= 10.0:
                # Prefer segments around 6 seconds
                score = abs(duration - 6.0)
                candidates.append({
                    'path': audio_path,
                    'text': text,
                    'duration': duration,
                    'score': score
                })

    if not candidates:
        raise ValueError(
            "No suitable reference audio found in training data. "
            "Audio segments should be 3-10 seconds with clear speech."
        )

    # Sort by score (lower is better) and select best
    candidates.sort(key=lambda x: x['score'])
    best = candidates[0]

    emit_progress("INFO", f"Selected reference: {os.path.basename(best['path'])} "
                         f"({best['duration']:.1f}s, {len(best['text'])} chars)")

    return best['path'], best['text']


def validate_audio_file(audio_file: str, min_duration: float = 10.0, max_duration: float = 600.0) -> float:
    """Validate audio file exists and meets duration requirements"""
    if not os.path.exists(audio_file):
        raise FileNotFoundError(f"Audio file not found: {audio_file}")

    duration = get_audio_duration(audio_file)

    if duration < min_duration:
        raise ValueError(
            f"Audio too short: {duration:.1f}s (minimum: {min_duration:.1f}s). "
            "Please provide at least 10 seconds of audio."
        )

    if duration > max_duration:
        emit_progress("WARNING", f"Audio is {duration:.1f}s (>{max_duration:.1f}s), will be trimmed")

    return duration


def create_directory_structure(workspace_dir: str):
    """Create workspace directory structure"""
    dirs = [
        "training_data",
        "processed/sliced",
        "processed/denoised",
        "processed/features",
        "checkpoints/gpt",
        "checkpoints/sovits",
        "models",
        "logs",
        "logs_s2",      # SoVITS checkpoint directory (s2_train.py saves G_/D_ here)
        "3-bert",       # BERT features directory (for GPT training)
        "4-cnhubert",   # SSL features directory (for SoVITS training)
        "5-wav32k"      # 32kHz audio directory (for SoVITS training)
    ]

    for dir_path in dirs:
        full_path = os.path.join(workspace_dir, dir_path)
        os.makedirs(full_path, exist_ok=True)
        logger.info(f"Created directory: {full_path}")


def generate_gpt_config(workspace_dir: str, language: str, gpt_epochs: int = 15, batch_size: int = 6) -> str:
    """Generate GPT training config YAML"""

    # Paths for training data
    semantic_path = os.path.join(workspace_dir, "processed/features/semantic.tsv")
    phoneme_path = os.path.join(workspace_dir, "processed/features/phoneme.txt")
    output_dir = os.path.join(workspace_dir, "checkpoints/gpt")
    model_dir = os.path.join(workspace_dir, "models")

    # Determine pretrained model path
    pretrained_gpt = os.path.join(get_pretrained_models_dir(), "gsv-v2final-pretrained/s1bert25hz-5kh-longer-epoch=12-step=369668.ckpt")
    if not os.path.exists(pretrained_gpt):
        emit_progress("ERROR", "Pretrained GPT model not found! Few-shot training requires pretrained model.")
        emit_progress("ERROR", f"Please download s1bert25hz-5kh-longer-epoch=12-step=369668.ckpt from HuggingFace")
        emit_progress("ERROR", f"and place it at: {pretrained_gpt}")
        raise FileNotFoundError(f"Pretrained GPT model not found: {pretrained_gpt}")

    # Model architecture must match the pretrained model (GPT-SoVITS v2)
    # These values are from the official pretrained model config
    config = {
        "output_dir": output_dir,
        "train_semantic_path": semantic_path,
        "train_phoneme_path": phoneme_path,

        "data": {
            "exp_dir": workspace_dir,
            "num_workers": 4,
            "max_sec": 54,
            "pad_val": 1024
        },

        "train": {
            "seed": 1234,
            "epochs": gpt_epochs,
            "batch_size": batch_size,
            "learning_rate": 0.0001,
            "save_every_n_epoch": 5,
            "if_save_latest": True,
            "if_save_every_weights": True,
            "half_weights_save_dir": model_dir,
            "exp_name": "gpt_model",
            "precision": "bf16-mixed" if torch.cuda.is_available() else "32"
        },

        # Model architecture matching pretrained GPT-SoVITS v2 model
        "model": {
            "hidden_dim": 512,
            "embedding_dim": 512,
            "head": 16,              # Must match pretrained (was 8)
            "n_layer": 24,           # Must match pretrained (was 12)
            "vocab_size": 1025,
            "phoneme_vocab_size": 732,  # Must match pretrained (was 512)
            "EOS": 1024,
            "dropout": 0.0
        },

        "optimizer": {
            "lr": 0.002,
            "lr_init": 1e-6,
            "lr_end": 0.002,
            "warmup_steps": 2000,
            "decay_steps": 40000
        },

        "pretrained_s1": pretrained_gpt
    }

    config_path = os.path.join(workspace_dir, "config_gpt.yaml")

    import yaml
    with open(config_path, 'w', encoding='utf-8') as f:
        yaml.dump(config, f, allow_unicode=True)

    logger.info(f"Generated GPT config: {config_path}")
    return config_path


def generate_sovits_config(workspace_dir: str, language: str, sovits_epochs: int = 20, batch_size: int = 4) -> str:
    """Generate SoVITS training config JSON"""

    # Paths
    training_files_path = os.path.join(workspace_dir, "processed/asr_labels.list")
    exp_dir = workspace_dir
    s2_ckpt_dir = os.path.join(workspace_dir, "checkpoints/sovits")

    # Determine pretrained model paths
    pretrained_s2G = os.path.join(get_pretrained_models_dir(), "gsv-v2final-pretrained/s2G2333k.pth")
    pretrained_s2D = os.path.join(get_pretrained_models_dir(), "gsv-v2final-pretrained/s2D2333k.pth")

    if not os.path.exists(pretrained_s2G):
        emit_progress("WARNING", "Pretrained SoVITS models not found, training from scratch")
        pretrained_s2G = ""
        pretrained_s2D = ""

    config = {
        "train": {
            "log_interval": 100,
            "eval_interval": 500,
            "seed": 1234,
            "epochs": sovits_epochs,
            "learning_rate": 0.0001,
            "betas": [0.8, 0.99],
            "eps": 1e-09,
            "batch_size": batch_size,
            "fp16_run": True,
            "lr_decay": 0.999875,
            "segment_size": 20480,
            "init_lr_ratio": 1,
            "warmup_epochs": 0,
            "c_mel": 45,
            "c_kl": 1.0,
            "text_low_lr_rate": 0.4,
            "pretrained_s2G": pretrained_s2G,
            "pretrained_s2D": pretrained_s2D,
            "if_save_latest": 1,
            "if_save_every_weights": True,
            "save_every_epoch": 5,
            "gpu_numbers": "0"
        },
        "data": {
            "max_wav_value": 32768.0,
            "sampling_rate": 32000,
            "filter_length": 2048,
            "hop_length": 640,
            "win_length": 2048,
            "n_mel_channels": 128,
            "mel_fmin": 0.0,
            "mel_fmax": None,
            "add_blank": True,
            "n_speakers": 300,
            "cleaned_text": True,
            "exp_dir": exp_dir,
            "training_files": training_files_path,
            "max_wav_value": 32768.0
        },
        "model": {
            "inter_channels": 192,
            "hidden_channels": 192,
            "filter_channels": 768,
            "n_heads": 2,
            "n_layers": 6,
            "kernel_size": 3,
            "p_dropout": 0.1,
            "resblock": "1",
            "resblock_kernel_sizes": [3, 7, 11],
            "resblock_dilation_sizes": [[1, 3, 5], [1, 3, 5], [1, 3, 5]],
            "upsample_rates": [10, 8, 2, 2, 2],
            "upsample_initial_channel": 512,
            "upsample_kernel_sizes": [16, 16, 8, 2, 2],
            "n_layers_q": 3,
            "use_spectral_norm": False,
            "gin_channels": 512,
            "semantic_frame_rate": "25hz",
            "freeze_quantizer": True
        },
        "s2_ckpt_dir": s2_ckpt_dir,
        "save_weight_dir": os.path.join(workspace_dir, "models"),
        "content_module": "cnhubert",
        "name": "sovits_model"
    }

    config_path = os.path.join(workspace_dir, "config_sovits.json")

    with open(config_path, 'w', encoding='utf-8') as f:
        json.dump(config, f, indent=2, ensure_ascii=False)

    logger.info(f"Generated SoVITS config: {config_path}")
    return config_path


def extract_features_for_gpt_training(workspace_dir: str, language: str, asr_list_path: str):
    """
    Extract semantic tokens and phoneme sequences for GPT training.

    Creates:
    - semantic.tsv: audio_name \t semantic_tokens
    - phoneme.txt: audio_name \t language \t text \t phonemes
    """
    emit_progress("INFO", "Extracting features for GPT training")

    # Import feature extractor
    from moyoyo_tts.feature_extractor import cnhubert
    from moyoyo_tts.text import cleaned_text_to_sequence
    from moyoyo_tts.text.cleaner import clean_text
    from moyoyo_tts.tools.my_utils import load_audio
    from moyoyo_tts.module.models import SynthesizerTrn
    # DictToAttrRecursive is defined at module level in this file

    # Set cnhubert model path before loading
    cnhubert.cnhubert_base_path = os.environ.get(
        "cnhubert_base_path",
        os.path.join(get_pretrained_models_dir(), "chinese-hubert-base")
    )

    # Load SSL model (HuBERT)
    ssl_model = cnhubert.get_model()
    device = "cuda" if torch.cuda.is_available() else "cpu"
    is_half = torch.cuda.is_available()

    if is_half:
        ssl_model = ssl_model.half().to(device)
    else:
        ssl_model = ssl_model.to(device)

    # Load a SoVITS model for VQ quantization (extract_latent)
    # The VQ codebook is shared across all fine-tuned models, so any will work.
    # Try pretrained first, then fall back to any existing SoVITS weights.
    models_dir = get_pretrained_models_dir()
    sovits_path = os.path.join(models_dir, "gsv-v2final-pretrained", "s2G2333k.pth")
    if not os.path.exists(sovits_path):
        sovits_weights_dir = os.path.join(models_dir, "SoVITS_weights")
        if os.path.isdir(sovits_weights_dir):
            sovits_files = [f for f in os.listdir(sovits_weights_dir) if f.endswith('.pth')]
            if sovits_files:
                sovits_path = os.path.join(sovits_weights_dir, sovits_files[0])
            else:
                raise FileNotFoundError(
                    f"No SoVITS model found. Checked:\n"
                    f"  1. {os.path.join(models_dir, 'gsv-v2final-pretrained', 's2G2333k.pth')}\n"
                    f"  2. {sovits_weights_dir}/*.pth"
                )
        else:
            raise FileNotFoundError(
                f"No SoVITS model found. Checked:\n"
                f"  1. {os.path.join(models_dir, 'gsv-v2final-pretrained', 's2G2333k.pth')}\n"
                f"  2. {sovits_weights_dir} (directory not found)"
            )
    emit_progress("INFO", f"Loading SoVITS quantizer from {sovits_path}")

    dict_s2 = torch.load(sovits_path, map_location="cpu")
    hps = dict_s2["config"]
    hps = DictToAttrRecursive(hps)
    hps.model.semantic_frame_rate = "25hz"
    if dict_s2['weight']['enc_p.text_embedding.weight'].shape[0] == 322:
        hps.model.version = "v1"
    else:
        hps.model.version = "v2"

    vq_model = SynthesizerTrn(
        hps.data.filter_length // 2 + 1,
        hps.train.segment_size // hps.data.hop_length,
        n_speakers=hps.data.n_speakers,
        **hps.model
    )
    del vq_model.enc_q
    if is_half:
        vq_model = vq_model.half().to(device)
    else:
        vq_model = vq_model.to(device)
    vq_model.eval()
    vq_model.load_state_dict(dict_s2["weight"], strict=False)

    # Read ASR results
    with open(asr_list_path, 'r', encoding='utf-8') as f:
        asr_lines = f.readlines()

    semantic_data = []
    phoneme_data = []

    for idx, line in enumerate(asr_lines):
        try:
            parts = line.strip().split('|')
            if len(parts) != 4:
                continue

            audio_path, speaker, lang, text = parts

            if not os.path.exists(audio_path):
                continue

            audio_name = os.path.splitext(os.path.basename(audio_path))[0]

            # Extract semantic tokens:
            # 1. Load audio at 16kHz for HuBERT
            audio = load_audio(audio_path, 16000)

            # 2. Run through HuBERT feature_extractor + model
            input_values = ssl_model.feature_extractor(
                audio, return_tensors="pt", sampling_rate=16000
            ).input_values

            if is_half:
                input_values = input_values.half()
            input_values = input_values.to(device)

            with torch.no_grad():
                ssl_content = ssl_model.model(input_values)["last_hidden_state"]
                # ssl_content shape: (1, seq_len, 768) → transpose to (1, 768, seq_len)
                ssl_content = ssl_content.transpose(1, 2)

                # 3. Quantize through SoVITS VQ model to get integer codebook indices
                codes = vq_model.extract_latent(ssl_content)
                # codes shape: (1, n_codebook, seq_len) — use first codebook
                semantic_ids = codes[0, 0].cpu().numpy().astype(int)  # (seq_len,)

            # Semantic line format: audio_name \t space-separated integer token IDs
            semantic_str = ' '.join(str(idx) for idx in semantic_ids.tolist())
            semantic_data.append(f"{audio_name}\t{semantic_str}")

            # Phoneme line format: audio_name \t phonemes \t word2ph \t text
            # (must match dataset.py: phoneme, word2ph, text = phoneme_data[item_name])
            norm_text = text.replace("\n", "").strip()

            # Get phonemes (clean_text handles normalization internally)
            phones, word2ph, norm_text = clean_text(norm_text, language, "v2")
            phones_str = ' '.join(map(str, phones))
            word2ph_str = ' '.join(map(str, word2ph))

            phoneme_data.append(f"{audio_name}\t{phones_str}\t{word2ph_str}\t{norm_text}")

            if (idx + 1) % 10 == 0:
                emit_progress("INFO", f"Extracted features {idx + 1}/{len(asr_lines)}")

        except Exception as e:
            logger.error(f"Failed to process {audio_path}: {e}")
            logger.error(traceback.format_exc())
            emit_progress("WARNING", f"Failed to process segment: {e}")
            continue

    if not semantic_data:
        raise ValueError("No valid training data generated")

    # Save semantic features (TSV format for GPT training)
    # Header row is required: pd.read_csv treats first line as column names
    semantic_path = os.path.join(workspace_dir, "processed/features/semantic.tsv")
    os.makedirs(os.path.dirname(semantic_path), exist_ok=True)
    with open(semantic_path, 'w', encoding='utf-8') as f:
        f.write("item_name\tsemantic_ids\n")
        f.write('\n'.join(semantic_data))

    # Save phoneme features (TXT format for GPT training)
    phoneme_path = os.path.join(workspace_dir, "processed/features/phoneme.txt")
    with open(phoneme_path, 'w', encoding='utf-8') as f:
        f.write('\n'.join(phoneme_data))

    emit_progress("INFO", f"Extracted features for {len(semantic_data)} audio segments")

    return semantic_path, phoneme_path


def extract_features_for_sovits_training(workspace_dir: str, language: str, asr_list_path: str):
    """
    Extract SSL features and prepare data structure for SoVITS training.

    Creates the expected directory structure:
    - exp_dir/2-name2text.txt: phoneme data
    - exp_dir/4-cnhubert/*.pt: SSL feature tensors
    - exp_dir/5-wav32k/*.wav: 32kHz audio files
    """
    emit_progress("INFO", "Preparing data for SoVITS training")

    from moyoyo_tts.feature_extractor import cnhubert
    from moyoyo_tts.text import cleaned_text_to_sequence
    from moyoyo_tts.text.cleaner import clean_text
    from moyoyo_tts.tools.my_utils import load_audio

    # Set cnhubert model path before loading
    cnhubert.cnhubert_base_path = os.environ.get(
        "cnhubert_base_path",
        os.path.join(get_pretrained_models_dir(), "chinese-hubert-base")
    )

    # Load SSL model
    ssl_model = cnhubert.get_model()
    device = "cuda" if torch.cuda.is_available() else "cpu"
    is_half = torch.cuda.is_available()

    if is_half:
        ssl_model = ssl_model.half().to(device)
    else:
        ssl_model = ssl_model.to(device)

    # Create directories
    cnhubert_dir = os.path.join(workspace_dir, "4-cnhubert")
    wav32k_dir = os.path.join(workspace_dir, "5-wav32k")
    os.makedirs(cnhubert_dir, exist_ok=True)
    os.makedirs(wav32k_dir, exist_ok=True)

    # Read ASR results
    with open(asr_list_path, 'r', encoding='utf-8') as f:
        asr_lines = f.readlines()

    phoneme_data = []

    for idx, line in enumerate(asr_lines):
        try:
            parts = line.strip().split('|')
            if len(parts) != 4:
                continue

            audio_path, speaker, lang, text = parts

            if not os.path.exists(audio_path):
                continue

            audio_name = os.path.splitext(os.path.basename(audio_path))[0]

            # Copy audio to 5-wav32k directory
            # File must be saved WITHOUT extension: data_utils.py intersects
            # names5 (raw filenames) with phoneme_data keys (no extension)
            wav32k_path = os.path.join(wav32k_dir, audio_name)
            shutil.copy2(audio_path, wav32k_path)

            # Extract and save SSL features to 4-cnhubert directory
            # Load audio at 16kHz (required by HuBERT)
            audio = load_audio(audio_path, 16000)

            # Use feature_extractor to preprocess (expects numpy), then run model on device
            input_values = ssl_model.feature_extractor(
                audio, return_tensors="pt", sampling_rate=16000
            ).input_values

            if is_half:
                input_values = input_values.half()
            input_values = input_values.to(device)

            with torch.no_grad():
                ssl_content = ssl_model.model(input_values)["last_hidden_state"]
                # ssl_content shape: (1, seq_len, 768) → transpose to (1, 768, seq_len)
                ssl_content = ssl_content.transpose(1, 2)

            # Save SSL feature as .pt file
            ssl_path = os.path.join(cnhubert_dir, f"{audio_name}.pt")
            torch.save(ssl_content.cpu(), ssl_path)

            # Prepare phoneme data
            norm_text = text.replace("\n", "").strip()

            # Get phonemes (clean_text handles normalization internally)
            phones, word2ph, norm_text = clean_text(norm_text, language, "v2")
            phones_str = ' '.join(map(str, phones))
            word2ph_str = ' '.join(map(str, word2ph))

            # Format: audio_name \t phonemes \t word2ph \t text
            # (must match data_utils.py: phoneme_data[name] = [tmp[1]] where tmp[1] = phonemes)
            phoneme_data.append(f"{audio_name}\t{phones_str}\t{word2ph_str}\t{norm_text}")

            if (idx + 1) % 10 == 0:
                emit_progress("INFO", f"Processed {idx + 1}/{len(asr_lines)} files for SoVITS")

        except Exception as e:
            logger.error(f"Failed to process {audio_path}: {e}")
            logger.error(traceback.format_exc())
            emit_progress("WARNING", f"Failed to process segment for SoVITS: {e}")
            continue

    if not phoneme_data:
        raise ValueError("No valid training data generated for SoVITS")

    # Save 2-name2text.txt
    phoneme_path = os.path.join(workspace_dir, "2-name2text.txt")
    with open(phoneme_path, 'w', encoding='utf-8') as f:
        f.write('\n'.join(phoneme_data))

    emit_progress("INFO", f"Prepared {len(phoneme_data)} files for SoVITS training")

    return phoneme_path


def run_training_pipeline(request: Dict):
    """
    Execute complete few-shot training pipeline.

    request = {
        "voice_id": str,
        "voice_name": str,
        "audio_file": str,
        "language": str,
        "workspace_dir": str,
        "training_params": {
            "gpt_epochs": 15,
            "sovits_epochs": 20,
            "batch_size": 4,
        }
    }
    """
    voice_id = request["voice_id"]
    voice_name = request["voice_name"]
    audio_file = request["audio_file"]
    language = request["language"]
    workspace_dir = request["workspace_dir"]
    params = request.get("training_params", {})

    gpt_epochs = params.get("gpt_epochs", 15)
    sovits_epochs = params.get("sovits_epochs", 20)
    batch_size = params.get("batch_size", 4)

    # Set version env var for v2 text processing (used by dataset.py, data_utils.py)
    os.environ["version"] = "v2"

    emit_progress("STAGE", "Preparing workspace", {"current": 1, "total": 7})

    try:
        # Stage 1: Prepare workspace
        create_directory_structure(workspace_dir)

        # Stage 2: Validate and copy audio
        emit_progress("STAGE", "Validating audio", {"current": 2, "total": 7})
        duration = validate_audio_file(audio_file)

        # Copy to training data directory
        training_audio_path = os.path.join(workspace_dir, "training_data/recording.wav")
        shutil.copy2(audio_file, training_audio_path)
        emit_progress("INFO", f"Audio validated: {duration:.1f}s")

        # Stage 3: Slice audio into segments
        emit_progress("STAGE", "Slicing audio into segments", {"current": 3, "total": 7})

        # Add tools directory to path for slicer2 import
        tools_dir = os.path.join(os.path.dirname(__file__), "tools")
        if tools_dir not in sys.path:
            sys.path.insert(0, tools_dir)

        from moyoyo_tts.tools.slice_audio import slice

        sliced_dir = os.path.join(workspace_dir, "processed/sliced")
        result = slice(
            inp=training_audio_path,
            opt_root=sliced_dir,
            threshold=-34,      # dB threshold for silence detection
            min_length=4000,    # 4 seconds minimum segment length (ms)
            min_interval=300,   # 300ms minimum cut interval
            hop_size=10,        # Precision of silence detection
            max_sil_kept=500,   # Keep max 500ms silence
            _max=0.9,           # Normalization ceiling
            alpha=0.25,         # Mix ratio
            i_part=0,           # Parallel processing part index
            all_part=1          # Total parallel processing parts
        )

        logger.info(f"Slice result: {result}")

        num_slices = len([f for f in os.listdir(sliced_dir) if f.endswith('.wav')])
        emit_progress("INFO", f"Audio sliced into {num_slices} segments")

        if num_slices == 0:
            raise ValueError("Audio slicing produced no segments. Check audio quality and volume.")

        # Stage 4: Denoise audio segments
        emit_progress("STAGE", "Denoising audio segments", {"current": 4, "total": 7})

        # Import from cmd-denoise.py (has hyphen in filename)
        import importlib.util
        denoise_path = os.path.join(
            os.path.dirname(__file__),
            "tools/cmd-denoise.py"
        )
        spec = importlib.util.spec_from_file_location("cmd_denoise", denoise_path)
        cmd_denoise = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(cmd_denoise)

        denoised_dir = os.path.join(workspace_dir, "processed/denoised")
        cmd_denoise.execute_denoise(
            input_folder=sliced_dir,
            output_folder=denoised_dir
        )

        num_denoised = len([f for f in os.listdir(denoised_dir) if f.endswith('.wav')])
        emit_progress("INFO", f"Denoised {num_denoised} audio segments")

        # Stage 5: ASR transcription
        emit_progress("STAGE", "Transcribing audio (ASR)", {"current": 5, "total": 7})

        asr_output_dir = os.path.join(workspace_dir, "processed")

        if language == "zh":
            from moyoyo_tts.tools.asr.funasr_asr import execute_asr
            asr_output_path = execute_asr(
                input_folder=denoised_dir,
                output_folder=asr_output_dir,
                model_size="large",
                language="zh"
            )
        else:  # English or other
            from moyoyo_tts.tools.asr.fasterwhisper_asr import execute_asr
            asr_output_path = execute_asr(
                input_folder=denoised_dir,
                output_folder=asr_output_dir,
                model_size="large",
                language="en"
            )

        emit_progress("INFO", f"ASR transcription completed: {asr_output_path}")

        # Auto-select reference audio
        ref_audio_path, ref_text = select_best_reference_audio(asr_output_path)

        # Stage 5b: Extract features for GPT training
        emit_progress("STAGE", "Extracting features for GPT", {"current": 5, "total": 7, "substage": "gpt_features"})
        semantic_path, phoneme_path = extract_features_for_gpt_training(workspace_dir, language, asr_output_path)

        # Stage 5c: Prepare data for SoVITS training
        emit_progress("INFO", "Preparing features for SoVITS")
        extract_features_for_sovits_training(workspace_dir, language, asr_output_path)

        # Stage 6: Train GPT model
        emit_progress("STAGE", f"Training GPT model ({gpt_epochs} epochs)", {"current": 6, "total": 7, "stage_epochs": gpt_epochs})

        gpt_config_path = generate_gpt_config(workspace_dir, language, gpt_epochs, batch_size)

        from moyoyo_tts.s1_train import main as train_gpt_main

        # Create args object for GPT training
        class GPTArgs:
            def __init__(self, config_file):
                self.config_file = config_file

        gpt_args = GPTArgs(gpt_config_path)

        def gpt_epoch_callback(epoch, total_epochs):
            emit_progress("PROGRESS", f"GPT训练: epoch {epoch}/{total_epochs}", {
                "stage": "gpt_training",
                "epoch": epoch,
                "total_epochs": total_epochs,
            })

        emit_progress("INFO", "Starting GPT training (this may take 20-60 minutes)")
        train_gpt_main(gpt_args, epoch_callback=gpt_epoch_callback)

        # Find final GPT checkpoint
        model_dir = os.path.join(workspace_dir, "models")
        gpt_checkpoints = [f for f in os.listdir(model_dir) if f.startswith("gpt_model") and f.endswith(".ckpt")]

        if not gpt_checkpoints:
            raise FileNotFoundError("GPT training completed but no checkpoint found")

        # Sort by epoch number and get the latest
        gpt_checkpoints.sort()
        gpt_final_path = os.path.join(model_dir, gpt_checkpoints[-1])

        emit_progress("INFO", f"GPT training completed: {os.path.basename(gpt_final_path)}")

        # Stage 7: Train SoVITS model
        emit_progress("STAGE", f"Training SoVITS model ({sovits_epochs} epochs)", {"current": 7, "total": 7, "stage_epochs": sovits_epochs})

        sovits_config_path = generate_sovits_config(workspace_dir, language, sovits_epochs, batch_size)

        emit_progress("INFO", "Starting SoVITS training (this may take 30-90 minutes)")

        def sovits_epoch_callback(epoch, total_epochs):
            emit_progress("PROGRESS", f"SoVITS训练: epoch {epoch}/{total_epochs}", {
                "stage": "sovits_training",
                "epoch": epoch,
                "total_epochs": total_epochs,
            })

        # Set sys.argv for s2_train to parse
        original_argv = sys.argv.copy()
        sys.argv = ["s2_train.py", "-c", sovits_config_path]

        try:
            # Import and run s2_train
            from moyoyo_tts import s2_train
            s2_train.main(epoch_callback=sovits_epoch_callback)
        finally:
            sys.argv = original_argv

        # Find final SoVITS checkpoint
        # savee() saves inference-ready weights to models/ as "sovits_model_eE_sS.pth"
        # (format: {"weight": ..., "config": ...})
        # G_*.pth in logs_s2/ are training checkpoints with different format
        sovits_weights = [f for f in os.listdir(model_dir) if f.startswith("sovits_model") and f.endswith(".pth")]

        if not sovits_weights:
            raise FileNotFoundError("SoVITS training completed but no checkpoint found in models/")

        # Get latest by name (sorted by epoch/step)
        sovits_weights.sort()
        sovits_final_path = os.path.join(model_dir, sovits_weights[-1])

        # Rename GPT checkpoint to standard name
        gpt_final_renamed = os.path.join(model_dir, "gpt_final.ckpt")
        if gpt_final_path != gpt_final_renamed:
            shutil.copy2(gpt_final_path, gpt_final_renamed)

        emit_progress("INFO", f"SoVITS training completed: {os.path.basename(sovits_final_path)}")

        # Stage 8: Training complete
        emit_progress("COMPLETE", "Training completed successfully", {
            "gpt_weights": gpt_final_renamed,
            "sovits_weights": sovits_final_path,
            "reference_audio": ref_audio_path,
            "reference_text": ref_text,
            "voice_id": voice_id,
            "voice_name": voice_name
        })

    except Exception as e:
        error_msg = str(e)
        error_trace = traceback.format_exc()
        logger.error(f"Training pipeline failed: {error_msg}")
        logger.error(error_trace)

        emit_progress("ERROR", error_msg, {"traceback": error_trace})
        sys.exit(1)


def main():
    """Entry point - read request from stdin, execute training"""
    try:
        # Read JSON request from stdin
        request_json = sys.stdin.read()
        request = json.loads(request_json)

        logger.info(f"Received training request: {request.get('voice_name', 'unknown')}")

        # Execute training pipeline
        run_training_pipeline(request)

    except json.JSONDecodeError as e:
        emit_progress("ERROR", f"Invalid JSON request: {e}")
        sys.exit(1)
    except Exception as e:
        emit_progress("ERROR", str(e), {"traceback": traceback.format_exc()})
        sys.exit(1)


if __name__ == "__main__":
    main()
