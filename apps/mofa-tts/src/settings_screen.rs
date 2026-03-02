//! Settings Screen - Application settings and preferences
//!
//! This screen provides access to application settings including language selection.

use makepad_widgets::*;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;
    use mofa_widgets::theme::*;

    // Settings Screen
    pub SettingsScreen = {{SettingsScreen}} {
        width: Fill, height: Fill
        flow: Down
        spacing: 20
        padding: 20

        // Header
        header = <View> {
            width: Fill, height: Fit
            flow: Down
            spacing: 8

            title = <Label> {
                width: Fill, height: Fit
                draw_text: {
                    instance dark_mode: 0.0
                    text_style: <FONT_BOLD>{ font_size: 24.0 }
                    fn get_color(self) -> vec4 {
                        return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                    }
                }
                text: "Settings"
            }

            subtitle = <Label> {
                width: Fill, height: Fit
                draw_text: {
                    instance dark_mode: 0.0
                    text_style: <FONT_REGULAR>{ font_size: 14.0 }
                    fn get_color(self) -> vec4 {
                        return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                    }
                }
                text: "Configure application preferences"
            }
        }

        // Language Section
        language_section = <View> {
            width: Fill, height: Fit
            flow: Down
            spacing: 12

            section_title = <Label> {
                width: Fill, height: Fit
                draw_text: {
                    instance dark_mode: 0.0
                    text_style: <FONT_SEMIBOLD>{ font_size: 16.0 }
                    fn get_color(self) -> vec4 {
                        return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                    }
                }
                text: "Language"
            }

            // Language options
            language_options = <View> {
                width: Fill, height: Fit
                flow: Down
                spacing: 8

                english_button = <Button> {
                    width: Fill, height: Fit
                    text: "English"
                }

                chinese_button = <Button> {
                    width: Fill, height: Fit
                    text: "中文 (简体)"
                }
            }
        }

        // Back button
        back_button = <Button> {
            width: Fit, height: Fit
            padding: {left: 16, right: 16, top: 8, bottom: 8}
            text: "Back"
        }
    }
}

#[derive(Live, LiveHook, Widget)]
pub struct SettingsScreen {
    #[deref]
    view: View,

    #[rust]
    current_language: String,
}

impl Widget for SettingsScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        self.widget_match_event(cx, event, scope);
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

impl WidgetMatchEvent for SettingsScreen {
    fn handle_actions(&mut self, cx: &mut Cx, actions: &Actions, scope: &mut Scope) {
        // Handle language selection
        if self.view.button(id!(english_button)).clicked(&actions) {
            self.set_language(cx, "en");
            cx.action(SettingsScreenAction::LanguageChanged("en".to_string()));
        }

        if self.view.button(id!(chinese_button)).clicked(&actions) {
            self.set_language(cx, "zh-CN");
            cx.action(SettingsScreenAction::LanguageChanged("zh-CN".to_string()));
        }

        // Handle back button
        if self.view.button(id!(back_button)).clicked(&actions) {
            cx.action(SettingsScreenAction::Close);
        }
    }
}

impl SettingsScreen {
    /// Initialize the settings screen with current preferences
    pub fn init(&mut self, cx: &mut Cx) {
        // Load current language preference
        self.current_language = crate::preferences::load_language_preference();

        // Update UI to reflect current selection
        self.update_language_selection(cx);
    }

    /// Set the selected language
    fn set_language(&mut self, cx: &mut Cx, language: &str) {
        self.current_language = language.to_string();

        // Save preference
        if let Err(e) = crate::preferences::save_language_preference(language) {
            ::log::error!("Failed to save language preference: {}", e);
        }

        self.update_language_selection(cx);
    }

    /// Update the UI to show the currently selected language
    fn update_language_selection(&mut self, _cx: &mut Cx) {
        // Visual indication can be added later with button styling
        // For now, the language is saved and will take effect on next action
    }
}

/// Actions emitted by the settings screen
#[derive(Clone, Debug, DefaultNone)]
pub enum SettingsScreenAction {
    None,
    LanguageChanged(String),
    Close,
}
