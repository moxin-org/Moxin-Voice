//! Voice selector component - displays list of available voices

use crate::voice_data::{
    get_builtin_voices, matches_select_voice_trait_category, matches_timbre_filters,
    LanguageFilter, Voice, VoiceFilter, VoiceGenderAge, VoiceStyle, VoiceTraitCategory,
};
use crate::voice_persistence;
use makepad_widgets::*;
use mofa_ui::app_data::MofaAppData;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    use mofa_widgets::theme::*;

    FilterChip = <Button> {
        width: Fit, height: 28
        padding: {left: 10, right: 10}

        draw_bg: {
            instance dark_mode: 0.0
            instance hover: 0.0
            instance active: 0.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 14.0);
                let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                let hover_color = mix((SLATE_200), (SLATE_600), self.dark_mode);
                let active_color = (PRIMARY_500);
                let bg = mix(base, hover_color, self.hover);
                let bg = mix(bg, active_color, self.active);
                sdf.fill(bg);
                return sdf.result;
            }
        }

        draw_text: {
            instance dark_mode: 0.0
            instance active: 0.0
            text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
            fn get_color(self) -> vec4 {
                let normal = mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                let active = vec4(1.0, 1.0, 1.0, 1.0);
                return mix(normal, active, self.active);
            }
        }
    }

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
                align: {x: 0.3, y: 0.6}
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
            padding: {left: 16, right: 16, top: 12, bottom: 12}
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

        // Divider
        <View> {
            width: Fill, height: 1
            show_bg: true
            draw_bg: {
                instance dark_mode: 0.0
                fn pixel(self) -> vec4 {
                    return mix((BORDER), (BORDER_DARK), self.dark_mode);
                }
            }
        }

        filters = <View> {
            width: Fill, height: Fit
            flow: Down
            spacing: 8
            padding: {left: 12, right: 12, top: 8, bottom: 8}

            gender_row = <View> {
                width: Fill, height: Fit
                flow: Right
                spacing: 6
                align: {y: 0.5}

                gender_label = <Label> {
                    width: Fit, height: Fit
                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                        fn get_color(self) -> vec4 {
                            return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                        }
                    }
                    text: "性别年龄"
                }
                male_btn = <FilterChip> { text: "男声" }
                female_btn = <FilterChip> { text: "女声" }
            }

            style_row = <View> {
                width: Fill, height: Fit
                flow: Right
                spacing: 6
                align: {y: 0.5}

                style_label = <Label> {
                    width: Fit, height: Fit
                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                        fn get_color(self) -> vec4 {
                            return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                        }
                    }
                    text: "风格"
                }
                sweet_btn = <FilterChip> { text: "甜美" }
                magnetic_btn = <FilterChip> { text: "磁性" }
                youth_btn = <FilterChip> { text: "青年音" }
            }

            trait_row = <View> {
                width: Fill, height: Fit
                flow: Right
                spacing: 6
                align: {y: 0.5}

                trait_label = <Label> {
                    width: Fit, height: Fit
                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                        fn get_color(self) -> vec4 {
                            return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                        }
                    }
                    text: "声音特质"
                }
                professional_broadcast_btn = <FilterChip> { text: "专业播音" }
                featured_character_btn = <FilterChip> { text: "特色人物" }
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

    /// Store drawn item areas for hit testing: (item_id, item_area, preview_btn_area, delete_btn_area)
    #[rust]
    item_areas: Vec<(usize, Area, Area, Area)>,
    #[rust]
    gender_filters: Vec<VoiceGenderAge>,
    #[rust]
    style_filters: Vec<VoiceStyle>,
    #[rust]
    selected_trait_filter: Option<VoiceTraitCategory>,
}

impl Widget for VoiceSelector {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);

        // Initialize voices on first event
        if !self.initialized {
            self.reload_voices();
            self.sync_selected_voice_for_current_filter();
            self.update_filter_buttons(cx);
            self.update_selected_voice_badge(cx);
            // Update UI text with translations
            if let Some(app_data) = scope.data.get::<MofaAppData>() {
                self.update_ui_text(cx, app_data);
            }
            self.initialized = true;
        }

        // Handle portal list item clicks using stored areas (BEFORE Actions early return)
        for (item_id, item_area, preview_area, delete_area) in self.item_areas.clone() {
            if item_id >= self.voices.len() {
                continue;
            }

            // Check delete button click first
            match event.hits(cx, delete_area) {
                Hit::FingerUp(fe) if fe.was_tap() => {
                    let voice = &self.voices[item_id];
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
                    let voice_id = self.voices[item_id].id.clone();
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
                    let voice_id = self.voices[item_id].id.clone();
                    self.selected_voice_id = Some(voice_id.clone());
                    self.update_selected_voice_badge(cx);

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

        let male_area = self.view.button(ids!(filters.gender_row.male_btn)).area();
        if let Hit::FingerUp(fe) = event.hits(cx, male_area) {
            if fe.was_tap() {
                self.toggle_gender_filter(cx, VoiceGenderAge::Male);
            }
        }

        let female_area = self.view.button(ids!(filters.gender_row.female_btn)).area();
        if let Hit::FingerUp(fe) = event.hits(cx, female_area) {
            if fe.was_tap() {
                self.toggle_gender_filter(cx, VoiceGenderAge::Female);
            }
        }

        let sweet_area = self.view.button(ids!(filters.style_row.sweet_btn)).area();
        if let Hit::FingerUp(fe) = event.hits(cx, sweet_area) {
            if fe.was_tap() {
                self.toggle_style_filter(cx, VoiceStyle::Sweet);
            }
        }

        let magnetic_area = self
            .view
            .button(ids!(filters.style_row.magnetic_btn))
            .area();
        if let Hit::FingerUp(fe) = event.hits(cx, magnetic_area) {
            if fe.was_tap() {
                self.toggle_style_filter(cx, VoiceStyle::Magnetic);
            }
        }

        let youth_area = self.view.button(ids!(filters.style_row.youth_btn)).area();
        if let Hit::FingerUp(fe) = event.hits(cx, youth_area) {
            if fe.was_tap() {
                self.toggle_style_filter(cx, VoiceStyle::Youth);
            }
        }

        let professional_broadcast_area = self
            .view
            .button(ids!(filters.trait_row.professional_broadcast_btn))
            .area();
        if let Hit::FingerUp(fe) = event.hits(cx, professional_broadcast_area) {
            if fe.was_tap() {
                self.toggle_trait_filter(cx, VoiceTraitCategory::ProfessionalBroadcast);
            }
        }

        let featured_character_area = self
            .view
            .button(ids!(filters.trait_row.featured_character_btn))
            .area();
        if let Hit::FingerUp(fe) = event.hits(cx, featured_character_area) {
            if fe.was_tap() {
                self.toggle_trait_filter(cx, VoiceTraitCategory::FeaturedCharacter);
            }
        }
        // Note: All click handling is now done above using event.hits()
        // The items_with_actions pattern doesn't work well for View-based list items
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Initialize if needed (in case draw happens before handle_event)
        if !self.initialized {
            self.reload_voices();
            self.sync_selected_voice_for_current_filter();
            self.update_filter_buttons(cx);
            self.update_selected_voice_badge(cx);
            // Update UI text with translations
            if let Some(app_data) = scope.data.get::<MofaAppData>() {
                self.update_ui_text(cx, app_data);
            }
            self.initialized = true;
        }

        // Clear item areas before redrawing
        self.item_areas.clear();

        // Draw portal list items using borrow pattern
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.as_portal_list().borrow_mut() {
                let filtered_indices = self.filtered_voice_indices();
                list.set_item_range(cx, 0, filtered_indices.len());

                while let Some(item_id) = list.next_visible_item(cx) {
                    if item_id < filtered_indices.len() {
                        let voice_idx = filtered_indices[item_id];
                        let voice = &self.voices[voice_idx];
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

                        // Apply preview button state
                        let is_playing = self.preview_playing_voice_id.as_ref() == Some(&voice.id);
                        let playing_val = if is_playing { 1.0 } else { 0.0 };
                        let is_hovered = self.hovered_preview_idx == Some(voice_idx);
                        let hover_val = if is_hovered { 1.0 } else { 0.0 };
                        item.view(ids!(preview_btn)).apply_over(cx, live! {
                            draw_bg: { dark_mode: (self.dark_mode), playing: (playing_val), hover: (hover_val) }
                        });

                        // Show delete button only for custom voices
                        let is_custom = voice.is_custom();
                        item.view(ids!(delete_btn)).set_visible(cx, is_custom);
                        if is_custom {
                            let delete_hover = if self.hovered_delete_idx == Some(voice_idx) {
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
                        self.item_areas
                            .push((voice_idx, item_area, preview_area, delete_area));
                    }
                }
            }
        }
        DrawStep::done()
    }
}

impl VoiceSelector {
    /// Reload all voices (built-in + custom)
    fn reload_voices(&mut self) {
        self.voices = get_builtin_voices();
        self.custom_voices = voice_persistence::load_custom_voices();
        // Append custom voices to the main list
        self.voices.extend(self.custom_voices.clone());
    }

    fn filtered_voice_indices(&self) -> Vec<usize> {
        self.voices
            .iter()
            .enumerate()
            .filter_map(|(idx, voice)| {
                let matches_timbre =
                    matches_timbre_filters(voice, &self.gender_filters, &self.style_filters);
                let matches_trait = self
                    .selected_trait_filter
                    .map(|category| matches_select_voice_trait_category(voice, category))
                    .unwrap_or(true);

                if matches_timbre && matches_trait {
                    Some(idx)
                } else {
                    None
                }
            })
            .collect()
    }

    fn sync_selected_voice_for_current_filter(&mut self) {
        let filtered_indices = self.filtered_voice_indices();
        if filtered_indices.is_empty() {
            self.selected_voice_id = None;
            return;
        }

        let selected_is_visible = self.selected_voice_id.as_ref().is_some_and(|voice_id| {
            filtered_indices
                .iter()
                .any(|idx| self.voices[*idx].id == *voice_id)
        });

        if !selected_is_visible {
            self.selected_voice_id = Some(self.voices[filtered_indices[0]].id.clone());
        }
    }

    fn update_selected_voice_badge(&mut self, cx: &mut Cx) {
        let selected_name = self
            .selected_voice_id
            .as_ref()
            .and_then(|voice_id| {
                self.voices
                    .iter()
                    .find(|voice| voice.id == *voice_id)
                    .map(|voice| voice.name.clone())
            })
            .unwrap_or_else(|| "未选择".to_string());

        self.view
            .label(ids!(
                header.badge_row.selected_voice_badge.selected_voice_label
            ))
            .set_text(cx, &selected_name);
    }

    fn toggle_gender_filter(&mut self, cx: &mut Cx, filter: VoiceGenderAge) {
        if self.gender_filters.contains(&filter) {
            self.gender_filters.retain(|item| item != &filter);
        } else {
            self.gender_filters.push(filter);
        }
        self.sync_selected_voice_for_current_filter();
        self.update_selected_voice_badge(cx);
        self.update_filter_buttons(cx);
        self.view.redraw(cx);
    }

    fn toggle_style_filter(&mut self, cx: &mut Cx, filter: VoiceStyle) {
        if self.style_filters.contains(&filter) {
            self.style_filters.retain(|item| item != &filter);
        } else {
            self.style_filters.push(filter);
        }
        self.sync_selected_voice_for_current_filter();
        self.update_selected_voice_badge(cx);
        self.update_filter_buttons(cx);
        self.view.redraw(cx);
    }

    fn toggle_trait_filter(&mut self, cx: &mut Cx, filter: VoiceTraitCategory) {
        if self.selected_trait_filter == Some(filter) {
            self.selected_trait_filter = None;
        } else {
            self.selected_trait_filter = Some(filter);
        }
        self.sync_selected_voice_for_current_filter();
        self.update_selected_voice_badge(cx);
        self.update_filter_buttons(cx);
        self.view.redraw(cx);
    }

    fn update_filter_buttons(&mut self, cx: &mut Cx) {
        let male_active = self.gender_filters.contains(&VoiceGenderAge::Male) as i64 as f64;
        let female_active = self.gender_filters.contains(&VoiceGenderAge::Female) as i64 as f64;
        let sweet_active = self.style_filters.contains(&VoiceStyle::Sweet) as i64 as f64;
        let magnetic_active = self.style_filters.contains(&VoiceStyle::Magnetic) as i64 as f64;
        let youth_active = self.style_filters.contains(&VoiceStyle::Youth) as i64 as f64;
        let professional_active = (self.selected_trait_filter
            == Some(VoiceTraitCategory::ProfessionalBroadcast))
            as i64 as f64;
        let featured_active = (self.selected_trait_filter
            == Some(VoiceTraitCategory::FeaturedCharacter)) as i64
            as f64;
        let dark_mode = self.dark_mode;

        self.view
            .label(ids!(filters.gender_row.gender_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(filters.style_row.style_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(filters.trait_row.trait_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });

        self.view
            .button(ids!(filters.gender_row.male_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode), active: (male_active) }
                    draw_text: { dark_mode: (dark_mode), active: (male_active) }
                },
            );
        self.view
            .button(ids!(filters.gender_row.female_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode), active: (female_active) }
                    draw_text: { dark_mode: (dark_mode), active: (female_active) }
                },
            );
        self.view
            .button(ids!(filters.style_row.sweet_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode), active: (sweet_active) }
                    draw_text: { dark_mode: (dark_mode), active: (sweet_active) }
                },
            );
        self.view
            .button(ids!(filters.style_row.magnetic_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode), active: (magnetic_active) }
                    draw_text: { dark_mode: (dark_mode), active: (magnetic_active) }
                },
            );
        self.view
            .button(ids!(filters.style_row.youth_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode), active: (youth_active) }
                    draw_text: { dark_mode: (dark_mode), active: (youth_active) }
                },
            );
        self.view
            .button(ids!(filters.trait_row.professional_broadcast_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode), active: (professional_active) }
                    draw_text: { dark_mode: (dark_mode), active: (professional_active) }
                },
            );
        self.view
            .button(ids!(filters.trait_row.featured_character_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode), active: (featured_active) }
                    draw_text: { dark_mode: (dark_mode), active: (featured_active) }
                },
            );
    }

    /// Update UI text with translations
    fn update_ui_text(&mut self, cx: &mut Cx, app_data: &MofaAppData) {
        // Update header title
        let title = app_data.i18n().t("tts.voice.selector_label");
        self.view
            .label(ids!(header.title_row.title))
            .set_text(cx, &title);
        self.view
            .label(ids!(filters.gender_row.gender_label))
            .set_text(cx, "性别年龄");
        self.view
            .label(ids!(filters.style_row.style_label))
            .set_text(cx, "风格");
        self.view
            .label(ids!(filters.trait_row.trait_label))
            .set_text(cx, "声音特质");
    }
}

impl VoiceSelectorRef {
    /// Get currently selected voice
    pub fn selected_voice(&self) -> Option<Voice> {
        if let Some(inner) = self.borrow() {
            if let Some(voice_id) = &inner.selected_voice_id {
                return inner.voices.iter().find(|v| &v.id == voice_id).cloned();
            }
        }
        None
    }

    /// Get selected voice ID
    pub fn selected_voice_id(&self) -> Option<String> {
        self.borrow()
            .and_then(|inner| inner.selected_voice_id.clone())
    }

    /// Get voice by ID
    pub fn get_voice(&self, voice_id: &str) -> Option<Voice> {
        if let Some(inner) = self.borrow() {
            return inner.voices.iter().find(|v| v.id == voice_id).cloned();
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

    /// Reload voices from storage
    pub fn reload_voices(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.reload_voices();
            inner.sync_selected_voice_for_current_filter();
            inner.update_selected_voice_badge(cx);
            inner.update_filter_buttons(cx);
            inner.view.redraw(cx);
        }
    }

    /// Add a newly created custom voice
    pub fn add_custom_voice(&self, cx: &mut Cx, voice: Voice) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.custom_voices.push(voice.clone());
            inner.voices.push(voice);
            inner.sync_selected_voice_for_current_filter();
            inner.update_selected_voice_badge(cx);
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

            // If the deleted voice was selected, select another
            if inner.selected_voice_id.as_ref() == Some(&voice_id.to_string()) {
                inner.selected_voice_id = None;
            }

            inner.sync_selected_voice_for_current_filter();
            inner.update_selected_voice_badge(cx);
            inner.view.redraw(cx);
        }

        Ok(())
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

            inner.update_filter_buttons(cx);
            inner.view.redraw(cx);
        }
    }
}
