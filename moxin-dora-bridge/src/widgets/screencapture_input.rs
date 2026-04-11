//! ScreenCaptureKit system audio capture (macOS only)
//!
//! Captures system audio (browser, video players, etc.) using Apple's
//! ScreenCaptureKit framework. No additional software required from the user —
//! the OS prompts for screen-recording permission on first use.
//!
//! Output: f32 PCM samples at 16 kHz mono, via a shared ring buffer.
//! This matches the format the ASR pipeline already expects, so no resampling
//! is needed.

use screencapturekit::prelude::*;
use screencapturekit::cm::CMSampleBuffer;
use screencapturekit::stream::output_type::SCStreamOutputType;
use std::sync::{Arc, Mutex};
use std::sync::atomic::{AtomicI8, Ordering};
use tracing::{error, info, warn};

/// -1 = not yet probed, 0 = denied/unavailable, 1 = granted
static PERMISSION_STATUS: AtomicI8 = AtomicI8::new(-1);

/// Trigger a background screen-recording permission probe (idempotent; only runs once).
///
/// Calling this early — e.g. when the user navigates to the translation page —
/// causes macOS to show the permission dialog before the user clicks Start,
/// so the restart-after-grant cycle doesn't surprise them mid-flow.
pub fn probe_permission_async() {
    if PERMISSION_STATUS.load(Ordering::Relaxed) != -1 {
        return; // already probed or in-flight
    }
    std::thread::spawn(|| {
        let ok = SCShareableContent::get().is_ok();
        PERMISSION_STATUS.store(if ok { 1 } else { 0 }, Ordering::Relaxed);
    });
}

/// Returns the screen-recording permission result.
/// `None` means the probe hasn't completed yet.
pub fn permission_granted() -> Option<bool> {
    match PERMISSION_STATUS.load(Ordering::Relaxed) {
        1 => Some(true),
        0 => Some(false),
        _ => None,
    }
}

/// Captures system audio through ScreenCaptureKit.
///
/// Internally owns an SCStream; audio callbacks push f32 samples into a
/// shared buffer that callers drain via [`get_audio`](Self::get_audio).
pub struct ScreenCaptureInput {
    stream: Option<SCStream>,
    audio_buffer: Arc<Mutex<Vec<f32>>>,
    is_recording: bool,
}

impl ScreenCaptureInput {
    /// Create a new (not yet started) capture.
    pub fn new() -> Result<Self, String> {
        // Eagerly check permission.  SCShareableContent::get() blocks until the
        // system permission dialog is resolved, which is fine since we call this
        // from a worker thread.
        SCShareableContent::get()
            .map_err(|e| format!("ScreenCaptureKit permission denied or unavailable: {e:?}"))?;

        Ok(Self {
            stream: None,
            audio_buffer: Arc::new(Mutex::new(Vec::new())),
            is_recording: false,
        })
    }

    /// Start capturing system audio.
    pub fn start(&mut self) -> Result<(), String> {
        if self.is_recording {
            return Ok(());
        }

        // Get the primary display for the content filter.
        // ScreenCaptureKit requires a display even for audio-only capture.
        let content = SCShareableContent::get()
            .map_err(|e| format!("Failed to get shareable content: {e:?}"))?;
        let displays = content.displays();
        let display = displays
            .first()
            .ok_or_else(|| "No display found for ScreenCaptureKit filter".to_string())?;

        // Content filter: capture the entire display (audio follows automatically).
        let filter = SCContentFilter::create()
            .with_display(display)
            .with_excluding_windows(&[])
            .build();

        // Stream config: audio-only at 16 kHz mono.
        // ScreenCaptureKit natively supports Rate16000 — no resampling needed.
        // Minimise video dimensions to near-zero to avoid GPU overhead.
        let config = SCStreamConfiguration::new()
            // Audio
            .with_captures_audio(true)
            .with_sample_rate(16000)
            .with_channel_count(1)
            .with_excludes_current_process_audio(true)
            // Minimal video (can't disable video entirely in older SDK versions)
            .with_width(2)
            .with_height(2);

        let mut stream = SCStream::new(&filter, &config);

        // Register audio callback: copy f32 PCM samples into the shared buffer.
        let audio_buffer = Arc::clone(&self.audio_buffer);
        stream.add_output_handler(
            move |sample: CMSampleBuffer, output_type: SCStreamOutputType| {
                if output_type != SCStreamOutputType::Audio {
                    return;
                }
                if let Some(buf_list) = sample.audio_buffer_list() {
                    // ScreenCaptureKit delivers non-interleaved float32 PCM.
                    // For mono (1 channel) there is exactly one buffer.
                    for ab in buf_list.iter() {
                        let bytes = ab.data();
                        if bytes.is_empty() {
                            continue;
                        }
                        // Interpret raw bytes as little-endian f32 samples.
                        let samples: Vec<f32> = bytes
                            .chunks_exact(4)
                            .map(|c| f32::from_le_bytes([c[0], c[1], c[2], c[3]]))
                            .collect();
                        if let Ok(mut buf) = audio_buffer.lock() {
                            buf.extend_from_slice(&samples);
                        }
                    }
                }
            },
            SCStreamOutputType::Audio,
        );

        stream
            .start_capture()
            .map_err(|e| format!("SCStream start_capture failed: {e:?}"))?;

        info!("[ScreenCaptureInput] System audio capture started (16 kHz mono)");
        self.stream = Some(stream);
        self.is_recording = true;
        Ok(())
    }

    /// Stop the capture stream.
    pub fn stop(&mut self) {
        if let Some(mut stream) = self.stream.take() {
            if let Err(e) = stream.stop_capture() {
                warn!("[ScreenCaptureInput] stop_capture error: {e:?}");
            }
            info!("[ScreenCaptureInput] System audio capture stopped");
        }
        self.is_recording = false;
        // Drain leftover samples so the next session starts clean.
        if let Ok(mut buf) = self.audio_buffer.lock() {
            buf.clear();
        }
    }

    /// Returns `true` if the capture is active.
    pub fn is_recording(&self) -> bool {
        self.is_recording
    }

    /// Drain all available samples from the internal buffer.
    ///
    /// Returns `None` when not recording or no samples have arrived yet.
    /// The caller is responsible for applying VAD / segmentation on top.
    pub fn get_audio(&self) -> Option<Vec<f32>> {
        if !self.is_recording {
            return None;
        }
        let mut buf = self.audio_buffer.lock().ok()?;
        if buf.is_empty() {
            return None;
        }
        Some(buf.drain(..).collect())
    }
}

impl Drop for ScreenCaptureInput {
    fn drop(&mut self) {
        self.stop();
    }
}
