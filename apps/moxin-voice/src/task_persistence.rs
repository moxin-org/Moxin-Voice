//! Task persistence module for saving/loading clone tasks
//!
//! Clone tasks are stored in:
//! - Config: ~/.dora/primespeech/clone_tasks.json
//! - Audio: ~/.dora/primespeech/clone_tasks/{task_id}/

use serde::{Deserialize, Serialize};
use std::fs;
use std::path::PathBuf;

/// Clone task status
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum CloneTaskStatus {
    Pending,
    Processing,
    Completed,
    Failed,
    Cancelled,
}

/// Clone task information
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CloneTask {
    pub id: String,
    pub name: String,
    pub status: CloneTaskStatus,
    pub progress: f32,
    pub created_at: String,
    pub audio_path: Option<String>,
    pub reference_text: Option<String>,
    pub started_at: Option<String>,
    pub completed_at: Option<String>,
    pub message: Option<String>,
    #[serde(default)]
    pub current_step: Option<u32>,   // Current training stage index (0-7)
    #[serde(default)]
    pub sub_step: Option<u32>,       // Current epoch within the active stage
    #[serde(default)]
    pub sub_total: Option<u32>,      // Total epochs for the active stage
}

/// Clone tasks configuration file format
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CloneTasksConfig {
    /// Config version for future compatibility
    pub version: String,
    /// List of clone tasks
    pub tasks: Vec<CloneTask>,
}

impl Default for CloneTasksConfig {
    fn default() -> Self {
        Self {
            version: "1.0".to_string(),
            tasks: Vec::new(),
        }
    }
}

/// Get the base directory for PrimeSpeech data
pub fn get_primespeech_dir() -> PathBuf {
    let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
    home.join(".dora").join("primespeech")
}

/// Get the clone tasks config file path
pub fn get_config_path() -> PathBuf {
    get_primespeech_dir().join("clone_tasks.json")
}

/// Get the clone tasks directory
pub fn get_clone_tasks_dir() -> PathBuf {
    get_primespeech_dir().join("clone_tasks")
}

/// Get the directory for a specific clone task
pub fn get_task_dir(task_id: &str) -> PathBuf {
    get_clone_tasks_dir().join(task_id)
}

/// Ensure all required directories exist
pub fn ensure_directories() -> std::io::Result<()> {
    let primespeech_dir = get_primespeech_dir();
    if !primespeech_dir.exists() {
        fs::create_dir_all(&primespeech_dir)?;
    }

    let clone_tasks_dir = get_clone_tasks_dir();
    if !clone_tasks_dir.exists() {
        fs::create_dir_all(&clone_tasks_dir)?;
    }

    Ok(())
}

/// Load clone tasks from config file
pub fn load_clone_tasks() -> Vec<CloneTask> {
    let config_path = get_config_path();

    if !config_path.exists() {
        return Vec::new();
    }

    match fs::read_to_string(&config_path) {
        Ok(content) => match serde_json::from_str::<CloneTasksConfig>(&content) {
            Ok(config) => config.tasks,
            Err(e) => {
                log::error!("Failed to parse clone tasks config: {}", e);
                Vec::new()
            }
        },
        Err(e) => {
            log::error!("Failed to read clone tasks config: {}", e);
            Vec::new()
        }
    }
}

/// Save clone tasks to config file
pub fn save_clone_tasks(tasks: &[CloneTask]) -> Result<(), String> {
    ensure_directories().map_err(|e| format!("Failed to create directories: {}", e))?;

    let config = CloneTasksConfig {
        version: "1.0".to_string(),
        tasks: tasks.to_vec(),
    };

    let config_path = get_config_path();
    let json = serde_json::to_string_pretty(&config)
        .map_err(|e| format!("Failed to serialize config: {}", e))?;

    fs::write(&config_path, json).map_err(|e| format!("Failed to write config: {}", e))?;

    // log::info!("Saved {} clone tasks to {:?}", tasks.len(), config_path);
    Ok(())
}

/// Add a new clone task
pub fn add_task(task: CloneTask) -> Result<(), String> {
    let mut tasks = load_clone_tasks();
    tasks.push(task);
    save_clone_tasks(&tasks)
}

/// Update an existing clone task
pub fn update_task(task: CloneTask) -> Result<(), String> {
    let mut tasks = load_clone_tasks();
    
    if let Some(existing) = tasks.iter_mut().find(|t| t.id == task.id) {
        *existing = task;
        save_clone_tasks(&tasks)
    } else {
        Err(format!("Task not found: {}", task.id))
    }
}

/// Delete a clone task
pub fn delete_task(task_id: &str) -> Result<(), String> {
    let mut tasks = load_clone_tasks();
    tasks.retain(|t| t.id != task_id);
    save_clone_tasks(&tasks)?;

    // Also delete the task directory if it exists
    let task_dir = get_task_dir(task_id);
    if task_dir.exists() {
        fs::remove_dir_all(&task_dir)
            .map_err(|e| format!("Failed to delete task directory: {}", e))?;
    }

    Ok(())
}

/// Get a specific task by ID
pub fn get_task(task_id: &str) -> Option<CloneTask> {
    load_clone_tasks().into_iter().find(|t| t.id == task_id)
}

/// Update task progress fields (overall and optional sub-epoch progress)
pub fn update_task_progress(
    task_id: &str,
    progress: f32,
    current_step: u32,
    message: &str,
) -> Result<(), String> {
    update_task_progress_full(task_id, progress, current_step, message, None, None)
}

/// Update task progress including sub-epoch progress for long training stages
pub fn update_task_progress_full(
    task_id: &str,
    progress: f32,
    current_step: u32,
    message: &str,
    sub_step: Option<u32>,
    sub_total: Option<u32>,
) -> Result<(), String> {
    let mut tasks = load_clone_tasks();
    if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
        task.progress = progress;
        task.current_step = Some(current_step);
        task.message = Some(message.to_string());
        if let Some(ss) = sub_step {
            task.sub_step = Some(ss);
        }
        if let Some(st) = sub_total {
            task.sub_total = Some(st);
        }
    }
    save_clone_tasks(&tasks)
}

/// Update task status (and optional timestamps)
pub fn update_task_status(
    task_id: &str,
    status: CloneTaskStatus,
    started_at: Option<String>,
    completed_at: Option<String>,
) -> Result<(), String> {
    let mut tasks = load_clone_tasks();
    if let Some(task) = tasks.iter_mut().find(|t| t.id == task_id) {
        task.status = status;
        if let Some(s) = started_at {
            task.started_at = Some(s);
        }
        if let Some(c) = completed_at {
            task.completed_at = Some(c);
        }
    }
    save_clone_tasks(&tasks)
}

/// Mark all Processing tasks as Failed (called on app startup to clean up interrupted training)
pub fn mark_stale_tasks_as_failed() -> Result<(), String> {
    let mut tasks = load_clone_tasks();
    let mut changed = false;
    for task in tasks.iter_mut() {
        if task.status == CloneTaskStatus::Processing {
            task.status = CloneTaskStatus::Failed;
            task.message = Some("Training was interrupted (app was closed)".to_string());
            changed = true;
        }
    }
    if changed {
        save_clone_tasks(&tasks)
    } else {
        Ok(())
    }
}
