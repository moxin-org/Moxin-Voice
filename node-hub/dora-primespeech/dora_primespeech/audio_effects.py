"""Audio post-processing for independent speed/pitch/volume control."""

from __future__ import annotations

from typing import Callable, Optional, Tuple

import numpy as np

try:
    import librosa
except Exception:  # pragma: no cover - optional dependency
    librosa = None

try:
    import ffmpeg
except Exception:  # pragma: no cover - optional dependency
    ffmpeg = None


Logger = Optional[Callable[[str, str], None]]


def _log(logger: Logger, level: str, message: str) -> None:
    if logger is None:
        return
    try:
        logger(level, message)
    except Exception:
        pass


def _clamp(value: float, minimum: float, maximum: float) -> float:
    return max(minimum, min(maximum, value))


def _time_stretch_ffmpeg(
    audio: np.ndarray,
    sample_rate: int,
    speed: float,
    logger: Logger,
) -> Tuple[np.ndarray, bool]:
    """Pitch-preserving speed change using ffmpeg atempo."""
    if ffmpeg is None:
        _log(logger, "WARNING", "ffmpeg-python unavailable, skipping atempo transform")
        return audio, False

    raw_audio = (np.clip(audio, -1.0, 1.0) * 32767.0).astype(np.int16).tobytes()
    input_stream = ffmpeg.input("pipe:", format="s16le", acodec="pcm_s16le", ar=str(sample_rate), ac=1)
    output_stream = input_stream.filter("atempo", speed)

    try:
        out, _ = output_stream.output(
            "pipe:",
            format="s16le",
            acodec="pcm_s16le",
            ar=str(sample_rate),
            ac=1,
        ).run(input=raw_audio, capture_stdout=True, capture_stderr=True)
    except Exception as exc:
        _log(logger, "WARNING", "ffmpeg atempo failed: {}".format(exc))
        return audio, False

    if not out:
        return audio, False
    return np.frombuffer(out, dtype=np.int16).astype(np.float32) / 32768.0, True


def apply_decoupled_speed_pitch_volume(
    audio: np.ndarray,
    sample_rate: int,
    speed: float = 1.0,
    pitch: float = 0.0,
    volume: float = 100.0,
    logger: Logger = None,
) -> np.ndarray:
    """Apply pitch (semitones), speed, and volume in a decoupled pipeline."""
    if audio is None:
        return audio

    processed = np.asarray(audio, dtype=np.float32)
    if processed.ndim != 1:
        processed = np.ravel(processed)
    if processed.size == 0 or sample_rate <= 0:
        return processed

    speed = _clamp(float(speed), 0.5, 2.0)
    pitch = _clamp(float(pitch), -12.0, 12.0)
    volume = _clamp(float(volume), 0.0, 200.0)

    apply_speed = abs(speed - 1.0) > 1e-3
    apply_pitch = abs(pitch) > 1e-3
    apply_volume = abs(volume - 100.0) > 1e-3

    if apply_pitch:
        if librosa is None:
            _log(logger, "WARNING", "librosa unavailable, skipping pitch transform")
        else:
            try:
                processed = librosa.effects.pitch_shift(
                    processed,
                    sr=sample_rate,
                    n_steps=pitch,
                    bins_per_octave=12,
                )
            except Exception as exc:
                _log(logger, "WARNING", "pitch shift failed: {}".format(exc))

    if apply_speed:
        processed, ffmpeg_applied = _time_stretch_ffmpeg(processed, sample_rate, speed, logger)
        if not ffmpeg_applied and librosa is not None:
            try:
                processed = librosa.effects.time_stretch(processed, rate=speed)
            except Exception as exc:
                _log(logger, "WARNING", "time stretch failed: {}".format(exc))

    if apply_volume:
        gain = volume / 100.0
        processed = processed * gain

    processed = np.nan_to_num(processed)
    return np.clip(processed, -1.0, 1.0).astype(np.float32, copy=False)

