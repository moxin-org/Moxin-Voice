//! # Translation Overlay Widget
//!
//! Full-window content widget displayed in the translation overlay window.
//! Shows a scrolling list of completed sentences (source + translation) and
//! the current in-progress ASR text at the bottom.
//!
//! ## Layout
//!
//! ```text
//! ┌──────────────────────────────────────────────┐
//! │                     ZH → EN      ● LISTENING │  toolbar
//! ├──────────────────────────────────────────────┤
//! │  [source 1 - small, gray]                    │  ↑
//! │  Translation 1 - large, white                │  │ scroll
//! │                                              │  │
//! │  [source 2 - small, gray]                    │  │
//! │  Translation 2 - large, white                │  │
//! │                                              │  │
//! │  [pending ASR text - small, amber]           │  ↓
//! │  (bottom_spacer — dynamic, for anchor)       │
//! └──────────────────────────────────────────────┘
//! ```
//!
//! ## Scroll anchor
//!
//! `scroll_anchor_fraction` controls where the *bottom edge* of the last
//! sentence sits inside the viewport after auto-scroll:
//!
//!   - `1.0`  → last sentence at the very bottom (classic scroll-to-bottom)
//!   - `0.5`  → last sentence at the middle of the viewport  ← default
//!   - `0.25` → last sentence in the upper quarter
//!
//! A `bottom_spacer` whose height = `viewport_h × (1 − anchor)` is appended
//! after the content so that scrolling to the physical bottom of the scroll
//! view places the last sentence at `anchor × viewport_h` from the top.
//!
//! **To change the default anchor**, edit `SCROLL_ANCHOR_FRACTION` below.

use makepad_widgets::*;

/// Where the last sentence appears in the viewport (0.0 = top, 1.0 = bottom).
/// Change this constant to reposition the auto-scroll target without touching
/// any other code.
const SCROLL_ANCHOR_FRACTION: f64 = 0.5;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    use crate::theme::FONT_REGULAR;
    use crate::theme::FONT_SEMIBOLD;
    use crate::theme::FONT_BOLD;
    use crate::theme::SLATE_300;
    use crate::theme::WHITE;
    use crate::theme::ACCENT_GREEN;
    use crate::theme::MOXIN_BG_PRIMARY_DARK;
    use crate::theme::MOXIN_BG_SECONDARY_DARK;
    use crate::theme::MOXIN_TEXT_MUTED_DARK;

    pub TranslationOverlay = {{TranslationOverlay}} {
        width: Fill, height: Fill
        flow: Down
        show_bg: true
        draw_bg: {
            instance bg_opacity: 1.0
            fn pixel(self) -> vec4 {
                let base = (MOXIN_BG_PRIMARY_DARK);
                return vec4(base.x, base.y, base.z, self.bg_opacity);
            }
        }

        // ── Toolbar ──────────────────────────────────────────────────────────
        // Left side is intentionally empty (macOS window buttons occupy that space).
        toolbar = <View> {
            width: Fill, height: 44
            flow: Right
            align: { y: 0.5 }
            padding: { left: 16, right: 16 }
            show_bg: true
            draw_bg: {
                instance bg_opacity: 1.0
                fn pixel(self) -> vec4 {
                    let base = (MOXIN_BG_SECONDARY_DARK);
                    return vec4(base.x, base.y, base.z, self.bg_opacity);
                }
            }

            // Spacer pushes lang + status to the right
            <View> { width: Fill, height: 1 }

            lang_label = <Label> {
                width: Fit
                margin: { right: 12 }
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

        // ── Scrolling sentence list ──────────────────────────────────────────
        content_scroll = <ScrollYView> {
            width: Fill, height: Fill
            flow: Down
            padding: { left: 16, right: 16, top: 12, bottom: 0 }

            // history_label: all completed sentences rendered as a single text block
            history_label = <Label> {
                width: Fill, height: Fit
                draw_text: {
                    color: (WHITE)
                    text_style: <FONT_REGULAR> { font_size: 14.0 }
                    wrap: Word
                }
                text: ""
            }

            // pending_label: current ASR text (not yet translated)
            pending_label = <Label> {
                width: Fill, height: Fit
                margin: { top: 8 }
                draw_text: {
                    color: (MOXIN_TEXT_MUTED_DARK)
                    text_style: <FONT_REGULAR> { font_size: 13.0 }
                    wrap: Word
                }
                text: ""
            }

            // Dynamic spacer — height is recomputed each time the viewport
            // changes so that the scroll anchor stays correct.
            // Do NOT place any content after this view.
            bottom_spacer = <View> {
                width: Fill
                height: 0.0
            }
        }
    }
}

/// Translation display status
#[derive(Debug, Clone, PartialEq, Default)]
pub enum TranslationStatus {
    #[default]
    WarmingUp,
    Listening,
    Transcribing,
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

    /// Cached history length for detecting changes
    #[rust]
    last_history_len: usize,

    /// Cached pending text for detecting changes
    #[rust]
    last_pending_text: String,

    /// Where the bottom of the last sentence sits in the viewport.
    /// Defaults to SCROLL_ANCHOR_FRACTION (0.5 = middle).
    #[rust]
    scroll_anchor_fraction: f64,

    /// Viewport height from the most recent draw — used to update the spacer.
    #[rust]
    last_viewport_height: f64,

    /// True when content changed and we need to scroll to bottom on next draw.
    #[rust]
    pending_scroll: bool,
}

impl Widget for TranslationOverlay {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // ── Update bottom_spacer height ───────────────────────────────────────
        //
        // viewport_h from previous frame (1-frame lag is imperceptible).
        // spacer_h = viewport_h × (1 − anchor) ensures that scrolling to the
        // physical bottom lands the last sentence at anchor × viewport_h.

        let viewport_h = self
            .view
            .view(ids!(content_scroll))
            .area()
            .rect(cx)
            .size
            .y;

        if viewport_h > 1.0 && (viewport_h - self.last_viewport_height).abs() > 1.0 {
            self.last_viewport_height = viewport_h;
            let spacer_h = viewport_h * (1.0 - self.scroll_anchor_fraction);
            self.view
                .view(ids!(content_scroll.bottom_spacer))
                .apply_over(cx, live! { height: (spacer_h) });
            // Spacer changed → re-scroll to maintain anchor.
            self.pending_scroll = true;
        }

        // ── Draw content first ────────────────────────────────────────────────
        //
        // IMPORTANT: draw_walk must complete before set_scroll_pos is called.
        // Reason: scroll_bar.set_scroll_pos() clamps immediately to
        //   min(value, view_total - view_visible)
        // where view_total is updated only during draw_scroll_bars() which runs
        // at the end of view.draw_walk(). If we set scroll before draw, view_total
        // is stale (previous frame) and f64::MAX clamps to old max — often 0.
        //
        // After draw_walk, view_total reflects the freshly laid-out content, so
        // f64::MAX clamps to the correct new maximum scroll position.

        let result = self.view.draw_walk(cx, scope, walk);

        // ── Set scroll after draw (view_total is now current) ─────────────────
        if self.pending_scroll {
            self.pending_scroll = false;
            self.view
                .view(ids!(content_scroll))
                .set_scroll_pos(cx, dvec2(0.0, f64::MAX));
            // One more redraw to render with the updated scroll position.
            self.view.redraw(cx);
        }

        result
    }
}

impl TranslationOverlay {
    fn after_new_from_doc(&mut self, _cx: &mut Cx) {
        self.scroll_anchor_fraction = SCROLL_ANCHOR_FRACTION;
    }

    /// Set the language pair label, e.g. "ZH → EN"
    pub fn set_lang_pair(&mut self, cx: &mut Cx, src: &str, tgt: &str) {
        self.lang_pair = format!("{} → {}", src.to_uppercase(), tgt.to_uppercase());
        self.view
            .label(ids!(toolbar.lang_label))
            .set_text(cx, &self.lang_pair);
    }

    /// Set overlay background opacity (0.0 = fully transparent, 1.0 = opaque).
    pub fn set_opacity(&mut self, cx: &mut Cx, opacity: f64) {
        self.view.apply_over(cx, live! {
            draw_bg: { bg_opacity: (opacity) }
        });
        self.view.view(ids!(toolbar)).apply_over(cx, live! {
            draw_bg: { bg_opacity: (opacity) }
        });
        self.view.redraw(cx);
    }

    /// Render translation history and pending ASR text.
    ///
    /// `history` — completed sentences as `(source_text, translation)` pairs.
    /// `pending` — current in-progress ASR text (not yet translated).
    pub fn set_translation_update(
        &mut self,
        cx: &mut Cx,
        history: &[(String, String)],
        pending: &str,
    ) {
        // Build history text: each sentence is "source\ntranslation\n"
        let history_text = Self::format_history(history);
        self.view
            .label(ids!(content_scroll.history_label))
            .set_text(cx, &history_text);

        // Pending ASR text
        let pending = pending.trim();
        self.view
            .label(ids!(content_scroll.pending_label))
            .set_text(cx, pending);

        // Update status
        self.status = if !pending.is_empty() {
            TranslationStatus::Transcribing
        } else if !history.is_empty() {
            TranslationStatus::Complete
        } else {
            TranslationStatus::Listening
        };
        self.update_status_label(cx);

        // ── Auto-scroll ───────────────────────────────────────────────────────
        //
        // Key insight: we call set_scroll_pos(f64::MAX) here — BEFORE the next
        // draw. The scroll bars store f64::MAX; during the NEXT draw_walk the
        // scroll view lays out the freshly-updated content, computes the real
        // max_scroll, and clamps f64::MAX to it automatically.
        //
        // This is intentionally done here (not inside draw_walk) so the clamping
        // uses the CURRENT frame's new layout rather than the stale previous one.

        let new_len = history.len();
        let pending_changed = pending != self.last_pending_text;
        if new_len != self.last_history_len || pending_changed {
            self.last_history_len = new_len;
            self.last_pending_text = pending.to_string();
            // Mark scroll pending — actual set_scroll_pos happens in draw_walk
            // AFTER view.draw_walk() so that view_total reflects new content.
            self.pending_scroll = true;
        }

        self.view.redraw(cx);
    }

    fn format_history(history: &[(String, String)]) -> String {
        let mut out = String::new();
        for (i, (source, translation)) in history.iter().enumerate() {
            if i > 0 {
                out.push('\n');
            }
            out.push_str(source.trim());
            out.push('\n');
            out.push_str(translation.trim());
            if i < history.len() - 1 {
                out.push('\n');
            }
        }
        out
    }

    /// Show warm-up state (status label only, no content text).
    pub fn set_warming_up(&mut self, cx: &mut Cx) {
        self.status = TranslationStatus::WarmingUp;
        self.update_status_label(cx);
        self.view.redraw(cx);
    }

    /// Show ready/listening state (status label only, no content text).
    pub fn set_listening(&mut self, cx: &mut Cx) {
        self.status = TranslationStatus::Listening;
        self.update_status_label(cx);
        self.view.redraw(cx);
    }

    /// Clear all text and reset to listening state.
    pub fn clear(&mut self, cx: &mut Cx) {
        self.last_history_len = 0;
        self.last_pending_text.clear();
        self.view
            .label(ids!(content_scroll.history_label))
            .set_text(cx, "");
        self.view
            .label(ids!(content_scroll.pending_label))
            .set_text(cx, "");
        self.set_listening(cx);
    }

    fn update_status_label(&self, cx: &mut Cx) {
        let (text, color) = match self.status {
            TranslationStatus::WarmingUp => ("● WARMING UP", vec4(0.906, 0.620, 0.204, 1.0)),
            TranslationStatus::Listening => ("● LISTENING", vec4(0.098, 0.725, 0.506, 1.0)),
            TranslationStatus::Transcribing => ("● TRANSCRIBING", vec4(0.906, 0.620, 0.204, 1.0)),
            TranslationStatus::Complete => ("✓ DONE", vec4(0.098, 0.725, 0.506, 1.0)),
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

    /// Update with sentence history and pending ASR text
    pub fn set_translation_update(
        &self,
        cx: &mut Cx,
        history: &[(String, String)],
        pending: &str,
    ) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_translation_update(cx, history, pending);
        }
    }

    /// Clear and reset to listening state
    pub fn clear(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.clear(cx);
        }
    }

    pub fn set_warming_up(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_warming_up(cx);
        }
    }

    pub fn set_listening(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_listening(cx);
        }
    }

    pub fn set_opacity(&self, cx: &mut Cx, opacity: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_opacity(cx, opacity);
        }
    }
}
