//! Moxin Voice App - Main application
//!
//! This is a simplified shell that directly shows the TTS screen
//! without sidebar, tabs, or app switching.

use makepad_widgets::*;
use moxin_dora_bridge::SharedDoraState;
use moxin_ui::MoxinAppData;
use moxin_voice::MoxinTTSApp;
use moxin_widgets::MoxinApp;
use parking_lot::RwLock;
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

    // Import TTS screen
    use moxin_voice::screen::TTSScreen;

    // ========================================================================
    // App Window - Simplified (no sidebar, no tabs)
    // ========================================================================

    App = {{App}} {
        ui: <Window> {
            window: {
                title: "Moxin Voice - Voice Cloning & Text-to-Speech"
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
    }
}

// ============================================================================
// APP STRUCT
// ============================================================================

#[derive(Live, LiveHook)]
pub struct App {
    #[live]
    ui: WidgetRef,

    #[rust]
    app_data: Option<Arc<RwLock<MoxinAppData>>>,
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
        self.match_event(cx, event);
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, _cx: &mut Cx) {
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

        ::log::info!("Moxin Voice initialization complete");
    }

    fn handle_shutdown(&mut self, _cx: &mut Cx) {
        ::log::info!("Moxin Voice application shutting down");

        // Cleanup Dora state if needed
        if self.app_data.is_some() {
            // TODO: Stop dataflow gracefully
            ::log::info!("Cleaning up Dora dataflow");
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
