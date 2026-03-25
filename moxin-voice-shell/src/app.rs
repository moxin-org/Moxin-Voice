//! Moxin Voice App - Main application
//!
//! This is a simplified shell that directly shows the TTS screen
//! without sidebar, tabs, or app switching.

use makepad_widgets::*;
use makepad_widgets::event::WindowGeom;
use moxin_voice::MoxinTTSApp;
use moxin_voice::TTSScreenWidgetRefExt;
use moxin_widgets::MoxinApp;
use moxin_widgets::translation_overlay::TranslationOverlay;
use std::process::Command;
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

    /// Poll timer for reading SharedDoraState updates
    #[rust]
    poll_timer: Timer,

    #[rust]
    translation_window_id: Option<WindowId>,
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
        if let Event::WindowGeomChange(ev) = event {
            if self.translation_window_id.is_none()
                && Self::is_translation_window_geom(&ev.new_geom)
            {
                self.translation_window_id = Some(ev.window_id);
                ::log::info!(
                    "[translation_ui] detected window_id={:?}",
                    ev.window_id
                );
                // Keep hidden by default at startup.
                cx.push_unique_platform_op(CxOsOp::MinimizeWindow(ev.window_id));
            }
        }

        if let Event::WindowCloseRequested(ev) = event {
            if self.translation_window_id == Some(ev.window_id) {
                // Prevent actual destroy; treat close as "hide".
                ev.accept_close.set(false);
                cx.push_unique_platform_op(CxOsOp::MinimizeWindow(ev.window_id));
                ::log::info!("[translation_ui] close intercepted -> minimize");
            }
        }

        self.ui.handle_event(cx, event, &mut Scope::empty());
        self.translation_ui.handle_event(cx, event, &mut Scope::empty());
        self.match_event(cx, event);
    }
}

impl MatchEvent for App {
    fn handle_startup(&mut self, cx: &mut Cx) {
        ::log::info!("Moxin Voice application started");

        // Keep window widget itself visible; use OS minimize/restore for show/hide.
        // Otherwise an OS-restored window may render only clear color (black) with no widgets.
        self.translation_ui.set_visible(cx, true);

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
        if self.poll_timer.is_timer(event).is_none() {
            return;
        }

        let dora_state = match self
            .ui
            .ttsscreen(ids!(body.tts_screen))
            .translation_shared_dora_state()
        {
            Some(state) => state,
            None => return,
        };

        // ── Translation window visibility ─────────────────────────────────────
        if let Some(visible) = dora_state.translation_window_visible.read_if_dirty() {
            let window_visible: bool = visible;
            ::log::info!("[translation_ui] set_visible={}", window_visible);
            if let Some(window_id) = self.translation_window_id {
                if window_visible {
                    #[cfg(target_os = "macos")]
                    {
                        // On macOS, RestoreWindow may reopen in prior zoom/fullscreen state.
                        // Deminiaturize + Normalize keeps overlay in its configured size.
                        cx.push_unique_platform_op(CxOsOp::Deminiaturize(window_id));
                        cx.push_unique_platform_op(CxOsOp::NormalizeWindow(window_id));
                    }
                    #[cfg(not(target_os = "macos"))]
                    {
                        cx.push_unique_platform_op(CxOsOp::RestoreWindow(window_id));
                    }
                } else {
                    cx.push_unique_platform_op(CxOsOp::MinimizeWindow(window_id));
                }
            }
        }

        // ── Translation overlay fullscreen toggle ─────────────────────────────
        if let Some(fullscreen) = dora_state.translation_overlay_fullscreen.read_if_dirty() {
            let size = if fullscreen {
                dvec2(900.0, 600.0)
            } else {
                dvec2(600.0, 260.0)
            };
            self.translation_ui.as_window().resize(cx, size);
        }

        // ── Translation content update ────────────────────────────────────────
        if let Some(update_opt) = dora_state.translation.read_if_dirty() {
            ::log::info!(
                "[translation_ui] received update: {}",
                match &update_opt {
                    Some(u) => format!(
                        "source_len={}, translation_len={}, complete={}",
                        u.source_text.len(),
                        u.translation.len(),
                        u.is_complete
                    ),
                    None => "clear".to_string(),
                }
            );
            let overlay_ref = self.translation_ui.widget(ids!(body.translation_overlay));
            let mut applied_via_widget = false;
            if let Some(mut overlay) = overlay_ref.borrow_mut::<TranslationOverlay>() {
                applied_via_widget = true;
                match &update_opt {
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
            } else {
                ::log::warn!(
                    "[translation_ui] TranslationOverlay borrow_mut failed, using label fallback"
                );
            };

            // Fallback path: update labels directly to avoid dropping visible updates
            // when typed widget borrow fails for any reason.
            if !applied_via_widget {
                match &update_opt {
                    Some(update) => {
                        self.translation_ui
                            .label(ids!(body.translation_overlay.source_area.source_label))
                            .set_text(cx, &update.source_text);
                        self.translation_ui
                            .label(ids!(body.translation_overlay.translation_scroll.translation_label))
                            .set_text(cx, &update.translation);
                        let status_text = if update.is_complete {
                            "✓ DONE"
                        } else {
                            "● TRANSLATING"
                        };
                        self.translation_ui
                            .label(ids!(body.translation_overlay.toolbar.status_label))
                            .set_text(cx, status_text);
                    }
                    None => {
                        self.translation_ui
                            .label(ids!(body.translation_overlay.source_area.source_label))
                            .set_text(cx, "");
                        self.translation_ui
                            .label(ids!(body.translation_overlay.translation_scroll.translation_label))
                            .set_text(cx, "");
                        self.translation_ui
                            .label(ids!(body.translation_overlay.toolbar.status_label))
                            .set_text(cx, "● LISTENING");
                    }
                };
            };
            self.translation_ui.redraw(cx);
        }
    }

    fn handle_shutdown(&mut self, _cx: &mut Cx) {
        ::log::info!("Moxin Voice application shutting down");

        ::log::info!("Cleaning up Dora dataflow");

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
    fn is_translation_window_geom(geom: &WindowGeom) -> bool {
        let w = geom.inner_size.x;
        let h = geom.inner_size.y;
        (w - 600.0).abs() < 2.0 && (h - 260.0).abs() < 2.0
    }
}

// ============================================================================
// APP ENTRY POINT
// ============================================================================

app_main!(App);
