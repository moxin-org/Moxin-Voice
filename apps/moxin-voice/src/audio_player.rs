//! Audio Player Module - Circular buffer audio playback using cpal
//!
//! Adapted from moxin-debate/conference-dashboard for continuous TTS streaming.

use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};
use crossbeam_channel::{unbounded, Receiver, Sender};
use parking_lot::Mutex;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

/// Commands sent to the audio thread
enum AudioCommand {
    Write(Vec<f32>), // Append samples
    Reset,           // Clear playing buffer
    Pause,
    Resume,
    #[allow(dead_code)]
    Stop,            // Reserved for explicit thread shutdown
}

/// Circular audio buffer for thread-safe audio streaming
struct CircularAudioBuffer {
    buffer: Vec<f32>,
    write_pos: usize,
    read_pos: usize,
    available_samples: usize,
    buffer_size: usize,
    /// Counter for samples dropped due to buffer overflow
    dropped_samples: usize,
}

impl CircularAudioBuffer {
    fn new(size_seconds: f32, sample_rate: u32) -> Self {
        let buffer_size = (size_seconds * sample_rate as f32) as usize;
        Self {
            buffer: vec![0.0; buffer_size],
            write_pos: 0,
            read_pos: 0,
            available_samples: 0,
            buffer_size,
            dropped_samples: 0,
        }
    }

    fn write(&mut self, samples: &[f32]) -> usize {
        let mut written = 0;
        let mut dropped_in_write = 0;

        for &sample in samples {
            if self.available_samples < self.buffer_size {
                self.buffer[self.write_pos] = sample;
                self.write_pos = (self.write_pos + 1) % self.buffer_size;
                self.available_samples += 1;
                written += 1;
            } else {
                // Buffer full - overwrite oldest (ring buffer behavior)
                // Ideally this shouldn't happen if consumer is fast enough
                self.buffer[self.write_pos] = sample;
                self.write_pos = (self.write_pos + 1) % self.buffer_size;
                self.read_pos = (self.read_pos + 1) % self.buffer_size;
                self.dropped_samples += 1;
                dropped_in_write += 1;
                written += 1;
            }
        }

        // Log warning if samples were dropped
        if dropped_in_write > 0 {
            log::warn!(
                "Audio buffer overflow: dropped {} samples (total dropped: {})",
                dropped_in_write,
                self.dropped_samples
            );
        }

        written
    }

    fn sample_at_offset(&self, offset: usize) -> Option<f32> {
        if offset >= self.available_samples {
            return None;
        }
        Some(self.buffer[(self.read_pos + offset) % self.buffer_size])
    }

    fn consume(&mut self, count: usize) -> usize {
        let consumed = count.min(self.available_samples);
        if consumed > 0 {
            self.read_pos = (self.read_pos + consumed) % self.buffer_size;
            self.available_samples -= consumed;
        }
        consumed
    }

    fn reset(&mut self) {
        self.write_pos = 0;
        self.read_pos = 0;
        self.available_samples = 0;
        self.dropped_samples = 0;
    }

    fn available(&self) -> usize {
        self.available_samples
    }
}

struct PlaybackResampler {
    playback_rate: f32,
    source_pos: f32,
}

impl PlaybackResampler {
    fn new(playback_rate: f32) -> Self {
        Self {
            playback_rate: playback_rate.max(0.000_001),
            source_pos: 0.0,
        }
    }

    fn reset(&mut self) {
        self.source_pos = 0.0;
    }

    fn render_mono(&mut self, buffer: &mut CircularAudioBuffer, output: &mut [f32]) -> usize {
        if output.is_empty() {
            return 0;
        }
        if buffer.available() == 0 {
            self.reset();
            output.fill(0.0);
            return 0;
        }

        let mut produced = 0usize;
        for sample in output.iter_mut() {
            let idx = self.source_pos.floor().max(0.0) as usize;
            let frac = self.source_pos - idx as f32;
            if let Some(s0) = buffer.sample_at_offset(idx) {
                let s1 = buffer.sample_at_offset(idx + 1).unwrap_or(s0);
                *sample = s0 + frac * (s1 - s0);
                produced += 1;
            } else {
                *sample = 0.0;
            }
            self.source_pos += self.playback_rate;
        }

        let complete_source_samples = self.source_pos.floor().max(0.0) as usize;
        let consumed = buffer.consume(complete_source_samples);
        self.source_pos -= consumed as f32;
        if buffer.available() == 0 {
            self.reset();
        }

        produced
    }
}

/// Shared state between audio thread and main thread
pub struct SharedAudioState {
    pub buffer_fill: f64,
    pub is_playing: bool,
    pub output_waveform: Vec<f32>, // Samples currently being played (for visualization)
}

/// Audio player handle
#[derive(Clone)]
pub struct TTSPlayer {
    command_tx: Sender<AudioCommand>,
    state: Arc<Mutex<SharedAudioState>>,
    playback_finished: Arc<AtomicBool>,
    #[allow(dead_code)]
    sample_rate: u32, // Stored for future API needs
}

impl TTSPlayer {
    /// Create a new audio player that accepts audio at `source_sample_rate`.
    pub fn new(source_sample_rate: u32) -> Self {
        Self::new_with_output_device(source_sample_rate, None)
    }

    pub fn new_with_output_device(source_sample_rate: u32, preferred_output_device: Option<&str>) -> Self {
        let sample_rate = source_sample_rate;
        let (command_tx, command_rx) = unbounded::<AudioCommand>();
        let preferred_output_device = preferred_output_device.map(|s| s.to_string());

        let state = Arc::new(Mutex::new(SharedAudioState {
            buffer_fill: 0.0,
            is_playing: false,
            output_waveform: vec![0.0; 512],
        }));

        let playback_finished = Arc::new(AtomicBool::new(false));
        let state_clone = Arc::clone(&state);
        let playback_finished_clone = Arc::clone(&playback_finished);

        std::thread::spawn(move || {
            if let Err(e) = run_audio_thread(
                sample_rate,
                preferred_output_device,
                command_rx,
                state_clone,
                playback_finished_clone,
            ) {
                eprintln!("Audio thread error: {}", e);
            }
        });

        Self {
            command_tx,
            state,
            playback_finished,
            sample_rate,
        }
    }

    /// Check if playback has finished (call this in handle_event to detect completion)
    pub fn check_playback_finished(&self) -> bool {
        self.playback_finished.swap(false, Ordering::AcqRel)
    }

    /// Add audio samples to the buffer for streaming playback
    pub fn write_audio(&self, samples: &[f32]) {
        if samples.is_empty() {
            return;
        }
        let _ = self.command_tx.send(AudioCommand::Write(samples.to_vec()));
        let _ = self.command_tx.send(AudioCommand::Resume);
    }

    /// Reset playback (clear buffer)
    pub fn stop(&self) {
        let _ = self.command_tx.send(AudioCommand::Reset);
    }

    pub fn pause(&self) {
        let _ = self.command_tx.send(AudioCommand::Pause);
    }

    pub fn resume(&self) {
        let _ = self.command_tx.send(AudioCommand::Resume);
    }

    pub fn is_playing(&self) -> bool {
        self.state.lock().is_playing
    }

    pub fn get_waveform_data(&self) -> Vec<f32> {
        self.state.lock().output_waveform.clone()
    }
}

pub fn list_output_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut devices = Vec::new();
    if let Ok(iter) = host.output_devices() {
        for dev in iter {
            if let Ok(name) = dev.name() {
                devices.push(name);
            }
        }
    }
    devices.sort();
    devices.dedup();
    devices
}

pub fn default_output_device_name() -> Option<String> {
    let host = cpal::default_host();
    host.default_output_device().and_then(|d| d.name().ok())
}

pub fn default_input_device_name() -> Option<String> {
    let host = cpal::default_host();
    host.default_input_device().and_then(|d| d.name().ok())
}

pub fn list_input_devices() -> Vec<String> {
    let host = cpal::default_host();
    let mut devices = Vec::new();
    if let Ok(iter) = host.input_devices() {
        for dev in iter {
            if let Ok(name) = dev.name() {
                devices.push(name);
            }
        }
    }
    devices.sort();
    devices.dedup();
    devices
}

/// Run the audio thread with cpal stream
fn run_audio_thread(
    sample_rate: u32,
    preferred_output_device: Option<String>,
    command_rx: Receiver<AudioCommand>,
    state: Arc<Mutex<SharedAudioState>>,
    playback_finished: Arc<AtomicBool>,
) -> Result<(), String> {
    let buffer_seconds = 400.0; // Large buffer for TTS (supports up to ~341s audio after resampling)
    let buffer = Arc::new(Mutex::new(CircularAudioBuffer::new(
        buffer_seconds,
        sample_rate,
    )));
    let is_playing = Arc::new(AtomicBool::new(false));

    let host = cpal::default_host();
    let device = if let Some(preferred) = preferred_output_device.as_deref() {
        host.output_devices()
            .ok()
            .and_then(|mut devices| {
                devices.find(|d| d.name().map(|n| n == preferred).unwrap_or(false))
            })
            .or_else(|| host.default_output_device())
            .ok_or_else(|| "No audio output device found".to_string())?
    } else {
        host.default_output_device()
            .ok_or_else(|| "No audio output device found".to_string())?
    };

    eprintln!(
        "Audio player started - device: {}",
        device.name().unwrap_or_default()
    );

    // Get default config
    let default_config = device.default_output_config().map_err(|e| e.to_string())?;
    let channels = default_config.channels();
    let config: cpal::StreamConfig = default_config.into();
    let stream_sample_rate = config.sample_rate.0;

    eprintln!(
        "Audio config: {} channels, {} Hz (source: {} Hz)",
        channels, stream_sample_rate, sample_rate
    );

    let buffer_clone = Arc::clone(&buffer);
    let is_playing_clone = Arc::clone(&is_playing);
    let _state_for_callback = Arc::clone(&state); // Unused, just for symmetry or if needed later
    let output_channels = channels as usize;
    let resampler_reset = Arc::new(AtomicBool::new(false));

    let playback_rate = sample_rate as f32 / stream_sample_rate as f32;

    // Helper to build stream with correct sample format.
    fn build_stream_for_format<T>(
        device: &cpal::Device,
        config: &cpal::StreamConfig,
        buffer: Arc<Mutex<CircularAudioBuffer>>,
        is_playing: Arc<AtomicBool>,
        state: Arc<Mutex<SharedAudioState>>,
        playback_finished: Arc<AtomicBool>,
        output_channels: usize,
        playback_rate: f32,
        resampler_reset: Arc<AtomicBool>,
    ) -> Result<cpal::Stream, cpal::BuildStreamError>
    where
        T: cpal::Sample + cpal::FromSample<f32> + cpal::SizedSample,
    {
        let mut resampler = PlaybackResampler::new(playback_rate);
        let mut mono = Vec::<f32>::new();

        device.build_output_stream(
            config,
            move |data: &mut [T], _: &cpal::OutputCallbackInfo| {
                if resampler_reset.swap(false, Ordering::AcqRel) {
                    resampler.reset();
                }

                if is_playing.load(Ordering::Relaxed) {
                    let frames = data.len() / output_channels;
                    if mono.len() < frames {
                        mono.resize(frames, 0.0);
                    }
                    let produced = {
                        let mut buf = buffer.lock();
                        resampler.render_mono(&mut buf, &mut mono[..frames])
                    };

                    if produced == 0 {
                        is_playing.store(false, Ordering::Relaxed);
                        playback_finished.store(true, Ordering::Release);
                        for sample in data.iter_mut() {
                            *sample = T::from_sample(0.0);
                        }
                        return;
                    }

                    for i in 0..frames {
                        let output_val = T::from_sample(mono[i]);
                        for ch in 0..output_channels {
                            data[i * output_channels + ch] = output_val;
                        }
                    }
                } else {
                    for sample in data.iter_mut() {
                        *sample = T::from_sample(0.0);
                    }
                }

                if let Some(mut s) = state.try_lock() {
                    s.is_playing = is_playing.load(Ordering::Relaxed);
                }
            },
            |err| eprintln!("Stream error: {}", err),
            None,
        )
    }

    // Select format
    let stream_result = match device.default_output_config().unwrap().sample_format() {
        cpal::SampleFormat::F32 => build_stream_for_format::<f32>(
            &device,
            &config,
            buffer_clone,
            is_playing_clone,
            Arc::clone(&state),
            Arc::clone(&playback_finished),
            output_channels,
            playback_rate,
            Arc::clone(&resampler_reset),
        ),
        cpal::SampleFormat::I16 => build_stream_for_format::<i16>(
            &device,
            &config,
            buffer_clone,
            is_playing_clone,
            Arc::clone(&state),
            Arc::clone(&playback_finished),
            output_channels,
            playback_rate,
            Arc::clone(&resampler_reset),
        ),
        cpal::SampleFormat::U16 => build_stream_for_format::<u16>(
            &device,
            &config,
            buffer_clone,
            is_playing_clone,
            Arc::clone(&state),
            Arc::clone(&playback_finished),
            output_channels,
            playback_rate,
            Arc::clone(&resampler_reset),
        ),
        _ => build_stream_for_format::<f32>(
            &device,
            &config,
            buffer_clone,
            is_playing_clone,
            Arc::clone(&state),
            Arc::clone(&playback_finished),
            output_channels,
            playback_rate,
            Arc::clone(&resampler_reset),
        ),
    };

    let stream = stream_result.map_err(|e| e.to_string())?;
    stream.play().map_err(|e| e.to_string())?;

    loop {
        match command_rx.recv() {
            Ok(AudioCommand::Write(samples)) => {
                let mut buf = buffer.lock();
                if !samples.is_empty() {
                    buf.write(&samples);
                    playback_finished.store(false, Ordering::Release);
                }
                // Auto-start immediately whenever new samples arrive.
                if buf.available() > 0 {
                    is_playing.store(true, Ordering::Relaxed);
                }
            }
            Ok(AudioCommand::Reset) => {
                is_playing.store(false, Ordering::Relaxed);
                resampler_reset.store(true, Ordering::Release);
                buffer.lock().reset();
                playback_finished.store(false, Ordering::Release);
            }
            Ok(AudioCommand::Pause) => is_playing.store(false, Ordering::Relaxed),
            Ok(AudioCommand::Resume) => {
                if buffer.lock().available() > 0 {
                    playback_finished.store(false, Ordering::Release);
                    is_playing.store(true, Ordering::Relaxed);
                }
            }
            Ok(AudioCommand::Stop) => break,
            Err(_) => break,
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{CircularAudioBuffer, PlaybackResampler};

    #[test]
    fn resampler_consumes_only_completed_source_samples() {
        let mut buffer = CircularAudioBuffer::new(1.0, 24_000);
        let source: Vec<f32> = (0..600).map(|n| n as f32).collect();
        buffer.write(&source);

        let mut resampler = PlaybackResampler::new(24_000.0 / 48_000.0);
        let mut output = vec![0.0; 512];
        resampler.render_mono(&mut buffer, &mut output);

        assert_eq!(buffer.read_pos, 256);
        assert_eq!(buffer.available_samples, source.len() - 256);
        assert_eq!(resampler.source_pos, 0.0);
    }

    #[test]
    fn resampler_carries_fractional_phase_between_callbacks() {
        let mut buffer = CircularAudioBuffer::new(1.0, 24_000);
        let source: Vec<f32> = (0..600).map(|n| n as f32).collect();
        buffer.write(&source);

        let mut resampler = PlaybackResampler::new(24_000.0 / 44_100.0);
        let mut output = vec![0.0; 512];
        resampler.render_mono(&mut buffer, &mut output);

        assert_eq!(buffer.read_pos, 278);
        assert!(resampler.source_pos > 0.62 && resampler.source_pos < 0.66);

        let first_after_boundary = {
            let mut next = vec![0.0; 1];
            resampler.render_mono(&mut buffer, &mut next);
            next[0]
        };

        assert!(first_after_boundary > 278.62 && first_after_boundary < 278.66);
    }
}
