/// Few-Shot Voice Training Manager
///
/// Manages the training service subprocess that orchestrates GPT-SoVITS training.
/// Supports:
/// - Option A: Python training service
/// - Option B: Rust `moxin-fewshot-trainer`
/// Communicates via JSON events over stdin/stdout.
use crossbeam_channel::{bounded, Receiver, Sender};
use parking_lot::Mutex;
use serde::{Deserialize, Serialize};
use std::io::{BufRead, BufReader, Write};
use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::sync::Arc;
use std::thread;
use std::time::Instant;

/// Commands sent to the training worker thread
#[derive(Debug, Clone)]
pub enum TrainingCommand {
    /// Start a new training session
    Start {
        voice_id: String,
        voice_name: String,
        audio_file: PathBuf,
        language: String,
        reference_text: String,
        backend: TrainingBackend,
    },
    /// Cancel the current training session
    Cancel,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TrainingBackend {
    /// Option A (default): Python training service
    OptionA,
    /// Option B: Rust/MLX few-shot trainer
    OptionB,
}

impl TrainingBackend {
    pub fn from_str(v: &str) -> Option<Self> {
        match v.trim().to_ascii_lowercase().as_str() {
            "option_a" | "a" | "python" | "legacy" => Some(Self::OptionA),
            "option_b" | "b" | "rust" | "mlx" => Some(Self::OptionB),
            _ => None,
        }
    }

    pub fn from_env() -> Self {
        std::env::var("MOXIN_TRAINING_BACKEND")
            .ok()
            .as_deref()
            .and_then(Self::from_str)
            .unwrap_or(Self::OptionA)
    }

    pub fn as_str(self) -> &'static str {
        match self {
            Self::OptionA => "option_a",
            Self::OptionB => "option_b",
        }
    }
}

/// Training status states
#[derive(Debug, Clone, PartialEq)]
pub enum TrainingStatus {
    /// No training in progress
    Idle,
    /// Training is currently running
    Running,
    /// Training completed successfully
    Completed {
        gpt_weights: PathBuf,
        sovits_weights: PathBuf,
        reference_audio: PathBuf,
        reference_text: String,
    },
    /// Training failed with error
    Failed { error: String },
    /// Training was cancelled by user
    Cancelled,
}

impl Default for TrainingStatus {
    fn default() -> Self {
        Self::Idle
    }
}

/// Training progress information shared between worker and UI
#[derive(Debug, Clone)]
pub struct TrainingProgress {
    /// Current training status
    pub status: TrainingStatus,
    /// Current stage description (e.g., "Slicing audio")
    pub current_stage: String,
    /// Current step number (1-7)
    pub current_step: usize,
    /// Total number of steps (typically 7)
    pub total_steps: usize,
    /// Current epoch within the active training stage (GPT/SoVITS)
    pub sub_step: usize,
    /// Total epochs for the active training stage
    pub sub_total: usize,
    /// Log lines from training process
    pub log_lines: Vec<String>,
    /// Last update timestamp
    pub last_updated: Instant,
}

impl Default for TrainingProgress {
    fn default() -> Self {
        Self {
            status: TrainingStatus::Idle,
            current_stage: String::new(),
            current_step: 0,
            total_steps: 7,
            sub_step: 0,
            sub_total: 0,
            log_lines: Vec::new(),
            last_updated: Instant::now(),
        }
    }
}

/// JSON event from Python training service
#[derive(Debug, Deserialize)]
struct TrainingEvent {
    #[serde(rename = "type")]
    event_type: String,
    message: String,
    #[serde(default)]
    data: Option<serde_json::Value>,
}

/// Training request sent to Python service
#[derive(Debug, Serialize)]
struct TrainingRequest {
    voice_id: String,
    voice_name: String,
    audio_file: String,
    language: String,
    workspace_dir: String,
    reference_text: String,
    training_params: TrainingParams,
}

#[derive(Debug, Serialize)]
struct TrainingParams {
    gpt_epochs: u32,
    sovits_epochs: u32,
    batch_size: u32,
}

/// Main training manager
///
    /// Spawns a background worker thread that manages the training subprocess.
/// Provides thread-safe access to training progress via Arc<Mutex<TrainingProgress>>.
pub struct TrainingManager {
    command_tx: Sender<TrainingCommand>,
    progress: Arc<Mutex<TrainingProgress>>,
    worker_handle: Option<thread::JoinHandle<()>>,
    stop_tx: Option<Sender<()>>,
}

impl TrainingManager {
    /// Create a new training manager
    pub fn new() -> Self {
        let (command_tx, command_rx) = bounded(10);
        let (stop_tx, stop_rx) = bounded(1);
        let progress = Arc::new(Mutex::new(TrainingProgress::default()));
        let progress_clone = Arc::clone(&progress);

        let worker = thread::Builder::new()
            .name("training-worker".to_string())
            .spawn(move || {
                Self::run_worker(command_rx, stop_rx, progress_clone);
            })
            .expect("Failed to spawn training worker thread");

        Self {
            command_tx,
            progress,
            worker_handle: Some(worker),
            stop_tx: Some(stop_tx),
        }
    }

    /// Start a new training session
    ///
    /// Returns false if a training is already in progress or command failed to send.
    pub fn start_training(
        &self,
        voice_id: String,
        voice_name: String,
        audio_file: PathBuf,
        language: String,
        reference_text: String,
        backend: TrainingBackend,
    ) -> bool {
        self.command_tx
            .try_send(TrainingCommand::Start {
                voice_id,
                voice_name,
                audio_file,
                language,
                reference_text,
                backend,
            })
            .is_ok()
    }

    /// Cancel the current training session
    ///
    /// Returns false if no training is in progress or command failed to send.
    pub fn cancel_training(&self) -> bool {
        self.command_tx.try_send(TrainingCommand::Cancel).is_ok()
    }

    /// Get a snapshot of current training progress
    pub fn get_progress(&self) -> TrainingProgress {
        self.progress.lock().clone()
    }

    /// Worker thread that manages Python subprocess lifecycle
    fn run_worker(
        command_rx: Receiver<TrainingCommand>,
        stop_rx: Receiver<()>,
        progress: Arc<Mutex<TrainingProgress>>,
    ) {
        let mut current_process: Option<Child> = None;

        loop {
            // Check for stop signal
            if stop_rx.try_recv().is_ok() {
                if let Some(mut child) = current_process.take() {
                    let _ = child.kill();
                }
                break;
            }

            // Process commands
            if let Ok(cmd) = command_rx.try_recv() {
                match cmd {
                    TrainingCommand::Start {
                        voice_id,
                        voice_name,
                        audio_file,
                        language,
                        reference_text,
                        backend,
                    } => {
                        // Kill existing process if any
                        if let Some(mut child) = current_process.take() {
                            log::warn!("Killing existing training process");
                            let _ = child.kill();
                        }

                        current_process = Self::execute_training(
                            voice_id,
                            voice_name,
                            audio_file,
                            language,
                            reference_text,
                            backend,
                            &progress,
                        );
                    }
                    TrainingCommand::Cancel => {
                        if let Some(mut child) = current_process.take() {
                            log::info!("Cancelling training...");
                            let _ = child.kill();

                            let mut prog = progress.lock();
                            prog.status = TrainingStatus::Cancelled;
                            prog.log_lines
                                .push("[INFO] Training cancelled by user".to_string());
                            prog.last_updated = Instant::now();
                        }
                    }
                }
            }

            std::thread::sleep(std::time::Duration::from_millis(50));
        }

        log::info!("Training worker thread exiting");
    }

    /// Execute training by spawning configured service subprocess
    fn execute_training(
        voice_id: String,
        voice_name: String,
        audio_file: PathBuf,
        language: String,
        reference_text: String,
        backend: TrainingBackend,
        progress: &Arc<Mutex<TrainingProgress>>,
    ) -> Option<Child> {
        // Reset progress
        {
            let mut prog = progress.lock();
            prog.status = TrainingStatus::Running;
            prog.current_stage = "Starting...".to_string();
            prog.current_step = 0;
            prog.total_steps = 7;
            prog.sub_step = 0;
            prog.sub_total = 0;
            prog.log_lines.clear();
            prog.log_lines.push("[INFO] Training started".to_string());
            prog.last_updated = Instant::now();
        }

        // Determine workspace directory
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        let workspace_dir = home
            .join(".dora")
            .join("primespeech")
            .join("trained_models")
            .join(&voice_id);

        // Build training request
        let request = TrainingRequest {
            voice_id: voice_id.clone(),
            voice_name: voice_name.clone(),
            audio_file: audio_file.to_string_lossy().to_string(),
            language: language.clone(),
            workspace_dir: workspace_dir.to_string_lossy().to_string(),
            reference_text: reference_text.clone(),
            training_params: TrainingParams {
                gpt_epochs: 15,
                sovits_epochs: 20,
                batch_size: 4,
            },
        };

        let request_json = match serde_json::to_string(&request) {
            Ok(json) => json,
            Err(e) => {
                let mut prog = progress.lock();
                prog.status = TrainingStatus::Failed {
                    error: format!("Failed to serialize request: {}", e),
                };
                prog.log_lines.push(format!("[ERROR] {}", e));
                prog.last_updated = Instant::now();
                return None;
            }
        };

        let mut child = match backend {
            TrainingBackend::OptionA => {
                log::info!("Spawning Python training service (Option A)");
                log::info!("Workspace: {}", workspace_dir.display());
                Command::new("python")
                    .arg("-m")
                    .arg("dora_primespeech.moyoyo_tts.training_service")
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
            }
            TrainingBackend::OptionB => {
                let trainer_bin = match Self::resolve_rust_trainer_bin() {
                    Ok(path) => path,
                    Err(e) => {
                        let mut prog = progress.lock();
                        prog.status = TrainingStatus::Failed {
                            error: e.clone(),
                        };
                        prog.log_lines.push(format!("[ERROR] {}", e));
                        prog.last_updated = Instant::now();
                        return None;
                    }
                };

                log::info!(
                    "Spawning Rust few-shot training service (Option B): {}",
                    trainer_bin.display()
                );
                log::info!("Workspace: {}", workspace_dir.display());
                Command::new(trainer_bin)
                    .stdin(Stdio::piped())
                    .stdout(Stdio::piped())
                    .stderr(Stdio::piped())
                    .spawn()
            }
        }
        .map_err(|e| {
            let mut prog = progress.lock();
            prog.status = TrainingStatus::Failed {
                error: format!("Failed to spawn training service: {}", e),
            };
            prog.log_lines.push(format!(
                "[ERROR] Failed to start training service (backend={}): {}",
                backend.as_str(),
                e
            ));
            prog.last_updated = Instant::now();
            e
        })
        .ok()?;

        // Send request to stdin
        if let Some(mut stdin) = child.stdin.take() {
            if let Err(e) = writeln!(stdin, "{}", request_json) {
                log::error!("Failed to write request to training service: {}", e);
                let mut prog = progress.lock();
                prog.status = TrainingStatus::Failed {
                    error: format!("Failed to send request: {}", e),
                };
                prog.log_lines.push(format!("[ERROR] {}", e));
                prog.last_updated = Instant::now();
                return None;
            }
        }

        // Spawn thread to read stdout events
        let progress_clone = Arc::clone(progress);
        if let Some(stdout) = child.stdout.take() {
            thread::Builder::new()
                .name("training-stdout-reader".to_string())
                .spawn(move || {
                    let reader = BufReader::new(stdout);
                    for line in reader.lines() {
                        match line {
                            Ok(line) => {
                                Self::handle_event(&line, &progress_clone);
                            }
                            Err(e) => {
                                log::error!("Error reading stdout: {}", e);
                                break;
                            }
                        }
                    }

                    // Training process stdout closed
                    let mut prog = progress_clone.lock();
                    if prog.status == TrainingStatus::Running {
                        // Process exited without COMPLETE event = failure
                        prog.status = TrainingStatus::Failed {
                            error: "Training process exited unexpectedly".to_string(),
                        };
                        prog.log_lines.push(
                            "[ERROR] Training process exited without completion event".to_string(),
                        );
                        prog.last_updated = Instant::now();
                    }
                })
                .expect("Failed to spawn stdout reader thread");
        }

        // Spawn thread to read stderr (for debugging)
        if let Some(stderr) = child.stderr.take() {
            thread::Builder::new()
                .name("training-stderr-reader".to_string())
                .spawn(move || {
                    let reader = BufReader::new(stderr);
                    for line in reader.lines() {
                        if let Ok(line) = line {
                            log::info!("[Trainer] {}", line);
                        }
                    }
                })
                .expect("Failed to spawn stderr reader thread");
        }

        Some(child)
    }

    fn resolve_rust_trainer_bin() -> Result<PathBuf, String> {
        if let Ok(explicit) = std::env::var("MOXIN_FEWSHOT_TRAINER_BIN") {
            let path = PathBuf::from(explicit);
            if path.exists() {
                return Ok(path);
            }
            return Err(format!(
                "MOXIN_FEWSHOT_TRAINER_BIN does not exist: {}",
                path.display()
            ));
        }

        let workspace_candidate =
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/release/moxin-fewshot-trainer");
        if workspace_candidate.exists() {
            return Ok(workspace_candidate);
        }

        if let Ok(current_exe) = std::env::current_exe() {
            if let Some(parent) = current_exe.parent() {
                let sibling = parent.join("moxin-fewshot-trainer");
                if sibling.exists() {
                    return Ok(sibling);
                }
            }
        }

        Err(
            "Cannot find moxin-fewshot-trainer binary. Build it with `cargo build -p dora-primespeech-mlx --release` or set MOXIN_FEWSHOT_TRAINER_BIN."
                .to_string(),
        )
    }

    /// Parse and handle a JSON event from Python service
    fn handle_event(json_line: &str, progress: &Arc<Mutex<TrainingProgress>>) {
        // Try to parse as JSON event
        let event: TrainingEvent = match serde_json::from_str(json_line) {
            Ok(e) => e,
            Err(_) => {
                // Not a JSON event, treat as raw log line
                let mut prog = progress.lock();
                prog.log_lines.push(json_line.to_string());
                prog.last_updated = Instant::now();
                return;
            }
        };

        let mut prog = progress.lock();

        match event.event_type.as_str() {
            "STAGE" => {
                prog.current_stage = event.message.clone();
                // Reset sub-epoch progress whenever a new stage starts,
                // so stale values from the previous stage don't bleed through.
                prog.sub_step = 0;
                prog.sub_total = 0;

                // Extract progress data if available
                if let Some(data) = event.data {
                    if let Some(current) = data.get("current").and_then(|v| v.as_u64()) {
                        prog.current_step = current as usize;
                    }
                    if let Some(total) = data.get("total").and_then(|v| v.as_u64()) {
                        prog.total_steps = total as usize;
                    }
                }

                prog.log_lines
                    .push(format!("[STAGE] {}", event.message));
                log::info!("Training stage: {}", event.message);
            }

            "PROGRESS" => {
                // Epoch-level progress within the current stage (GPT or SoVITS training)
                if let Some(data) = event.data {
                    if let Some(epoch) = data.get("epoch").and_then(|v| v.as_u64()) {
                        prog.sub_step = epoch as usize;
                    }
                    if let Some(total) = data.get("total_epochs").and_then(|v| v.as_u64()) {
                        prog.sub_total = total as usize;
                    }
                }
                prog.log_lines.push(format!("[PROGRESS] {}", event.message));
                log::debug!("Training sub-progress: {}", event.message);
            }

            "INFO" | "LOG" => {
                prog.log_lines
                    .push(format!("[INFO] {}", event.message));
            }

            "WARNING" => {
                prog.log_lines
                    .push(format!("[WARNING] {}", event.message));
                log::warn!("{}", event.message);
            }

            "ERROR" => {
                prog.status = TrainingStatus::Failed {
                    error: event.message.clone(),
                };
                prog.log_lines
                    .push(format!("[ERROR] {}", event.message));
                log::error!("Training error: {}", event.message);

                // Log traceback if available
                if let Some(data) = event.data {
                    if let Some(traceback) = data.get("traceback").and_then(|v| v.as_str()) {
                        log::error!("Traceback:\n{}", traceback);
                    }
                }
            }

            "COMPLETE" => {
                if let Some(data) = event.data {
                    let gpt_weights = data
                        .get("gpt_weights")
                        .and_then(|v| v.as_str())
                        .map(PathBuf::from)
                        .unwrap_or_default();

                    let sovits_weights = data
                        .get("sovits_weights")
                        .and_then(|v| v.as_str())
                        .map(PathBuf::from)
                        .unwrap_or_default();

                    let reference_audio = data
                        .get("reference_audio")
                        .and_then(|v| v.as_str())
                        .map(PathBuf::from)
                        .unwrap_or_default();

                    let reference_text = data
                        .get("reference_text")
                        .and_then(|v| v.as_str())
                        .unwrap_or("")
                        .to_string();

                    prog.status = TrainingStatus::Completed {
                        gpt_weights,
                        sovits_weights,
                        reference_audio,
                        reference_text,
                    };

                    prog.log_lines
                        .push("[SUCCESS] Training completed successfully".to_string());
                    log::info!("Training completed successfully");
                }
            }

            _ => {
                // Unknown event type, log as info
                prog.log_lines
                    .push(format!("[{}] {}", event.event_type, event.message));
            }
        }

        prog.last_updated = Instant::now();

        // Keep only last 500 log lines to prevent memory issues
        if prog.log_lines.len() > 500 {
            let drain_count = prog.log_lines.len() - 500;
            prog.log_lines.drain(0..drain_count);
        }
    }
}

impl Drop for TrainingManager {
    fn drop(&mut self) {
        // Signal worker to stop
        if let Some(stop_tx) = self.stop_tx.take() {
            let _ = stop_tx.send(());
        }

        // Wait for worker to finish
        if let Some(handle) = self.worker_handle.take() {
            let _ = handle.join();
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_training_manager_creation() {
        let manager = TrainingManager::new();
        let progress = manager.get_progress();
        assert_eq!(progress.status, TrainingStatus::Idle);
        assert_eq!(progress.total_steps, 7);
    }

    #[test]
    fn test_json_event_parsing() {
        let json = r#"{"type":"STAGE","message":"Slicing audio","data":{"current":3,"total":7}}"#;
        let event: TrainingEvent = serde_json::from_str(json).unwrap();
        assert_eq!(event.event_type, "STAGE");
        assert_eq!(event.message, "Slicing audio");
    }

    #[test]
    fn test_training_request_serialization() {
        let request = TrainingRequest {
            voice_id: "test_voice".to_string(),
            voice_name: "Test Voice".to_string(),
            audio_file: "/tmp/test.wav".to_string(),
            language: "zh".to_string(),
            workspace_dir: "/tmp/workspace".to_string(),
            reference_text: "测试文本".to_string(),
            training_params: TrainingParams {
                gpt_epochs: 15,
                sovits_epochs: 20,
                batch_size: 4,
            },
        };

        let json = serde_json::to_string(&request).unwrap();
        assert!(json.contains("test_voice"));
        assert!(json.contains("gpt_epochs"));
    }
}
