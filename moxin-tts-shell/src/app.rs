//! Moxin TTS App - Main application
//!
//! This is a simplified shell that directly shows the TTS screen
//! without sidebar, tabs, or app switching.

use makepad_widgets::*;
use mofa_dora_bridge::SharedDoraState;
use mofa_ui::MofaAppData;
use mofa_tts::MoFaTTSApp;
use mofa_widgets::MofaApp;
use std::sync::OnceLock;

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

    use mofa_widgets::theme::DARK_BG;

    // Import TTS screen
    use mofa_tts::screen::TTSScreen;

    // ========================================================================
    // App Window - Simplified (no sidebar, no tabs)
    // ========================================================================

    App = {{App}} {
        ui: <Window> {
            window: {
                title: "Moxin TTS - Voice Cloning & Text-to-Speech"
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
    app_data: Option<MofaAppData>,
}

impl LiveRegister for App {
    fn live_register(cx: &mut Cx) {
        ::log::info!("LiveRegister::live_register called");

        // Register Makepad core widgets (Window, View, etc.)
        ::log::info!("Registering makepad_widgets");
        makepad_widgets::live_design(cx);

        // Register shared widgets and theme
        ::log::info!("Registering mofa_widgets");
        mofa_widgets::live_design(cx);
        ::log::info!("Registering mofa_ui");
        mofa_ui::live_design(cx);

        // Register TTS app
        ::log::info!("Registering MoFaTTSApp");
        MoFaTTSApp::live_design(cx);

        ::log::info!("LiveRegister::live_register completed");
    }
}

impl AppMain for App {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        if let Some(app_data) = self.app_data.as_mut() {
            self.ui
                .handle_event(cx, event, &mut Scope::with_data(app_data));
        } else {
            self.ui.handle_event(cx, event, &mut Scope::empty());
        }
        self.match_event(cx, event);
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, _cx: &mut Cx) {
        ::log::info!("Moxin TTS application started");

        // Initialize Dora state (new() already returns Arc<Self>)
        let dora_state = SharedDoraState::new();

        // Initialize app data with dora_state
        self.app_data = Some(MofaAppData::new(dora_state));

        // Note: TTSScreen will be automatically initialized by Makepad's live_design system
        // The screen can access shared state through the event system if needed

        // Start Dora dataflow if specified
        if let Some(dataflow_path) = &get_cli_args().dataflow {
            ::log::info!("Starting Dora dataflow: {}", dataflow_path);
            // TODO: Start dataflow via app_data's dora_state
            // This would typically involve calling dora_state.start_dataflow(dataflow_path)
        }

        ::log::info!("Moxin TTS initialization complete");
    }

    fn handle_shutdown(&mut self, _cx: &mut Cx) {
        ::log::info!("Moxin TTS application shutting down");

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
