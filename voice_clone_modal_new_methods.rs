// New methods to add to VoiceCloneModal impl block
// Copy these methods into apps/moxin-primespeech/src/voice_clone_modal.rs
// Insert before the closing brace of `impl VoiceCloneModal` (before line 1960)

fn switch_to_mode(&mut self, cx: &mut Cx, mode: CloneMode) {
    if self.clone_mode == mode {
        return;
    }

    self.clone_mode = mode;

    match mode {
        CloneMode::Express => {
            // Update tab visuals
            self.view.button(ids!(
                modal_container.modal_wrapper.modal_content.mode_tabs.express_tab
            )).apply_over(cx, live! {
                draw_bg: { active: 1.0 }
                draw_text: { active: 1.0 }
            });

            self.view.button(ids!(
                modal_container.modal_wrapper.modal_content.mode_tabs.pro_tab
            )).apply_over(cx, live! {
                draw_bg: { active: 0.0 }
                draw_text: { active: 0.0 }
            });

            // Show/hide content
            self.view.view(ids!(modal_container.modal_wrapper.modal_content.body.express_mode_content))
                .set_visible(true);
            self.view.view(ids!(modal_container.modal_wrapper.modal_content.body.pro_mode_content))
                .set_visible(false);
            self.view.view(ids!(modal_container.modal_wrapper.modal_content.footer.express_actions))
                .set_visible(true);
            self.view.view(ids!(modal_container.modal_wrapper.modal_content.footer.pro_actions))
                .set_visible(false);
        }

        CloneMode::Pro => {
            self.view.button(ids!(
                modal_container.modal_wrapper.modal_content.mode_tabs.express_tab
            )).apply_over(cx, live! {
                draw_bg: { active: 0.0 }
                draw_text: { active: 0.0 }
            });

            self.view.button(ids!(
                modal_container.modal_wrapper.modal_content.mode_tabs.pro_tab
            )).apply_over(cx, live! {
                draw_bg: { active: 1.0 }
                draw_text: { active: 1.0 }
            });

            self.view.view(ids!(modal_container.modal_wrapper.modal_content.body.express_mode_content))
                .set_visible(false);
            self.view.view(ids!(modal_container.modal_wrapper.modal_content.body.pro_mode_content))
                .set_visible(true);
            self.view.view(ids!(modal_container.modal_wrapper.modal_content.footer.express_actions))
                .set_visible(false);
            self.view.view(ids!(modal_container.modal_wrapper.modal_content.footer.pro_actions))
                .set_visible(true);

            // Check GPU availability
            self.check_gpu_availability(cx);
        }
    }

    self.view.redraw(cx);
}

fn toggle_training_recording(&mut self, cx: &mut Cx) {
    if self.recording_for_training {
        self.stop_training_recording(cx);
    } else {
        self.start_training_recording(cx);
    }
}

fn start_training_recording(&mut self, cx: &mut Cx) {
    use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

    self.add_training_log(cx, "[INFO] Starting long recording (target: 5-10 minutes)");
    self.add_training_log(cx, "[INFO] Speak clearly with varied sentences for best training results");

    self.recording_for_training = true;
    self.training_audio_samples.clear();
    self.training_recording_start = Some(Instant::now());

    // Update UI
    self.view.label(ids!(
        modal_container.modal_wrapper.modal_content.body.pro_mode_content
        .training_recording_section.record_row.recording_info.duration_label
    )).set_text("Recording... 0:00 / 10:00");

    self.view.view(ids!(
        modal_container.modal_wrapper.modal_content.body.pro_mode_content
        .training_recording_section.duration_bar
    )).set_visible(true);

    self.view.button(ids!(
        modal_container.modal_wrapper.modal_content.body.pro_mode_content
        .training_recording_section.record_row.record_btn
    )).apply_over(cx, live! { draw_bg: { recording: 1.0 } });

    // Start CPAL recording (reuse existing audio capture logic from Express mode)
    // Initialize buffer and atomic flags
    self.recording_buffer = Arc::new(Mutex::new(Vec::new()));
    self.is_recording = Arc::new(AtomicBool::new(true));
    self.recording_sample_rate = Arc::new(Mutex::new(48000));

    let buffer = Arc::clone(&self.recording_buffer);
    let is_recording = Arc::clone(&self.is_recording);
    let sample_rate_store = Arc::clone(&self.recording_sample_rate);

    std::thread::spawn(move || {
        let host = cpal::default_host();
        let device = match host.default_input_device() {
            Some(d) => d,
            None => {
                eprintln!("[TrainingRec] No input device found");
                return;
            }
        };

        let supported_config = match device.default_input_config() {
            Ok(c) => c,
            Err(e) => {
                eprintln!("[TrainingRec] Failed to get config: {}", e);
                return;
            }
        };

        let sample_rate = supported_config.sample_rate().0;
        let channels = supported_config.channels() as usize;
        *sample_rate_store.lock() = sample_rate;

        let config: cpal::StreamConfig = supported_config.into();
        let buffer_clone = Arc::clone(&buffer);
        let is_recording_clone = Arc::clone(&is_recording);

        let stream = match device.build_input_stream(
            &config,
            move |data: &[f32], _: &cpal::InputCallbackInfo| {
                if is_recording_clone.load(Ordering::Relaxed) {
                    // Convert to mono
                    if channels > 1 {
                        let mono: Vec<f32> = data
                            .chunks(channels)
                            .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                            .collect();
                        buffer_clone.lock().extend_from_slice(&mono);
                    } else {
                        buffer_clone.lock().extend_from_slice(data);
                    }
                }
            },
            |err| eprintln!("[TrainingRec] Error: {}", err),
            None,
        ) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("[TrainingRec] Failed to build stream: {}", e);
                return;
            }
        };

        if let Err(e) = stream.play() {
            eprintln!("[TrainingRec] Failed to start: {}", e);
            return;
        }

        eprintln!("[TrainingRec] Recording started ({}Hz, {} channels)", sample_rate, channels);

        // Keep alive (max 12 minutes)
        let max_duration = std::time::Duration::from_secs(12 * 60);
        let start = std::time::Instant::now();

        while is_recording.load(Ordering::Relaxed) && start.elapsed() < max_duration {
            std::thread::sleep(std::time::Duration::from_millis(100));
        }

        is_recording.store(false, Ordering::Relaxed);
        eprintln!("[TrainingRec] Recording stopped");
    });

    self.view.redraw(cx);
}

fn stop_training_recording(&mut self, cx: &mut Cx) {
    self.is_recording.store(false, Ordering::Relaxed);
    self.recording_for_training = false;

    // Calculate duration
    let duration = self.training_recording_start
        .map(|t| t.elapsed().as_secs_f32())
        .unwrap_or(0.0);

    self.add_training_log(cx, &format!("[INFO] Recording stopped ({:.1}s)", duration));

    // Validate duration (must be 3-10 minutes = 180-600 seconds)
    if duration < 180.0 {
        self.add_training_log(cx, &format!(
            "[ERROR] Recording too short: {:.1}s (minimum: 180s = 3 minutes)",
            duration
        ));
        self.training_audio_file = None;
        return;
    }

    if duration > 600.0 {
        self.add_training_log(cx, &format!(
            "[WARNING] Recording too long: {:.1}s (will be trimmed to 600s = 10 minutes)",
            duration
        ));
    }

    // Get recorded samples
    let samples: Vec<f32> = {
        let buffer = self.recording_buffer.lock();
        buffer.clone()
    };

    let source_sample_rate = *self.recording_sample_rate.lock();

    // Resample to 32kHz (required by GPT-SoVITS)
    let target_sample_rate = 32000;
    let resampled = if source_sample_rate != target_sample_rate {
        Self::resample(&samples, source_sample_rate, target_sample_rate)
    } else {
        samples
    };

    // Store samples
    self.training_audio_samples = resampled;

    // Save to temp file
    let temp_dir = std::env::temp_dir();
    let temp_file = temp_dir.join(format!(
        "training_audio_{}.wav",
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_secs()
    ));

    match Self::save_wav_static(&temp_file, &self.training_audio_samples, target_sample_rate) {
        Ok(_) => {
            self.training_audio_file = Some(temp_file.clone());
            self.add_training_log(cx, &format!(
                "[SUCCESS] Recording saved ({:.1}s, {:.1} MB)",
                duration,
                (self.training_audio_samples.len() * 4) as f64 / 1_000_000.0
            ));

            // Enable start training button
            self.view.button(ids!(
                modal_container.modal_wrapper.modal_content.footer.pro_actions.start_training_btn
            )).set_enabled(true);
        }
        Err(e) => {
            self.add_training_log(cx, &format!("[ERROR] Failed to save audio: {}", e));
        }
    }

    // Update UI
    self.view.button(ids!(
        modal_container.modal_wrapper.modal_content.body.pro_mode_content
        .training_recording_section.record_row.record_btn
    )).apply_over(cx, live! { draw_bg: { recording: 0.0 } });

    self.view.redraw(cx);
}

fn start_training(&mut self, cx: &mut Cx, scope: &mut Scope) {
    // Validate inputs
    let voice_name = self.view.text_input(ids!(
        modal_container.modal_wrapper.modal_content.body.pro_mode_content.voice_name_input.input
    )).text();

    if voice_name.is_empty() {
        self.add_training_log(cx, "[ERROR] Voice name is required");
        return;
    }

    let Some(audio_file) = &self.training_audio_file else {
        self.add_training_log(cx, "[ERROR] No training audio recorded");
        return;
    };

    let language = self.selected_language.clone();
    let voice_id = voice_persistence::generate_voice_id(&voice_name);

    // Show progress section
    self.view.view(ids!(
        modal_container.modal_wrapper.modal_content.body.pro_mode_content.training_progress_section
    )).set_visible(true);

    // Show cancel button, hide start button
    self.view.button(ids!(
        modal_container.modal_wrapper.modal_content.footer.pro_actions.cancel_training_btn
    )).set_visible(true);

    self.view.button(ids!(
        modal_container.modal_wrapper.modal_content.footer.pro_actions.start_training_btn
    )).set_visible(false);

    // Start training via manager
    let manager = self.training_manager.as_ref().unwrap();
    if !manager.start_training(voice_id, voice_name, audio_file.clone(), language) {
        self.add_training_log(cx, "[ERROR] Failed to start training");
        return;
    }

    self.add_training_log(cx, "[INFO] Training started...");
    self.add_training_log(cx, "[INFO] This will take 30-120 minutes. Do not close the application.");

    self.view.redraw(cx);
}

fn cancel_training(&mut self, cx: &mut Cx) {
    if let Some(ref manager) = self.training_manager {
        manager.cancel_training();
        self.add_training_log(cx, "[INFO] Cancelling training (may take a few seconds)...");
    }
}

fn poll_training_progress(&mut self, cx: &mut Cx) {
    let Some(ref manager) = self.training_manager else {
        return;
    };

    let progress = manager.get_progress();

    // Only update if changed
    if progress.last_updated > self.training_progress.last_updated {
        self.training_progress = progress.clone();
        self.update_training_ui(cx, &progress);
    }
}

fn update_training_ui(&mut self, cx: &mut Cx, progress: &TrainingProgress) {
    // Update stage label
    self.view.label(ids!(
        modal_container.modal_wrapper.modal_content.body.pro_mode_content
        .training_progress_section.stage_label
    )).set_text(&format!(
        "Step {} of {}: {}",
        progress.current_step, progress.total_steps, progress.current_stage
    ));

    // Update progress bar
    let progress_pct = if progress.total_steps > 0 {
        progress.current_step as f32 / progress.total_steps as f32
    } else {
        0.0
    };

    self.view.view(ids!(
        modal_container.modal_wrapper.modal_content.body.pro_mode_content
        .training_progress_section.progress_bar
    )).apply_over(cx, live! { draw_bg: { progress: (progress_pct) } });

    // Update log content (show last 100 lines)
    let log_text = progress.log_lines
        .iter()
        .rev()
        .take(100)
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    self.view.label(ids!(
        modal_container.modal_wrapper.modal_content.body.pro_mode_content
        .training_progress_section.log_scroll.log_content
    )).set_text(&log_text);

    // Handle training completion/failure/cancel
    match &progress.status {
        TrainingStatus::Completed {
            gpt_weights,
            sovits_weights,
            reference_audio,
            reference_text,
        } => {
            self.on_training_completed(
                cx,
                gpt_weights.clone(),
                sovits_weights.clone(),
                reference_audio.clone(),
                reference_text.clone(),
            );
        }
        TrainingStatus::Failed { error } => {
            self.add_training_log(cx, &format!("[ERROR] Training failed: {}", error));
            // Re-enable start button
            self.view.button(ids!(
                modal_container.modal_wrapper.modal_content.footer.pro_actions.start_training_btn
            )).set_visible(true);
            self.view.button(ids!(
                modal_container.modal_wrapper.modal_content.footer.pro_actions.cancel_training_btn
            )).set_visible(false);
        }
        TrainingStatus::Cancelled => {
            self.add_training_log(cx, "[INFO] Training cancelled");
            self.view.button(ids!(
                modal_container.modal_wrapper.modal_content.footer.pro_actions.start_training_btn
            )).set_visible(true);
            self.view.button(ids!(
                modal_container.modal_wrapper.modal_content.footer.pro_actions.cancel_training_btn
            )).set_visible(false);
        }
        _ => {}
    }

    self.view.redraw(cx);
}

fn on_training_completed(
    &mut self,
    cx: &mut Cx,
    gpt_weights: PathBuf,
    sovits_weights: PathBuf,
    reference_audio: PathBuf,
    reference_text: String,
) {
    let voice_name = self.view.text_input(ids!(
        modal_container.modal_wrapper.modal_content.body.pro_mode_content.voice_name_input.input
    )).text();

    let voice_id = voice_persistence::generate_voice_id(&voice_name);

    // Create new trained voice entry
    let new_voice = Voice {
        id: voice_id.clone(),
        name: voice_name.clone(),
        description: format!("Custom trained voice (Few-Shot)"),
        category: VoiceCategory::Character,
        language: self.selected_language.clone(),
        source: VoiceSource::Trained,
        reference_audio_path: Some(reference_audio.to_string_lossy().to_string()),
        prompt_text: Some(reference_text),
        gpt_weights: Some(gpt_weights.to_string_lossy().to_string()),
        sovits_weights: Some(sovits_weights.to_string_lossy().to_string()),
        created_at: Some(
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs(),
        ),
        preview_audio: Some(reference_audio.to_string_lossy().to_string()),
    };

    // Save to custom voices config
    if let Err(e) = voice_persistence::add_custom_voice(new_voice.clone()) {
        self.add_training_log(cx, &format!("[ERROR] Failed to save voice: {}", e));
        return;
    }

    self.add_training_log(cx, "[SUCCESS] Voice saved successfully!");

    // Emit action to notify parent screen
    // NOTE: This requires access to scope, which needs to be passed as a parameter
    // For now, we'll just log success. The integration with parent screen
    // can be added later when the full modal is updated.

    // Show success message
    self.view.button(ids!(
        modal_container.modal_wrapper.modal_content.footer.pro_actions.start_training_btn
    )).set_visible(true);
    self.view.button(ids!(
        modal_container.modal_wrapper.modal_content.footer.pro_actions.cancel_training_btn
    )).set_visible(false);
}

fn check_gpu_availability(&mut self, cx: &mut Cx) {
    // Check if CUDA is available
    let has_gpu = std::process::Command::new("python")
        .arg("-c")
        .arg("import torch; print(torch.cuda.is_available())")
        .output()
        .map(|out| String::from_utf8_lossy(&out.stdout).trim() == "True")
        .unwrap_or(false);

    if !has_gpu {
        // Show warning
        self.view.view(ids!(
            modal_container.modal_wrapper.modal_content.body.pro_mode_content.gpu_warning
        )).set_visible(true);
    } else {
        self.view.view(ids!(
            modal_container.modal_wrapper.modal_content.body.pro_mode_content.gpu_warning
        )).set_visible(false);
    }
}

fn add_training_log(&mut self, cx: &mut Cx, message: &str) {
    self.training_progress.log_lines.push(message.to_string());

    let log_text = self.training_progress.log_lines
        .iter()
        .rev()
        .take(100)
        .rev()
        .cloned()
        .collect::<Vec<_>>()
        .join("\n");

    self.view.label(ids!(
        modal_container.modal_wrapper.modal_content.body.pro_mode_content
        .training_progress_section.log_scroll.log_content
    )).set_text(&log_text);

    self.view.redraw(cx);
}
