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

// ── macOS hide traffic lights ─────────────────────────────────────────────────
// Hides the close/minimize/zoom buttons on the window whose title contains
// `title_fragment`. Hidden state persists across minimize/restore cycles.
#[cfg(target_os = "macos")]
unsafe fn hide_nswindow_traffic_lights(title_fragment: &str) {
    use makepad_objc_sys::runtime::{Object, YES};
    #[allow(unused_imports)]
    use makepad_objc_sys::{class, msg_send, sel, sel_impl};
    let app: *mut Object = msg_send![class!(NSApplication), sharedApplication];
    let windows: *mut Object = msg_send![app, windows];
    let count: usize = msg_send![windows, count];
    for i in 0..count {
        let win: *mut Object = msg_send![windows, objectAtIndex: i];
        let title: *mut Object = msg_send![win, title];
        if title.is_null() { continue; }
        let utf8: *const std::os::raw::c_char = msg_send![title, UTF8String];
        if utf8.is_null() { continue; }
        let s = std::ffi::CStr::from_ptr(utf8).to_str().unwrap_or("");
        if s.contains(title_fragment) {
            // NSWindowCloseButton=0, NSWindowMiniaturizeButton=1, NSWindowZoomButton=2
            for btn_type in [0usize, 1usize, 2usize] {
                let btn: *mut Object = msg_send![win, standardWindowButton: btn_type];
                if !btn.is_null() {
                    let () = msg_send![btn, setHidden: YES];
                }
            }
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
    main_window_id: Option<WindowId>,

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
                // Remove traffic light buttons from the overlay window.
                #[cfg(target_os = "macos")]
                unsafe { hide_nswindow_traffic_lights("Translation"); }
                // Keep hidden by default at startup.
                cx.push_unique_platform_op(CxOsOp::MinimizeWindow(ev.window_id));
            } else if self.main_window_id.is_none() {
                self.main_window_id = Some(ev.window_id);
                ::log::info!("[main_ui] detected window_id={:?}", ev.window_id);
            } else if self.translation_window_id == Some(ev.window_id) {
                // Keep anchor formula in sync with real window size (including
                // user resize and platform-specific window state transitions).
                let viewport_h = (ev.new_geom.inner_size.y - 38.0).max(0.0);
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
            } else if Self::should_intercept_main_window_close(
                Some(ev.window_id),
                self.main_window_id,
            ) {
                // Keep the main window restorable from the dock instead of
                // letting macOS promote the minimized overlay as the only window.
                ev.accept_close.set(false);
                cx.push_unique_platform_op(CxOsOp::MinimizeWindow(ev.window_id));
                ::log::info!("[main_ui] close intercepted -> minimize");
            }
        }

        if let Event::WindowGotFocus(window_id) = event {
            if Self::should_redirect_overlay_focus(
                Some(*window_id),
                self.translation_window_id,
                self.main_window_id,
                self.translation_overlay_visible,
            ) {
                ::log::info!(
                    "[translation_ui] unexpected focus while hidden -> restore main window"
                );
                cx.push_unique_platform_op(CxOsOp::MinimizeWindow(*window_id));
                if let Some(main_window_id) = self.main_window_id {
                    #[cfg(target_os = "macos")]
                    cx.push_unique_platform_op(CxOsOp::Deminiaturize(main_window_id));
                    #[cfg(not(target_os = "macos"))]
                    cx.push_unique_platform_op(CxOsOp::RestoreWindow(main_window_id));
                }
            }
        }

        self.ui.handle_event(cx, event, &mut Scope::empty());
        self.translation_ui.handle_event(cx, event, &mut Scope::empty());
        self.match_event(cx, event);
    }
}

impl MatchEvent for App {
    fn handle_actions(&mut self, _cx: &mut Cx, _actions: &Actions) {
        // Translation overlay no longer emits actions; nothing to handle here.
    }

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
        self.main_window_id = None;
        self.translation_overlay_visible = false;
        self.last_overlay_opacity = -1.0; // force first apply

        // Set initial scroll anchor for compact window (260px high, 44px toolbar → 216px viewport).
        let overlay_ref = self.translation_ui.widget(ids!(body.translation_overlay));
        if let Some(mut overlay) = overlay_ref.borrow_mut::<TranslationOverlay>() {
            overlay.set_viewport_height(cx, 222.0);
            overlay.set_font_size_preset(cx, "24");
            overlay.set_anchor_position_preset(cx, "50");
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
                    cx.push_unique_platform_op(CxOsOp::Deminiaturize(window_id));
                    #[cfg(not(target_os = "macos"))]
                    cx.push_unique_platform_op(CxOsOp::RestoreWindow(window_id));
                } else {
                    cx.push_unique_platform_op(CxOsOp::MinimizeWindow(window_id));
                }
            }

            // Reset overlay content on hide so a future re-open starts clean.
            if !window_visible {
                let overlay_ref = self.translation_ui.widget(ids!(body.translation_overlay));
                if let Some(mut overlay) = overlay_ref.borrow_mut::<TranslationOverlay>() {
                    overlay.clear(cx);
                };
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
            // No toolbar anymore — viewport height is the full inner size minus
            // the (auto-sized) footer; the widget falls back to measured height
            // when this hint is too coarse, so passing the full size is fine.
            let viewport_h = size.y;
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

        // Locale and language-pair fields are still pushed by screen.rs but no
        // longer drive any overlay UI (toolbar removed). Drain their dirty bits
        // so they don't keep firing redundant work.
        let _ = dora_state.translation_locale_en.read_if_dirty();
        let _ = dora_state.translation_lang_pair.read_if_dirty();

        if let Some(preset) = dora_state.translation_font_size_preset.read_if_dirty() {
            let overlay_ref = self.translation_ui.widget(ids!(body.translation_overlay));
            if let Some(mut overlay) = overlay_ref.borrow_mut::<TranslationOverlay>() {
                overlay.set_font_size_preset(cx, &preset);
            };
        }

        if let Some(preset) = dora_state.translation_footer_font_size_preset.read_if_dirty() {
            let overlay_ref = self.translation_ui.widget(ids!(body.translation_overlay));
            if let Some(mut overlay) = overlay_ref.borrow_mut::<TranslationOverlay>() {
                overlay.set_footer_font_size_preset(cx, &preset);
            };
        }

        if let Some(preset) = dora_state.translation_anchor_position_preset.read_if_dirty() {
            let overlay_ref = self.translation_ui.widget(ids!(body.translation_overlay));
            if let Some(mut overlay) = overlay_ref.borrow_mut::<TranslationOverlay>() {
                overlay.set_anchor_position_preset(cx, &preset);
            };
        }

        // ── Translation overlay status heartbeat (warming/listening) ──────────
        // The overlay no longer renders this status itself, but the settings
        // page in screen.rs reads `translation_overlay_status` and shows it on
        // the runtime-logs card. Keep the bridge-readiness check here.
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

            let new_status = if bridges_ready { "listening" } else { "warming" };
            // Set unconditionally; DirtyValue collapses redundant writes for the
            // consumer side (read_if_dirty), and screen.rs guards on actual change.
            dora_state
                .translation_overlay_status
                .set(new_status.to_string());
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
        self.ui
            .ttsscreen(ids!(body.tts_screen))
            .shutdown_cleanup();
    }
}

impl App {
    fn should_intercept_main_window_close(
        window_id: Option<WindowId>,
        main_window_id: Option<WindowId>,
    ) -> bool {
        matches!((window_id, main_window_id), (Some(window_id), Some(main_window_id)) if window_id == main_window_id)
    }

    fn should_redirect_overlay_focus(
        focused_window_id: Option<WindowId>,
        translation_window_id: Option<WindowId>,
        main_window_id: Option<WindowId>,
        translation_overlay_visible: bool,
    ) -> bool {
        matches!(
            (focused_window_id, translation_window_id, main_window_id, translation_overlay_visible),
            (Some(focused_window_id), Some(translation_window_id), Some(_), false)
                if focused_window_id == translation_window_id
        )
    }

    fn is_translation_window_geom(geom: &WindowGeom) -> bool {
        let w = geom.inner_size.x;
        let h = geom.inner_size.y;
        (w - 600.0).abs() < 2.0 && (h - 260.0).abs() < 2.0
    }
}

#[cfg(test)]
mod tests {
    use super::App;
    use makepad_widgets::WindowId;

    #[test]
    fn main_window_close_is_intercepted_when_main_window_is_known() {
        let window_id = WindowId(1, 1);
        assert!(App::should_intercept_main_window_close(
            Some(window_id),
            Some(window_id)
        ));
        assert!(!App::should_intercept_main_window_close(
            Some(WindowId(2, 1)),
            Some(window_id)
        ));
    }

    #[test]
    fn hidden_overlay_focus_is_redirected_back_to_main_window() {
        let main_window_id = WindowId(1, 1);
        let overlay_window_id = WindowId(2, 1);
        assert!(App::should_redirect_overlay_focus(
            Some(overlay_window_id),
            Some(overlay_window_id),
            Some(main_window_id),
            false
        ));
        assert!(!App::should_redirect_overlay_focus(
            Some(overlay_window_id),
            Some(overlay_window_id),
            Some(main_window_id),
            true
        ));
    }
}

// ============================================================================
// APP ENTRY POINT
// ============================================================================

app_main!(App);
