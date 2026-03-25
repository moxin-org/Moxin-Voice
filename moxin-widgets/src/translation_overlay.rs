//! # Translation Overlay Widget
//!
//! Full-window content widget displayed in the translation overlay window.
//! Shows the original ASR text (source language) and its live translation
//! (target language) as they stream in from `dora-qwen3-translator`.
//!
//! ## Layout
//!
//! ```text
//! ┌──────────────────────────────────────────────┐
//! │  ZH → EN                         ● LISTENING │  toolbar
//! ├──────────────────────────────────────────────┤
//! │  [source text, small, dimmed]                │
//! │                                              │
//! │  TRANSLATION TEXT (large, bright)            │
//! │  ...streaming tokens appended here...        │
//! └──────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! // In shell handle_event, on timer:
//! if let Some(update) = shared_state.translation.read_if_dirty() {
//!     let overlay = self.ui.translation_overlay(id!(translation_overlay));
//!     match update {
//!         Some(u) => overlay.set_translation(cx, &u.source_text, &u.translation, u.is_complete),
//!         None    => overlay.clear(cx),
//!     }
//! }
//! ```

use makepad_widgets::*;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    use crate::theme::FONT_REGULAR;
    use crate::theme::FONT_SEMIBOLD;
    use crate::theme::FONT_BOLD;
    use crate::theme::SLATE_800;
    use crate::theme::SLATE_400;
    use crate::theme::SLATE_300;
    use crate::theme::WHITE;
    use crate::theme::ACCENT_BLUE;
    use crate::theme::MOXIN_BG_PRIMARY_DARK;
    use crate::theme::MOXIN_BG_SECONDARY_DARK;
    use crate::theme::MOXIN_TEXT_MUTED_DARK;
    use crate::theme::ACCENT_GREEN;

    pub TranslationOverlay = {{TranslationOverlay}} {
        width: Fill, height: Fill
        flow: Down
        show_bg: true
        draw_bg: { color: (MOXIN_BG_PRIMARY_DARK) }

        // ── Toolbar ──────────────────────────────────────────────────────────
        toolbar = <View> {
            width: Fill, height: 44
            flow: Right
            align: { y: 0.5 }
            padding: { left: 16, right: 16 }
            show_bg: true
            draw_bg: { color: (MOXIN_BG_SECONDARY_DARK) }

            lang_label = <Label> {
                width: Fill
                draw_text: {
                    color: (SLATE_300)
                    text_style: <FONT_SEMIBOLD> { font_size: 13.0 }
                }
                text: "ZH → EN"
            }

            status_label = <Label> {
                width: Fit
                draw_text: {
                    color: (ACCENT_GREEN)
                    text_style: <FONT_REGULAR> { font_size: 11.0 }
                }
                text: "● LISTENING"
            }
        }

        // ── Source text ───────────────────────────────────────────────────────
        source_area = <View> {
            width: Fill, height: Fit
            flow: Down
            padding: { left: 16, right: 16, top: 10, bottom: 6 }

            source_label = <Label> {
                width: Fill, height: Fit
                draw_text: {
                    color: (MOXIN_TEXT_MUTED_DARK)
                    text_style: <FONT_REGULAR> { font_size: 12.0 }
                    wrap: Word
                }
                text: ""
            }
        }

        // ── Divider ───────────────────────────────────────────────────────────
        <View> {
            width: Fill, height: 1
            margin: { left: 16, right: 16 }
            show_bg: true
            draw_bg: { color: #2a2a3a }
        }

        // ── Translation text ──────────────────────────────────────────────────
        translation_scroll = <ScrollYView> {
            width: Fill, height: Fill
            flow: Down
            padding: { left: 16, right: 16, top: 12, bottom: 16 }

            translation_label = <Label> {
                width: Fill, height: Fit
                draw_text: {
                    color: (WHITE)
                    text_style: <FONT_BOLD> { font_size: 18.0 }
                    wrap: Word
                }
                text: ""
            }
        }
    }
}

/// Translation display status
#[derive(Debug, Clone, PartialEq, Default)]
pub enum TranslationStatus {
    #[default]
    Listening,
    Translating,
    Complete,
}

#[derive(Live, LiveHook, Widget)]
pub struct TranslationOverlay {
    #[deref]
    view: View,

    /// Language pair display string, e.g. "ZH → EN"
    #[rust]
    lang_pair: String,

    /// Current display status
    #[rust]
    status: TranslationStatus,
}

impl Widget for TranslationOverlay {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

impl TranslationOverlay {
    /// Set the language pair label, e.g. "ZH → EN"
    pub fn set_lang_pair(&mut self, cx: &mut Cx, src: &str, tgt: &str) {
        self.lang_pair = format!("{} → {}", src.to_uppercase(), tgt.to_uppercase());
        self.view
            .label(ids!(toolbar.lang_label))
            .set_text(cx, &self.lang_pair);
    }

    /// Update translation content (called on each streaming chunk or completion).
    ///
    /// `source_text` — original ASR transcription
    /// `translation`  — accumulated translation text so far
    /// `is_complete`  — true when the sentence is fully translated
    pub fn set_translation(
        &mut self,
        cx: &mut Cx,
        source_text: &str,
        translation: &str,
        is_complete: bool,
    ) {
        self.view
            .label(ids!(source_area.source_label))
            .set_text(cx, source_text);
        self.view
            .label(ids!(translation_scroll.translation_label))
            .set_text(cx, translation);

        self.status = if is_complete {
            TranslationStatus::Complete
        } else {
            TranslationStatus::Translating
        };
        self.update_status_label(cx);
        self.view.redraw(cx);
    }

    /// Clear all text and reset to listening state.
    pub fn clear(&mut self, cx: &mut Cx) {
        self.view
            .label(ids!(source_area.source_label))
            .set_text(cx, "");
        self.view
            .label(ids!(translation_scroll.translation_label))
            .set_text(cx, "");
        self.status = TranslationStatus::Listening;
        self.update_status_label(cx);
        self.view.redraw(cx);
    }

    fn update_status_label(&self, cx: &mut Cx) {
        let (text, color) = match self.status {
            TranslationStatus::Listening => ("● LISTENING", vec4(0.098, 0.725, 0.506, 1.0)),    // green
            TranslationStatus::Translating => ("● TRANSLATING", vec4(0.231, 0.510, 0.831, 1.0)), // blue
            TranslationStatus::Complete => ("✓ DONE", vec4(0.098, 0.725, 0.506, 1.0)),           // green
        };
        let label = self.view.label(ids!(toolbar.status_label));
        label.set_text(cx, text);
        label.apply_over(cx, live! { draw_text: { color: (color) } });
    }
}

impl TranslationOverlayRef {
    /// Set the language pair, e.g. ("ZH", "EN")
    pub fn set_lang_pair(&self, cx: &mut Cx, src: &str, tgt: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_lang_pair(cx, src, tgt);
        }
    }

    /// Update with a new translation chunk from SharedDoraState
    pub fn set_translation(
        &self,
        cx: &mut Cx,
        source_text: &str,
        translation: &str,
        is_complete: bool,
    ) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_translation(cx, source_text, translation, is_complete);
        }
    }

    /// Clear and reset to listening state
    pub fn clear(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.clear(cx);
        }
    }
}
