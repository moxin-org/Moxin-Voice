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
//! `bottom_spacer` height and pending_label margin (60px for translation placeholder)
//! create an anchor effect: when scrolled to bottom, the last sentence appears at
//! ~50% of viewport height.
//!
//! Behavior rules:
//! 1) In-progress text (ASR/translating) and completed text share the same
//!    scroll behavior.
//! 2) If content is still short, keep it naturally top-aligned (no forced center).
//! 3) Only when content grows enough do we auto-scroll so the latest line stays
//!    near the vertical center.

use makepad_widgets::*;

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
            width: Fill, height: Fit
            flow: Right
            align: { y: 0.5 }
            padding: { left: 16, right: 16, top: 6, bottom: 6 }

            // Spacer pushes right controls group to the edge.
            <View> { width: Fill, height: 1 }

            right_controls = <View> {
                width: Fit, height: Fit
                flow: Right
                align: { y: 0.5 }
                spacing: 3

                font_size_dropdown = <DropDown> {
                    width: Fit, height: 26
                    margin: { top: 1, bottom: -1 }
                    padding: {
                        left: 3,
                        right: 3,
                        top: 2,
                        bottom: 2
                    }
                    labels: ["小", "正常", "大"]
                    values: ["small", "normal", "large"]
                    popup_menu_position: BelowInput
                    draw_bg: {
                        fn pixel(self) -> vec4 {
                            return vec4(0.0, 0.0, 0.0, 0.0);
                        }
                    }
                    draw_text: {
                        color: (SLATE_300)
                        text_style: <FONT_SEMIBOLD> { font_size: 11.0 }
                    }
                }

                lang_label = <Label> {
                    width: Fit
                    draw_text: {
                        color: (SLATE_300)
                        text_style: <FONT_SEMIBOLD> { font_size: 11.0 }
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
        }

        // ── Scrolling sentence list ──────────────────────────────────────────
        // bottom_spacer height is set dynamically in set_viewport_height() so the
        // last sentence anchors at ~50% of the viewport regardless of window size.
        content_scroll = <ScrollYView> {
            width: Fill, height: Fill
            flow: Down
            align: { x: 0.0, y: 0.0 }
            padding: { left: 16, right: 16, top: 12, bottom: 0 }

            // history_label: all completed sentences rendered as a single text block
            history_label = <Label> {
                width: Fill, height: Fit
                align: { x: 0.0, y: 0.0 }
                padding: 0.0
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
                margin: { top: 8, bottom: 8 }
                align: { x: 0.0, y: 0.0 }
                padding: 0.0
                draw_text: {
                    color: (MOXIN_TEXT_MUTED_DARK)
                    text_style: <FONT_REGULAR> { font_size: 13.0 }
                    wrap: Word
                }
                text: ""
            }

            // Dynamic spacer used only when content is long enough to require
            // centering the newest line; otherwise stays zero.
            bottom_spacer = <View> { width: Fill, height: 0.0 }
        }

        // ── Bottom branding footer ────────────────────────────────────────────
        overlay_footer = <View> {
            width: Fill, height: 30
            flow: Right
            align: {x: 0.5, y: 0.5}
            spacing: 5
            padding: {bottom: 2}

            footer_logo = <Image> {
                width: 22, height: 22
                source: dep("crate://self/resources/moxin_icon_fixed.png")
                fit: Smallest
            }

            footer_label = <Label> {
                width: Fit
                draw_text: {
                    color: (MOXIN_TEXT_MUTED_DARK)
                    text_style: <FONT_REGULAR> { font_size: 10.0 }
                }
                text: "Powered by Moxin Voice"
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

#[derive(Clone, Debug, DefaultNone)]
pub enum TranslationOverlayAction {
    None,
    FontSizePresetChanged(String),
}

#[derive(Live, LiveHook, Widget)]
pub struct TranslationOverlay {
    #[deref]
    view: View,

    /// Language pair display string, e.g. "ZH → EN"
    #[rust]
    lang_pair: String,

    /// Raw source language code for locale-aware display.
    #[rust]
    src_lang_code: String,

    /// Raw target language code for locale-aware display.
    #[rust]
    tgt_lang_code: String,

    /// Current display status
    #[rust]
    status: TranslationStatus,

    /// Locale flag for status labels. false=zh, true=en.
    #[rust]
    locale_en: bool,

    /// Font size preset id: "small" | "normal" | "large".
    #[rust]
    font_size_preset: String,

    /// Anchor position preset percentage: "50" | "60" | ... | "100".
    #[rust]
    anchor_position_preset: String,

    /// Cached history length for detecting changes
    #[rust]
    last_history_len: usize,

    /// Cached pending text for detecting changes
    #[rust]
    last_pending_text: String,

    /// True when content changed and we need to scroll to bottom on next draw.
    #[rust]
    pending_scroll: bool,

    /// Viewport height hint (window height minus toolbar) set by shell.
    /// The widget prefers measuring real scroll-view height during draw and
    /// falls back to this value when area data is unavailable.
    #[rust]
    viewport_height: f64,

    /// Last applied bottom spacer height, to avoid redundant apply_over calls.
    #[rust]
    last_spacer_height: f64,

    /// True when there is in-progress text shown in pending_label.
    #[rust]
    pending_active: bool,

    /// Whether content exceeds half viewport and should follow tail scrolling.
    #[rust]
    follow_tail_scroll: bool,

    /// True when only pending text is present (no completed history yet).
    #[rust]
    pending_only_mode: bool,
}

impl Widget for TranslationOverlay {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        if let Event::Actions(actions) = event {
            if let Some(idx) = self
                .view
                .drop_down(ids!(toolbar.right_controls.font_size_dropdown))
                .changed(actions)
            {
                let preset = Self::font_size_preset_for_index(idx).to_string();
                self.set_font_size_preset(cx, &preset);
                cx.widget_action(
                    self.widget_uid(),
                    &scope.path,
                    TranslationOverlayAction::FontSizePresetChanged(preset),
                );
            }
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // ── Draw content first ────────────────────────────────────────────────
        //
        // IMPORTANT: draw_walk must complete before set_scroll_pos is called.
        // Reason: scroll_bar.set_scroll_pos() clamps immediately to
        //   min(value, view_total - view_visible)
        // where view_total is updated only during draw_scroll_bars() which runs
        // at the end of view.draw_walk(). If we set scroll before draw, view_total
        // is stale (previous frame) and f64::MAX clamps to old max.
        //
        // After draw_walk, view_total reflects the freshly laid-out content, so
        // f64::MAX clamps to the correct new maximum scroll position.

        let result = self.view.draw_walk(cx, scope, walk);

        // Keep last sentence vertically centered while compensating for dynamic
        // pending label height (wrap differs between compact/fullscreen widths).
        self.update_anchor_spacer_from_layout(cx);

        // ── Set scroll after draw (view_total is now current) ─────────────────
        // The fixed bottom padding (108px) + pending_label margin (60px) creates
        // the anchor effect: scrolling to bottom lands last sentence at ~middle.
        if self.pending_only_mode {
            // First pending ASR line must stay in natural top-flow.
            self.view
                .view(ids!(content_scroll))
                .set_scroll_pos(cx, dvec2(0.0, 0.0));
        } else if self.pending_scroll {
            self.pending_scroll = false;
            let target_y = if self.follow_tail_scroll { f64::MAX } else { 0.0 };
            self.view
                .view(ids!(content_scroll))
                .set_scroll_pos(cx, dvec2(0.0, target_y));
            // One more redraw to render with the updated scroll position.
            self.view.redraw(cx);
        }

        result
    }
}

impl TranslationOverlay {
    const SCROLL_PADDING_TOP: f64 = 12.0;
    const PENDING_MARGIN_TOP: f64 = 8.0;
    const PENDING_MARGIN_BOTTOM: f64 = 8.0;
    const TAIL_SAFE_GAP: f64 = 10.0;

    fn toolbar_font_dropdown_vertical_padding() -> (f64, f64) {
        (2.0, 2.0)
    }

    fn localized_lang_name(code: &str, locale_en: bool) -> String {
        match (code, locale_en) {
            ("zh", true) => "ZH".to_string(),
            ("en", true) => "EN".to_string(),
            ("ja", true) => "JA".to_string(),
            ("fr", true) => "FR".to_string(),
            ("de", true) => "DE".to_string(),
            ("ko", true) => "KO".to_string(),
            ("zh", false) => "中".to_string(),
            ("en", false) => "英".to_string(),
            ("ja", false) => "日".to_string(),
            ("fr", false) => "法".to_string(),
            ("de", false) => "德".to_string(),
            ("ko", false) => "韩".to_string(),
            _ => code.to_uppercase(),
        }
    }

    fn format_lang_pair_for_locale(src: &str, tgt: &str, locale_en: bool) -> String {
        // Passthrough (no translation): show only source language — no target arrow.
        if tgt.eq_ignore_ascii_case("none") {
            return Self::localized_lang_name(src, locale_en);
        }
        format!(
            "{}-{}",
            Self::localized_lang_name(src, locale_en),
            Self::localized_lang_name(tgt, locale_en)
        )
    }

    fn font_size_preset_for_index(idx: usize) -> &'static str {
        match idx {
            0 => "small",
            2 => "large",
            _ => "normal",
        }
    }

    fn font_size_preset_index(preset: &str) -> usize {
        match preset {
            "small" => 0,
            "large" => 2,
            _ => 1,
        }
    }

    fn font_size_preset_values(preset: &str) -> (f64, f64) {
        match preset {
            "small" => (12.0, 11.0),
            "large" => (16.0, 15.0),
            _ => (14.0, 13.0),
        }
    }

    fn anchor_position_ratio(preset: &str) -> f64 {
        match preset {
            "60" => 0.6,
            "70" => 0.7,
            "80" => 0.8,
            "90" => 0.9,
            "100" => 1.0,
            _ => 0.5,
        }
    }

    fn font_size_dropdown_labels(locale_en: bool) -> Vec<String> {
        if locale_en {
            vec![
                "SMALL".to_string(),
                "NORMAL".to_string(),
                "LARGE".to_string(),
            ]
        } else {
            vec![
                "小".to_string(),
                "正常".to_string(),
                "大".to_string(),
            ]
        }
    }

    fn update_font_size_dropdown_labels(&self, cx: &mut Cx) {
        let labels = Self::font_size_dropdown_labels(self.locale_en);
        self.view
            .drop_down(ids!(toolbar.right_controls.font_size_dropdown))
            .set_labels(cx, labels);
        self.view
            .drop_down(ids!(toolbar.right_controls.font_size_dropdown))
            .set_selected_item(cx, Self::font_size_preset_index(&self.font_size_preset));
    }

    fn update_font_size_draw_styles(&self, cx: &mut Cx) {
        let (history_size, pending_size) =
            Self::font_size_preset_values(&self.font_size_preset);
        self.view
            .label(ids!(content_scroll.history_label))
            .apply_over(cx, live! { draw_text: { text_style: { font_size: (history_size) } } });
        self.view
            .label(ids!(content_scroll.pending_label))
            .apply_over(cx, live! { draw_text: { text_style: { font_size: (pending_size) } } });
    }

    fn compute_anchor_spacer_height(
        viewport_height: f64,
        content_without_spacer: f64,
        anchor_ratio: f64,
    ) -> f32 {
        // User rule: follow starts only after content exceeds the chosen anchor position.
        if content_without_spacer <= viewport_height * anchor_ratio {
            return 0.0;
        }

        // To place the latest line at `anchor_ratio` from the top after scrolling
        // to the bottom, the bottom spacer must consume the remaining lower area.
        ((viewport_height * (1.0 - anchor_ratio)) - Self::TAIL_SAFE_GAP).max(0.0) as f32
    }

    fn update_anchor_spacer_from_layout(&mut self, cx: &mut Cx2d) {
        let measured_viewport_h = self
            .view
            .view(ids!(content_scroll))
            .area()
            .rect(cx)
            .size
            .y
            .max(0.0);
        let viewport_h = if self.viewport_height > 1.0 {
            self.viewport_height
        } else {
            measured_viewport_h
        };

        let history_height = self
            .view
            .label(ids!(content_scroll.history_label))
            .area()
            .rect(cx)
            .size
            .y
            .max(0.0);
        let pending_height = self
            .view
            .label(ids!(content_scroll.pending_label))
            .area()
            .rect(cx)
            .size
            .y
            .max(0.0);
        let pending_block_h = if self.pending_active {
            Self::PENDING_MARGIN_TOP + pending_height + Self::PENDING_MARGIN_BOTTOM
        } else {
            0.0
        };
        let content_without_spacer =
            Self::SCROLL_PADDING_TOP + history_height + pending_block_h;
        let anchor_ratio = Self::anchor_position_ratio(&self.anchor_position_preset);
        self.follow_tail_scroll =
            content_without_spacer > viewport_h * anchor_ratio;
        let mut spacer_h =
            Self::compute_anchor_spacer_height(viewport_h, content_without_spacer, anchor_ratio);

        // Hard guard for first ASR line: when only pending text exists,
        // keep top-flow and disable center-follow.
        if self.pending_only_mode {
            self.follow_tail_scroll = false;
            spacer_h = 0.0;
        }
        let changed = (self.last_spacer_height - spacer_h as f64).abs() >= 0.5;
        self.last_spacer_height = spacer_h as f64;
        // Always write spacer (including 0) to avoid stale initial/default values.
        self.view
            .view(ids!(content_scroll.bottom_spacer))
            .apply_over(cx, live! {
                height: (spacer_h)
            });
        if changed {
            // Spacer changed => request one more bottom snap on the next frame so
            // scroll position matches the new layout.
            self.pending_scroll = true;
            self.view.redraw(cx);
        }
    }

    /// Set the language pair label, e.g. "ZH → EN"
    pub fn set_lang_pair(&mut self, cx: &mut Cx, src: &str, tgt: &str) {
        self.src_lang_code = src.to_lowercase();
        self.tgt_lang_code = tgt.to_lowercase();
        self.lang_pair = Self::format_lang_pair_for_locale(
            &self.src_lang_code,
            &self.tgt_lang_code,
            self.locale_en,
        );
        self.view
            .label(ids!(toolbar.right_controls.lang_label))
            .set_text(cx, &self.lang_pair);
    }

    /// Update the content viewport height (window height minus the 44px toolbar).
    ///
    /// Must be called whenever the translation window is created or resized so that
    /// the scroll anchor stays at ~50% of the visible area.
    pub fn set_viewport_height(&mut self, cx: &mut Cx, viewport_height: f64) {
        self.viewport_height = viewport_height;
        self.last_spacer_height = -1.0; // force recompute on next draw
        self.view.redraw(cx);
    }

    /// Set locale for toolbar status text.
    pub fn set_locale_en(&mut self, cx: &mut Cx, locale_en: bool) {
        if self.locale_en == locale_en {
            return;
        }
        self.locale_en = locale_en;
        self.lang_pair = Self::format_lang_pair_for_locale(
            &self.src_lang_code,
            &self.tgt_lang_code,
            self.locale_en,
        );
        self.view
            .label(ids!(toolbar.right_controls.lang_label))
            .set_text(cx, &self.lang_pair);
        self.update_font_size_dropdown_labels(cx);
        self.update_status_label(cx);
        self.view.redraw(cx);
    }

    pub fn set_font_size_preset(&mut self, cx: &mut Cx, preset: &str) {
        let normalized = match preset {
            "small" | "large" | "normal" => preset,
            _ => "normal",
        };
        if self.font_size_preset == normalized {
            self.view
                .drop_down(ids!(toolbar.right_controls.font_size_dropdown))
                .set_selected_item(cx, Self::font_size_preset_index(normalized));
            return;
        }
        self.font_size_preset = normalized.to_string();
        self.view
            .drop_down(ids!(toolbar.right_controls.font_size_dropdown))
            .set_selected_item(cx, Self::font_size_preset_index(normalized));
        self.update_font_size_draw_styles(cx);
        self.view.redraw(cx);
    }

    pub fn set_anchor_position_preset(&mut self, cx: &mut Cx, preset: &str) {
        let normalized = match preset {
            "50" | "60" | "70" | "80" | "90" | "100" => preset,
            _ => "50",
        };
        if self.anchor_position_preset == normalized {
            return;
        }
        self.anchor_position_preset = normalized.to_string();
        self.last_spacer_height = -1.0;
        self.pending_scroll = true;
        self.view.redraw(cx);
    }

    /// Set overlay background opacity (0.0 = fully transparent, 1.0 = opaque).
    pub fn set_opacity(&mut self, cx: &mut Cx, opacity: f64) {
        self.view.apply_over(cx, live! {
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
        let pending_label = self.view.label(ids!(content_scroll.pending_label));
        if pending.is_empty() {
            pending_label.set_text(cx, "");
            pending_label.apply_over(cx, live! { margin: { top: 0.0, bottom: 0.0 } });
            self.pending_active = false;
        } else {
            pending_label.set_text(cx, pending);
            pending_label.apply_over(cx, live! { margin: { top: 8.0, bottom: 8.0 } });
            self.pending_active = true;
        }
        self.pending_only_mode = history.is_empty() && self.pending_active;
        if self.pending_only_mode {
            // Hard reset on first pending line: never reuse previous scroll state.
            self.follow_tail_scroll = false;
            self.pending_scroll = true;
        }

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
        if self.pending_only_mode || new_len != self.last_history_len || pending_changed {
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
                // Blank line between entries (same inter-entry spacing as
                // source/translation pairs in the normal mode).
                out.push_str("\n\n");
            }
            out.push_str(source.trim());
            let translation_trimmed = translation.trim();
            // Passthrough (no translation) sends empty translation text — skip
            // the second line entirely instead of inserting a blank gap.
            if !translation_trimmed.is_empty() {
                out.push('\n');
                out.push_str(translation_trimmed);
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
        self.pending_active = false;
        self.follow_tail_scroll = false;
        self.pending_only_mode = false;
        self.pending_scroll = false;
        self.last_spacer_height = 0.0;
        self.view
            .label(ids!(content_scroll.history_label))
            .set_text(cx, "");
        let pending_label = self.view.label(ids!(content_scroll.pending_label));
        pending_label.set_text(cx, "");
        pending_label.apply_over(cx, live! { margin: { top: 0.0, bottom: 0.0 } });
        self.set_listening(cx);
    }

    fn update_status_label(&self, cx: &mut Cx) {
        let (text, color) = match self.status {
            TranslationStatus::WarmingUp => (
                if self.locale_en {
                    "● WARMING UP"
                } else {
                    "● 预热中"
                },
                vec4(0.906, 0.620, 0.204, 1.0),
            ),
            TranslationStatus::Listening => (
                if self.locale_en {
                    "● LISTENING"
                } else {
                    "● 监听中"
                },
                vec4(0.098, 0.725, 0.506, 1.0),
            ),
            TranslationStatus::Transcribing => (
                if self.locale_en {
                    "● TRANSCRIBING"
                } else {
                    "● 识别中"
                },
                vec4(0.906, 0.620, 0.204, 1.0),
            ),
            TranslationStatus::Complete => (
                if self.locale_en { "✓ DONE" } else { "✓ 完成" },
                vec4(0.098, 0.725, 0.506, 1.0),
            ),
        };
        let label = self.view.label(ids!(toolbar.right_controls.status_label));
        label.set_text(cx, text);
        label.apply_over(cx, live! { draw_text: { color: (color) } });
    }
}

#[cfg(test)]
mod tests {
    use super::TranslationOverlay;

    #[test]
    fn anchor_spacer_shrinks_when_pending_height_grows() {
        let viewport = 216.0;
        let short_content = 100.0;
        let long_content = 280.0;
        let short_spacer = TranslationOverlay::compute_anchor_spacer_height(
            viewport,
            short_content,
            0.5,
        );
        let long_spacer = TranslationOverlay::compute_anchor_spacer_height(
            viewport,
            long_content,
            0.5,
        );
        assert!(short_spacer < long_spacer);
    }

    #[test]
    fn anchor_spacer_is_never_negative() {
        let spacer = TranslationOverlay::compute_anchor_spacer_height(216.0, 300.0, 0.5);
        assert!(spacer >= 0.0);
    }

    #[test]
    fn anchor_spacer_stays_zero_before_half_viewport() {
        let spacer = TranslationOverlay::compute_anchor_spacer_height(216.0, 108.0, 0.5);
        assert_eq!(spacer, 0.0);
    }

    #[test]
    fn font_size_preset_values_match_expected_scale() {
        assert_eq!(
            TranslationOverlay::font_size_preset_values("normal"),
            (14.0, 13.0)
        );
        assert_eq!(
            TranslationOverlay::font_size_preset_values("large"),
            (16.0, 15.0)
        );
    }

    #[test]
    fn anchor_position_preset_values_match_expected_ratios() {
        assert_eq!(TranslationOverlay::anchor_position_ratio("50"), 0.5);
        assert_eq!(TranslationOverlay::anchor_position_ratio("60"), 0.6);
        assert_eq!(TranslationOverlay::anchor_position_ratio("100"), 1.0);
    }

    #[test]
    fn upper_anchor_creates_more_bottom_spacer_than_lower_anchor() {
        let upper = TranslationOverlay::compute_anchor_spacer_height(216.0, 300.0, 0.5);
        let lower = TranslationOverlay::compute_anchor_spacer_height(216.0, 300.0, 0.9);
        assert!(upper > lower);
    }

    #[test]
    fn format_lang_pair_follows_locale() {
        assert_eq!(
            TranslationOverlay::format_lang_pair_for_locale("zh", "en", false),
            "中-英"
        );
        assert_eq!(
            TranslationOverlay::format_lang_pair_for_locale("zh", "en", true),
            "ZH-EN"
        );
    }

    #[test]
    fn format_lang_pair_passthrough_drops_target() {
        assert_eq!(
            TranslationOverlay::format_lang_pair_for_locale("zh", "none", false),
            "中"
        );
        assert_eq!(
            TranslationOverlay::format_lang_pair_for_locale("en", "none", true),
            "EN"
        );
    }

    #[test]
    fn format_history_skips_empty_translation_line() {
        let items = vec![("hello".to_string(), "".to_string())];
        let out = TranslationOverlay::format_history(&items);
        assert_eq!(out, "hello");

        let items = vec![
            ("hi".to_string(), "".to_string()),
            ("world".to_string(), "".to_string()),
        ];
        let out = TranslationOverlay::format_history(&items);
        assert_eq!(out, "hi\n\nworld");
    }

    #[test]
    fn format_history_keeps_translation_line_when_present() {
        let items = vec![("hello".to_string(), "你好".to_string())];
        let out = TranslationOverlay::format_history(&items);
        assert_eq!(out, "hello\n你好");

        // Inter-entry spacing is the same blank-line separator as before.
        let items = vec![
            ("hi".to_string(), "你好".to_string()),
            ("bye".to_string(), "再见".to_string()),
        ];
        let out = TranslationOverlay::format_history(&items);
        assert_eq!(out, "hi\n你好\n\nbye\n再见");
    }

    #[test]
    fn toolbar_font_dropdown_vertical_padding_stays_centered() {
        assert_eq!(
            TranslationOverlay::toolbar_font_dropdown_vertical_padding(),
            (2.0, 2.0)
        );
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

    pub fn set_locale_en(&self, cx: &mut Cx, locale_en: bool) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_locale_en(cx, locale_en);
        }
    }

    pub fn set_font_size_preset(&self, cx: &mut Cx, preset: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_font_size_preset(cx, preset);
        }
    }

    pub fn set_anchor_position_preset(&self, cx: &mut Cx, preset: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_anchor_position_preset(cx, preset);
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

    pub fn set_viewport_height(&self, cx: &mut Cx, viewport_height: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_viewport_height(cx, viewport_height);
        }
    }
}
