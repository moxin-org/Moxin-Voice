//! Voice selector component - displays list of available voices

use crate::voice_data::{get_builtin_voices, LanguageFilter, Voice, VoiceFilter};
use crate::voice_persistence;
use makepad_widgets::*;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    use moxin_widgets::theme::*;

    // Voice item in the list
    VoiceItem = <View> {
        width: Fill, height: Fit
        padding: {left: 12, right: 16, top: 10, bottom: 10}
        flow: Right
        align: {y: 0.5}
        spacing: 12
        cursor: Hand

        show_bg: true
        draw_bg: {
            instance dark_mode: 0.0
            instance selected: 0.0
            instance hover: 0.0

            fn pixel(self) -> vec4 {
                let base = mix((SURFACE), (SURFACE_DARK), self.dark_mode);
                let selected_color = mix((PRIMARY_50), (PRIMARY_900), self.dark_mode);
                let hover_color = mix((SLATE_50), (SLATE_800), self.dark_mode);

                let color = mix(base, selected_color, self.selected);
                let color = mix(color, hover_color, self.hover * (1.0 - self.selected));
                return color;
            }
        }

        // Selection indicator - left edge bar
        selection_indicator = <View> {
            width: 3, height: 36
            show_bg: true
            draw_bg: {
                instance selected: 0.0
                fn pixel(self) -> vec4 {
                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                    sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 1.5);
                    let color = mix(vec4(0.0, 0.0, 0.0, 0.0), (PRIMARY_500), self.selected);
                    sdf.fill(color);
                    return sdf.result;
                }
            }
        }

        // Voice avatar - circular with initial
        avatar = <RoundedView> {
            width: 36, height: 36
            align: {x: 0.5, y: 0.5}
            draw_bg: {
                instance dark_mode: 0.0
                instance selected: 0.0
                fn pixel(self) -> vec4 {
                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                    sdf.circle(18.0, 18.0, 18.0);
                    let base_color = mix((PRIMARY_500), (PRIMARY_400), self.dark_mode);
                    let selected_color = mix((PRIMARY_600), (PRIMARY_300), self.dark_mode);
                    let color = mix(base_color, selected_color, self.selected);
                    sdf.fill(color);
                    return sdf.result;
                }
            }

            // Initial letter
            initial = <Label> {
                width: Fill, height: Fill
                padding: {left: 0.0, right: 0.0, top: 4.0, bottom: 0.0}
                align: {x: 0.5, y: 0.5}
                draw_text: {
                    text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                    fn get_color(self) -> vec4 {
                        return (WHITE);
                    }
                }
                text: "L"
            }
        }

        // Voice info - name and description
        info = <View> {
            width: Fill, height: Fit
            flow: Down
            spacing: 2

            name = <Label> {
                width: Fill, height: Fit
                padding: {top: 2, bottom: 2}
                draw_text: {
                    instance dark_mode: 0.0
                    instance selected: 0.0
                    text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                    fn get_color(self) -> vec4 {
                        let base = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                        let selected_color = mix((PRIMARY_600), (PRIMARY_300), self.dark_mode);
                        return mix(base, selected_color, self.selected);
                    }
                }
                text: "Voice Name"
            }

            description = <Label> {
                width: Fill, height: Fit
                draw_text: {
                    instance dark_mode: 0.0
                    text_style: { font_size: 11.0 }
                    fn get_color(self) -> vec4 {
                        return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                    }
                }
                text: "Voice description"
            }
        }

        // Preview button - plays reference audio sample
        preview_btn = <View> {
            width: 28, height: 28
            align: {x: 0.5, y: 0.5}
            cursor: Hand
            visible: true

            show_bg: true
            draw_bg: {
                instance dark_mode: 0.0
                instance hover: 0.0
                instance playing: 0.0

                fn pixel(self) -> vec4 {
                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                    sdf.circle(14.0, 14.0, 14.0);
                    let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                    let hover_color = mix((PRIMARY_100), (PRIMARY_700), self.dark_mode);
                    let playing_color = mix((PRIMARY_200), (PRIMARY_600), self.dark_mode);
                    let color = mix(base, hover_color, self.hover);
                    let color = mix(color, playing_color, self.playing);
                    sdf.fill(color);

                    // Draw play triangle or stop square based on playing state
                    if self.playing > 0.5 {
                        // Stop icon (square)
                        sdf.rect(10.0, 10.0, 8.0, 8.0);
                        let icon_color = mix((PRIMARY_600), (PRIMARY_300), self.dark_mode);
                        sdf.fill(icon_color);
                    } else {
                        // Play icon (triangle) - centered
                        sdf.move_to(11.0, 9.0);
                        sdf.line_to(20.0, 14.0);
                        sdf.line_to(11.0, 19.0);
                        sdf.close_path();
                        let icon_color = mix((SLATE_500), (SLATE_400), self.dark_mode);
                        let icon_hover = mix((PRIMARY_600), (PRIMARY_300), self.dark_mode);
                        sdf.fill(mix(icon_color, icon_hover, self.hover));
                    }

                    return sdf.result;
                }
            }
        }

        // Delete button - only for custom voices
        delete_btn = <View> {
            width: 28, height: 28
            align: {x: 0.5, y: 0.5}
            cursor: Hand
            visible: false

            show_bg: true
            draw_bg: {
                instance dark_mode: 0.0
                instance hover: 0.0

                fn pixel(self) -> vec4 {
                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                    sdf.circle(14.0, 14.0, 14.0);
                    let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                    let hover_color = mix((RED_100), (RED_700), self.dark_mode);
                    let color = mix(base, hover_color, self.hover);
                    sdf.fill(color);

                    // Draw X icon (delete)
                    let icon_color = mix((SLATE_500), (SLATE_400), self.dark_mode);
                    let icon_hover = mix((RED_600), (RED_300), self.dark_mode);
                    let line_color = mix(icon_color, icon_hover, self.hover);

                    // X lines
                    sdf.move_to(9.0, 9.0);
                    sdf.line_to(19.0, 19.0);
                    sdf.stroke(line_color, 1.5);

                    sdf.move_to(19.0, 9.0);
                    sdf.line_to(9.0, 19.0);
                    sdf.stroke(line_color, 1.5);

                    return sdf.result;
                }
            }
        }
    }

    // Voice selector panel
    pub VoiceSelector = {{VoiceSelector}} {
        width: Fill, height: Fill
        flow: Down

        show_bg: true
        draw_bg: {
            instance dark_mode: 0.0
            fn pixel(self) -> vec4 {
                return mix((SURFACE), (SURFACE_DARK), self.dark_mode);
            }
        }

        // Header with title and selected voice indicator (single row)
        header = <View> {
            width: Fill, height: Fit
            padding: {left: 16, right: 16, top: 12, bottom: 8}
            flow: Right
            align: {y: 0.5}
            spacing: 8
            show_bg: true
            draw_bg: {
                instance dark_mode: 0.0
                fn pixel(self) -> vec4 {
                    return mix((SLATE_50), (SLATE_800), self.dark_mode);
                }
            }

            title_row = <View> {
                width: Fit, height: Fit
                flow: Right
                align: {y: 0.5}

                title = <Label> {
                    width: Fit, height: Fit
                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                        fn get_color(self) -> vec4 {
                            return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                        }
                    }
                    text: "Select Voice"
                }
            }

            <View> { width: Fill, height: 1 }

            // Selected voice badge (inline with title)
            badge_row = <View> {
                width: Fit, height: Fit
                flow: Right
                align: {y: 0.5}

                selected_voice_badge = <RoundedView> {
                    width: Fit, height: Fit
                    padding: {left: 8, right: 8, top: 4, bottom: 4}
                    draw_bg: {
                        instance dark_mode: 0.0
                        border_radius: 4.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                            let bg = mix((PRIMARY_100), (PRIMARY_800), self.dark_mode);
                            sdf.fill(bg);
                            return sdf.result;
                        }
                    }

                    selected_voice_label = <Label> {
                        width: Fit, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                            fn get_color(self) -> vec4 {
                                return mix((PRIMARY_700), (PRIMARY_200), self.dark_mode);
                            }
                        }
                        text: "豆包 (Doubao)"
                    }
                }
            }
        }

        // Filter bar - compact two-row layout
        filter_bar = <View> {
            width: Fill, height: Fit
            padding: {left: 12, right: 12, top: 4, bottom: 8}
            flow: Down
            spacing: 6
            show_bg: true
            draw_bg: {
                instance dark_mode: 0.0
                fn pixel(self) -> vec4 {
                    return mix((SLATE_50), (SLATE_800), self.dark_mode);
                }
            }

            // Category filter row
            category_row = <View> {
                width: Fill, height: Fit
                flow: Right
                spacing: 4
                align: {y: 0.5}

                category_filter_group = <View> {
                    width: Fit, height: Fit
                    flow: Right
                    spacing: 0
                    padding: {left: 2, right: 2, top: 2, bottom: 2}
                    show_bg: true
                    draw_bg: {
                        instance dark_mode: 0.0
                        instance border_radius: 6.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                            let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                            sdf.fill(bg);
                            return sdf.result;
                        }
                    }

                    filter_all_btn = <Button> {
                        width: Fit, height: 24
                        padding: {left: 8, right: 8}
                        text: "全部"

                        draw_bg: {
                            instance hover: 0.0
                            instance active: 1.0
                            instance border_radius: 4.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let normal = vec4(0.0, 0.0, 0.0, 0.0);
                                let active_color = (WHITE);
                                let bg = mix(normal, active_color, self.active);
                                sdf.fill(bg);
                                return sdf.result;
                            }
                        }

                        draw_text: {
                            instance active: 1.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                            fn get_color(self) -> vec4 {
                                let normal = vec4(0.5, 0.5, 0.55, 1.0);
                                let active = (MOXIN_PRIMARY);
                                return mix(normal, active, self.active);
                            }
                        }
                    }

                    filter_male_btn = <Button> {
                        width: Fit, height: 24
                        padding: {left: 8, right: 8}
                        text: "男"

                        draw_bg: {
                            instance hover: 0.0
                            instance active: 0.0
                            instance border_radius: 4.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let normal = vec4(0.0, 0.0, 0.0, 0.0);
                                let active_color = (WHITE);
                                let bg = mix(normal, active_color, self.active);
                                sdf.fill(bg);
                                return sdf.result;
                            }
                        }

                        draw_text: {
                            instance active: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                            fn get_color(self) -> vec4 {
                                let normal = vec4(0.5, 0.5, 0.55, 1.0);
                                let active = (MOXIN_PRIMARY);
                                return mix(normal, active, self.active);
                            }
                        }
                    }

                    filter_female_btn = <Button> {
                        width: Fit, height: 24
                        padding: {left: 8, right: 8}
                        text: "女"

                        draw_bg: {
                            instance hover: 0.0
                            instance active: 0.0
                            instance border_radius: 4.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let normal = vec4(0.0, 0.0, 0.0, 0.0);
                                let active_color = (WHITE);
                                let bg = mix(normal, active_color, self.active);
                                sdf.fill(bg);
                                return sdf.result;
                            }
                        }

                        draw_text: {
                            instance active: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                            fn get_color(self) -> vec4 {
                                let normal = vec4(0.5, 0.5, 0.55, 1.0);
                                let active = (MOXIN_PRIMARY);
                                return mix(normal, active, self.active);
                            }
                        }
                    }

                    filter_character_btn = <Button> {
                        width: Fit, height: 24
                        padding: {left: 8, right: 8}
                        text: "角色"

                        draw_bg: {
                            instance hover: 0.0
                            instance active: 0.0
                            instance border_radius: 4.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let normal = vec4(0.0, 0.0, 0.0, 0.0);
                                let active_color = (WHITE);
                                let bg = mix(normal, active_color, self.active);
                                sdf.fill(bg);
                                return sdf.result;
                            }
                        }

                        draw_text: {
                            instance active: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                            fn get_color(self) -> vec4 {
                                let normal = vec4(0.5, 0.5, 0.55, 1.0);
                                let active = (MOXIN_PRIMARY);
                                return mix(normal, active, self.active);
                            }
                        }
                    }
                }

                <View> { width: Fill, height: 1 }

                // Language filter group
                language_filter_group = <View> {
                    width: Fit, height: Fit
                    flow: Right
                    spacing: 0
                    padding: {left: 2, right: 2, top: 2, bottom: 2}
                    show_bg: true
                    draw_bg: {
                        instance dark_mode: 0.0
                        instance border_radius: 6.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                            let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                            sdf.fill(bg);
                            return sdf.result;
                        }
                    }

                    lang_all_btn = <Button> {
                        width: Fit, height: 24
                        padding: {left: 8, right: 8}
                        text: "全部"

                        draw_bg: {
                            instance hover: 0.0
                            instance active: 1.0
                            instance border_radius: 4.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let normal = vec4(0.0, 0.0, 0.0, 0.0);
                                let active_color = (WHITE);
                                let bg = mix(normal, active_color, self.active);
                                sdf.fill(bg);
                                return sdf.result;
                            }
                        }

                        draw_text: {
                            instance active: 1.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                            fn get_color(self) -> vec4 {
                                let normal = vec4(0.5, 0.5, 0.55, 1.0);
                                let active = (MOXIN_PRIMARY);
                                return mix(normal, active, self.active);
                            }
                        }
                    }

                    lang_zh_btn = <Button> {
                        width: Fit, height: 24
                        padding: {left: 8, right: 8}
                        text: "中"

                        draw_bg: {
                            instance hover: 0.0
                            instance active: 0.0
                            instance border_radius: 4.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let normal = vec4(0.0, 0.0, 0.0, 0.0);
                                let active_color = (WHITE);
                                let bg = mix(normal, active_color, self.active);
                                sdf.fill(bg);
                                return sdf.result;
                            }
                        }

                        draw_text: {
                            instance active: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                            fn get_color(self) -> vec4 {
                                let normal = vec4(0.5, 0.5, 0.55, 1.0);
                                let active = (MOXIN_PRIMARY);
                                return mix(normal, active, self.active);
                            }
                        }
                    }

                    lang_en_btn = <Button> {
                        width: Fit, height: 24
                        padding: {left: 8, right: 8}
                        text: "EN"

                        draw_bg: {
                            instance hover: 0.0
                            instance active: 0.0
                            instance border_radius: 4.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let normal = vec4(0.0, 0.0, 0.0, 0.0);
                                let active_color = (WHITE);
                                let bg = mix(normal, active_color, self.active);
                                sdf.fill(bg);
                                return sdf.result;
                            }
                        }

                        draw_text: {
                            instance active: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                            fn get_color(self) -> vec4 {
                                let normal = vec4(0.5, 0.5, 0.55, 1.0);
                                let active = (MOXIN_PRIMARY);
                                return mix(normal, active, self.active);
                            }
                        }
                    }
                }
            }
        }

        // Voice list with scrolling
        voice_list = <PortalList> {
            width: Fill, height: Fill
            flow: Down

            VoiceItem = <VoiceItem> {}
        }
    }
}

/// Action emitted by voice selector
#[derive(Clone, Debug, DefaultNone)]
pub enum VoiceSelectorAction {
    None,
    VoiceSelected(String),                     // voice_id
    PreviewRequested(String),                  // voice_id
    CloneVoiceClicked,                         // Open clone modal
    RequestStartDora,                          // Request parent to show "please start dora" message
    RequestDeleteConfirmation(String, String), // (voice_id, voice_name) - Request parent to show delete confirmation
    DeleteVoiceClicked(String), // voice_id (custom voices only) - Actually delete the voice
    
    // Filter actions
    FilterCategoryChanged(VoiceFilter),
    FilterLanguageChanged(LanguageFilter),
}

#[derive(Live, LiveHook, Widget)]
pub struct VoiceSelector {
    #[deref]
    view: View,

    #[rust]
    voices: Vec<Voice>,

    #[rust]
    filtered_voices: Vec<Voice>,

    #[rust]
    custom_voices: Vec<Voice>,

    #[rust]
    selected_voice_id: Option<String>,

    #[rust]
    dark_mode: f64,

    #[rust]
    initialized: bool,

    #[rust]
    preview_playing_voice_id: Option<String>,

    #[rust]
    hovered_preview_idx: Option<usize>,

    #[rust]
    hovered_delete_idx: Option<usize>,

    #[rust]
    dora_running: bool,

    #[rust]
    category_filter: VoiceFilter,

    #[rust]
    language_filter: LanguageFilter,

    #[rust]
    search_query: String,

    /// Store drawn item areas for hit testing: (item_id, item_area, preview_btn_area, delete_btn_area)
    #[rust]
    item_areas: Vec<(usize, Area, Area, Area)>,
}

impl Widget for VoiceSelector {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Capture actions from child widgets (including filter buttons)
        let actions = cx.capture_actions(|cx| {
            self.view.handle_event(cx, event, scope);
        });

        // Handle filter button clicks first (before any early returns)
        self.handle_filter_button_clicks(cx, &actions);

        // Initialize voices on first event
        if !self.initialized {
            self.reload_voices();
            // Select first voice by default
            if let Some(first) = self.filtered_voices.first() {
                self.selected_voice_id = Some(first.id.clone());
            }
            self.initialized = true;
        }

        // Handle portal list item clicks using stored areas (BEFORE Actions early return)
        for (item_id, item_area, preview_area, delete_area) in self.item_areas.iter().cloned() {
            if item_id >= self.filtered_voices.len() {
                continue;
            }

            // Check delete button click first
            match event.hits(cx, delete_area) {
                Hit::FingerUp(fe) if fe.was_tap() => {
                    let voice = &self.filtered_voices[item_id];
                    if voice.is_custom() {
                        cx.widget_action(
                            self.widget_uid(),
                            &scope.path,
                            VoiceSelectorAction::RequestDeleteConfirmation(
                                voice.id.clone(),
                                voice.name.clone(),
                            ),
                        );
                        self.view.redraw(cx);
                        continue;
                    }
                }
                Hit::FingerHoverIn(_) => {
                    self.hovered_delete_idx = Some(item_id);
                    self.view.redraw(cx);
                }
                Hit::FingerHoverOut(_) => {
                    if self.hovered_delete_idx == Some(item_id) {
                        self.hovered_delete_idx = None;
                        self.view.redraw(cx);
                    }
                }
                _ => {}
            }

            // Check preview button click
            match event.hits(cx, preview_area) {
                Hit::FingerUp(fe) if fe.was_tap() => {
                    let voice_id = self.filtered_voices[item_id].id.clone();
                    // Toggle preview: if same voice is playing, stop it
                    if self.preview_playing_voice_id.as_ref() == Some(&voice_id) {
                        self.preview_playing_voice_id = None;
                    } else {
                        self.preview_playing_voice_id = Some(voice_id.clone());
                    }
                    cx.widget_action(
                        self.widget_uid(),
                        &scope.path,
                        VoiceSelectorAction::PreviewRequested(voice_id),
                    );
                    self.view.redraw(cx);
                    continue;
                }
                Hit::FingerHoverIn(_) => {
                    self.hovered_preview_idx = Some(item_id);
                    self.view.redraw(cx);
                }
                Hit::FingerHoverOut(_) => {
                    if self.hovered_preview_idx == Some(item_id) {
                        self.hovered_preview_idx = None;
                        self.view.redraw(cx);
                    }
                }
                _ => {}
            }

            // Check item click (for selection)
            match event.hits(cx, item_area) {
                Hit::FingerUp(fe) if fe.was_tap() => {
                    let voice_id = self.filtered_voices[item_id].id.clone();
                    let voice_name = self.filtered_voices[item_id].name.clone();
                    self.selected_voice_id = Some(voice_id.clone());

                    // Update selected voice label in header badge
                    self.view
                        .label(ids!(
                            header.badge_row.selected_voice_badge.selected_voice_label
                        ))
                        .set_text(cx, &voice_name);

                    cx.widget_action(
                        self.widget_uid(),
                        &scope.path,
                        VoiceSelectorAction::VoiceSelected(voice_id),
                    );
                    self.view.redraw(cx);
                }
                _ => {}
            }
        }

    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Initialize if needed (in case draw happens before handle_event)
        if !self.initialized {
            self.reload_voices();
            if let Some(first) = self.filtered_voices.first() {
                self.selected_voice_id = Some(first.id.clone());
            }
            self.initialized = true;
        }

        // Clear item areas before redrawing
        self.item_areas.clear();

        // Draw portal list items using borrow pattern
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.as_portal_list().borrow_mut() {
                list.set_item_range(cx, 0, self.filtered_voices.len());

                while let Some(item_id) = list.next_visible_item(cx) {
                    if item_id < self.filtered_voices.len() {
                        let voice = &self.filtered_voices[item_id];
                        let item = list.item(cx, item_id, live_id!(VoiceItem));

                        // Set voice data
                        let initial = voice.name.chars().next().unwrap_or('?').to_string();
                        item.label(ids!(avatar.initial)).set_text(cx, &initial);
                        item.label(ids!(info.name)).set_text(cx, &voice.name);
                        item.label(ids!(info.description))
                            .set_text(cx, &voice.description);

                        // Set selection state
                        let is_selected = self.selected_voice_id.as_ref() == Some(&voice.id);
                        let selected_val = if is_selected { 1.0 } else { 0.0 };

                        // Apply selection and dark mode to item background
                        item.apply_over(
                            cx,
                            live! {
                                draw_bg: { selected: (selected_val), dark_mode: (self.dark_mode) }
                            },
                        );

                        // Apply selection indicator
                        item.view(ids!(selection_indicator)).apply_over(
                            cx,
                            live! {
                                draw_bg: { selected: (selected_val) }
                            },
                        );

                        // Apply dark mode and selection to avatar
                        item.view(ids!(avatar)).apply_over(
                            cx,
                            live! {
                                draw_bg: { dark_mode: (self.dark_mode), selected: (selected_val) }
                            },
                        );

                        // Apply dark mode and selection to name label
                        item.label(ids!(info.name)).apply_over(
                            cx,
                            live! {
                                draw_text: { dark_mode: (self.dark_mode), selected: (selected_val) }
                            },
                        );

                        // Apply dark mode to description
                        item.label(ids!(info.description)).apply_over(
                            cx,
                            live! {
                                draw_text: { dark_mode: (self.dark_mode) }
                            },
                        );

                        // Apply preview button state — hide when no audio is available
                        let can_preview = voice.preview_audio.is_some()
                            || voice.reference_audio_path.is_some();
                        item.view(ids!(preview_btn)).set_visible(cx, can_preview);
                        if can_preview {
                            let is_playing = self.preview_playing_voice_id.as_ref() == Some(&voice.id);
                            let playing_val = if is_playing { 1.0 } else { 0.0 };
                            let is_hovered = self.hovered_preview_idx == Some(item_id);
                            let hover_val = if is_hovered { 1.0 } else { 0.0 };
                            item.view(ids!(preview_btn)).apply_over(cx, live! {
                                draw_bg: { dark_mode: (self.dark_mode), playing: (playing_val), hover: (hover_val) }
                            });
                        }

                        // Show delete button only for custom voices
                        let is_custom = voice.is_custom();
                        item.view(ids!(delete_btn)).set_visible(cx, is_custom);
                        if is_custom {
                            let delete_hover = if self.hovered_delete_idx == Some(item_id) {
                                1.0
                            } else {
                                0.0
                            };
                            item.view(ids!(delete_btn)).apply_over(
                                cx,
                                live! {
                                    draw_bg: { dark_mode: (self.dark_mode), hover: (delete_hover) }
                                },
                            );
                        }

                        item.draw_all(cx, scope);

                        // Store item areas for hit testing in handle_event
                        let item_area = item.area();
                        let preview_area = item.view(ids!(preview_btn)).area();
                        let delete_area = item.view(ids!(delete_btn)).area();
                        self.item_areas.push((item_id, item_area, preview_area, delete_area));
                    }
                }
            }
        }
        DrawStep::done()
    }
}

impl VoiceSelector {
    /// Reload all voices (built-in + custom) and apply filters
    fn reload_voices(&mut self) {
        self.voices = get_builtin_voices();
        self.custom_voices = voice_persistence::load_custom_voices();
        self.voices.extend(self.custom_voices.clone());
        self.apply_filters();
    }

    /// Apply filters to voice list (from all voices)
    fn apply_filters(&mut self) {
        self.filtered_voices = self.voices
            .iter()
            .filter(|v| v.matches_filter(&self.category_filter))
            .filter(|v| v.matches_language(&self.language_filter))
            .filter(|v| v.matches_search(&self.search_query))
            .cloned()
            .collect();

        // If selected voice is no longer in filtered list, select first available
        if let Some(ref selected_id) = self.selected_voice_id {
            if !self.filtered_voices.iter().any(|v| &v.id == selected_id) {
                self.selected_voice_id = self.filtered_voices.first().map(|v| v.id.clone());
            }
        }
    }

    /// Handle filter button clicks
    fn handle_filter_button_clicks(&mut self, cx: &mut Cx, actions: &ActionsBuf) {
        // Handle category filter button clicks
        if self.view.button(ids!(filter_bar.category_row.category_filter_group.filter_all_btn)).clicked(actions) {
            self.category_filter = VoiceFilter::All;
            self.apply_filters();
            self.update_category_filter_buttons(cx);
            self.view.redraw(cx);
        }
        if self.view.button(ids!(filter_bar.category_row.category_filter_group.filter_male_btn)).clicked(actions) {
            self.category_filter = VoiceFilter::Male;
            self.apply_filters();
            self.update_category_filter_buttons(cx);
            self.view.redraw(cx);
        }
        if self.view.button(ids!(filter_bar.category_row.category_filter_group.filter_female_btn)).clicked(actions) {
            self.category_filter = VoiceFilter::Female;
            self.apply_filters();
            self.update_category_filter_buttons(cx);
            self.view.redraw(cx);
        }
        if self.view.button(ids!(filter_bar.category_row.category_filter_group.filter_character_btn)).clicked(actions) {
            self.category_filter = VoiceFilter::Character;
            self.apply_filters();
            self.update_category_filter_buttons(cx);
            self.view.redraw(cx);
        }

        // Handle language filter button clicks
        if self.view.button(ids!(filter_bar.category_row.language_filter_group.lang_all_btn)).clicked(actions) {
            self.language_filter = LanguageFilter::All;
            self.apply_filters();
            self.update_language_filter_buttons(cx);
            self.view.redraw(cx);
        }
        if self.view.button(ids!(filter_bar.category_row.language_filter_group.lang_zh_btn)).clicked(actions) {
            self.language_filter = LanguageFilter::Chinese;
            self.apply_filters();
            self.update_language_filter_buttons(cx);
            self.view.redraw(cx);
        }
        if self.view.button(ids!(filter_bar.category_row.language_filter_group.lang_en_btn)).clicked(actions) {
            self.language_filter = LanguageFilter::English;
            self.apply_filters();
            self.update_language_filter_buttons(cx);
            self.view.redraw(cx);
        }
    }

    /// Update category filter button active states
    fn update_category_filter_buttons(&mut self, cx: &mut Cx) {
        let all_active = if self.category_filter == VoiceFilter::All { 1.0 } else { 0.0 };
        let male_active = if self.category_filter == VoiceFilter::Male { 1.0 } else { 0.0 };
        let female_active = if self.category_filter == VoiceFilter::Female { 1.0 } else { 0.0 };
        let character_active = if self.category_filter == VoiceFilter::Character { 1.0 } else { 0.0 };

        self.view.button(ids!(filter_bar.category_row.category_filter_group.filter_all_btn))
            .apply_over(cx, live! { draw_bg: { active: (all_active) } draw_text: { active: (all_active) } });
        self.view.button(ids!(filter_bar.category_row.category_filter_group.filter_male_btn))
            .apply_over(cx, live! { draw_bg: { active: (male_active) } draw_text: { active: (male_active) } });
        self.view.button(ids!(filter_bar.category_row.category_filter_group.filter_female_btn))
            .apply_over(cx, live! { draw_bg: { active: (female_active) } draw_text: { active: (female_active) } });
        self.view.button(ids!(filter_bar.category_row.category_filter_group.filter_character_btn))
            .apply_over(cx, live! { draw_bg: { active: (character_active) } draw_text: { active: (character_active) } });
    }

    /// Update language filter button active states
    fn update_language_filter_buttons(&mut self, cx: &mut Cx) {
        let all_active = if self.language_filter == LanguageFilter::All { 1.0 } else { 0.0 };
        let zh_active = if self.language_filter == LanguageFilter::Chinese { 1.0 } else { 0.0 };
        let en_active = if self.language_filter == LanguageFilter::English { 1.0 } else { 0.0 };

        self.view.button(ids!(filter_bar.category_row.language_filter_group.lang_all_btn))
            .apply_over(cx, live! { draw_bg: { active: (all_active) } draw_text: { active: (all_active) } });
        self.view.button(ids!(filter_bar.category_row.language_filter_group.lang_zh_btn))
            .apply_over(cx, live! { draw_bg: { active: (zh_active) } draw_text: { active: (zh_active) } });
        self.view.button(ids!(filter_bar.category_row.language_filter_group.lang_en_btn))
            .apply_over(cx, live! { draw_bg: { active: (en_active) } draw_text: { active: (en_active) } });
    }

    /// Set category filter
    pub fn set_category_filter(&mut self, cx: &mut Cx, filter: VoiceFilter) {
        self.category_filter = filter;
        self.apply_filters();
        self.view.redraw(cx);
    }

    /// Set language filter
    pub fn set_language_filter(&mut self, cx: &mut Cx, filter: LanguageFilter) {
        self.language_filter = filter;
        self.apply_filters();
        self.view.redraw(cx);
    }

    /// Set search query
    pub fn set_search_query(&mut self, cx: &mut Cx, query: String) {
        self.search_query = query;
        self.apply_filters();
        self.view.redraw(cx);
    }

    /// Get filtered voices for display
    pub fn get_filtered_voices(&self) -> &Vec<Voice> {
        &self.filtered_voices
    }
}

impl VoiceSelectorRef {
    /// Get currently selected voice
    pub fn selected_voice(&self) -> Option<Voice> {
        if let Some(inner) = self.borrow() {
            if let Some(voice_id) = &inner.selected_voice_id {
                return inner.filtered_voices.iter().find(|v| &v.id == voice_id).cloned();
            }
        }
        None
    }

    /// Get selected voice ID
    pub fn selected_voice_id(&self) -> Option<String> {
        self.borrow()
            .and_then(|inner| inner.selected_voice_id.clone())
    }

    /// Get voice by ID (searches in all voices, not filtered)
    pub fn get_voice(&self, voice_id: &str) -> Option<Voice> {
        if let Some(inner) = self.borrow() {
            return inner.voices.iter().find(|v| v.id == voice_id).cloned();
        }
        None
    }

    /// Get filtered voices for display
    pub fn get_filtered_voices(&self) -> Option<Vec<Voice>> {
        if let Some(inner) = self.borrow() {
            return Some(inner.filtered_voices.clone());
        }
        None
    }

    /// Set preview playing state
    pub fn set_preview_playing(&self, cx: &mut Cx, voice_id: Option<String>) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.preview_playing_voice_id = voice_id;
            inner.view.redraw(cx);
        }
    }

    /// Check if preview is playing for a voice
    pub fn is_preview_playing(&self, voice_id: &str) -> bool {
        if let Some(inner) = self.borrow() {
            return inner.preview_playing_voice_id.as_ref() == Some(&voice_id.to_string());
        }
        false
    }

    /// Reload voices from storage and apply current filters
    pub fn reload_voices(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.reload_voices();
            inner.view.redraw(cx);
        }
    }

    /// Add a newly created custom voice
    pub fn add_custom_voice(&self, cx: &mut Cx, voice: Voice) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.custom_voices.push(voice.clone());
            inner.voices.push(voice);
            inner.apply_filters();
            inner.view.redraw(cx);
        }
    }

    /// Delete a custom voice by ID
    pub fn delete_custom_voice(&self, cx: &mut Cx, voice_id: &str) -> Result<(), String> {
        // First remove from persistence
        voice_persistence::remove_custom_voice(voice_id)?;

        // Then update internal state
        if let Some(mut inner) = self.borrow_mut() {
            inner.custom_voices.retain(|v| v.id != voice_id);
            inner.voices.retain(|v| v.id != voice_id);
            inner.apply_filters();

            // If the deleted voice was selected, select another
            if inner.selected_voice_id.as_ref() == Some(&voice_id.to_string()) {
                inner.selected_voice_id = inner.filtered_voices.first().map(|v| v.id.clone());
            }

            inner.view.redraw(cx);
        }

        Ok(())
    }

    /// Set category filter
    pub fn set_category_filter(&self, cx: &mut Cx, filter: VoiceFilter) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_category_filter(cx, filter);
        }
    }

    /// Set language filter
    pub fn set_language_filter(&self, cx: &mut Cx, filter: LanguageFilter) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_language_filter(cx, filter);
        }
    }

    /// Set search query
    pub fn set_search_query(&self, cx: &mut Cx, query: String) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.set_search_query(cx, query);
        }
    }

    /// Set dora running state
    pub fn set_dora_running(&self, cx: &mut Cx, is_running: bool) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.dora_running = is_running;
            inner.view.redraw(cx);
        }
    }

    /// Update dark mode
    pub fn update_dark_mode(&self, cx: &mut Cx, dark_mode: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.dark_mode = dark_mode;

            inner.view.apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                },
            );

            // Header background
            inner.view.view(ids!(header)).apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                },
            );

            // Header title
            inner.view.label(ids!(header.title_row.title)).apply_over(
                cx,
                live! {
                    draw_text: { dark_mode: (dark_mode) }
                },
            );

            // Selected voice badge
            inner
                .view
                .view(ids!(header.badge_row.selected_voice_badge))
                .apply_over(
                    cx,
                    live! {
                        draw_bg: { dark_mode: (dark_mode) }
                    },
                );

            // Selected voice label
            inner
                .view
                .label(ids!(
                    header.badge_row.selected_voice_badge.selected_voice_label
                ))
                .apply_over(
                    cx,
                    live! {
                        draw_text: { dark_mode: (dark_mode) }
                    },
                );

            inner.view.redraw(cx);
        }
    }
}
