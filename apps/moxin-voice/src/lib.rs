//! Moxin TTS App - Text to Speech using GPT-SoVITS with voice cloning

// Local modules
pub mod audio_player; // Keep local: simplified TTS-specific version
pub mod dora_integration;
pub mod i18n;


pub mod screen;

pub mod training_executor;
pub mod training_manager;
pub mod voice_clone_modal;
pub mod voice_data;
pub mod voice_persistence;
pub mod voice_selector;
pub mod task_persistence;
pub mod tts_history;

// Re-export shared components from moxin-ui
pub use moxin_ui::log_bridge;
pub use moxin_ui::system_monitor;
pub use moxin_ui::widgets::moxin_hero::{self, ConnectionStatus, MoxinHero, MoxinHeroAction};

pub use screen::TTSScreenRef;
pub use screen::TTSScreenWidgetRefExt;

use makepad_widgets::Cx;
use moxin_widgets::{AppInfo, MoxinApp};

/// Moxin TTS app descriptor
pub struct MoxinTTSApp;

impl MoxinApp for MoxinTTSApp {
    fn info() -> AppInfo {
        AppInfo {
            name: "TTS",
            id: "moxin-voice",
            description: "GPT-SoVITS Text to Speech with voice cloning",
            ..Default::default()
        }
    }

    fn live_design(cx: &mut Cx) {
        // Note: Shared components (moxin_hero, system_monitor, log_bridge) are already
        // registered by moxin_ui::live_design(cx) in the shell, so we only register
        // app-specific components here.

        voice_selector::live_design(cx);
        voice_clone_modal::live_design(cx);
        screen::live_design(cx);
    }
}

/// Initialize the TTS app - must be called before using the app
pub fn init() {
    // Initialize i18n translations
    i18n::init_translations();
}

/// Register all TTS widgets with Makepad
pub fn live_design(cx: &mut Cx) {
    MoxinTTSApp::live_design(cx);
}
