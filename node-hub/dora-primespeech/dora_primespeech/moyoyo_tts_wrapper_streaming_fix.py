#!/usr/bin/env python3
"""Fixed Moxin Voice wrapper with real streaming support via audio chunking."""

import sys
import os
import logging
from pathlib import Path
import numpy as np
import soundfile as sf
import importlib
import importlib.util
from .audio_effects import apply_decoupled_speed_pitch_volume

# Force single-threaded operations on macOS to prevent Accelerate BLAS deadlocks
# when PyTorch models are reloaded or ref audio is re-processed.
if sys.platform == "darwin":
    os.environ.setdefault("VECLIB_MAXIMUM_THREADS", "1")
    os.environ.setdefault("OPENBLAS_NUM_THREADS", "1")
    try:
        import torch
        torch.set_num_threads(1)
    except ImportError:
        pass


def _ensure_langsegment_compatibility():
    """Ensure newer LangSegment releases work with legacy Moxin imports."""
    try:
        import LangSegment  # noqa: F401  # Trigger default import
    except ImportError as exc:
        # Newer releases removed setLangfilters; fall back to bundled shim
        if "setLangfilters" in str(exc):
            fix_path = Path(__file__).parent / "moyoyo_tts" / "LangSegment_fix.py"
            spec = importlib.util.spec_from_file_location(
                "LangSegment.LangSegment",
                str(fix_path),
            )
            langseg_module = importlib.util.module_from_spec(spec)
            sys.modules['LangSegment'] = langseg_module
            sys.modules['LangSegment.LangSegment'] = langseg_module
            spec.loader.exec_module(langseg_module)  # type: ignore[attr-defined]
        else:
            raise
    else:
        # Module imported successfully; alias missing helper if needed
        import LangSegment.LangSegment as langseg_module  # type: ignore

        if (not hasattr(langseg_module, "setLangfilters")
                and hasattr(langseg_module, "setfilters")):
            langseg_module.setLangfilters = langseg_module.setfilters  # type: ignore[attr-defined]

        if hasattr(langseg_module, "LangSegment"):
            langseg_class = langseg_module.LangSegment
            if (not hasattr(langseg_class, "setLangfilters")
                    and hasattr(langseg_class, "setfilters")):
                langseg_class.setLangfilters = langseg_class.setfilters  # type: ignore[attr-defined]
from typing import Generator, Tuple, Optional
import re

# Setup module logger
logger = logging.getLogger(__name__)

# Check if Moxin Voice is available
MOXIN_AVAILABLE = False

# Try to import Moxin Voice
try:
    # Add local moyoyo_tts to path
    local_moyoyo_path = Path(__file__).parent
    if local_moyoyo_path.exists() and str(local_moyoyo_path) not in sys.path:
        sys.path.insert(0, str(local_moyoyo_path))
        logger.debug(f"Using local moyoyo_tts from: {local_moyoyo_path}")

    # CRITICAL: Add moyoyo_tts subdirectory to sys.path so that "import text.chinese2" works
    # The text module imports use absolute paths like "text.chinese2"
    moyoyo_tts_dir = local_moyoyo_path / "moyoyo_tts"
    if moyoyo_tts_dir.exists() and str(moyoyo_tts_dir) not in sys.path:
        sys.path.insert(0, str(moyoyo_tts_dir))
        logger.debug(f"Added moyoyo_tts subdirectory to path: {moyoyo_tts_dir}")

    # Ensure LangSegment compatibility across versions
    _ensure_langsegment_compatibility()

    # Import Moxin Voice
    from moyoyo_tts.TTS_infer_pack.TTS import TTS_Config, TTS
    from moyoyo_tts.TTS_infer_pack.text_segmentation_method import get_method as get_seg_method
    from moyoyo_tts.utils import HParams

    if "utils" not in sys.modules:
        import moyoyo_tts.utils as _moyoyo_utils
        sys.modules['utils'] = _moyoyo_utils

    MOXIN_AVAILABLE = True
    logger.info("Moxin Voice successfully imported")
except ImportError as e:
    logger.error(f"Failed to import Moxin Voice: {e}")
    MOXIN_AVAILABLE = False


class StreamingMoxinTTSWrapper:
    """Fixed wrapper for Moxin Voice with real streaming via audio chunking."""
    
    def __init__(self, voice="doubao", device="cpu", enable_streaming=True, chunk_duration=0.5, models_path=None, voice_config=None, logger_func=None):
        """Initialize streaming Moxin Voice wrapper.
        
        Args:
            voice: Voice name (doubao, luoxiang, yangmi, etc.)
            device: Device to use (cpu or cuda)
            enable_streaming: Enable streaming output
            chunk_duration: Duration of each audio chunk in seconds for streaming
            models_path: Optional path to models directory
            voice_config: Optional voice configuration dict from config.py
            logger_func: Optional logging function (e.g., send_log)
        """
        self.voice = voice
        self.device = device
        self.enable_streaming = enable_streaming
        self.chunk_duration = chunk_duration  # Duration of each streamed chunk
        self.tts = None
        self.voice_config = voice_config  # Store the config from config.py
        self.logger_func = logger_func  # Logging function
        
        # Use PRIMESPEECH_MODEL_DIR for all model paths
        if os.environ.get("PRIMESPEECH_MODEL_DIR"):
            # Use environment variable path with moyoyo subdirectory
            self.models_path = Path(os.path.expanduser(os.environ.get("PRIMESPEECH_MODEL_DIR"))) / "moyoyo"
        else:
            # No models path available
            raise RuntimeError("No models path available. Please set PRIMESPEECH_MODEL_DIR environment variable.")
        
        # Abort flag for interrupting synthesis
        self._abort_synthesis = False
        
        # Optimization parameters - disable Moxin's broken "streaming"
        self.optimization_config = {
            "batch_size": 100,  # Smaller batches for faster first output
            "text_split_method": "cut5",  # Automatic segmentation
            "split_bucket": True,  # Enable bucketing
            "return_fragment": False,  # DISABLE Moxin's broken streaming
            # "fragment_interval": 0.07,
            "parallel_infer": False,
            "top_k": 5,
            "top_p": 1,
            "temperature": 1,
            # "repetition_penalty": 1.35,
            "seed": 233333,
        }
        
        if MOXIN_AVAILABLE:
            self._init_tts()
    
    def log(self, level, message):
        """Log a message using the provided logger function or print.

        Uses stderr as primary output to avoid blocking on dora node.send_output()
        which can hang on macOS after model reload.
        """
        print(f"[TTS-{level}] {message}", file=sys.stderr, flush=True)
        if self.logger_func:
            try:
                self.logger_func(level, message)
            except Exception:
                pass  # Don't block on dora logging failures
    
    def _init_tts(self):
        """Initialize the TTS engine."""
        if not MOXIN_AVAILABLE:
            self.log("ERROR", "Moxin Voice not available - cannot initialize TTS engine")
            self.log("ERROR", f"MOXIN_AVAILABLE: {MOXIN_AVAILABLE}")
            self.log("ERROR", f"models_path: {self.models_path}")
            return
        
        # Use provided voice_config or fall back to hardcoded defaults
        if self.voice_config:
            # Use configuration from config.py
            voice_config = {
                "t2s_weights": self.voice_config.get("gpt_weights", f"GPT_weights/{self.voice}_best_gpt.ckpt"),
                "vits_weights": self.voice_config.get("sovits_weights", f"SoVITS_weights/{self.voice}_best_sovits.pth"),
                "ref_audio": self.voice_config.get("reference_audio", f"ref_audios/{self.voice}_ref.wav"),
                "prompt_text": self.voice_config.get("prompt_text", "你好，很高兴见到你。"),
            }
        else:
            # Fall back to hardcoded configurations for backward compatibility
            voice_configs = {
                "doubao": {
                    "t2s_weights": "GPT_weights/doubao-mixed.ckpt",
                    "vits_weights": "SoVITS_weights/doubao-mixed.pth",
                    "ref_audio": "ref_audios/doubao_ref_mix_new.wav",
                    "prompt_text": "这家resturant的steak很有名，但是vegetable salad的price有点贵",
                },
                "luoxiang": {
                    "t2s_weights": "GPT_weights/luoxiang_best_gpt.ckpt",
                    "vits_weights": "SoVITS_weights/luoxiang_best_sovits.pth",
                    "ref_audio": "ref_audios/luoxiang_ref.wav",
                    "prompt_text": "我觉得你的逻辑非常混乱，这已经涉及法外狂徒了。希望大家提高法律意识，培养法制观念，千万不要有侥幸的心理。",
                },
                "yangmi": {
                    "t2s_weights": "GPT_weights/yangmi_best_gpt.ckpt",
                    "vits_weights": "SoVITS_weights/yangmi_best_sovits.pth",
                    "ref_audio": "ref_audios/yangmi_ref.wav",
                    "prompt_text": "让梦想，照进现实。不管在什么情况下，只要是坚定的，永远相信美好的事情，即将发生。",
                },
            }
            
            voice_config = voice_configs.get(self.voice, voice_configs["doubao"])
        
        # Create TTS configuration
        # Check if reference audio is an absolute path (custom voice) or relative path (built-in voice)
        ref_audio_path = voice_config["ref_audio"]
        if Path(ref_audio_path).is_absolute():
            # Custom voice - use absolute path directly
            self.ref_audio_path = ref_audio_path
        else:
            # Built-in voice - relative to models_path
            self.ref_audio_path = str(self.models_path / ref_audio_path)

        custom_config = {
            "device": self.device,
            "is_half": self.device not in ["cpu"],  # Use FP16 for CUDA and MPS
            "version": "v2",
            "t2s_weights_path": str(self.models_path / voice_config["t2s_weights"]),
            "vits_weights_path": str(self.models_path / voice_config["vits_weights"]),
            "cnhuhbert_base_path": str(self.models_path / "chinese-hubert-base"),
            "bert_base_path": str(self.models_path / "chinese-roberta-wwm-ext-large"),
        }
        
        config_dict = {
            "version": "v2",
            "custom": custom_config
        }
        
        try:
            self.log("INFO", f"Initializing Moxin Voice with voice: {self.voice}")
            self.log("INFO", f"Model paths:")
            self.log("INFO", f"  t2s_weights: {custom_config['t2s_weights_path']}")
            self.log("INFO", f"  vits_weights: {custom_config['vits_weights_path']}")
            self.log("INFO", f"  cnhuhbert_base: {custom_config['cnhuhbert_base_path']}")
            self.log("INFO", f"  bert_base: {custom_config['bert_base_path']}")
            
            # Check if model files exist
            for key, path in custom_config.items():
                if 'path' in key and not Path(path).exists():
                    self.log("ERROR", f"Model file does not exist: {path}")
            
            self.tts = TTS(config_dict)
            
            # Store prompt text (ref_audio_path already set above)
            self.prompt_text = voice_config["prompt_text"]
            
            self.log("INFO", f"Reference audio: {self.ref_audio_path}")
            if not Path(self.ref_audio_path).exists():
                self.log("ERROR", f"Reference audio does not exist: {self.ref_audio_path}")
            
            # Pre-cache reference audio
            self.tts.set_ref_audio(self.ref_audio_path)
            
            self.log("INFO", "Moxin Voice initialized successfully")
        except Exception as e:
            self.log("ERROR", f"Failed to initialize Moxin Voice: {e}")
            import traceback
            self.log("ERROR", traceback.format_exc())
            self.tts = None
    
    def _split_text_smartly(self, text, max_chunk_chars=50):
        """Split text into smaller chunks for progressive synthesis.
        
        Args:
            text: Input text
            max_chunk_chars: Maximum characters per chunk
            
        Returns:
            list: Text chunks
        """
        # First split by sentences
        sentence_delimiters = r'[。！？.!?；;]'
        sentences = re.split(f'({sentence_delimiters})', text)
        
        # Reconstruct sentences with their delimiters
        reconstructed = []
        for i in range(0, len(sentences), 2):
            if i + 1 < len(sentences):
                reconstructed.append(sentences[i] + sentences[i + 1])
            else:
                reconstructed.append(sentences[i])
        
        # Group sentences into chunks
        chunks = []
        current_chunk = ""
        
        for sentence in reconstructed:
            sentence = sentence.strip()
            if not sentence:
                continue
                
            if len(current_chunk) + len(sentence) <= max_chunk_chars:
                current_chunk += sentence
            else:
                if current_chunk:
                    chunks.append(current_chunk)
                
                # If single sentence is too long, split by commas
                if len(sentence) > max_chunk_chars:
                    comma_parts = re.split(r'([，,])', sentence)
                    sub_chunk = ""
                    for part in comma_parts:
                        if len(sub_chunk) + len(part) <= max_chunk_chars:
                            sub_chunk += part
                        else:
                            if sub_chunk:
                                chunks.append(sub_chunk)
                            sub_chunk = part
                    if sub_chunk:
                        chunks.append(sub_chunk)
                else:
                    current_chunk = sentence
        
        if current_chunk:
            chunks.append(current_chunk)
        
        return chunks
    
    def _chunk_audio(self, audio_data, sample_rate):
        """Split audio into smaller chunks for streaming.
        
        Args:
            audio_data: Audio numpy array
            sample_rate: Sample rate
            
        Yields:
            Audio chunks
        """
        chunk_samples = int(sample_rate * self.chunk_duration)
        total_samples = len(audio_data)
        
        for start_idx in range(0, total_samples, chunk_samples):
            end_idx = min(start_idx + chunk_samples, total_samples)
            chunk = audio_data[start_idx:end_idx]
            
            # Apply fade in/out to avoid clicks
            if len(chunk) > 100:
                # Fade in for first 10 samples
                if start_idx == 0:
                    fade_len = min(10, len(chunk) // 2)
                    fade_in = np.linspace(0, 1, fade_len)
                    chunk[:fade_len] *= fade_in
                
                # Fade out for last 10 samples
                if end_idx == total_samples:
                    fade_len = min(10, len(chunk) // 2)
                    fade_out = np.linspace(1, 0, fade_len)
                    chunk[-fade_len:] *= fade_out
            
            yield chunk
    
    def abort_synthesis(self):
        """Abort any ongoing synthesis."""
        self._abort_synthesis = True
        self.log("INFO", "Synthesis abort requested")
    
    def synthesize_streaming(self, text, language="zh", speed=1.0, pitch=0.0, volume=100.0) -> Generator[Tuple[int, np.ndarray], None, None]:
        """Synthesize speech with real streaming output.
        
        Args:
            text: Text to synthesize
            language: Language code
            speed: Speed factor
            pitch: Pitch shift in semitones
            volume: Output volume in percent
        
        Yields:
            tuple: (sample_rate, audio_fragment) for each fragment
        """
        if not MOXIN_AVAILABLE or self.tts is None:
            self.log("ERROR", "Moxin Voice not available - cannot synthesize")
            if not MOXIN_AVAILABLE:
                self.log("ERROR", "MOXIN_AVAILABLE is False - TTS libraries not imported")
            if self.tts is None:
                self.log("ERROR", "self.tts is None - TTS engine not initialized")
                self.log("ERROR", f"models_path: {self.models_path}")
            raise RuntimeError("TTS engine not available. Check model paths and configuration.")
            return
        
        # Reset abort flag at start of new synthesis (safe timing)
        self._abort_synthesis = False
        
        try:
            self.log("INFO", f"Starting streaming synthesis for {len(text)} chars")
            
            # Split text into smaller chunks for progressive synthesis
            text_chunks = self._split_text_smartly(text, max_chunk_chars=40)
            self.log("INFO", f"Split into {len(text_chunks)} text chunks")
            
            fragment_count = 0
            
            # Process each text chunk
            for chunk_idx, text_chunk in enumerate(text_chunks):
                # Check abort flag
                if self._abort_synthesis:
                    self.log("INFO", f"Synthesis aborted at text chunk {chunk_idx + 1}/{len(text_chunks)}")
                    break
                if not text_chunk.strip():
                    continue
                
                self.log("INFO", f"[STREAM] Processing chunk {chunk_idx + 1}: {text_chunk[:30]}...")

                # Prepare inputs without Moxin's broken streaming
                inputs = {
                    "text": text_chunk,
                    "text_lang": language,
                    "ref_audio_path": self.ref_audio_path,
                    "prompt_text": self.prompt_text,
                    "prompt_lang": "zh",
                    # Keep model synthesis speed fixed. Runtime speed/pitch/volume are
                    # applied in post-process to keep controls decoupled.
                    "speed_factor": 1.0,
                    "return_fragment": False,  # Don't use Moxin's broken streaming
                    **self.optimization_config
                }

                # Generate audio for this text chunk
                self.log("INFO", f"[STREAM] Calling tts.run() for chunk {chunk_idx + 1}...")
                try:
                    for result in self.tts.run(inputs):
                        sample_rate, chunk_audio = result
                        self.log("INFO", f"[STREAM] Got chunk {chunk_idx + 1}: {len(chunk_audio)/sample_rate:.2f}s")
                        break  # Only yields once when return_fragment=False
                except Exception as chunk_error:
                    self.log("ERROR", f"[STREAM] Chunk {chunk_idx + 1} failed: {chunk_error}")
                    import traceback
                    self.log("ERROR", f"[STREAM] Traceback: {traceback.format_exc()}")
                    raise
                
                # Convert to float32 if needed
                if chunk_audio.dtype == np.int16:
                    chunk_audio = chunk_audio.astype(np.float32) / 32768.0
                else:
                    chunk_audio = chunk_audio.astype(np.float32, copy=False)

                chunk_audio = apply_decoupled_speed_pitch_volume(
                    chunk_audio,
                    sample_rate=sample_rate,
                    speed=speed,
                    pitch=pitch,
                    volume=volume,
                    logger=self.log,
                )
                
                # Stream this chunk's audio in smaller pieces
                for audio_fragment in self._chunk_audio(chunk_audio, sample_rate):
                    # Check abort flag before yielding each fragment
                    if self._abort_synthesis:
                        self.log("INFO", f"Synthesis aborted at audio fragment {fragment_count + 1}")
                        return  # Exit generator completely
                    
                    fragment_count += 1
                    self.log("DEBUG", f"Yielding fragment {fragment_count}: {len(audio_fragment)/sample_rate:.3f}s")
                    yield sample_rate, audio_fragment
            
            self.log("INFO", f"Streaming complete: {fragment_count} fragments")
            
        except Exception as e:
            self.log("ERROR", f"Streaming synthesis failed: {e}")
            raise

    def _clean_text(self, text: str) -> str:
        """去除文本中的中英文标点符号。"""
        # 去除中英文标点符号，保留字母、数字、下划线、中文和空格
        return re.sub(r'[^\w\s\u4e00-\u9fa5]', ' ', text)

    def synthesize(self, text, language="zh", speed=1.1, pitch=0.0, volume=100.0, fragment_interval=0.07):
        """Synthesize speech from text (non-streaming).
        
        Args:
            text: Text to synthesize
            language: Language code
            speed: Speed factor
            pitch: Pitch shift in semitones
            volume: Output volume in percent
        
        Returns:
            tuple: (sample_rate, audio_data) or (None, None) if failed
        """
        # Reset abort flag at start of new synthesis (safe timing)
        self._abort_synthesis = False
        
        if not MOXIN_AVAILABLE or self.tts is None:
            self.log("ERROR", "Moxin Voice not available - cannot synthesize")
            if not MOXIN_AVAILABLE:
                self.log("ERROR", "MOXIN_AVAILABLE is False - TTS libraries not imported")
            if self.tts is None:
                self.log("ERROR", "self.tts is None - TTS engine not initialized")
                self.log("ERROR", f"models_path: {self.models_path}")
            raise RuntimeError("TTS engine not available. Check model paths and configuration.")
        
        try:
            # Prepare inputs
            # text = self._clean_text(text)
            inputs = {
                "text": text,
                "text_lang": language,
                "ref_audio_path": self.ref_audio_path,
                "prompt_text": self.prompt_text,
                "prompt_lang": "zh",
                # Keep model synthesis speed fixed. Runtime speed/pitch/volume are
                # applied in post-process to keep controls decoupled.
                "speed_factor": 1.0,
                "return_fragment": False,
                'fragment_interval': fragment_interval,
                **self.optimization_config
            }
            
            self.log("INFO", f"[SYNTHESIS START] Synthesizing {len(text)} chars")
            self.log("INFO", f"[SYNTHESIS] ref_audio: {self.ref_audio_path}")
            self.log("INFO", f"[SYNTHESIS] prompt_text: {self.prompt_text[:50]}...")
            self.log("INFO", f"[SYNTHESIS] text_lang: {language}")

            # Generate audio
            import sys
            self.log("INFO", "[SYNTHESIS] Calling self.tts.run()...")
            print(f"DEBUG [WRAPPER]: About to call self.tts.run() with inputs:", file=sys.stderr, flush=True)
            print(f"  text='{inputs['text'][:80]}...' (len={len(inputs['text'])})", file=sys.stderr, flush=True)
            print(f"  text_lang={inputs.get('text_lang')}", file=sys.stderr, flush=True)
            print(f"  ref_audio_path={inputs.get('ref_audio_path', 'N/A')[:80]}", file=sys.stderr, flush=True)
            print(f"  prompt_text='{inputs.get('prompt_text', 'N/A')[:50]}...'", file=sys.stderr, flush=True)
            print(f"  prompt_lang={inputs.get('prompt_lang')}", file=sys.stderr, flush=True)
            print(f"  speed_factor={inputs.get('speed_factor')}", file=sys.stderr, flush=True)
            print(f"  return_fragment={inputs.get('return_fragment')}", file=sys.stderr, flush=True)
            print(f"  fragment_interval={inputs.get('fragment_interval')}", file=sys.stderr, flush=True)
            print(f"  text_split_method={inputs.get('text_split_method')}", file=sys.stderr, flush=True)
            print(f"  split_bucket={inputs.get('split_bucket')}", file=sys.stderr, flush=True)
            print(f"  batch_size={inputs.get('batch_size')}", file=sys.stderr, flush=True)
            print(f"  All input keys: {list(inputs.keys())}", file=sys.stderr, flush=True)
            audio_chunks = []
            sample_rate = None
            chunk_count = 0
            try:
                for result in self.tts.run(inputs):
                    sr, audio_chunk = result
                    chunk_count += 1
                    if sample_rate is None:
                        sample_rate = sr
                    chunk_len = len(audio_chunk) if audio_chunk is not None else 0
                    print(f"DEBUG [WRAPPER]: Chunk {chunk_count}: sr={sr}, len={chunk_len} samples ({chunk_len/sr:.2f}s)", file=sys.stderr, flush=True)
                    self.log("INFO", f"[SYNTHESIS] Got chunk {chunk_count}: sr={sr}, audio_shape={audio_chunk.shape if audio_chunk is not None else 'None'}")
                    if audio_chunk is not None and len(audio_chunk) > 0:
                        audio_chunks.append(audio_chunk)
                    # Don't break - collect ALL chunks!

                print(f"DEBUG [WRAPPER]: tts.run() finished. Total chunks collected: {chunk_count}", file=sys.stderr, flush=True)
                self.log("INFO", f"[SYNTHESIS] Collected {chunk_count} chunks total")
            except Exception as run_error:
                self.log("ERROR", f"[SYNTHESIS] self.tts.run() failed: {run_error}")
                import traceback
                self.log("ERROR", f"[SYNTHESIS] Traceback: {traceback.format_exc()}")
                raise

            if not audio_chunks:
                print(f"DEBUG [WRAPPER]: ERROR - No audio chunks collected!", file=sys.stderr, flush=True)
                self.log("ERROR", "[SYNTHESIS] No audio chunks collected after tts.run()")
                return None, None

            # Concatenate all chunks
            if len(audio_chunks) == 1:
                audio_data = audio_chunks[0]
                print(f"DEBUG [WRAPPER]: Single chunk, length={len(audio_data)} samples", file=sys.stderr, flush=True)
            else:
                audio_data = np.concatenate(audio_chunks)
                print(f"DEBUG [WRAPPER]: Concatenated {len(audio_chunks)} chunks into {len(audio_data)} samples", file=sys.stderr, flush=True)
                self.log("INFO", f"[SYNTHESIS] Concatenated {len(audio_chunks)} chunks into {len(audio_data)} samples")

            # Convert to float32 if needed
            if audio_data.dtype == np.int16:
                audio_data = audio_data.astype(np.float32) / 32768.0
            else:
                audio_data = audio_data.astype(np.float32, copy=False)

            audio_data = apply_decoupled_speed_pitch_volume(
                audio_data,
                sample_rate=sample_rate,
                speed=speed,
                pitch=pitch,
                volume=volume,
                logger=self.log,
            )

            self.log("INFO", f"[SYNTHESIS COMPLETE] Generated {len(audio_data)/sample_rate:.2f}s audio")
            return sample_rate, audio_data
            
        except Exception as e:
            self.log("ERROR", f"TTS synthesis failed: {e}")
            raise


# Test function
def test_streaming_fix():
    """Test the fixed streaming wrapper."""
    import time
    import sounddevice as sd
    
    print("\n" + "=" * 60)
    print("Testing Fixed Streaming Moxin Voice Wrapper")
    print("=" * 60)
    
    # Initialize wrapper
    tts = StreamingMoxinTTSWrapper(
        voice="doubao", 
        device="cpu", 
        enable_streaming=True,
        chunk_duration=0.3  # 300ms chunks
    )
    
    # Test text
    test_text = "你好，这是真正的流式语音合成测试。人工智能技术正在改变我们的世界，让机器能够理解和生成人类语言。"
    
    print(f"\nTest text: {test_text}")
    print("\nStarting streaming synthesis...")
    
    start_time = time.time()
    fragment_count = 0
    first_fragment_time = None
    
    for sample_rate, audio_fragment in tts.synthesize_streaming(test_text, language="zh", speed=1.1):
        fragment_count += 1
        elapsed = time.time() - start_time
        
        if fragment_count == 1:
            first_fragment_time = elapsed
            print(f"\n🎯 First fragment at {elapsed:.3f}s")
            if elapsed < 1.0:
                print("✅ EXCELLENT - Under 1 second!")
            elif elapsed < 2.0:
                print("👍 GOOD - Under 2 seconds")
            else:
                print("⚠️  SLOW - Over 2 seconds")
        
        duration = len(audio_fragment) / sample_rate
        print(f"  Fragment {fragment_count}: {duration:.3f}s audio at {elapsed:.2f}s")
        
        # Play the audio
        sd.play(audio_fragment, sample_rate)
        
        if fragment_count >= 10:
            print("  ... (stopping after 10 fragments)")
            break
    
    total_time = time.time() - start_time
    
    print("\n" + "=" * 60)
    print("Results:")
    print(f"  Total fragments: {fragment_count}")
    print(f"  First fragment latency: {first_fragment_time:.3f}s" if first_fragment_time else "  No fragments")
    print(f"  Total time: {total_time:.2f}s")
    
    if fragment_count > 1:
        print("\n✅ REAL STREAMING IS WORKING!")
    else:
        print("\n❌ Still not streaming properly")
    
    print("=" * 60)
    
    # Wait for audio to finish
    sd.wait()


if __name__ == "__main__":
    test_streaming_fix()
