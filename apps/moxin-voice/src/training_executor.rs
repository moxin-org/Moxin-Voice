//! Training Executor - Sequentially executes CloneTask queue
//!
//! Picks the first Pending task from task_persistence and runs it through
//! TrainingManager. Emits ExecutorEvents that the TTSScreen uses to refresh UI.

use crate::task_persistence::{self, CloneTask, CloneTaskStatus};
use crate::training_manager::{TrainingBackend, TrainingManager, TrainingProgress, TrainingStatus};
use crate::task_persistence::update_task_progress_full;
use std::path::PathBuf;
use std::sync::Arc;
use std::time::Instant;

/// Events emitted by TrainingExecutor::poll()
#[derive(Clone, Debug)]
pub enum ExecutorEvent {
    /// Progress update for a running task
    ProgressUpdated(String, TrainingProgress),
    /// Task completed successfully — carries all info needed to register the voice
    TaskCompleted {
        task_id: String,
        task_name: String,
        gpt_weights: PathBuf,
        sovits_weights: PathBuf,
        reference_audio: PathBuf,
        reference_text: String,
    },
    /// Task failed
    TaskFailed(String, String),
}

/// Sequentially executes CloneTask queue using TrainingManager
pub struct TrainingExecutor {
    training_manager: Option<Arc<TrainingManager>>,
    current_task_id: Option<String>,
    current_task_name: Option<String>,
    last_progress_time: Option<Instant>,
}

impl TrainingExecutor {
    pub fn new() -> Self {
        Self {
            training_manager: None,
            current_task_id: None,
            current_task_name: None,
            last_progress_time: None,
        }
    }

    /// Poll for training progress and task scheduling.
    /// Should be called on every NextFrame tick.
    /// Returns Some(ExecutorEvent) when there is something to report.
    pub fn poll(&mut self) -> Option<ExecutorEvent> {
        // If no manager is active, try to start the next pending task
        if self.training_manager.is_none() {
            if let Some(pending_task) = self.get_next_pending_task() {
                self.start_task(pending_task);
            }
            return None;
        }

        let manager = self.training_manager.as_ref()?;
        let task_id = self.current_task_id.clone()?;

        let progress = manager.get_progress();

        // Check for terminal states
        match &progress.status {
            TrainingStatus::Completed {
                gpt_weights,
                sovits_weights,
                reference_audio,
                reference_text,
            } => {
                let completed_at = now_string();
                let _ = task_persistence::update_task_status(
                    &task_id,
                    CloneTaskStatus::Completed,
                    None,
                    Some(completed_at),
                );
                let _ = task_persistence::update_task_progress(&task_id, 1.0, 8, "Training completed");

                let gpt = gpt_weights.clone();
                let sovits = sovits_weights.clone();
                let ref_audio = reference_audio.clone();
                let ref_text = reference_text.clone();
                let task_name = self.current_task_name.clone().unwrap_or_else(|| task_id.clone());

                log::info!("[Executor] Task {} completed — gpt={} sovits={} ref={}",
                    task_id, gpt.display(), sovits.display(), ref_audio.display());

                self.training_manager = None;
                self.current_task_id = None;
                self.current_task_name = None;
                self.last_progress_time = None;
                return Some(ExecutorEvent::TaskCompleted {
                    task_id,
                    task_name,
                    gpt_weights: gpt,
                    sovits_weights: sovits,
                    reference_audio: ref_audio,
                    reference_text: ref_text,
                });
            }

            TrainingStatus::Failed { error } => {
                let _ = task_persistence::update_task_status(
                    &task_id,
                    CloneTaskStatus::Failed,
                    None,
                    None,
                );
                log::error!("[Executor] Task {} failed: {}", task_id, error);

                let err = error.clone();
                self.training_manager = None;
                self.current_task_id = None;
                self.current_task_name = None;
                self.last_progress_time = None;
                return Some(ExecutorEvent::TaskFailed(task_id, err));
            }

            TrainingStatus::Cancelled => {
                let _ = task_persistence::update_task_status(
                    &task_id,
                    CloneTaskStatus::Cancelled,
                    None,
                    None,
                );
                log::info!("[Executor] Task {} cancelled", task_id);

                self.training_manager = None;
                self.current_task_id = None;
                self.current_task_name = None;
                self.last_progress_time = None;
                return None;
            }

            TrainingStatus::Running => {
                // Update persistence every ~2 seconds
                let should_update = self.last_progress_time
                    .map(|t| t.elapsed().as_secs() >= 2)
                    .unwrap_or(true);

                if should_update {
                    self.last_progress_time = Some(Instant::now());
                    // Use (current_step - 1) / total_steps so that entering the last
                    // Python stage doesn't immediately show 100%.  The final 1.0 is
                    // only set when the COMPLETE event arrives.
                    let pct = if progress.total_steps > 0 && progress.current_step > 0 {
                        (progress.current_step as f32 - 1.0) / progress.total_steps as f32
                    } else {
                        0.0
                    };
                    let sub_step = if progress.sub_step > 0 { Some(progress.sub_step as u32) } else { None };
                    let sub_total = if progress.sub_total > 0 { Some(progress.sub_total as u32) } else { None };
                    let _ = update_task_progress_full(
                        &task_id,
                        pct,
                        progress.current_step as u32,
                        &progress.current_stage,
                        sub_step,
                        sub_total,
                    );
                    return Some(ExecutorEvent::ProgressUpdated(task_id, progress));
                }
            }

            TrainingStatus::Idle => {
                // Just started, no update yet
            }
        }

        None
    }

    /// Cancel the currently running task
    pub fn cancel_current(&mut self) -> bool {
        if let Some(ref m) = self.training_manager {
            m.cancel_training()
        } else {
            false
        }
    }

    /// Get the ID of the currently running task
    pub fn current_task_id(&self) -> Option<&str> {
        self.current_task_id.as_deref()
    }

    fn get_next_pending_task(&self) -> Option<CloneTask> {
        task_persistence::load_clone_tasks()
            .into_iter()
            .find(|t| t.status == CloneTaskStatus::Pending)
    }

    fn start_task(&mut self, task: CloneTask) {
        let audio_path = match &task.audio_path {
            Some(p) => PathBuf::from(p),
            None => {
                log::error!("[Executor] Task {} has no audio path, skipping", task.id);
                let _ = task_persistence::update_task_status(
                    &task.id,
                    CloneTaskStatus::Failed,
                    None,
                    None,
                );
                return;
            }
        };

        let started_at = now_string();
        let _ = task_persistence::update_task_status(
            &task.id,
            CloneTaskStatus::Processing,
            Some(started_at),
            None,
        );

        log::info!("[Executor] Starting task: {} ({})", task.name, task.id);

        let manager = Arc::new(TrainingManager::new());
        manager.start_training(
            task.id.clone(),
            task.name.clone(),
            audio_path,
            "zh".to_string(), // default language
            task.reference_text.clone().unwrap_or_default(),
            task.training_backend
                .as_deref()
                .and_then(TrainingBackend::from_str)
                .unwrap_or_else(TrainingBackend::from_env),
        );

        self.training_manager = Some(manager);
        self.current_task_id = Some(task.id);
        self.current_task_name = Some(task.name);
        self.last_progress_time = None;
    }
}

/// Convert days since Unix epoch (1970-01-01) to (year, month, day) using
/// Howard Hinnant's civil_from_days algorithm — handles leap years correctly.
fn civil_from_days(z: i64) -> (i64, u32, u32) {
    let z = z + 719468;
    let era = if z >= 0 { z } else { z - 146096 } / 146097;
    let doe = z - era * 146097;
    let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
    let y = yoe + era * 400;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = (doy - (153 * mp + 2) / 5 + 1) as u32;
    let m = if mp < 10 { (mp + 3) as u32 } else { (mp - 9) as u32 };
    let y = if m <= 2 { y + 1 } else { y };
    (y, m, d)
}

fn now_string() -> String {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| {
            let secs = d.as_secs();
            let s = secs % 60;
            let m = (secs / 60) % 60;
            let h = (secs / 3600) % 24;
            let total_days = (secs / 86400) as i64;
            let (year, month, day) = civil_from_days(total_days);
            format!("{}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, h, m, s)
        })
        .unwrap_or_else(|_| "unknown".to_string())
}
