"""
Simplified Dora PrimeSpeech Node - Main entry point
High-quality text-to-speech using GPT-SoVITS technology.
"""

import time
import os
import sys
import traceback
import json
import numpy as np
import pyarrow as pa
from dora import Node
from pathlib import Path
from typing import Optional

from .config import PrimeSpeechConfig, VOICE_CONFIGS
from .model_manager import ModelManager
from .moyoyo_tts_wrapper_streaming_fix import StreamingMoxinTTSWrapper as MoxinTTSWrapper, MOXIN_AVAILABLE

# Add common logging to path
sys.path.append(os.path.join(os.path.dirname(__file__), '..', '..', 'dora-common'))
from dora_common.logging import send_log as common_send_log, get_log_level_from_env

# Voice format parsing constants
VOICE_PREFIX = "VOICE:"
VOICE_CUSTOM_PREFIX = "VOICE:CUSTOM|"
VOICE_TRAINED_PREFIX = "VOICE:TRAINED|"


def send_log(node, level, message, config_level="INFO"):
    """Wrapper for backward compatibility during migration to common logging."""
    # Convert old format to new format
    common_send_log(node, level, message, "primespeech-tts", config_level)


def validate_language_config(lang_code, param_name, node, log_level):
    """Validate language configuration and provide helpful error messages"""
    # Valid language codes for Moxin Voice v2
    VALID_LANGUAGES = ["auto", "auto_yue", "en", "zh", "ja", "yue", "ko",
                      "all_zh", "all_ja", "all_yue", "all_ko"]

    if lang_code in VALID_LANGUAGES:
        return lang_code

    # Invalid language code - log error
    main_error = f"INVALID {param_name}: '{lang_code}' is NOT a valid language!"
    send_log(node, "ERROR", main_error, log_level)

    # Check for common mistakes and suggest corrections
    if lang_code.lower() == "cn":
        hint = "Did you mean 'zh' for Chinese? Use 'zh' not 'cn'!"
        send_log(node, "ERROR", hint, log_level)
    elif lang_code.lower() == "chinese":
        hint = "Use 'zh' for Chinese, not 'chinese'!"
        send_log(node, "ERROR", hint, log_level)
    elif lang_code.lower() == "english":
        hint = "Use 'en' for English, not 'english'!"
        send_log(node, "ERROR", hint, log_level)

    valid_msg = f"Valid languages: {', '.join(VALID_LANGUAGES)}"
    send_log(node, "ERROR", valid_msg, log_level)
    send_log(node, "ERROR", f"TTS will fail until you fix {param_name}!", log_level)

    # Return the invalid code as-is (will cause TTS to fail with clear error)
    return lang_code


def _validate_models_path(logger, models_env_var="PRIMESPEECH_MODEL_DIR") -> Optional[Path]:
    """Validate that required model directory exists and contains Moxin subdir.
    Returns the resolved path if valid, else None.
    """
    raw = os.environ.get(models_env_var)
    if not raw:
        logger("ERROR", f"Missing {models_env_var} environment variable; TTS cannot load models")
        return None
    # Expand env vars (e.g., $HOME) and user (~)
    base = Path(os.path.expanduser(os.path.expandvars(raw)))
    if not base.exists():
        logger("ERROR", f"{models_env_var} points to non-existent path: {base}")
        return None
    moyoyo_dir = base / "moyoyo"
    if not moyoyo_dir.exists():
        logger("WARNING", f"Expected models under: {moyoyo_dir} (directory missing)")
    return base


def main():
    """Main entry point for PrimeSpeech node"""

    node = Node()
    config = PrimeSpeechConfig()

    # Get voice configuration
    voice_name = config.VOICE_NAME
    if voice_name not in VOICE_CONFIGS:
        send_log(node, "ERROR", f"Unknown voice: {voice_name}. Available: {list(VOICE_CONFIGS.keys())}", config.LOG_LEVEL)
        voice_name = "Doubao"

    voice_config = VOICE_CONFIGS[voice_name]

    # Override with environment variables if provided
    if config.PROMPT_TEXT:
        voice_config["prompt_text"] = config.PROMPT_TEXT

    # Validate and set text language
    send_log(node, "DEBUG", f"TEXT_LANG from env: '{config.TEXT_LANG}'", config.LOG_LEVEL)
    if config.TEXT_LANG:
        validated_text_lang = validate_language_config(
            config.TEXT_LANG, "TEXT_LANG", node, config.LOG_LEVEL)
        voice_config["text_lang"] = validated_text_lang
        send_log(node, "DEBUG", f"Validated TEXT_LANG: '{validated_text_lang}'", config.LOG_LEVEL)

    # Validate and set prompt language
    send_log(node, "DEBUG", f"PROMPT_LANG from env: '{config.PROMPT_LANG}'", config.LOG_LEVEL)
    if config.PROMPT_LANG:
        validated_prompt_lang = validate_language_config(
            config.PROMPT_LANG, "PROMPT_LANG", node, config.LOG_LEVEL)
        voice_config["prompt_lang"] = validated_prompt_lang
        send_log(node, "DEBUG", f"Validated PROMPT_LANG: '{validated_prompt_lang}'", config.LOG_LEVEL)
    
    # Add inference parameters
    effective_speed_factor = (
        config.SPEED_FACTOR
        if config.SPEED_FACTOR is not None
        else voice_config.get("speed_factor", 1.0)
    )

    if config.SPEED_FACTOR is not None:
        send_log(
            node,
            "INFO",
            f"Overriding speed_factor via env to {effective_speed_factor}",
            config.LOG_LEVEL,
        )

    effective_fragment_interval = (
        config.FRAGMENT_INTERVAL
        if config.FRAGMENT_INTERVAL is not None
        else voice_config.get("fragment_interval")
    )
    if config.FRAGMENT_INTERVAL is not None:
        send_log(
            node,
            "INFO",
            f"Overriding fragment_interval via env to {effective_fragment_interval}",
            config.LOG_LEVEL,
        )

    voice_config.update({
        "top_k": config.TOP_K,
        "top_p": config.TOP_P,
        "temperature": config.TEMPERATURE,
        "speed_factor": effective_speed_factor,
        "batch_size": config.BATCH_SIZE,
        "seed": config.SEED,
        "text_split_method": config.TEXT_SPLIT_METHOD,
        "split_bucket": config.SPLIT_BUCKET,
        "return_fragment": config.RETURN_FRAGMENT,
        "use_gpu": config.USE_GPU,
        "device": config.DEVICE,
        "sample_rate": config.SAMPLE_RATE,
    })
    if effective_fragment_interval is not None:
        voice_config["fragment_interval"] = effective_fragment_interval
    
    # Initialize model manager
    model_manager = ModelManager(config.get_models_dir())
    
    send_log(node, "INFO", "PrimeSpeech Node initialized", config.LOG_LEVEL)
    
    if MOXIN_AVAILABLE:
        send_log(node, "INFO", "✓ Moxin Voice engine available", config.LOG_LEVEL)
    else:
        send_log(node, "WARNING", "⚠️  Moxin Voice not fully available", config.LOG_LEVEL)
    
    # Log the configuration being used
    send_log(node, "INFO", f"Voice: {voice_name}", config.LOG_LEVEL)
    send_log(node, "INFO", f"Text Language: {voice_config.get('text_lang', 'auto')} (configured: {config.TEXT_LANG})", config.LOG_LEVEL)
    send_log(node, "INFO", f"Prompt Language: {voice_config.get('prompt_lang', 'auto')} (configured: {config.PROMPT_LANG})", config.LOG_LEVEL)

    # Print to stdout for immediate visibility
    speed_factor_value = voice_config.get('speed_factor')
    fragment_interval_value = voice_config.get('fragment_interval')
    send_log(
        node,
        "INFO",
        f"Speed Factor: {speed_factor_value} (env override: {config.SPEED_FACTOR_OVERRIDE is not None})",
        config.LOG_LEVEL,
    )
    if fragment_interval_value is not None:
        send_log(
            node,
            "INFO",
            f"Fragment Interval: {fragment_interval_value} (env override: {config.FRAGMENT_INTERVAL_OVERRIDE is not None})",
            config.LOG_LEVEL,
        )
    send_log(node, "INFO", f"Device: {config.DEVICE}", config.LOG_LEVEL)

    # Validate the final configuration
    final_text_lang = voice_config.get('text_lang', 'auto')
    final_prompt_lang = voice_config.get('prompt_lang', 'auto')

    VALID_LANGUAGES = ["auto", "auto_yue", "en", "zh", "ja", "yue", "ko",
                      "all_zh", "all_ja", "all_yue", "all_ko"]

    if final_text_lang not in VALID_LANGUAGES:
        send_log(node, "ERROR",
                f"CRITICAL: text_lang '{final_text_lang}' is not valid! "
                f"This will cause TTS to fail. Please fix your configuration.",
                config.LOG_LEVEL)

    if final_prompt_lang not in VALID_LANGUAGES:
        send_log(node, "ERROR",
                f"CRITICAL: prompt_lang '{final_prompt_lang}' is not valid! "
                f"This will cause TTS to fail. Please fix your configuration.",
                config.LOG_LEVEL)
    
    # Initialize TTS engine
    tts_engine: Optional[MoxinTTSWrapper] = None
    model_loaded = False

    # Pre-initialize TTS engine to avoid first-call delay
    try:
        send_log(node, "INFO", "Pre-initializing TTS engine...", config.LOG_LEVEL)
        start_time = time.time()

        # Validate models directory early
        _validate_models_path(lambda lvl, msg: send_log(node, lvl, msg, config.LOG_LEVEL))

        # Initialize TTS wrapper
        moyoyo_voice = voice_name.lower().replace(" ", "")
        # Support CUDA, MPS (Apple Silicon), or CPU
        if config.USE_GPU:
            if config.DEVICE.startswith("cuda"):
                device = "cuda"
            elif config.DEVICE == "mps":
                device = "mps"
            else:
                device = "cpu"
        else:
            device = "cpu"
        enable_streaming = config.RETURN_FRAGMENT if hasattr(config, 'RETURN_FRAGMENT') else False

        tts_engine = MoxinTTSWrapper(
            voice=moyoyo_voice,
            device=device,
            enable_streaming=enable_streaming,
            chunk_duration=0.3,
            voice_config=voice_config,
            logger_func=lambda level, msg: send_log(node, level, msg, config.LOG_LEVEL)
        )

        # Verify initialization (don't access tts_engine.tts - can deadlock on macOS)
        if tts_engine is None:
            raise RuntimeError("TTS engine initialization failed")

        # Store current voice and model weights for change detection
        tts_engine._current_voice = voice_name
        tts_engine._gpt_weights = voice_config.get('gpt_weights', '')
        tts_engine._sovits_weights = voice_config.get('sovits_weights', '')
        
        model_loaded = True
        init_time = time.time() - start_time
        send_log(node, "INFO", f"TTS engine pre-initialized in {init_time:.2f}s", config.LOG_LEVEL)
        send_log(node, "INFO", f"Ready to synthesize speech with voice: {voice_name}", config.LOG_LEVEL)

    except Exception as init_err:
        send_log(node, "WARNING", f"Failed to pre-initialize TTS engine: {init_err}", config.LOG_LEVEL)
        send_log(node, "WARNING", "TTS engine will be initialized on first use", config.LOG_LEVEL)
        send_log(node, "DEBUG", f"Traceback: {traceback.format_exc()}", config.LOG_LEVEL)
        model_loaded = False
        tts_engine = None

    # Statistics
    total_syntheses = 0
    total_duration = 0
    
    for event in node:
        if event["type"] == "INPUT":
            input_id = event["id"]
            
            if input_id == "text":
                # Get text to synthesize
                raw_data = event["value"][0].as_py()
                metadata = event.get("metadata", {})
                
                print(f"DEBUG: Raw data received: {raw_data}", file=sys.stderr, flush=True)
                
                # Parse JSON payload {"prompt": "VOICE:name|text"} or {"prompt": "text"}
                try:
                    payload = json.loads(raw_data)
                    raw_text = payload.get("prompt", "")
                    print(f"DEBUG: Extracted prompt from JSON: {raw_text}", file=sys.stderr, flush=True)
                except (json.JSONDecodeError, TypeError) as e:
                    # Fallback: treat as plain text if not valid JSON
                    print(f"DEBUG: Not valid JSON, treating as plain text: {e}", file=sys.stderr, flush=True)
                    raw_text = raw_data
                
                # Parse VOICE: prefix for dynamic voice switching
                # Format 1 (built-in): "VOICE:voice_name|actual_text"
                # Format 2 (custom):   "VOICE:CUSTOM|ref_audio_path|prompt_text|language|actual_text"
                current_voice_name = voice_name  # Default to initial voice
                text = raw_text
                custom_voice_config = None  # For custom voices

                if raw_text.startswith(VOICE_PREFIX):
                    try:
                        # Check for trained voice format (Pro Mode few-shot trained models)
                        if raw_text.startswith(VOICE_TRAINED_PREFIX):
                            # Parse trained voice format: VOICE:TRAINED|gpt_weights|sovits_weights|ref_audio|prompt_text|language|text
                            parts = raw_text[len(VOICE_TRAINED_PREFIX):].split("|", 5)
                            if len(parts) == 6:
                                gpt_weights_path, sovits_weights_path, ref_audio_path, prompt_text, lang, text = parts
                                print(f"DEBUG: Parsed TRAINED VOICE - GPT: '{gpt_weights_path}', SoVITS: '{sovits_weights_path}', ref_audio: '{ref_audio_path}', prompt: '{prompt_text[:30]}...', lang: '{lang}'", file=sys.stderr, flush=True)
                                send_log(node, "INFO", f"Using trained voice with custom models", config.LOG_LEVEL)
                                send_log(node, "INFO", f"  GPT weights: {gpt_weights_path}", config.LOG_LEVEL)
                                send_log(node, "INFO", f"  SoVITS weights: {sovits_weights_path}", config.LOG_LEVEL)
                                send_log(node, "INFO", f"  Reference audio: {ref_audio_path}", config.LOG_LEVEL)

                                # Create trained voice config with custom model weights
                                # This enables few-shot voice cloning with trained GPT and SoVITS models
                                custom_voice_config = {
                                    "gpt_weights": gpt_weights_path,  # Custom trained GPT weights
                                    "sovits_weights": sovits_weights_path,  # Custom trained SoVITS weights
                                    "reference_audio": ref_audio_path,  # Training reference audio (absolute path)
                                    "prompt_text": prompt_text,  # Training prompt text
                                    "text_lang": lang if lang in ["zh", "en", "ja", "auto"] else "auto",
                                    "prompt_lang": lang if lang in ["zh", "en", "ja", "auto"] else "auto",
                                    "speed_factor": 1.1,
                                }
                                current_voice_name = "TRAINED"
                                print(f"DEBUG: Created custom_voice_config for TRAINED voice: {custom_voice_config}", file=sys.stderr, flush=True)
                            else:
                                send_log(node, "WARNING", f"Invalid TRAINED voice format (expected 6 parts), got {len(parts)}: {raw_text[:100]}", config.LOG_LEVEL)
                        # Check for custom voice format (Express Mode zero-shot cloning)
                        elif raw_text.startswith(VOICE_CUSTOM_PREFIX):
                            # Parse custom voice format: VOICE:CUSTOM|ref_audio|prompt_text|language|text
                            # Remove prefix robustly using len() instead of hardcoded index
                            parts = raw_text[len(VOICE_CUSTOM_PREFIX):].split("|", 3)
                            if len(parts) == 4:
                                ref_audio_path, prompt_text, lang, text = parts
                                print(f"DEBUG: Parsed CUSTOM VOICE - ref_audio: '{ref_audio_path}', prompt: '{prompt_text[:30]}...', lang: '{lang}'", file=sys.stderr, flush=True)
                                send_log(node, "INFO", f"Using custom voice with ref audio: {ref_audio_path}", config.LOG_LEVEL)

                                # Create custom voice config using default model weights
                                # This enables zero-shot voice cloning with user's reference audio
                                custom_voice_config = {
                                    "repository": "MoxinTech/tone-models",
                                    "gpt_weights": "GPT_weights/doubao-mixed.ckpt",  # Use default GPT weights
                                    "sovits_weights": "SoVITS_weights/doubao-mixed.pth",  # Use default SoVITS weights
                                    "reference_audio": ref_audio_path,  # User's reference audio (absolute path)
                                    "prompt_text": prompt_text,  # User's prompt text
                                    "text_lang": lang if lang in ["zh", "en", "ja", "auto"] else "auto",
                                    "prompt_lang": lang if lang in ["zh", "en", "ja", "auto"] else "auto",
                                    "speed_factor": 1.1,
                                }
                                current_voice_name = "CUSTOM"
                            else:
                                send_log(node, "WARNING", f"Invalid CUSTOM voice format (expected 4 parts), got {len(parts)}: {raw_text[:100]}", config.LOG_LEVEL)
                        else:
                            # Parse built-in voice format: VOICE:voice_name|text
                            parts = raw_text.split("|", 1)
                            if len(parts) == 2:
                                # Remove "VOICE:" prefix robustly using len() instead of hardcoded index
                                voice_prefix = parts[0][len(VOICE_PREFIX):].strip()
                                text = parts[1]

                                print(f"DEBUG: Parsed VOICE - name: '{voice_prefix}', text: '{text}'", file=sys.stderr, flush=True)
                                send_log(node, "DEBUG", f"Parsed VOICE prefix: '{voice_prefix}', text: '{text[:50]}...'", config.LOG_LEVEL)

                                # Check if voice exists
                                if voice_prefix in VOICE_CONFIGS:
                                    current_voice_name = voice_prefix
                                    send_log(node, "INFO", f"Switching to voice: {current_voice_name}", config.LOG_LEVEL)
                                else:
                                    send_log(node, "WARNING", f"Unknown voice '{voice_prefix}', using default: {voice_name}. Available: {list(VOICE_CONFIGS.keys())}", config.LOG_LEVEL)
                            else:
                                send_log(node, "WARNING", f"Invalid VOICE: format (expected 'VOICE:name|text'), got: {raw_text[:100]}", config.LOG_LEVEL)
                    except Exception as e:
                        send_log(node, "WARNING", f"Failed to parse VOICE: prefix: {e}, using raw text", config.LOG_LEVEL)

                # DEBUG: Log what we received (show only processed text, not the full VOICE: string)
                if raw_text.startswith(VOICE_PREFIX):
                    send_log(node, "DEBUG", f"RECEIVED with VOICE prefix, parsed text: '{text[:50]}...', voice={current_voice_name}", config.LOG_LEVEL)
                else:
                    send_log(node, "DEBUG", f"RECEIVED text: '{text}' (len={len(text)}, voice={current_voice_name})", config.LOG_LEVEL)
                
                print(f"DEBUG: Final TTS text: '{text}', voice: {current_voice_name}", file=sys.stderr, flush=True)

                segment_index = int(metadata.get("segment_index", -1))

                # Skip if text is only punctuation or whitespace
                text_stripped = text.strip()
                if not text_stripped or all(c in '。！？.!?,，、；：""''（）【】《》\n\r\t ' for c in text_stripped):
                    send_log(node, "DEBUG", f"SKIPPED - text is only punctuation/whitespace: '{text}'", config.LOG_LEVEL)
                    # Send segment_complete without audio
                    # Send segment skipped signal
                    node.send_output(
                        "segment_complete",
                        pa.array(["skipped"]),
                        metadata={
                            "question_id": metadata.get("question_id", "default"),  # Pass through question_id
                            "session_status": metadata.get("session_status", "unknown"),  # Pass through session status
                        }
                    )

                    # For empty text, just skip processing but send segment_complete for flow control
                    send_log(node, "DEBUG", f"Skipping empty segment", config.LOG_LEVEL)

                    # Send segment_complete to maintain proper flow control, passing through ALL metadata
                    node.send_output(
                        "segment_complete",
                        pa.array(["empty"]),
                        metadata=metadata if metadata else {}
                    )
                    continue

                send_log(node, "DEBUG", f"Processing segment {segment_index + 1} (len={len(text)})", config.LOG_LEVEL)

                # Check if voice changed and we need to reload model
                current_tts_voice = getattr(tts_engine, '_current_voice', voice_name) if tts_engine else voice_name
                voice_changed = (model_loaded and current_tts_voice != current_voice_name)

                print(f"DEBUG: Voice check - current: {current_tts_voice}, requested: {current_voice_name}, changed: {voice_changed}", file=sys.stderr, flush=True)

                # For custom voices using the same base model, just update ref audio
                # instead of full reload (avoids dora connection timeout from long model load)
                if voice_changed and model_loaded and tts_engine is not None and custom_voice_config is not None:
                    current_gpt = custom_voice_config.get('gpt_weights', '')
                    current_sovits = custom_voice_config.get('sovits_weights', '')
                    engine_gpt = getattr(tts_engine, '_gpt_weights', '')
                    engine_sovits = getattr(tts_engine, '_sovits_weights', '')
                    same_base_model = (current_gpt == engine_gpt and current_sovits == engine_sovits)

                    if same_base_model:
                        print(f"DEBUG: Same base model, updating ref audio only (no reload)", file=sys.stderr, flush=True)
                        new_ref = custom_voice_config.get('reference_audio', '')
                        new_prompt = custom_voice_config.get('prompt_text', '')
                        new_prompt_lang = custom_voice_config.get('prompt_lang', 'zh')

                        # Update wrapper attributes
                        tts_engine.ref_audio_path = new_ref
                        tts_engine.prompt_text = new_prompt

                        # Pre-process the new reference audio via set_ref_audio()
                        # (safe now: CNHuBERT uses eager attention, torch threads=1)
                        print(f"DEBUG: Calling set_ref_audio for: {new_ref}", file=sys.stderr, flush=True)
                        tts_engine.tts.set_ref_audio(new_ref)
                        # Update prompt text cache so TTS.run() doesn't re-process
                        tts_engine.tts.prompt_cache["prompt_text"] = ""  # Force re-processing by clearing
                        print(f"DEBUG: set_ref_audio completed successfully", file=sys.stderr, flush=True)

                        tts_engine._current_voice = current_voice_name

                        # Update voice_config for synthesis
                        current_voice_config = custom_voice_config.copy()
                        for key in ["text_lang", "prompt_lang", "top_k", "top_p", "temperature",
                                   "speed_factor", "batch_size", "seed", "text_split_method",
                                   "split_bucket", "return_fragment", "use_gpu", "device",
                                   "sample_rate", "fragment_interval"]:
                            if key in voice_config and key not in current_voice_config:
                                current_voice_config[key] = voice_config[key]

                        voice_changed = False  # Skip full reload
                        print(f"DEBUG: Ref audio updated to: {new_ref}", file=sys.stderr, flush=True)

                # Load or reload models if not loaded or voice changed
                print(f"DEBUG: Checking reload condition - model_loaded={model_loaded}, voice_changed={voice_changed}", file=sys.stderr, flush=True)
                if not model_loaded or voice_changed:
                    if voice_changed:
                        print(f"DEBUG: Voice changed from {tts_engine._current_voice} to {current_voice_name}, reloading model...", file=sys.stderr, flush=True)
                        send_log(node, "INFO", f"Voice changed from {tts_engine._current_voice} to {current_voice_name}, reloading model...", config.LOG_LEVEL)
                        # Release old engine resources before creating new one
                        # This helps prevent PyTorch/Accelerate deadlocks on macOS
                        if tts_engine is not None:
                            try:
                                del tts_engine
                                tts_engine = None
                                import gc; gc.collect()
                            except Exception:
                                pass
                    else:
                        print(f"DEBUG: Loading models for the first time...", file=sys.stderr, flush=True)
                        send_log(node, "DEBUG", "Loading models for the first time...", config.LOG_LEVEL)

                    # Validate models directory early so failures are visible
                    _validate_models_path(lambda lvl, msg: send_log(node, lvl, msg, config.LOG_LEVEL))

                    # Get voice config for current voice
                    if custom_voice_config is not None:
                        # Use custom voice config (zero-shot cloning with user's reference audio)
                        current_voice_config = custom_voice_config.copy()
                        print(f"DEBUG: Using custom_voice_config: voice={current_voice_name}, gpt={current_voice_config.get('gpt_weights', 'N/A')[:80]}, sovits={current_voice_config.get('sovits_weights', 'N/A')[:80]}", file=sys.stderr, flush=True)
                        send_log(node, "INFO", f"Using custom voice config with ref audio: {current_voice_config.get('reference_audio', 'N/A')}", config.LOG_LEVEL)
                    elif current_voice_name not in VOICE_CONFIGS:
                        send_log(node, "WARNING", f"Voice {current_voice_name} not found, using default: {voice_name}", config.LOG_LEVEL)
                        current_voice_name = voice_name
                        current_voice_config = VOICE_CONFIGS[current_voice_name].copy()
                    else:
                        current_voice_config = VOICE_CONFIGS[current_voice_name].copy()

                    # Apply overrides from main voice_config
                    for key in ["text_lang", "prompt_lang", "top_k", "top_p", "temperature",
                               "speed_factor", "batch_size", "seed", "text_split_method",
                               "split_bucket", "return_fragment", "use_gpu", "device",
                               "sample_rate", "fragment_interval"]:
                        if key in voice_config and key not in current_voice_config:
                            current_voice_config[key] = voice_config[key]

                    try:
                        # Always use PRIMESPEECH_MODEL_DIR
                        send_log(node, "DEBUG", "Using PRIMESPEECH_MODEL_DIR for models...", config.LOG_LEVEL)
                        # Initialize TTS engine
                        # Convert voice name to lowercase and remove spaces for Moxin compatibility
                        moyoyo_voice = current_voice_name.lower().replace(" ", "")

                        # Support CUDA, MPS (Apple Silicon), or CPU
                        if config.USE_GPU:
                            if config.DEVICE.startswith("cuda"):
                                device = "cuda"
                            elif config.DEVICE == "mps":
                                device = "mps"
                            else:
                                device = "cpu"
                        else:
                            device = "cpu"

                        enable_streaming = config.RETURN_FRAGMENT if hasattr(config, 'RETURN_FRAGMENT') else False

                        # Initialize TTS wrapper using PRIMESPEECH_MODEL_DIR
                        print(f"DEBUG: About to create MoxinTTSWrapper with voice='{moyoyo_voice}', device='{device}', config keys={list(current_voice_config.keys())}", file=sys.stderr, flush=True)
                        print(f"DEBUG: Config gpt_weights={current_voice_config.get('gpt_weights', 'NONE')}", file=sys.stderr, flush=True)
                        print(f"DEBUG: Config sovits_weights={current_voice_config.get('sovits_weights', 'NONE')}", file=sys.stderr, flush=True)
                        tts_engine = MoxinTTSWrapper(
                            voice=moyoyo_voice,
                            device=device,
                            enable_streaming=enable_streaming,
                            chunk_duration=0.3,
                            voice_config=current_voice_config,
                            logger_func=lambda level, msg: send_log(node, level, msg, config.LOG_LEVEL)
                        )
                        print(f"DEBUG: MoxinTTSWrapper created successfully", file=sys.stderr, flush=True)
                        
                        # Store current voice and model weights for change detection
                        tts_engine._current_voice = current_voice_name
                        tts_engine._gpt_weights = current_voice_config.get('gpt_weights', '')
                        tts_engine._sovits_weights = current_voice_config.get('sovits_weights', '')

                        # Skip tts_engine.tts access check - can deadlock on macOS
                        send_log(node, "DEBUG", "TTS engine initialized successfully", config.LOG_LEVEL)
                        model_loaded = True
                        send_log(node, "DEBUG", "TTS engine ready", config.LOG_LEVEL)
                    except Exception as init_err:
                        send_log(node, "ERROR", f"TTS init error: {init_err}", config.LOG_LEVEL)
                        send_log(node, "ERROR", f"Traceback: {traceback.format_exc()}", config.LOG_LEVEL)
                        # Mark as not loaded and send error completion without audio
                        model_loaded = False
                        # Send error completion signal
                        node.send_output(
                            "segment_complete",
                            pa.array(["error"]),
                            metadata={
                                "session_id": session_id,
                                "request_id": request_id,
                                "question_id": metadata.get("question_id", "default"),  # Pass through question_id
                                "session_status": "error",  # Explicit error status
                                "error": str(init_err),
                                "error_stage": "init"
                            }
                        )

                        # Session end signals are now handled by the text segmenter, not TTS
                        # The text segmenter will handle error cases appropriately
                        send_log(node, "ERROR", f"TTS initialization error for question_id {metadata.get('question_id', 'default')}: {init_err}", config.LOG_LEVEL)
                        # Skip this event since we cannot synthesize
                        continue
                
                # Synthesize speech
                print(f"DEBUG: ========== SYNTHESIS START ==========", file=sys.stderr, flush=True)
                print(f"DEBUG: Text to synthesize: '{text[:100]}...' (len={len(text)})", file=sys.stderr, flush=True)
                print(f"DEBUG: Voice: {current_voice_name}, Config keys: {list(voice_config.keys())}", file=sys.stderr, flush=True)
                start_time = time.time()

                try:
                    # Check if TTS engine is available
                    print(f"DEBUG: [S1] Checking tts_engine...", file=sys.stderr, flush=True)
                    if tts_engine is None:
                        send_log(node, "ERROR", "Cannot synthesize - TTS engine is None!", config.LOG_LEVEL)
                        raise RuntimeError("TTS engine not initialized")

                    # Skip tts_engine.tts check - accessing .tts can deadlock on macOS
                    # when PyTorch models are reloaded in the same process

                    print(f"DEBUG: [S4] Getting config values...", file=sys.stderr, flush=True)
                    language = voice_config.get("text_lang", "zh")
                    speed = voice_config.get("speed_factor", 1.0)
                    fragment_interval = voice_config.get("fragment_interval")

                    print(f"DEBUG: [SYNTHESIS PREP] text='{text[:50]}...', language={language}, speed={speed}, streaming={hasattr(tts_engine, 'enable_streaming') and tts_engine.enable_streaming}", file=sys.stderr, flush=True)

                    if hasattr(tts_engine, 'enable_streaming') and tts_engine.enable_streaming:
                        # Streaming synthesis
                        send_log(node, "DEBUG", "Using streaming synthesis...", config.LOG_LEVEL)
                        print(f"DEBUG: [STREAMING] About to call synthesize_streaming() for {len(text)} chars", file=sys.stderr, flush=True)
                        fragment_num = 0
                        total_audio_duration = 0

                        if fragment_interval is not None:
                            tts_engine.optimization_config["fragment_interval"] = fragment_interval

                        print(f"DEBUG: [STREAMING] Starting iteration over synthesize_streaming generator...", file=sys.stderr, flush=True)
                        for sample_rate, audio_fragment in tts_engine.synthesize_streaming(text, language=language, speed=speed):
                            print(f"DEBUG: [STREAMING] Got fragment {fragment_num+1}: sample_rate={sample_rate}, audio_len={len(audio_fragment) if audio_fragment is not None else 0}, dtype={audio_fragment.dtype if audio_fragment is not None else 'None'}", file=sys.stderr, flush=True)
                            fragment_num += 1
                            fragment_duration = len(audio_fragment) / sample_rate
                            total_audio_duration += fragment_duration

                            # Guard against empty fragments
                            if audio_fragment is None or len(audio_fragment) == 0:
                                send_log(node, "WARNING", f"Skipping empty audio fragment {fragment_num}", config.LOG_LEVEL)
                            else:
                                # Ensure type is float32 for consistency
                                if audio_fragment.dtype != np.float32:
                                    audio_fragment = audio_fragment.astype(np.float32)
                                node.send_output(
                                    "audio",
                                    pa.array([audio_fragment]),
                                    metadata={
                                        "question_id": metadata.get("question_id", "default"),  # Pass through question_id
                                        "session_status": metadata.get("session_status", "unknown"),  # Pass through session status
                                        "sample_rate": sample_rate,
                                        "duration": fragment_duration,
                                    }
                                )
                        
                        print(f"DEBUG: [STREAMING] Finished iteration. Total fragments={fragment_num}, total_duration={total_audio_duration:.2f}s", file=sys.stderr, flush=True)
                        synthesis_time = time.time() - start_time
                        send_log(node, "INFO", f"Streamed {fragment_num} fragments, {total_audio_duration:.2f}s audio in {synthesis_time:.3f}s", config.LOG_LEVEL)
                        # If nothing was streamed, mark as error to avoid hanging clients
                        if fragment_num == 0:
                            print(f"DEBUG: [STREAMING ERROR] No fragments produced!", file=sys.stderr, flush=True)
                            raise RuntimeError("No audio fragments produced during streaming synthesis")
                        
                    else:
                        # Batch synthesis
                        print(f"DEBUG: [BATCH] Using batch synthesis...", file=sys.stderr, flush=True)
                        synth_kwargs = {
                            "language": language,
                            "speed": speed,
                        }
                        if fragment_interval is not None:
                            synth_kwargs["fragment_interval"] = fragment_interval

                        print(f"DEBUG: [BATCH] About to call synthesize() with kwargs={synth_kwargs}", file=sys.stderr, flush=True)
                        sample_rate, audio_array = tts_engine.synthesize(text, **synth_kwargs)
                        print(f"DEBUG: [BATCH] Got result: sample_rate={sample_rate}, audio_len={len(audio_array) if audio_array is not None else 0}, dtype={audio_array.dtype if audio_array is not None else 'None'}", file=sys.stderr, flush=True)

                        synthesis_time = time.time() - start_time
                        audio_duration = len(audio_array) / sample_rate
                        print(f"DEBUG: [BATCH] Calculated duration={audio_duration:.2f}s, synthesis_time={synthesis_time:.3f}s", file=sys.stderr, flush=True)
                        if audio_array is None or len(audio_array) == 0:
                            raise RuntimeError("TTS returned empty audio array")
                        # Normalize dtype
                        if audio_array.dtype != np.float32:
                            audio_array = audio_array.astype(np.float32)
                        
                        total_syntheses += 1
                        total_duration += audio_duration
                        
                        send_log(node, "DEBUG", f"Synthesized: {audio_duration:.2f}s audio in {synthesis_time:.3f}s", config.LOG_LEVEL)

                        # Send audio output with segment counting metadata
                        node.send_output(
                            "audio",
                            pa.array([audio_array]),
                            metadata={
                                "question_id": metadata.get("question_id", "default"),  # Pass through question_id
                                "session_status": metadata.get("session_status", "unknown"),  # Pass through session status
                                "sample_rate": sample_rate,
                                "duration": audio_duration,
                            }
                        )
                        send_log(node, "INFO", f"📤 AUDIO SENT: {len(audio_array)} samples ({audio_duration:.2f}s)", config.LOG_LEVEL)

                    # Send segment completion signal
                    node.send_output(
                        "segment_complete",
                        pa.array(["completed"]),
                        metadata={
                            "question_id": metadata.get("question_id", "default"),  # Pass through question_id
                            "session_status": metadata.get("session_status", "unknown"),  # Pass through session status
                        }
                    )
                    send_log(node, "DEBUG", f"📤 SEGMENT_COMPLETE sent", config.LOG_LEVEL)

                    # Session end signals are now handled by the text segmenter, not TTS
                    # The text segmenter detects session end from session_status metadata and sends appropriate signals
                    session_status = metadata.get("session_status", "unknown")
                    if session_status in ["completed", "finished", "ended", "final"]:
                        send_log(node, "INFO", f"TTS completed session for question_id {metadata.get('question_id', 'default')} with status: {session_status}", config.LOG_LEVEL)

                except Exception as e:
                    error_details = traceback.format_exc()

                    # Check for specific language-related errors
                    if "assert text_lang" in str(e) or "assert prompt_lang" in str(e) or "AssertionError" in str(e.__class__.__name__):
                        send_log(node, "ERROR", "="*60, config.LOG_LEVEL)
                        send_log(node, "ERROR", "CRITICAL: Language configuration error detected!", config.LOG_LEVEL)
                        send_log(node, "ERROR", f"TEXT_LANG: '{language}' (from config: '{config.TEXT_LANG}')", config.LOG_LEVEL)
                        send_log(node, "ERROR", f"PROMPT_LANG: '{voice_config.get('prompt_lang', 'auto')}' (from config: '{config.PROMPT_LANG}')", config.LOG_LEVEL)
                        send_log(node, "ERROR", "Valid languages: auto, auto_yue, zh, en, ja, ko, yue, all_zh, all_ja, all_yue, all_ko", config.LOG_LEVEL)
                        send_log(node, "ERROR", "Common mistakes: 'cn' should be 'zh', 'chinese' should be 'zh'", config.LOG_LEVEL)
                        send_log(node, "ERROR", "Fix your configuration and restart!", config.LOG_LEVEL)
                        send_log(node, "ERROR", "="*60, config.LOG_LEVEL)

                    send_log(node, "ERROR", f"Synthesis error: {e}", config.LOG_LEVEL)
                    send_log(node, "ERROR", f"Traceback: {error_details}", config.LOG_LEVEL)
                    
                    # Do NOT send invalid audio on error; only notify completion with error
                    # Send error completion signal
                    node.send_output(
                        "segment_complete",
                        pa.array(["error"]),
                        metadata={
                            "question_id": metadata.get("question_id", "default"),  # Pass through question_id
                            "session_status": "error",  # Explicit error status
                            "error": str(e),
                            "error_stage": "synthesis"
                        }
                    )
                    question_id = metadata.get('question_id', 0)
                    if isinstance(question_id, (int, float)):
                        send_log(node, "ERROR", f"Sent error segment_complete with enhanced question_id={question_id}", config.LOG_LEVEL)
                    else:
                        send_log(node, "ERROR", f"Sent error segment_complete with question_id={question_id}", config.LOG_LEVEL)

                    # Session end signals are now handled by the text segmenter, not TTS
                    # The text segmenter will handle error cases appropriately based on session_status metadata
                    send_log(node, "ERROR", f"TTS synthesis error for question_id {metadata.get('question_id', 'default')}: {e}", config.LOG_LEVEL)

            elif input_id == "control":
                # Handle control commands
                command = event["value"][0].as_py()
                
                if command == "reset":
                    send_log(node, "INFO", "[PrimeSpeech] RESET received", config.LOG_LEVEL)
                    # Note: Can't actually stop ongoing synthesis, but it's OK
                    # because we only process one segment at a time now
                    send_log(node, "INFO", "[PrimeSpeech] Reset acknowledged", config.LOG_LEVEL)
                
                elif command == "stats":
                    send_log(node, "INFO", f"Total syntheses: {total_syntheses}", config.LOG_LEVEL)
                    send_log(node, "INFO", f"Total audio duration: {total_duration:.1f}s", config.LOG_LEVEL)
        
        elif event["type"] == "STOP":
            break
    
    send_log(node, "INFO", "PrimeSpeech node stopped", config.LOG_LEVEL)


if __name__ == "__main__":
    main()
