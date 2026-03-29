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

// ── macOS window alpha ────────────────────────────────────────────────────────
// Sets NSWindow.alphaValue on the window whose title contains `title_fragment`.
// NSWindow.alphaValue composites the entire window at the given opacity against
// the screen content behind it — no Makepad patches required.
#[cfg(target_os = "macos")]
unsafe fn set_nswindow_alpha(title_fragment: &str, alpha: f64) {
    use makepad_objc_sys::runtime::Object;
    #[allow(unused_imports)]
    use makepad_objc_sys::{class, msg_send, sel, sel_impl};
    let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
    let windows: *mut Object = msg_send![app, windows];
    let count: usize = msg_send![windows, count];
    for i in 0..count {
        let win: *mut Object = msg_send![windows, objectAtIndex: i];
        let title: *mut Object = msg_send![win, title];
        if title.is_null() {
            continue;
        }
        let utf8: *const std::os::raw::c_char = msg_send![title, UTF8String];
        if utf8.is_null() {
            continue;
        }
        let s = std::ffi::CStr::from_ptr(utf8).to_str().unwrap_or("");
        if s.contains(title_fragment) {
            let () = msg_send![win, setAlphaValue: alpha];
            return;
        }
    }
}

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

    #[rust]
    translation_overlay_visible: bool,

    /// Last opacity applied to the translation window; avoids per-tick ObjC calls.
    #[rust]
    last_overlay_opacity: f64,
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
            } else if self.translation_window_id == Some(ev.window_id) {
                // Keep anchor formula in sync with real window size (including
                // user resize and platform-specific window state transitions).
                let viewport_h = (ev.new_geom.inner_size.y - 44.0).max(0.0);
                let overlay_ref =
                    self.translation_ui.widget(ids!(body.translation_overlay));
                if let Some(mut overlay) = overlay_ref.borrow_mut::<TranslationOverlay>() {
                    overlay.set_viewport_height(cx, viewport_h);
                };
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
        self.translation_overlay_visible = false;
        self.last_overlay_opacity = -1.0; // force first apply

        // Set initial scroll anchor for compact window (260px high, 44px toolbar → 216px viewport).
        let overlay_ref = self.translation_ui.widget(ids!(body.translation_overlay));
        if let Some(mut overlay) = overlay_ref.borrow_mut::<TranslationOverlay>() {
            overlay.set_viewport_height(cx, 216.0);
        }

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
            self.translation_overlay_visible = window_visible;
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

            // Initialize overlay state immediately on show/hide.
            let overlay_ref = self.translation_ui.widget(ids!(body.translation_overlay));
            if window_visible {
                if let Some(mut overlay) = overlay_ref.borrow_mut::<TranslationOverlay>() {
                    overlay.set_warming_up(cx);
                }
            } else if let Some(mut overlay) = overlay_ref.borrow_mut::<TranslationOverlay>() {
                overlay.clear(cx);
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
            // Update scroll anchor after resize (toolbar is 44px).
            let viewport_h = size.y - 44.0;
            let overlay_ref = self.translation_ui.widget(ids!(body.translation_overlay));
            if let Some(mut overlay) = overlay_ref.borrow_mut::<TranslationOverlay>() {
                overlay.set_viewport_height(cx, viewport_h);
            };
        }

        // ── Translation content update ────────────────────────────────────────
        if let Some(update_opt) = dora_state.translation.read_if_dirty() {
            ::log::info!(
                "[translation_ui] received update: {}",
                match &update_opt {
                    Some(u) => format!(
                        "history={}, pending_len={}",
                        u.history.len(),
                        u.pending_source_text.len(),
                    ),
                    None => "clear".to_string(),
                }
            );
            let overlay_ref = self.translation_ui.widget(ids!(body.translation_overlay));
            if let Some(mut overlay) = overlay_ref.borrow_mut::<TranslationOverlay>() {
                match &update_opt {
                    Some(update) => {
                        let history: Vec<(String, String)> = update
                            .history
                            .iter()
                            .map(|u| (u.source_text.clone(), u.translation.clone()))
                            .collect();
                        overlay.set_translation_update(
                            cx,
                            &history,
                            &update.pending_source_text,
                        );
                    }
                    None => {
                        overlay.clear(cx);
                    }
                }
            } else {
                ::log::warn!(
                    "[translation_ui] TranslationOverlay borrow_mut failed"
                );
            };
            self.translation_ui.redraw(cx);
        }

        // ── Translation overlay status heartbeat (warming/listening) ──────────
        if self.translation_overlay_visible {
            let status_snapshot = dora_state.status.read();
            let bridges_ready = status_snapshot
                .active_bridges
                .iter()
                .any(|b| b == "moxin-mic-input")
                && status_snapshot
                    .active_bridges
                    .iter()
                    .any(|b| b == "moxin-translation-listener");

            // If no active translation update, keep status informative.
            let translation_snapshot = dora_state.translation.read();
            if translation_snapshot.is_none() {
                let overlay_ref = self.translation_ui.widget(ids!(body.translation_overlay));
                if let Some(mut overlay) = overlay_ref.borrow_mut::<TranslationOverlay>() {
                    if bridges_ready {
                        overlay.set_listening(cx);
                    } else {
                        overlay.set_warming_up(cx);
                    }
                };
            }
        }

        // ── Translation overlay opacity ──────────────────────────────────────
        let opacity = dora_state.translation_overlay_opacity.read();
        if (opacity - self.last_overlay_opacity).abs() > 0.001 {
            self.last_overlay_opacity = opacity;
            // On macOS: use NSWindow.setAlphaValue to composite the entire window
            // at the given opacity against the screen — no Makepad patches needed.
            #[cfg(target_os = "macos")]
            unsafe {
                set_nswindow_alpha("Translation", opacity);
            }
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
