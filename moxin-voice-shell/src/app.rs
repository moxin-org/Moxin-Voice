//! Moxin Voice App - Main application
//!
//! This is a simplified shell that directly shows the TTS screen
//! without sidebar, tabs, or app switching.

use makepad_widgets::*;
use moxin_dora_bridge::SharedDoraState;
use moxin_ui::MoxinAppData;
use moxin_voice::MoxinTTSApp;
use moxin_widgets::MoxinApp;
use moxin_widgets::translation_overlay::TranslationOverlay;
use parking_lot::RwLock;
use std::process::Command;
use std::sync::{Arc, OnceLock};

use crate::Args;

// ============================================================================
// CLI ARGS STORAGE
// ============================================================================

static CLI_ARGS: OnceLock<Args> = OnceLock::new();

pub fn set_cli_args(args: Args) {
    CLI_ARGS.set(args).ok();
}

pub fn get_cli_args() -> &'static Args {
    CLI_ARGS.get_or_init(Args::default)
}

// ============================================================================
// UI DEFINITIONS
// ============================================================================

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    use moxin_widgets::theme::DARK_BG;
    use moxin_widgets::theme::MOXIN_BG_PRIMARY_DARK;

    // Import TTS screen
    use moxin_voice::screen::TTSScreen;

    // Import translation overlay widget
    use moxin_widgets::translation_overlay::TranslationOverlay;

    // ========================================================================
    // App Window - Simplified (no sidebar, no tabs)
    // ========================================================================

    App = {{App}} {
        ui: <Window> {
            window: {
                title: "Moxin Voice"
                inner_size: vec2(1200, 800)
            }
            pass: { clear_color: (DARK_BG) }

            body = <View> {
                width: Fill, height: Fill
                flow: Down

                // Direct TTS screen (no wrapper, no sidebar)
                tts_screen = <TTSScreen> {}
            }
        }

        // ── Translation Overlay Window ────────────────────────────────────────
        // Starts hidden. Shown when the user activates translation mode in
        // the main screen. The window floats independently over any content.
        translation_ui: <Window> {
            window: {
                title: "Moxin Voice — Translation"
                inner_size: vec2(600, 260)
                position: vec2(100, 100)
            }
            pass: { clear_color: (MOXIN_BG_PRIMARY_DARK) }
            visible: false

            body = <View> {
                width: Fill, height: Fill

                translation_overlay = <TranslationOverlay> {}
            }
        }
    }
}

// ============================================================================
// APP STRUCT
// ============================================================================

#[derive(Live, LiveHook)]
pub struct App {
    #[live]
    ui: WidgetRef,

    /// Translation overlay window (independent OS window)
    #[live]
    translation_ui: WidgetRef,

    #[rust]
    app_data: Option<Arc<RwLock<MoxinAppData>>>,

    /// Poll timer for reading SharedDoraState updates
    #[rust]
    poll_timer: Timer,
}

impl LiveRegister for App {
    fn live_register(cx: &mut Cx) {
        ::log::info!("LiveRegister::live_register called");

        // Register Makepad core widgets (Window, View, etc.)
        ::log::info!("Registering makepad_widgets");
        makepad_widgets::live_design(cx);

        // Register shared widgets and theme
        ::log::info!("Registering moxin_widgets");
        moxin_widgets::live_design(cx);
        ::log::info!("Registering moxin_ui");
        moxin_ui::live_design(cx);

        // Register TTS app
        ::log::info!("Registering MoxinTTSApp");
        MoxinTTSApp::live_design(cx);

        ::log::info!("LiveRegister::live_register completed");
    }
}

impl AppMain for App {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        self.ui.handle_event(cx, event, &mut Scope::empty());
        self.translation_ui.handle_event(cx, event, &mut Scope::empty());
        self.match_event(cx, event);
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        ::log::info!("Moxin Voice application started");

        // Initialize Dora state (new() already returns Arc<Self>)
        let dora_state = SharedDoraState::new();

        // Initialize app data with dora_state
        let app_data = Arc::new(RwLock::new(MoxinAppData::new(dora_state)));
        self.app_data = Some(app_data);

        // Note: TTSScreen will be automatically initialized by Makepad's live_design system
        // The screen can access shared state through the event system if needed

        // Start Dora dataflow if specified
        if let Some(dataflow_path) = &get_cli_args().dataflow {
            ::log::info!("Starting Dora dataflow: {}", dataflow_path);
            // TODO: Start dataflow via app_data's dora_state
            // This would typically involve calling dora_state.start_dataflow(dataflow_path)
        }

        // Poll SharedDoraState every 50 ms for translation updates
        self.poll_timer = cx.start_interval(0.05);

        ::log::info!("Moxin Voice initialization complete");
    }

    fn handle_timer(&mut self, cx: &mut Cx, event: &TimerEvent) {
        if !event.timer.is_timer(self.poll_timer) {
            return;
        }

        let dora_state = match &self.app_data {
            Some(data) => data.read().dora_state().clone(),
            None => return,
        };

        // ── Translation window visibility ─────────────────────────────────────
        if let Some(visible) = dora_state.translation_window_visible.read_if_dirty() {
            self.translation_ui.apply_over(cx, live! {
                visible: (visible)
            });
        }

        // ── Translation content update ────────────────────────────────────────
        if let Some(update_opt) = dora_state.translation.read_if_dirty() {
            let overlay_ref = self.translation_ui.widget(id!(body.translation_overlay));
            if let Some(mut overlay) = overlay_ref.borrow_mut::<TranslationOverlay>() {
                match update_opt {
                    Some(update) => {
                        overlay.set_translation(
                            cx,
                            &update.source_text,
                            &update.translation,
                            update.is_complete,
                        );
                    }
                    None => {
                        overlay.clear(cx);
                    }
                }
            }
        }
    }

    fn handle_shutdown(&mut self, _cx: &mut Cx) {
        ::log::info!("Moxin Voice application shutting down");

        // Cleanup Dora state if needed
        if self.app_data.is_some() {
            ::log::info!("Cleaning up Dora dataflow");
        }

        // Best-effort runtime cleanup for both dev and bundled app:
        // actively destroy running Dora dataflows on app exit.
        match Command::new("dora").arg("destroy").output() {
            Ok(output) => {
                if output.status.success() {
                    ::log::info!("`dora destroy` executed successfully");
                } else {
                    let stderr = String::from_utf8_lossy(&output.stderr);
                    let stderr_trimmed = stderr.trim();
                    if stderr_trimmed.is_empty() {
                        ::log::warn!(
                            "`dora destroy` exited with status: {}",
                            output.status
                        );
                    } else {
                        ::log::warn!(
                            "`dora destroy` exited with status {}: {}",
                            output.status,
                            stderr_trimmed
                        );
                    }
                }
            }
            Err(err) => {
                ::log::warn!("failed to execute `dora destroy`: {}", err);
            }
        }
    }
}

impl App {
    // Additional helper methods can be added here if needed
}

// ============================================================================
// APP ENTRY POINT
// ============================================================================

app_main!(App);
