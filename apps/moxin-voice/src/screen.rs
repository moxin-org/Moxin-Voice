//! Screen - Moxin.tts style interface with sidebar layout
//! This is a variant of the TTS screen with a sidebar navigation similar to Moxin.tts

use crate::audio_player::TTSPlayer;
use crate::dora_integration::DoraIntegration;
use crate::i18n;
use crate::log_bridge;
use crate::training_executor::TrainingExecutor;
use crate::tts_history::{self, TtsHistoryEntry};
use crate::voice_clone_modal::{CloneMode, VoiceCloneModalAction, VoiceCloneModalWidgetExt};
use crate::voice_data::{LanguageFilter, TTSStatus, Voice, VoiceFilter};
use crate::voice_selector::{VoiceSelectorAction, VoiceSelectorWidgetExt};
use crate::task_persistence;
use hound::WavReader;
use makepad_widgets::*;
use std::path::PathBuf;
use std::time::{SystemTime, UNIX_EPOCH};

/// Current page in the application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppPage {
    #[default]
    TextToSpeech,
    VoiceLibrary,
    VoiceClone,
    TaskDetail,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TtsParamSliderKind {
    Speed,
    Pitch,
    Volume,
}

#[derive(Clone, Debug)]
enum ShareSource {
    CurrentAudio,
    History(String),
}

#[derive(Clone, Copy, Debug)]
enum ShareTarget {
    System,
    CapCut,
    Premiere,
    WeChat,
    Finder,
}

#[derive(Clone, Debug)]
struct TtsModelOption {
    id: String,
    name: String,
    description: String,
    tag_labels: Vec<String>,
    badge: Option<String>,
}

fn get_project_tts_models() -> Vec<TtsModelOption> {
    // Current project wiring uses one TTS engine node in dataflow:
    // apps/moxin-voice/dataflow/tts.yml -> primespeech-tts
    vec![TtsModelOption {
        id: "primespeech-gsv2".to_string(),
        name: "PrimeSpeech (GPT-SoVITS v2)".to_string(),
        description: "Current production TTS pipeline in this project. Supports built-in voices and cloned voices.".to_string(),
        tag_labels: vec![
            "Chinese".to_string(),
            "English".to_string(),
            "Voice Clone".to_string(),
        ],
        badge: Some("Available".to_string()),
    }]
}

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    use moxin_widgets::theme::*;
    use crate::voice_selector::VoiceSelector;
    use crate::voice_clone_modal::VoiceCloneModal;

    ParamAdjustBtn = <Button> {
        width: 28, height: 24
        draw_bg: {
            instance dark_mode: 0.0
            instance hover: 0.0
            instance border_radius: 6.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                let hover_bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                sdf.fill(mix(base, hover_bg, self.hover));
                let border = mix((SLATE_300), (SLATE_500), self.dark_mode);
                sdf.stroke(border, 1.0);
                return sdf.result;
            }
        }
        draw_text: {
            instance dark_mode: 0.0
            text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
            fn get_color(self) -> vec4 {
                return mix((SLATE_700), (SLATE_100), self.dark_mode);
            }
        }
    }

    ParamValueSlider = <View> {
        width: Fill, height: 24
        cursor: Hand
        show_bg: true
        draw_bg: {
            instance dark_mode: 0.0
            instance progress: 0.0
            instance dragging: 0.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                let p = clamp(self.progress, 0.0, 1.0);

                let track_h = 6.0;
                let track_y = self.rect_size.y * 0.5 - track_h * 0.5;
                let track_radius = track_h * 0.5;

                let track_bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                sdf.box(0.0, track_y, self.rect_size.x, track_h, track_radius);
                sdf.fill(track_bg);

                let fill_w = p * self.rect_size.x;
                if fill_w > 0.0 {
                    let fill_color = mix((PRIMARY_500), (PRIMARY_400), self.dark_mode);
                    sdf.box(0.0, track_y, fill_w, track_h, track_radius);
                    sdf.fill(fill_color);
                }

                let knob_size = 14.0 + self.dragging * 2.0;
                let knob_r = knob_size * 0.5;
                let knob_x = clamp(p * self.rect_size.x, knob_r, self.rect_size.x - knob_r);
                let knob_y = self.rect_size.y * 0.5;

                let knob_fill = mix((WHITE), (SLATE_100), self.dark_mode);
                let knob_border = mix((PRIMARY_500), (PRIMARY_300), self.dark_mode);
                sdf.circle(knob_x, knob_y, knob_r);
                sdf.fill_keep(knob_fill);
                sdf.stroke(knob_border, 1.2);

                return sdf.result;
            }
        }
    }

    // Confirmation dialog for deleting voices
    ConfirmDeleteModal = <View> {
        width: Fill, height: Fill
        flow: Overlay
        align: {x: 0.5, y: 0.5}
        visible: false

        // Semi-transparent backdrop
        backdrop = <View> {
            width: Fill, height: Fill
            show_bg: true
            draw_bg: {
                fn pixel(self) -> vec4 {
                    return vec4(0.0, 0.0, 0.0, 0.5);
                }
            }
        }

        // Dialog content
        dialog = <RoundedView> {
            width: 400, height: Fit
            flow: Down
            spacing: 0

            draw_bg: {
                instance dark_mode: 0.0
                instance border_radius: 12.0
                fn pixel(self) -> vec4 {
                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                    let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                    sdf.fill(bg);
                    return sdf.result;
                }
            }

            // Header
            header = <View> {
                width: Fill, height: Fit
                padding: {left: 24, right: 24, top: 20, bottom: 16}
                flow: Down
                spacing: 8

                title = <Label> {
                    width: Fill, height: Fit
                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: <FONT_BOLD>{ font_size: 18.0 }
                        fn get_color(self) -> vec4 {
                            return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                        }
                    }
                    text: "Delete Voice?"
                }

                voice_name = <Label> {
                    width: Fill, height: Fit
                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                        fn get_color(self) -> vec4 {
                            return mix((PRIMARY_600), (PRIMARY_400), self.dark_mode);
                        }
                    }
                    text: ""
                }

                message = <Label> {
                    width: Fill, height: Fit
                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: { font_size: 14.0 }
                        fn get_color(self) -> vec4 {
                            return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                        }
                    }
                    text: "This action cannot be undone."
                }
            }

            // Divider
            <View> {
                width: Fill, height: 1
                margin: {top: 8}
                show_bg: true
                draw_bg: {
                    instance dark_mode: 0.0
                    fn pixel(self) -> vec4 {
                        return mix((BORDER), (BORDER_DARK), self.dark_mode);
                    }
                }
            }

            // Footer with buttons
            footer = <View> {
                width: Fill, height: Fit
                padding: {left: 24, right: 24, top: 16, bottom: 20}
                flow: Right
                align: {x: 1.0, y: 0.5}
                spacing: 12

                cancel_btn = <Button> {
                    width: Fit, height: 36
                    padding: {left: 20, right: 20}
                    text: "Cancel"

                    draw_bg: {
                        instance dark_mode: 0.0
                        instance hover: 0.0
                        instance border_radius: 6.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                            let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                            let hover_color = mix((SLATE_200), (SLATE_600), self.dark_mode);
                            sdf.fill(mix(base, hover_color, self.hover));
                            sdf.stroke(mix((SLATE_300), (SLATE_500), self.dark_mode), 1.0);
                            return sdf.result;
                        }
                    }

                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                        fn get_color(self) -> vec4 {
                            return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                        }
                    }
                }

                confirm_btn = <Button> {
                    width: Fit, height: 36
                    padding: {left: 20, right: 20}
                    text: "Delete"

                    draw_bg: {
                        instance dark_mode: 0.0
                        instance hover: 0.0
                        instance border_radius: 6.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                            let base = mix((RED_500), (RED_600), self.dark_mode);
                            let hover_color = mix((RED_600), (RED_500), self.dark_mode);
                            sdf.fill(mix(base, hover_color, self.hover));
                            return sdf.result;
                        }
                    }

                    draw_text: {
                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                        fn get_color(self) -> vec4 {
                            return (WHITE);
                        }
                    }
                }
            }
        }
    }

    // Toast notification for success/error messages
    Toast = <RoundedView> {
        width: Fit, height: Fit
        padding: {left: 16, right: 16, top: 10, bottom: 10}
        visible: false

        draw_bg: {
            instance dark_mode: 0.0
            instance border_radius: 8.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                // Green success background
                let bg = mix(vec4(0.16, 0.65, 0.37, 0.95), vec4(0.13, 0.55, 0.32, 0.95), self.dark_mode);
                sdf.fill(bg);
                return sdf.result;
            }
        }

        toast_content = <View> {
            width: Fit, height: Fit
            flow: Right
            spacing: 8
            align: {y: 0.5}

            // Checkmark icon
            checkmark = <View> {
                width: 18, height: 18
                show_bg: true
                draw_bg: {
                    fn pixel(self) -> vec4 {
                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                        // Draw checkmark
                        sdf.move_to(3.0, 9.0);
                        sdf.line_to(7.0, 13.0);
                        sdf.line_to(15.0, 5.0);
                        sdf.stroke((WHITE), 2.0);
                        return sdf.result;
                    }
                }
            }

            toast_label = <Label> {
                width: Fit, height: Fit
                draw_text: {
                    text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                    fn get_color(self) -> vec4 {
                        return (WHITE);
                    }
                }
                text: "Downloaded successfully!"
            }
        }
    }

    // Confirmation dialog for cancelling tasks
    ConfirmCancelModal = <View> {
        width: Fill, height: Fill
        flow: Overlay
        align: {x: 0.5, y: 0.5}
        visible: false

        // Semi-transparent backdrop
        backdrop = <View> {
            width: Fill, height: Fill
            show_bg: true
            draw_bg: {
                fn pixel(self) -> vec4 {
                    return vec4(0.0, 0.0, 0.0, 0.5);
                }
            }
        }

        // Dialog content
        dialog = <RoundedView> {
            width: 400, height: Fit
            flow: Down
            spacing: 0

            draw_bg: {
                instance dark_mode: 0.0
                instance border_radius: 12.0
                fn pixel(self) -> vec4 {
                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                    let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                    sdf.fill(bg);
                    return sdf.result;
                }
            }

            // Header
            header = <View> {
                width: Fill, height: Fit
                padding: {left: 24, right: 24, top: 20, bottom: 16}
                flow: Down
                spacing: 8

                title = <Label> {
                    width: Fill, height: Fit
                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: <FONT_BOLD>{ font_size: 18.0 }
                        fn get_color(self) -> vec4 {
                            return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                        }
                    }
                    text: "Cancel Task?"
                }

                task_name = <Label> {
                    width: Fill, height: Fit
                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                        fn get_color(self) -> vec4 {
                            return mix((PRIMARY_600), (PRIMARY_400), self.dark_mode);
                        }
                    }
                    text: ""
                }

                message = <Label> {
                    width: Fill, height: Fit
                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: { font_size: 14.0 }
                        fn get_color(self) -> vec4 {
                            return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                        }
                    }
                    text: "The task will be stopped and cannot be resumed."
                }
            }

            // Divider
            <View> {
                width: Fill, height: 1
                margin: {top: 8}
                show_bg: true
                draw_bg: {
                    instance dark_mode: 0.0
                    fn pixel(self) -> vec4 {
                        return mix((BORDER), (BORDER_DARK), self.dark_mode);
                    }
                }
            }

            // Footer with buttons
            footer = <View> {
                width: Fill, height: Fit
                padding: {left: 24, right: 24, top: 16, bottom: 20}
                flow: Right
                align: {x: 1.0, y: 0.5}
                spacing: 12

                back_btn = <Button> {
                    width: Fit, height: 36
                    padding: {left: 20, right: 20}
                    text: "Go Back"

                    draw_bg: {
                        instance dark_mode: 0.0
                        instance hover: 0.0
                        instance border_radius: 6.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                            let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                            let hover_color = mix((SLATE_200), (SLATE_600), self.dark_mode);
                            sdf.fill(mix(base, hover_color, self.hover));
                            sdf.stroke(mix((SLATE_300), (SLATE_500), self.dark_mode), 1.0);
                            return sdf.result;
                        }
                    }

                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                        fn get_color(self) -> vec4 {
                            return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                        }
                    }
                }

                confirm_btn = <Button> {
                    width: Fit, height: 36
                    padding: {left: 20, right: 20}
                    text: "Cancel Task"

                    draw_bg: {
                        instance dark_mode: 0.0
                        instance hover: 0.0
                        instance border_radius: 6.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                            let base = mix((ORANGE_500), vec4(0.95, 0.45, 0.1, 1.0), self.dark_mode);
                            let hover_color = mix(vec4(0.95, 0.45, 0.1, 1.0), (ORANGE_500), self.dark_mode);
                            sdf.fill(mix(base, hover_color, self.hover));
                            return sdf.result;
                        }
                    }

                    draw_text: {
                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                        fn get_color(self) -> vec4 {
                            return (WHITE);
                        }
                    }
                }
            }
        }
    }

    VoicePickerFilterDropDown = <DropDown> {
        width: Fit, height: 34
        popup_menu_position: BelowInput
        draw_bg: {
            instance dark_mode: 0.0
            border_radius: 8.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                // Use a half-pixel inset for crisp 1px borders without blur.
                sdf.box(0.5, 0.5, self.rect_size.x - 1.0, self.rect_size.y - 1.0, self.border_radius);
                sdf.fill(vec4(0.0, 0.0, 0.0, 0.0));
                sdf.stroke(vec4(0.898, 0.906, 0.922, 1.0), 1.0);
                return sdf.result;
            }
        }
        draw_text: {
            text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
            fn get_color(self) -> vec4 { return vec4(0.0, 0.0, 0.0, 0.0); }
        }
        popup_menu: {
            draw_bg: {
                instance dark_mode: 0.0
                border_size: 1.0
                border_radius: 10.0
                fn pixel(self) -> vec4 {
                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                    let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                    let border = mix((SLATE_400), (SLATE_600), self.dark_mode);
                    sdf.fill(bg);
                    sdf.stroke(border, self.border_size);
                    return sdf.result;
                }
            }
            menu_item: {
                indent_width: 8.0
                padding: {left: 12, top: 7, bottom: 7, right: 12}
                draw_bg: {
                    instance dark_mode: 0.0
                    fn pixel(self) -> vec4 {
                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                        sdf.rect(0., 0., self.rect_size.x, self.rect_size.y);
                        let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                        let hover = mix((SLATE_50), (SLATE_700), self.dark_mode);
                        sdf.fill(mix(bg, hover, self.hover));
                        return sdf.result;
                    }
                }
                draw_text: {
                    instance dark_mode: 0.0
                    text_style: { font_size: 12.0 }
                    fn get_color(self) -> vec4 {
                        return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                    }
                }
            }
        }
    }

    ModelTagChip = <RoundedView> {
        width: Fit, height: Fit
        visible: false
        padding: {left: 10, right: 10, top: 4, bottom: 4}
        draw_bg: {
            border_radius: 10.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                sdf.fill((SLATE_100));
                return sdf.result;
            }
        }
        tag_label = <Label> {
            width: Fit, height: Fit
            draw_text: {
                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                fn get_color(self) -> vec4 { return (SLATE_600); }
            }
            text: "Tag"
        }
    }

    VoiceFilterChip = <Button> {
        width: Fit, height: 24
        padding: {left: 8, right: 8}
        draw_bg: {
            instance active: 0.0
            instance dark_mode: 0.0
            instance border_radius: 6.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                let normal = mix(vec4(0.0, 0.0, 0.0, 0.0), vec4(0.0, 0.0, 0.0, 0.0), self.dark_mode);
                let active = mix(vec4(0.88, 0.93, 1.0, 1.0), vec4(0.20, 0.27, 0.42, 1.0), self.dark_mode);
                sdf.fill(mix(normal, active, self.active));
                return sdf.result;
            }
        }
        draw_text: {
            instance active: 0.0
            instance dark_mode: 0.0
            text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
            fn get_color(self) -> vec4 {
                let normal = mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                let active = mix((PRIMARY_600), (PRIMARY_300), self.dark_mode);
                return mix(normal, active, self.active);
            }
        }
    }

    VoiceSelectedChip = <Button> {
        width: Fit, height: 28
        padding: {left: 10, right: 10}
        draw_bg: {
            instance dark_mode: 0.0
            instance border_radius: 8.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                let bg = mix(vec4(0.88, 0.93, 1.0, 1.0), vec4(0.20, 0.27, 0.42, 1.0), self.dark_mode);
                sdf.fill(bg);
                return sdf.result;
            }
        }
        draw_text: {
            instance dark_mode: 0.0
            text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
            fn get_color(self) -> vec4 {
                return mix((PRIMARY_600), (PRIMARY_300), self.dark_mode);
            }
        }
    }

    // Layout constants
    SECTION_SPACING = 12.0
    PANEL_RADIUS = 6.0
    PANEL_PADDING = 14.0

    // Splitter handle for resizing panels
    Splitter = <View> {
        width: 12, height: Fill
        margin: { left: 6, right: 6 }
        align: {x: 0.5, y: 0.5}
        cursor: ColResize

        show_bg: true
        draw_bg: {
            instance dark_mode: 0.0
            instance hover: 0.0

            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                // Draw subtle grip dots
                let dot_y_start = self.rect_size.y * 0.35;
                let dot_y_end = self.rect_size.y * 0.65;
                let dot_spacing = 8.0;
                let num_dots = (dot_y_end - dot_y_start) / dot_spacing;
                let color = mix(mix((SLATE_300), (SLATE_600), self.dark_mode), (PRIMARY_400), self.hover);

                // Draw vertical dots
                let y = dot_y_start;
                sdf.circle(6.0, y, 1.5);
                sdf.fill(color);
                sdf.circle(6.0, y + dot_spacing, 1.5);
                sdf.fill(color);
                sdf.circle(6.0, y + dot_spacing * 2.0, 1.5);
                sdf.fill(color);
                sdf.circle(6.0, y + dot_spacing * 3.0, 1.5);
                sdf.fill(color);
                sdf.circle(6.0, y + dot_spacing * 4.0, 1.5);
                sdf.fill(color);

                return sdf.result;
            }
        }
    }

    // Primary button style
    PrimaryButton = <Button> {
        width: Fit, height: 42
        padding: {left: 20, right: 20}

        draw_bg: {
            instance dark_mode: 0.0
            instance disabled: 0.0
            instance hover: 0.0
            instance pressed: 0.0

            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 8.0);

                let base = mix((PRIMARY_500), (PRIMARY_400), self.dark_mode);
                let hover_color = mix((PRIMARY_600), (PRIMARY_300), self.dark_mode);
                let pressed_color = mix((PRIMARY_700), (PRIMARY_500), self.dark_mode);
                let disabled_color = mix((GRAY_300), (GRAY_600), self.dark_mode);

                let color = mix(base, hover_color, self.hover);
                let color = mix(color, pressed_color, self.pressed);
                let color = mix(color, disabled_color, self.disabled);

                sdf.fill(color);
                return sdf.result;
            }
        }

        draw_text: {
            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
            fn get_color(self) -> vec4 {
                return (WHITE);
            }
        }
    }

    // Loading spinner for generate button
    GenerateSpinner = <View> {
        width: 20, height: 20
        visible: false

        show_bg: true
        draw_bg: {
            instance rotation: 0.0

            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                let center = self.rect_size * 0.5;
                let radius = 7.0;
                let dot_radius = 2.0;
                let num_dots = 6.0;

                let mut result = vec4(0.0, 0.0, 0.0, 0.0);

                // Draw 6 rotating dots in a circle
                for i in 0..6 {
                    let angle = (float(i) / num_dots) * 6.28318530718 + self.rotation * 6.28318530718;
                    let dot_x = center.x + cos(angle) * radius;
                    let dot_y = center.y + sin(angle) * radius;

                    // Calculate opacity based on position (creates rotation effect)
                    let opacity = (float(i) / num_dots) * 0.7 + 0.3;

                    sdf.circle(dot_x, dot_y, dot_radius);
                    let dot_color = vec4(1.0, 1.0, 1.0, opacity);
                    result = mix(result, dot_color, sdf.fill_keep(dot_color).a);
                }

                return result;
            }
        }

        animator: {
            spin = {
                default: off,
                off = {
                    from: {all: Forward {duration: 0.0}}
                    apply: { draw_bg: { rotation: 0.0 } }
                }
                on = {
                    from: {all: Loop {duration: 0.8, end: 1.0}}
                    apply: {
                        draw_bg: { rotation: [{time: 0.0, value: 0.0}, {time: 1.0, value: 1.0}] }
                    }
                }
            }
        }
    }

    // Icon button (circular) for stop
    IconButton = <Button> {
        width: 36, height: 36
        padding: 0

        draw_bg: {
            instance dark_mode: 0.0
            instance hover: 0.0
            instance pressed: 0.0

            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                let center = self.rect_size * 0.5;
                sdf.circle(center.x, center.y, 17.0);

                let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                let hover_color = mix((SLATE_200), (SLATE_600), self.dark_mode);
                let pressed_color = mix((SLATE_300), (SLATE_500), self.dark_mode);

                let color = mix(base, hover_color, self.hover);
                let color = mix(color, pressed_color, self.pressed);

                sdf.fill(color);
                return sdf.result;
            }
        }

        draw_text: {
            instance dark_mode: 0.0
            text_style: { font_size: 14.0 }
            fn get_color(self) -> vec4 {
                return mix((SLATE_600), (SLATE_300), self.dark_mode);
            }
        }
    }

    // Play button (primary color with play/pause icon)
    PlayButton = <Button> {
        width: 36, height: 36
        padding: 0
        margin: 0

        draw_bg: {
            instance dark_mode: 0.0
            instance hover: 0.0
            instance pressed: 0.0
            instance is_playing: 0.0

            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                let center = self.rect_size * 0.5;
                sdf.circle(center.x, center.y, 17.0);

                let base = mix((PRIMARY_500), (PRIMARY_400), self.dark_mode);
                let hover_color = mix((PRIMARY_600), (PRIMARY_300), self.dark_mode);
                let pressed_color = mix((PRIMARY_700), (PRIMARY_500), self.dark_mode);

                let color = mix(base, hover_color, self.hover);
                let color = mix(color, pressed_color, self.pressed);

                sdf.fill(color);

                // Draw play or pause icon
                if self.is_playing > 0.5 {
                    // Pause icon (two vertical bars)
                    sdf.rect(13.0, 12.0, 3.0, 12.0);
                    sdf.fill((WHITE));
                    sdf.rect(20.0, 12.0, 3.0, 12.0);
                    sdf.fill((WHITE));
                } else {
                    // Play icon (triangle) - slightly offset right for optical center
                    sdf.move_to(14.0, 11.0);
                    sdf.line_to(26.0, 18.0);
                    sdf.line_to(14.0, 25.0);
                    sdf.close_path();
                    sdf.fill((WHITE));
                }

                return sdf.result;
            }
        }

        draw_text: {
            text_style: { font_size: 0.0 }
            fn get_color(self) -> vec4 {
                return vec4(0.0, 0.0, 0.0, 0.0);
            }
        }
    }

    // Moxin.tts Navigation item button for sidebar
    NavItem = <Button> {
        width: Fill, height: 44
        padding: {left: 16, right: 16}
        align: {y: 0.5}
        
        draw_bg: {
            instance hover: 0.0
            instance active: 0.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 8.0);
                let normal = vec4(0.0, 0.0, 0.0, 0.0);
                let hover_color = vec4(1.0, 1.0, 1.0, 0.08);
                let active_color = (MOXIN_PRIMARY);
                let bg = mix(normal, hover_color, self.hover);
                let bg = mix(bg, active_color, self.active);
                sdf.fill(bg);
                return sdf.result;
            }
        }
        
        draw_text: {
            instance active: 0.0
            text_style: { font_size: 14.0 }
            fn get_color(self) -> vec4 {
                let normal = vec4(1.0, 1.0, 1.0, 0.7);
                let active = vec4(1.0, 1.0, 1.0, 1.0);
                return mix(normal, active, self.active);
            }
        }
    }

    // TTS Screen - Moxin.tts style layout with sidebar
    pub TTSScreen = {{TTSScreen}} {
        width: Fill, height: Fill
        flow: Overlay
        spacing: 0
        padding: 0
        align: {x: 0.0, y: 0.0}

        show_bg: true
        draw_bg: {
            instance dark_mode: 0.0
            fn pixel(self) -> vec4 {
                return mix((DARK_BG), (DARK_BG_DARK), self.dark_mode);
            }
        }

        // Moxin.tts style app layout with sidebar
        app_layout = <View> {
            width: Fill, height: Fill
            flow: Right
            spacing: 0
            padding: 0
            align: {x: 0.0, y: 0.0}

            // ============ Moxin.tts Sidebar ============
            sidebar = <View> {
                width: 220, height: Fill
                flow: Down
                spacing: 0
                
                show_bg: true
                draw_bg: {
                    fn pixel(self) -> vec4 {
                        return (MOXIN_BG_SIDEBAR);
                    }
                }

                // Sidebar Header: Logo
                sidebar_header = <View> {
                    width: Fill, height: Fit
                    flow: Right
                    padding: {left: 20, right: 20, top: 20, bottom: 16}
                    align: {x: 0.0, y: 0.5}
                    
                    show_bg: true
                    draw_bg: {
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.rect(0.0, self.rect_size.y - 1.0, self.rect_size.x, 1.0);
                            sdf.fill(vec4(1.0, 1.0, 1.0, 0.1));
                            return sdf.result;
                        }
                    }

                    logo_section = <View> {
                        width: Fill, height: Fit
                        flow: Right
                        spacing: 10
                        align: {y: 0.5}

                        logo_icon = <Label> {
                            width: Fit, height: Fit
                            draw_text: {
                                text_style: { font_size: 22.0 }
                                fn get_color(self) -> vec4 { return vec4(1.0, 1.0, 1.0, 1.0); }
                            }
                            text: "🎙"
                        }

                        logo_text = <Label> {
                            width: Fit, height: Fit
                            draw_text: {
                                text_style: <FONT_SEMIBOLD>{ font_size: 16.0 }
                                fn get_color(self) -> vec4 { return vec4(1.0, 1.0, 1.0, 1.0); }
                            }
                            text: "TTS Voice"
                        }
                    }
                }

                // Sidebar Navigation
                sidebar_nav = <View> {
                    width: Fill, height: Fill
                    flow: Down
                    padding: {left: 12, right: 12, top: 16, bottom: 16}
                    spacing: 4

                    nav_tts = <NavItem> {
                        text: "📝 Text to Speech"
                        draw_bg: { active: 1.0 }
                        draw_text: { active: 1.0 }
                    }

                    nav_library = <NavItem> {
                        text: "🎤 Voice Library"
                    }

                    nav_clone = <NavItem> {
                        text: "📋 Voice Clone"
                    }

                    nav_history = <NavItem> {
                        text: "🕘 History"
                    }
                }

                // Sidebar Footer: User Info
                sidebar_footer = <View> {
                    width: Fill, height: Fit
                    flow: Right
                    padding: {left: 16, right: 16, top: 16, bottom: 16}
                    spacing: 12
                    align: {y: 0.5}
                    
                    show_bg: true
                    draw_bg: {
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.rect(0.0, 0.0, self.rect_size.x, 1.0);
                            sdf.fill(vec4(1.0, 1.0, 1.0, 0.1));
                            return sdf.result;
                        }
                    }

                    user_avatar = <RoundedView> {
                        width: 36, height: 36
                        align: {x: 0.5, y: 0.5}
                        draw_bg: {
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.circle(self.rect_size.x * 0.5, self.rect_size.y * 0.5, 18.0);
                                sdf.fill((MOXIN_PRIMARY));
                                return sdf.result;
                            }
                        }

                        avatar_letter = <Label> {
                            width: Fill, height: Fill
                            align: {x: 0.5, y: 0.5}
                            draw_text: {
                                text_style: <FONT_BOLD>{ font_size: 14.0 }
                                fn get_color(self) -> vec4 { return vec4(1.0, 1.0, 1.0, 1.0); }
                            }
                            text: "U"
                        }
                    }

                    user_details = <View> {
                        width: Fill, height: Fit
                        flow: Down
                        spacing: 2

                        user_name = <Label> {
                            width: Fit, height: Fit
                            draw_text: {
                                text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                fn get_color(self) -> vec4 { return vec4(1.0, 1.0, 1.0, 1.0); }
                            }
                            text: "User"
                        }
                    }

                    // Settings button
                    global_settings_btn = <Button> {
                        width: 36, height: 36
                        text: "⚙"
                        draw_bg: {
                            instance hover: 0.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.circle(self.rect_size.x * 0.5, self.rect_size.y * 0.5, 16.0);
                                let bg = mix(vec4(1.0, 1.0, 1.0, 0.0), vec4(1.0, 1.0, 1.0, 0.15), self.hover);
                                sdf.fill(bg);
                                return sdf.result;
                            }
                        }
                        draw_text: {
                            text_style: <FONT_BOLD>{ font_size: 16.0 }
                            fn get_color(self) -> vec4 {
                                return vec4(1.0, 1.0, 1.0, 0.8);
                            }
                        }
                    }
                }
            }

            // ============ Main Content Area ============
            // IMPORTANT: Keep same widget path structure as original screen.rs for event handling compatibility
            content_wrapper = <View> {
                width: Fill, height: Fill
                flow: Down
                spacing: 0
                padding: 0

                show_bg: true
                draw_bg: {
                    instance dark_mode: 0.0
                    fn pixel(self) -> vec4 {
                        // Moxin.tts style: light gray background
                        return mix((MOXIN_BG_PRIMARY), (MOXIN_BG_PRIMARY_DARK), self.dark_mode);
                    }
                }

            // Main content area (fills remaining space) - Moxin.tts simplified layout
            main_content = <View> {
                width: Fill, height: Fill
                flow: Down
                spacing: 0
                padding: { left: 32, right: 32, top: 24, bottom: 0 }

            // Left column - we keep this structure for compatibility but it now contains everything
            left_column = <View> {
                width: Fill, height: Fill
                flow: Down
                spacing: 20
                align: {y: 0.0}

                // Main content area - Moxin.tts style unified layout
                content_area = <View> {
                    width: Fill, height: Fill
                    flow: Overlay  // Use Overlay to stack pages
                    spacing: 0

                    // ============ Text to Speech Page ============
                    tts_page = <View> {
                        width: Fill, height: Fill
                        flow: Down
                        spacing: 20
                        visible: true  // Default visible

                    // Page title - Moxin.tts style
                    page_header = <View> {
                        width: Fill, height: Fit
                        flow: Right
                        align: {y: 0.5}
                        
                        page_title = <Label> {
                            width: Fit, height: Fit
                            draw_text: {
                                instance dark_mode: 0.0
                                text_style: <FONT_SEMIBOLD>{ font_size: 24.0 }
                                fn get_color(self) -> vec4 {
                                    return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                }
                            }
                            text: "文本转语音"
                        }
                    }

                    // Cards container - horizontal layout
                    cards_container = <View> {
                        width: Fill, height: Fill
                        flow: Right
                        spacing: 20

                    // Text input section (fills space) - Moxin.tts card style
                    input_section = <RoundedView> {
                        width: Fill, height: Fill
                        flow: Down
                        show_bg: true
                        draw_bg: {
                            instance dark_mode: 0.0
                            instance border_radius: 16.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                // Moxin.tts style: white card background
                                let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                sdf.fill(bg);
                                return sdf.result;
                            }
                        }

                        // Header - hidden for Moxin.tts style (title is in page_header now)
                        header = <View> {
                            width: Fill, height: 0
                            visible: false
                            
                            title = <Label> {
                                text: ""
                            }
                        }

                        // Text input container - Moxin.tts clean style
                        input_container = <View> {
                            width: Fill, height: Fill
                            flow: Down
                            padding: {left: 24, right: 24, top: 24, bottom: 16}

                            text_input = <TextInput> {
                                width: Fill, height: Fill
                                padding: {left: 0, right: 0, top: 0, bottom: 0}
                                empty_text: "请输入要转换的文本..."
                                text: "复杂的问题背后也许没有统一的答案，选择站在正方还是反方，其实取决于你对一系列价值判断的回答。"

                                draw_bg: {
                                    fn pixel(self) -> vec4 {
                                        // Transparent background - text directly on card
                                        return vec4(0.0, 0.0, 0.0, 0.0);
                                    }
                                }

                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: { font_size: 16.0, line_spacing: 1.8 }
                                    fn get_color(self) -> vec4 {
                                        return mix((MOXIN_TEXT_PRIMARY), (MOXIN_TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }

                                draw_cursor: {
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 0.5);
                                        sdf.fill((MOXIN_PRIMARY));
                                        return sdf.result;
                                    }
                                }

                                draw_selection: {
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 1.0);
                                        sdf.fill(vec4(0.39, 0.40, 0.95, 0.2));
                                        return sdf.result;
                                    }
                                }
                            }
                        }

                        // Bottom bar with model control and generate button - Moxin.tts style
                        bottom_bar = <View> {
                            width: Fill, height: Fit
                            flow: Down
                            align: {x: 0.0, y: 0.0}
                            padding: {left: 20, right: 20, top: 12, bottom: 14}
                            spacing: 8
                            
                            show_bg: true
                            draw_bg: {
                                instance dark_mode: 0.0
                                fn pixel(self) -> vec4 {
                                    // Top border
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.rect(0.0, 0.0, self.rect_size.x, 1.0);
                                    let border = mix((MOXIN_BORDER_LIGHT), (SLATE_700), self.dark_mode);
                                    sdf.fill(border);
                                    return sdf.result;
                                }
                            }

                            // Model selector row (placed above generate button)
                            model_row = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                align: {y: 0.5}
                                spacing: 8

                                model_label = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                        }
                                    }
                                    text: "Model"
                                }

                                model_picker_btn = <Button> {
                                    width: Fill, height: 34
                                    padding: {left: 10, right: 10}
                                    text: "🔮 GPT-SoVITS v2"
                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        instance hover: 0.0
                                        instance border_radius: 7.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                            let bg = mix((SLATE_50), (SLATE_800), self.dark_mode);
                                            let bg = mix(bg, mix((SLATE_100), (SLATE_700), self.dark_mode), self.hover);
                                            sdf.fill(bg);
                                            let border = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                            sdf.stroke(border, 1.0);
                                            return sdf.result;
                                        }
                                    }
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                        }
                                    }
                                }
                            }

                            // TTS parameter controls moved from right panel
                            param_controls = <View> {
                                width: Fill, height: Fit
                                flow: Down
                                spacing: 8

                                // Speed slider
                                speed_row = <View> {
                                    width: Fill, height: Fit
                                    flow: Right
                                    align: {y: 0.5}
                                    spacing: 10

                                    speed_header = <View> {
                                        width: 92, height: Fit
                                        flow: Right
                                        align: {y: 0.5}

                                        speed_label = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "Speed"
                                        }
                                        <View> { width: Fill, height: 1 }
                                        speed_value = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: { font_size: 10.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "1.0x"
                                        }
                                    }

                                    speed_slider_row = <View> {
                                        width: Fill, height: Fit
                                        flow: Right
                                        align: {y: 0.5}
                                        spacing: 6

                                        speed_min_slot = <View> {
                                            width: 48, height: Fit
                                            flow: Right
                                            align: {y: 0.5}

                                            slower_label = <Label> {
                                                width: Fit, height: Fit
                                                draw_text: {
                                                    instance dark_mode: 0.0
                                                    text_style: { font_size: 9.0 }
                                                    fn get_color(self) -> vec4 {
                                                        return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                    }
                                                }
                                                text: "Slower"
                                            }
                                            <View> { width: Fill, height: 1 }
                                        }

                                        speed_slider = <ParamValueSlider> {}

                                        speed_max_slot = <View> {
                                            width: 48, height: Fit
                                            flow: Right
                                            align: {y: 0.5}

                                            <View> { width: Fill, height: 1 }
                                            faster_label = <Label> {
                                                width: Fit, height: Fit
                                                draw_text: {
                                                    instance dark_mode: 0.0
                                                    text_style: { font_size: 9.0 }
                                                    fn get_color(self) -> vec4 {
                                                        return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                    }
                                                }
                                                text: "Faster"
                                            }
                                        }
                                    }
                                }

                                // Pitch slider
                                pitch_row = <View> {
                                    width: Fill, height: Fit
                                    flow: Right
                                    align: {y: 0.5}
                                    spacing: 10

                                    pitch_header = <View> {
                                        width: 92, height: Fit
                                        flow: Right
                                        align: {y: 0.5}

                                        pitch_label = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "Pitch"
                                        }
                                        <View> { width: Fill, height: 1 }
                                        pitch_value = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: { font_size: 10.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "0"
                                        }
                                    }

                                    pitch_slider_row = <View> {
                                        width: Fill, height: Fit
                                        flow: Right
                                        align: {y: 0.5}
                                        spacing: 6

                                        pitch_min_slot = <View> {
                                            width: 48, height: Fit
                                            flow: Right
                                            align: {y: 0.5}

                                            lower_label = <Label> {
                                                width: Fit, height: Fit
                                                draw_text: {
                                                    instance dark_mode: 0.0
                                                    text_style: { font_size: 9.0 }
                                                    fn get_color(self) -> vec4 {
                                                        return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                    }
                                                }
                                                text: "Lower"
                                            }
                                            <View> { width: Fill, height: 1 }
                                        }

                                        pitch_slider = <ParamValueSlider> {}

                                        pitch_max_slot = <View> {
                                            width: 48, height: Fit
                                            flow: Right
                                            align: {y: 0.5}

                                            <View> { width: Fill, height: 1 }
                                            higher_label = <Label> {
                                                width: Fit, height: Fit
                                                draw_text: {
                                                    instance dark_mode: 0.0
                                                    text_style: { font_size: 9.0 }
                                                    fn get_color(self) -> vec4 {
                                                        return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                    }
                                                }
                                                text: "Higher"
                                            }
                                        }
                                    }
                                }

                                // Volume slider
                                volume_row = <View> {
                                    width: Fill, height: Fit
                                    flow: Right
                                    align: {y: 0.5}
                                    spacing: 10

                                    volume_header = <View> {
                                        width: 92, height: Fit
                                        flow: Right
                                        align: {y: 0.5}

                                        volume_label = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "Volume"
                                        }
                                        <View> { width: Fill, height: 1 }
                                        volume_value = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: { font_size: 10.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "100%"
                                        }
                                    }

                                    volume_slider_row = <View> {
                                        width: Fill, height: Fit
                                        flow: Right
                                        align: {y: 0.5}
                                        spacing: 6

                                        volume_min_slot = <View> {
                                            width: 48, height: Fit
                                            flow: Right
                                            align: {y: 0.5}

                                            quiet_label = <Label> {
                                                width: Fit, height: Fit
                                                draw_text: {
                                                    instance dark_mode: 0.0
                                                    text_style: { font_size: 9.0 }
                                                    fn get_color(self) -> vec4 {
                                                        return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                    }
                                                }
                                                text: "Quiet"
                                            }
                                            <View> { width: Fill, height: 1 }
                                        }

                                        volume_slider = <ParamValueSlider> {}

                                        volume_max_slot = <View> {
                                            width: 48, height: Fit
                                            flow: Right
                                            align: {y: 0.5}

                                            <View> { width: Fill, height: 1 }
                                            loud_label = <Label> {
                                                width: Fit, height: Fit
                                                draw_text: {
                                                    instance dark_mode: 0.0
                                                    text_style: { font_size: 9.0 }
                                                    fn get_color(self) -> vec4 {
                                                        return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                    }
                                                }
                                                text: "Loud"
                                            }
                                        }
                                    }
                                }
                            }

                            action_row = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                align: {x: 0.0, y: 0.5}
                                spacing: 16

                                // Character count
                                char_count = <Label> {
                                    width: Fit, height: Fit
                                    align: {y: 0.5}
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: { font_size: 13.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((MOXIN_TEXT_MUTED), (MOXIN_TEXT_MUTED_DARK), self.dark_mode);
                                        }
                                    }
                                    text: "0 / 5,000 字符"
                                }

                                <View> { width: Fill, height: 1 }

                                // Generate button with spinner - Moxin.tts style
                                generate_section = <View> {
                                    width: Fit, height: Fit
                                    flow: Right
                                    align: {y: 0.5}
                                    spacing: 8

                                    // Spinner on the left (hidden by default)
                                    generate_spinner = <GenerateSpinner> {}

                                    generate_btn = <Button> {
                                        width: Fit, height: 44
                                        padding: {left: 28, right: 28}
                                        text: "生成语音"

                                        draw_bg: {
                                            instance hover: 0.0
                                            instance disabled: 0.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 10.0);
                                                let base = (MOXIN_PRIMARY);
                                                let hover_color = (MOXIN_PRIMARY_LIGHT);
                                                let disabled_color = vec4(0.6, 0.6, 0.65, 1.0);
                                                let color = mix(base, hover_color, self.hover);
                                                let color = mix(color, disabled_color, self.disabled);
                                                sdf.fill(color);
                                                return sdf.result;
                                            }
                                        }

                                        draw_text: {
                                            text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                            fn get_color(self) -> vec4 {
                                                return vec4(1.0, 1.0, 1.0, 1.0);
                                            }
                                        }
                                    }
                                }
                            }
                        }
                    }

                    // Right panel - Settings/History (ElevenLabs style)
                    controls_panel = <View> {
                        width: 320, height: Fill
                        flow: Down
                        show_bg: true
                        draw_bg: {
                            instance dark_mode: 0.0
                            fn pixel(self) -> vec4 {
                                return mix((WHITE), (SLATE_900), self.dark_mode);
                            }
                        }

                        // Tab header: Voice | Settings | History
                        settings_tabs = <View> {
                            width: Fill, height: 48
                            flow: Right
                            padding: {left: 16, right: 16, top: 8, bottom: 8}
                            spacing: 0
                            show_bg: true
                            draw_bg: {
                                instance dark_mode: 0.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.rect(0.0, self.rect_size.y - 1.0, self.rect_size.x, 1.0);
                                    let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                                    sdf.fill(border);
                                    return sdf.result;
                                }
                            }

                            voice_management_tab_btn = <Button> {
                                width: 0, height: 0
                                padding: {left: 0, right: 0}
                                text: "音色管理"
                                draw_bg: {
                                    instance active: 1.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.rect(0.0, self.rect_size.y - 2.0, self.rect_size.x, 2.0);
                                        let underline = mix(vec4(0.0, 0.0, 0.0, 0.0), (MOXIN_PRIMARY), self.active);
                                        sdf.fill(underline);
                                        return sdf.result;
                                    }
                                }
                                draw_text: {
                                    instance active: 1.0
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                    fn get_color(self) -> vec4 {
                                        let normal = mix(vec4(0.5, 0.5, 0.55, 1.0), vec4(0.62, 0.62, 0.68, 1.0), self.dark_mode);
                                        let active = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                        return mix(normal, active, self.active);
                                    }
                                }
                            }

                            settings_tab_btn = <Button> {
                                width: Fit, height: Fill
                                padding: {left: 16, right: 16}
                                text: "参数"
                                draw_bg: {
                                    instance active: 0.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.rect(0.0, self.rect_size.y - 2.0, self.rect_size.x, 2.0);
                                        let underline = mix(vec4(0.0, 0.0, 0.0, 0.0), (MOXIN_PRIMARY), self.active);
                                        sdf.fill(underline);
                                        return sdf.result;
                                    }
                                }
                                draw_text: {
                                    instance active: 1.0
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                    fn get_color(self) -> vec4 {
                                        let normal = mix(vec4(0.5, 0.5, 0.55, 1.0), vec4(0.62, 0.62, 0.68, 1.0), self.dark_mode);
                                        let active = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                        return mix(normal, active, self.active);
                                    }
                                }
                            }

                            history_tab_btn = <Button> {
                                width: 0, height: 0
                                padding: {left: 0, right: 0}
                                text: "History"
                                draw_bg: {
                                    instance active: 0.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.rect(0.0, self.rect_size.y - 2.0, self.rect_size.x, 2.0);
                                        let underline = mix(vec4(0.0, 0.0, 0.0, 0.0), (MOXIN_PRIMARY), self.active);
                                        sdf.fill(underline);
                                        return sdf.result;
                                    }
                                }
                                draw_text: {
                                    instance active: 0.0
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                    fn get_color(self) -> vec4 {
                                        let normal = mix(vec4(0.5, 0.5, 0.55, 1.0), vec4(0.62, 0.62, 0.68, 1.0), self.dark_mode);
                                        let active = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                        return mix(normal, active, self.active);
                                    }
                                }
                            }
                        }

                        // Voice management panel content
                        voice_management_panel = <View> {
                            width: Fill, height: Fill
                            flow: Down
                            padding: {left: 16, right: 16, top: 16, bottom: 16}
                            spacing: 20
                            visible: false

                            voice_row = <View> {
                                width: Fill, height: 0
                                visible: false
                                flow: Down
                                spacing: 8

                                voice_label = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                        }
                                    }
                                    text: "Voice"
                                }

                                voice_picker_btn = <Button> {
                                    width: Fill, height: 52
                                    padding: {left: 12, right: 12}
                                    text: "🎤 Doubao - Natural & Expressive"
                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        instance hover: 0.0
                                        instance border_radius: 8.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                            let bg = mix((SLATE_50), (SLATE_800), self.dark_mode);
                                            let bg = mix(bg, mix((SLATE_100), (SLATE_700), self.dark_mode), self.hover);
                                            sdf.fill(bg);
                                            let border = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                            sdf.stroke(border, 1.0);
                                            return sdf.result;
                                        }
                                    }
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                        }
                                    }
                                }

                                voice_tags_row = <View> {
                                    width: Fill, height: Fit
                                    visible: false
                                    flow: Right
                                    spacing: 8

                                    gender_badge = <RoundedView> {
                                        width: Fit, height: Fit
                                        visible: false
                                        padding: {left: 10, right: 10, top: 5, bottom: 5}
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            border_radius: 999.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                let bg = mix((PRIMARY_50), (PRIMARY_900), self.dark_mode);
                                                sdf.fill(bg);
                                                return sdf.result;
                                            }
                                        }

                                        gender_badge_label = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((PRIMARY_700), (PRIMARY_200), self.dark_mode);
                                                }
                                            }
                                            text: "男声"
                                        }
                                    }

                                    age_badge = <RoundedView> {
                                        width: Fit, height: Fit
                                        padding: {left: 10, right: 10, top: 5, bottom: 5}
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            border_radius: 999.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                                sdf.fill(bg);
                                                return sdf.result;
                                            }
                                        }

                                        age_badge_label = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "成年音"
                                        }
                                    }

                                    style_badge = <RoundedView> {
                                        width: Fit, height: Fit
                                        padding: {left: 10, right: 10, top: 5, bottom: 5}
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            border_radius: 999.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                let bg = mix(vec4(0.98, 0.92, 0.96, 1.0), vec4(0.38, 0.24, 0.34, 1.0), self.dark_mode);
                                                sdf.fill(bg);
                                                return sdf.result;
                                            }
                                        }

                                        style_badge_label = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix(vec4(0.74, 0.28, 0.52, 1.0), vec4(1.0, 0.78, 0.88, 1.0), self.dark_mode);
                                                }
                                            }
                                            text: "甜美"
                                        }
                                    }
                                }
                            }
                        }

                        // Settings panel content
                        settings_panel = <View> {
                            width: Fill, height: Fill
                            flow: Down
                            padding: {left: 16, right: 16, top: 16, bottom: 16}
                            spacing: 0
                            visible: true

                            voice_row = <View> {
                                width: Fill, height: 0
                                visible: false
                                flow: Down
                                spacing: 8

                                voice_label = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                        }
                                    }
                                    text: "Voice"
                                }

                                voice_picker_btn = <Button> {
                                    width: Fill, height: 52
                                    padding: {left: 12, right: 12}
                                    text: "🎤 Doubao - Natural & Expressive"
                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        instance hover: 0.0
                                        instance border_radius: 8.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                            let bg = mix((SLATE_50), (SLATE_800), self.dark_mode);
                                            let bg = mix(bg, mix((SLATE_100), (SLATE_700), self.dark_mode), self.hover);
                                            sdf.fill(bg);
                                            let border = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                            sdf.stroke(border, 1.0);
                                            return sdf.result;
                                        }
                                    }
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                        }
                                    }
                                }

                                voice_tags_row = <View> {
                                    width: Fill, height: Fit
                                    visible: false
                                    flow: Right
                                    spacing: 8

                                    gender_badge = <RoundedView> {
                                        width: Fit, height: Fit
                                        visible: false
                                        padding: {left: 10, right: 10, top: 5, bottom: 5}
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            border_radius: 999.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                let bg = mix((PRIMARY_50), (PRIMARY_900), self.dark_mode);
                                                sdf.fill(bg);
                                                return sdf.result;
                                            }
                                        }

                                        gender_badge_label = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((PRIMARY_700), (PRIMARY_200), self.dark_mode);
                                                }
                                            }
                                            text: "男声"
                                        }
                                    }

                                    age_badge = <RoundedView> {
                                        width: Fit, height: Fit
                                        padding: {left: 10, right: 10, top: 5, bottom: 5}
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            border_radius: 999.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                                sdf.fill(bg);
                                                return sdf.result;
                                            }
                                        }

                                        age_badge_label = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "成年音"
                                        }
                                    }

                                    style_badge = <RoundedView> {
                                        width: Fit, height: Fit
                                        padding: {left: 10, right: 10, top: 5, bottom: 5}
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            border_radius: 999.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                let bg = mix(vec4(0.98, 0.92, 0.96, 1.0), vec4(0.38, 0.24, 0.34, 1.0), self.dark_mode);
                                                sdf.fill(bg);
                                                return sdf.result;
                                            }
                                        }

                                        style_badge_label = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix(vec4(0.74, 0.28, 0.52, 1.0), vec4(1.0, 0.78, 0.88, 1.0), self.dark_mode);
                                                }
                                            }
                                            text: "甜美"
                                        }
                                    }
                                }
                            }

                            inline_voice_picker = <View> {
                                width: Fill, height: Fill
                                flow: Down
                                spacing: 10

                                voice_filter_card = <RoundedView> {
                                    width: Fill, height: Fit
                                    flow: Down
                                    spacing: 8
                                    padding: {left: 12, right: 12, top: 10, bottom: 10}
                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        instance border_radius: 10.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                            let bg = mix(vec4(0.96, 0.97, 0.99, 1.0), vec4(0.16, 0.19, 0.25, 1.0), self.dark_mode);
                                            let border = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                            sdf.fill(bg);
                                            sdf.stroke(border, 1.0);
                                            return sdf.result;
                                        }
                                    }

                                    select_voice_row = <View> {
                                        width: Fill, height: Fit
                                        flow: Right
                                        align: {y: 0.5}
                                        spacing: 8

                                        select_voice_title = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "Select Voice"
                                        }
                                        <View> { width: Fill, height: 1 }
                                        selected_voice_btn = <VoiceSelectedChip> {
                                            text: "罗翔 (Luo Xiang)"
                                        }
                                    }

                                    tag_row_gender = <View> {
                                        width: Fill, height: Fit
                                        flow: Right
                                        align: {y: 0.5}
                                        spacing: 6

                                        tag_group_label = <Label> {
                                            width: 62, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: { font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "性别年龄"
                                        }

                                        gender_male_btn = <VoiceFilterChip> { text: "男声" }
                                        gender_female_btn = <VoiceFilterChip> { text: "女声" }
                                        age_adult_btn = <VoiceFilterChip> { text: "成年" }
                                        age_youth_btn = <VoiceFilterChip> { text: "青年" }
                                    }

                                    tag_row_style = <View> {
                                        width: Fill, height: Fit
                                        flow: Right
                                        align: {y: 0.5}
                                        spacing: 6

                                        tag_group_label = <Label> {
                                            width: 62, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: { font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "风格"
                                        }

                                        style_sweet_btn = <VoiceFilterChip> { text: "甜美" }
                                        style_magnetic_btn = <VoiceFilterChip> { text: "磁性" }
                                    }

                                    tag_row_trait = <View> {
                                        width: Fill, height: Fit
                                        flow: Right
                                        align: {y: 0.5}
                                        spacing: 6

                                        tag_group_label = <Label> {
                                            width: 62, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: { font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "声音特质"
                                        }

                                        trait_prof_btn = <VoiceFilterChip> { text: "专业播音" }
                                        trait_character_btn = <VoiceFilterChip> { text: "特色人物" }
                                    }
                                }

                                // Voice list (scrollable)
                                voice_picker_list = <PortalList> {
                                    width: Fill, height: Fill
                                    flow: Down

                                    VoicePickerItem = <View> {
                                        width: Fill, height: 84
                                        margin: {left: 0, right: 0, top: 8}
                                        padding: {left: 12, right: 12, top: 10, bottom: 10}
                                        flow: Right
                                        align: {y: 0.5}
                                        spacing: 12
                                        cursor: Hand

                                        show_bg: true
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            instance hover: 0.0
                                            instance selected: 0.0
                                            instance border_radius: 10.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                let base = mix((WHITE), (SLATE_800), self.dark_mode);
                                                let hover_color = mix((SLATE_50), (SLATE_700), self.dark_mode);
                                                let selected_color = mix((PRIMARY_50), (PRIMARY_900), self.dark_mode);
                                                let border = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                                let color = mix(base, hover_color, self.hover);
                                                let color = mix(color, selected_color, self.selected);
                                                sdf.fill(color);
                                                sdf.stroke(border, 1.0);
                                                return sdf.result;
                                            }
                                        }

                                        picker_avatar = <RoundedView> {
                                            width: 42, height: 42
                                            align: {x: 0.5, y: 0.5}
                                            draw_bg: {
                                                fn pixel(self) -> vec4 {
                                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                    sdf.circle(21.0, 21.0, 21.0);
                                                    sdf.fill((PRIMARY_500));
                                                    return sdf.result;
                                                }
                                            }
                                            picker_initial = <Label> {
                                                width: Fill, height: Fill
                                                align: {x: 0.3, y: 0.6}
                                                draw_text: {
                                                    text_style: <FONT_SEMIBOLD>{ font_size: 16.0 }
                                                    fn get_color(self) -> vec4 { return (WHITE); }
                                                }
                                                text: "D"
                                            }
                                        }

                                        picker_info = <View> {
                                            width: Fill, height: Fit
                                            flow: Down
                                            spacing: 8

                                            picker_name = <Label> {
                                                width: Fill, height: 24
                                                draw_text: {
                                                    instance dark_mode: 0.0
                                                    text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                                    wrap: Ellipsis
                                                    fn get_color(self) -> vec4 {
                                                        return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                    }
                                                }
                                                text: "Voice Name"
                                            }
                                            picker_desc = <Label> {
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

                                        picker_play_btn = <View> {
                                            width: 30, height: 30
                                            align: {x: 0.5, y: 0.5}
                                            cursor: Hand
                                            show_bg: true
                                            draw_bg: {
                                                instance dark_mode: 0.0
                                                instance hover: 0.0
                                                instance playing: 0.0
                                                fn pixel(self) -> vec4 {
                                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                    sdf.circle(15.0, 15.0, 15.0);
                                                    let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                                    let hover_bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                                    let bg = mix(base, hover_bg, self.hover);
                                                    let icon = mix((SLATE_600), (SLATE_200), self.dark_mode);
                                                    sdf.fill(bg);
                                                    if self.playing > 0.5 {
                                                        sdf.rect(10.0, 9.0, 3.0, 11.0);
                                                        sdf.fill(icon);
                                                        sdf.rect(17.0, 9.0, 3.0, 11.0);
                                                        sdf.fill(icon);
                                                    } else {
                                                        sdf.move_to(11.0, 9.0);
                                                        sdf.line_to(20.0, 15.0);
                                                        sdf.line_to(11.0, 21.0);
                                                        sdf.close_path();
                                                        sdf.fill(icon);
                                                    }
                                                    return sdf.result;
                                                }
                                            }
                                        }
                                    }
                                }

                                voice_picker_empty = <Label> {
                                    width: Fill, height: Fit
                                    margin: {left: 4, right: 4, top: 16}
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: { font_size: 12.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                        }
                                    }
                                    text: "No voices found with current filters."
                                }
                            }

                            // Hidden voice selector (for backward compatibility)
                            voice_section = <View> {
                                width: 0, height: 0
                                visible: false
                            voice_selector = <VoiceSelector> {
                                    width: 0, height: 0
                                }
                            }
                        }

                        // History panel content
                        history_panel = <View> {
                            width: Fill, height: Fill
                            flow: Down
                            padding: {left: 16, right: 16, top: 16, bottom: 16}
                            spacing: 10
                            visible: false

                            history_header = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                align: {y: 0.5}
                                spacing: 8

                                history_count = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                        }
                                    }
                                    text: "0 条记录"
                                }

                                <View> { width: Fill, height: 1 }

                                clear_history_btn = <Button> {
                                    width: Fit, height: 28
                                    padding: {left: 10, right: 10}
                                    text: "清空"

                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        instance hover: 0.0
                                        instance border_radius: 6.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                            let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                            let hover_bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                            sdf.fill(mix(bg, hover_bg, self.hover));
                                            return sdf.result;
                                        }
                                    }

                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                        }
                                    }
                                }
                            }

                            history_list = <PortalList> {
                                width: Fill, height: Fill
                                flow: Down
                                spacing: 8

                                HistoryCard = <RoundedView> {
                                    width: Fill, height: Fit
                                    padding: {left: 12, right: 12, top: 10, bottom: 10}
                                    flow: Down
                                    spacing: 8

                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        instance hover: 0.0
                                        instance border_radius: 10.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                            let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                            let hover_bg = mix(vec4(0.98, 0.98, 0.99, 1.0), (SLATE_700), self.dark_mode);
                                            sdf.fill(mix(bg, hover_bg, self.hover));
                                            let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                                            sdf.stroke(border, 1.0);
                                            return sdf.result;
                                        }
                                    }

                                    top_row = <View> {
                                        width: Fill, height: Fit
                                        flow: Right
                                        align: {y: 0.5}
                                        spacing: 8

                                        left_info = <View> {
                                            width: Fill, height: Fit
                                            flow: Down
                                            spacing: 2

                                            voice_name = <Label> {
                                                width: Fit, height: Fit
                                                draw_text: {
                                                    instance dark_mode: 0.0
                                                    text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                                    fn get_color(self) -> vec4 {
                                                        return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                    }
                                                }
                                                text: "豆包 (Doubao)"
                                            }

                                            created_time = <Label> {
                                                width: Fit, height: Fit
                                                draw_text: {
                                                    instance dark_mode: 0.0
                                                    text_style: { font_size: 10.0 }
                                                    fn get_color(self) -> vec4 {
                                                        return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                    }
                                                }
                                                text: "刚刚"
                                            }
                                        }
                                    }

                                    text_preview = <Label> {
                                        width: Fill, height: Fit
                                        draw_text: {
                                            instance dark_mode: 0.0
                                            text_style: { font_size: 11.0 }
                                            fn get_color(self) -> vec4 {
                                                return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                            }
                                        }
                                        text: "文本片段..."
                                    }

                                    meta_row = <View> {
                                        width: Fill, height: Fit
                                        flow: Right
                                        align: {y: 0.5}
                                        spacing: 10

                                        model_name = <Label> {
                                            width: Fill, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: { font_size: 10.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "PrimeSpeech (GPT-SoVITS v2)"
                                        }

                                        duration = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: { font_size: 10.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "00:06"
                                        }
                                    }

                                    actions_row = <View> {
                                        width: Fill, height: Fit
                                        flow: Right
                                        spacing: 6

                                        play_btn = <Button> {
                                            width: Fit, height: 28
                                            padding: {left: 10, right: 10}
                                            text: "播放"
                                            draw_bg: {
                                                instance dark_mode: 0.0
                                                instance hover: 0.0
                                                instance border_radius: 6.0
                                                fn pixel(self) -> vec4 {
                                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                    let bg = mix(vec4(0.39, 0.40, 0.95, 0.10), vec4(0.39, 0.40, 0.95, 0.18), self.hover);
                                                    sdf.fill(bg);
                                                    sdf.stroke((MOXIN_PRIMARY), 1.0);
                                                    return sdf.result;
                                                }
                                            }
                                            draw_text: {
                                                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return (MOXIN_PRIMARY);
                                                }
                                            }
                                        }

                                        use_btn = <Button> {
                                            width: Fit, height: 28
                                            padding: {left: 10, right: 10}
                                            text: "复用"
                                            draw_bg: {
                                                instance dark_mode: 0.0
                                                instance hover: 0.0
                                                instance border_radius: 6.0
                                                fn pixel(self) -> vec4 {
                                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                    let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                                    let hover_bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                                    sdf.fill(mix(bg, hover_bg, self.hover));
                                                    return sdf.result;
                                                }
                                            }
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                }
                                            }
                                        }

                                        download_btn = <Button> {
                                            width: Fit, height: 28
                                            padding: {left: 10, right: 10}
                                            text: "下载"
                                            draw_bg: {
                                                instance dark_mode: 0.0
                                                instance hover: 0.0
                                                instance border_radius: 6.0
                                                fn pixel(self) -> vec4 {
                                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                    let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                                    let hover_bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                                    sdf.fill(mix(bg, hover_bg, self.hover));
                                                    return sdf.result;
                                                }
                                            }
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                }
                                            }
                                        }

                                        share_btn = <Button> {
                                            width: Fit, height: 28
                                            padding: {left: 10, right: 10}
                                            text: "分享"
                                            draw_bg: {
                                                instance dark_mode: 0.0
                                                instance hover: 0.0
                                                instance border_radius: 6.0
                                                fn pixel(self) -> vec4 {
                                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                    let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                                    let hover_bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                                    sdf.fill(mix(bg, hover_bg, self.hover));
                                                    return sdf.result;
                                                }
                                            }
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                }
                                            }
                                        }

                                        <View> { width: Fill, height: 1 }

                                        delete_btn = <Button> {
                                            width: Fit, height: 28
                                            padding: {left: 10, right: 10}
                                            text: "删除"
                                            draw_bg: {
                                                instance dark_mode: 0.0
                                                instance hover: 0.0
                                                instance border_radius: 6.0
                                                fn pixel(self) -> vec4 {
                                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                    let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                                    let hover_bg = mix(vec4(0.98, 0.85, 0.85, 1.0), vec4(0.7, 0.3, 0.3, 1.0), self.dark_mode);
                                                    sdf.fill(mix(bg, hover_bg, self.hover));
                                                    return sdf.result;
                                                }
                                            }
                                            draw_text: {
                                                instance hover: 0.0
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    let normal = mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                                    let hover_color = mix(vec4(0.8, 0.2, 0.2, 1.0), vec4(1.0, 0.4, 0.4, 1.0), self.dark_mode);
                                                    return mix(normal, hover_color, self.hover);
                                                }
                                            }
                                        }
                                    }
                                }
                            }

                            history_empty = <View> {
                                width: Fill, height: Fill
                                align: {x: 0.5, y: 0.5}
                                flow: Down
                                spacing: 8

                                history_empty_icon = <Label> {
                                    width: Fit, height: Fit
                                    align: {x: 0.5, y: 0.5}
                                    draw_text: {
                                        text_style: { font_size: 32.0 }
                                        fn get_color(self) -> vec4 { return vec4(0.6, 0.6, 0.65, 1.0); }
                                    }
                                    text: "📜"
                                }

                                history_empty_text = <Label> {
                                    width: Fit, height: Fit
                                    align: {x: 0.5, y: 0.5}
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: { font_size: 13.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                        }
                                    }
                                    text: "No generation history yet"
                                }
                            }
                        }
                    }
                    } // End cards_container
                    } // End tts_page

                    // ============ Voice Library Page ============
                    library_page = <View> {
                        width: Fill, height: Fill
                        flow: Down
                        spacing: 0
                        visible: false  // Hidden by default

                        // Page header
                        library_header = <View> {
                            width: Fill, height: Fit
                            flow: Right
                            align: {y: 0.0}
                            spacing: 16

                            title_and_tags = <View> {
                                width: Fit, height: Fit
                                flow: Down
                                spacing: 10

                                library_title = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 24.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((MOXIN_TEXT_PRIMARY), (MOXIN_TEXT_PRIMARY_DARK), self.dark_mode);
                                        }
                                    }
                                    text: "音色库"
                                }

                                // Single-line category tags under title
                                category_filter = <View> {
                                    width: Fit, height: Fit
                                    flow: Right
                                    spacing: 8
                                    align: {y: 0.5}
                                    padding: {left: 8, right: 8, top: 8, bottom: 8}
                                    show_bg: true
                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        instance border_radius: 8.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                            let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                            sdf.fill(bg);
                                            return sdf.result;
                                        }
                                    }

                                    row_gender = <View> {
                                        width: Fit, height: Fit
                                        flow: Right
                                        align: {y: 0.5}
                                        spacing: 6

                                        row_label = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                text_style: { font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return (TEXT_TERTIARY);
                                                }
                                            }
                                            text: "性别年龄"
                                        }

                                        filter_male_btn = <VoiceFilterChip> { text: "男声" }
                                        filter_female_btn = <VoiceFilterChip> { text: "女声" }
                                        age_adult_btn = <VoiceFilterChip> { text: "成年" }
                                        age_youth_btn = <VoiceFilterChip> { text: "青年" }
                                    }

                                    row_style = <View> {
                                        width: Fit, height: Fit
                                        flow: Right
                                        align: {y: 0.5}
                                        spacing: 6

                                        row_label = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                text_style: { font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return (TEXT_TERTIARY);
                                                }
                                            }
                                            text: "风格"
                                        }

                                        style_sweet_btn = <VoiceFilterChip> { text: "甜美" }
                                        style_magnetic_btn = <VoiceFilterChip> { text: "磁性" }
                                    }

                                    row_trait = <View> {
                                        width: Fit, height: Fit
                                        flow: Right
                                        align: {y: 0.5}
                                        spacing: 6

                                        row_label = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                text_style: { font_size: 11.0 }
                                                fn get_color(self) -> vec4 {
                                                    return (TEXT_TERTIARY);
                                                }
                                            }
                                            text: "声音特质"
                                        }

                                        trait_prof_btn = <VoiceFilterChip> { text: "专业播音" }
                                        trait_character_btn = <VoiceFilterChip> { text: "特色人物" }
                                    }
                                }
                            }

                            // Language filter selector (All/Chinese/English)
                            language_filter = <View> {
                                width: Fit, height: Fit
                                flow: Right
                                spacing: 0
                                padding: {left: 4, right: 4, top: 4, bottom: 4}
                                show_bg: true
                                draw_bg: {
                                    instance dark_mode: 0.0
                                    instance border_radius: 8.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                        sdf.fill(bg);
                                        return sdf.result;
                                    }
                                }

                                lang_all_btn = <Button> {
                                    width: Fit, height: 28
                                    padding: {left: 12, right: 12}
                                    text: "全部语言"

                                    draw_bg: {
                                        instance hover: 0.0
                                        instance active: 1.0
                                        instance border_radius: 6.0
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
                                        text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                                        fn get_color(self) -> vec4 {
                                            let normal = vec4(0.6, 0.6, 0.65, 1.0);
                                            let active = (MOXIN_PRIMARY);
                                            return mix(normal, active, self.active);
                                        }
                                    }
                                }

                                lang_zh_btn = <Button> {
                                    width: Fit, height: 28
                                    padding: {left: 12, right: 12}
                                    text: "中文"

                                    draw_bg: {
                                        instance hover: 0.0
                                        instance active: 0.0
                                        instance border_radius: 6.0
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
                                        text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                                        fn get_color(self) -> vec4 {
                                            let normal = vec4(0.6, 0.6, 0.65, 1.0);
                                            let active = (MOXIN_PRIMARY);
                                            return mix(normal, active, self.active);
                                        }
                                    }
                                }

                                lang_en_btn = <Button> {
                                    width: Fit, height: 28
                                    padding: {left: 12, right: 12}
                                    text: "英文"

                                    draw_bg: {
                                        instance hover: 0.0
                                        instance active: 0.0
                                        instance border_radius: 6.0
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
                                        text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                                        fn get_color(self) -> vec4 {
                                            let normal = vec4(0.6, 0.6, 0.65, 1.0);
                                            let active = (MOXIN_PRIMARY);
                                            return mix(normal, active, self.active);
                                        }
                                    }
                                }
                            }

                            <View> { width: Fill, height: 1 }  // Spacer

                            // Search box
                            search_input = <TextInput> {
                                width: 200, height: 40
                                padding: {left: 12, right: 12}
                                empty_text: "搜索音色..."
                                text: ""

                                draw_bg: {
                                    instance dark_mode: 0.0
                                    instance focus: 0.0
                                    instance border_radius: 8.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                        sdf.fill(bg);
                                        let border_normal = mix((MOXIN_BORDER_LIGHT), (SLATE_700), self.dark_mode);
                                        let border_focused = (MOXIN_PRIMARY);
                                        let border = mix(border_normal, border_focused, self.focus);
                                        sdf.stroke(border, mix(1.0, 2.0, self.focus));
                                        return sdf.result;
                                    }
                                }

                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: { font_size: 14.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((MOXIN_TEXT_PRIMARY), (MOXIN_TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }

                                draw_cursor: {
                                    uniform border_radius: 0.5
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, self.border_radius);
                                        sdf.fill((MOXIN_PRIMARY));
                                        return sdf.result;
                                    }
                                }

                                draw_selection: {
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 1.0);
                                        sdf.fill(vec4(0.39, 0.40, 0.95, 0.25));
                                        return sdf.result;
                                    }
                                }
                            }

                            // Refresh button
                            refresh_btn = <Button> {
                                width: Fit, height: 40
                                padding: {left: 16, right: 16}
                                text: "刷新"

                                draw_bg: {
                                    instance hover: 0.0
                                    instance dark_mode: 0.0
                                    instance border_radius: 8.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                        let hover_color = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                        sdf.fill(mix(base, hover_color, self.hover));
                                        return sdf.result;
                                    }
                                }

                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                            }
                        }

                        // Empty state (shown when no voices)
                        empty_state = <View> {
                            width: Fill, height: 400
                            align: {x: 0.5, y: 0.5}
                            flow: Down
                            spacing: 16
                            visible: false

                            empty_icon = <Label> {
                                width: Fit, height: Fit
                                draw_text: {
                                    text_style: { font_size: 64.0 }
                                    fn get_color(self) -> vec4 {
                                        return vec4(0.6, 0.6, 0.65, 1.0);
                                    }
                                }
                                text: "🎤"
                            }

                            empty_text = <Label> {
                                width: Fit, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: { font_size: 15.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((MOXIN_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "暂无音色，请先创建自定义音色"
                            }
                        }

                        // Voice list using PortalList (direct child, no ScrollYView wrapper)
                        voice_list = <PortalList> {
                            width: Fill, height: Fill
                            flow: Down
                            spacing: 12

                                    VoiceCard = <RoundedView> {
                                        width: Fill, height: Fit
                                        padding: {left: 20, right: 20, top: 16, bottom: 16}
                                        flow: Right
                                        spacing: 16
                                        align: {y: 0.5}

                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            instance hover: 0.0
                                            instance border_radius: 12.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                                let hover_bg = mix(vec4(0.98, 0.98, 0.99, 1.0), (SLATE_700), self.dark_mode);
                                                sdf.fill(mix(bg, hover_bg, self.hover));
                                                let border = mix((MOXIN_BORDER_LIGHT), (SLATE_700), self.dark_mode);
                                                sdf.stroke(border, 1.0);
                                                return sdf.result;
                                            }
                                        }

                                        avatar = <View> {
                                            width: 48, height: 48
                                            show_bg: true
                                            draw_bg: {
                                                instance dark_mode: 0.0
                                                fn pixel(self) -> vec4 {
                                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                    sdf.circle(24.0, 24.0, 24.0);
                                                    let bg = mix((MOXIN_PRIMARY), (PRIMARY_400), self.dark_mode);
                                                    sdf.fill(bg);
                                                    return sdf.result;
                                                }
                                            }
                                            align: {x: 0.5, y: 0.5}

                                            avatar_initial = <Label> {
                                                width: Fit, height: Fit
                                                draw_text: {
                                                    text_style: <FONT_BOLD>{ font_size: 18.0 }
                                                    fn get_color(self) -> vec4 {
                                                        return (WHITE);
                                                    }
                                                }
                                                text: "D"
                                            }
                                        }

                                        voice_info = <View> {
                                            width: Fill, height: Fit
                                            flow: Down
                                            spacing: 4

                                            voice_name = <Label> {
                                                width: Fit, height: Fit
                                                draw_text: {
                                                    instance dark_mode: 0.0
                                                    text_style: <FONT_SEMIBOLD>{ font_size: 15.0 }
                                                    fn get_color(self) -> vec4 {
                                                        return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                    }
                                                }
                                                text: "Voice Name"
                                            }

                                            voice_meta = <View> {
                                                width: Fit, height: Fit
                                                flow: Right
                                                spacing: 12

                                                voice_language = <Label> {
                                                    width: Fit, height: Fit
                                                    draw_text: {
                                                        instance dark_mode: 0.0
                                                        text_style: { font_size: 12.0 }
                                                        fn get_color(self) -> vec4 {
                                                            return mix((MOXIN_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                        }
                                                    }
                                                    text: "zh"
                                                }

                                                voice_type = <Label> {
                                                    width: Fit, height: Fit
                                                    draw_text: {
                                                        instance dark_mode: 0.0
                                                        text_style: { font_size: 12.0 }
                                                        fn get_color(self) -> vec4 {
                                                            return mix((MOXIN_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                        }
                                                    }
                                                    text: "Built-in"
                                                }
                                            }
                                        }

                                        actions = <View> {
                                            width: Fit, height: Fit
                                            flow: Right
                                            spacing: 8

                                            preview_btn = <Button> {
                                                width: Fit, height: 32
                                                padding: {left: 12, right: 12}
                                                text: "Preview"

                                                draw_bg: {
                                                    instance hover: 0.0
                                                    instance dark_mode: 0.0
                                                    instance border_radius: 6.0
                                                    fn pixel(self) -> vec4 {
                                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                        let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                                        let hover_color = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                                        sdf.fill(mix(base, hover_color, self.hover));
                                                        return sdf.result;
                                                    }
                                                }

                                                draw_text: {
                                                    instance dark_mode: 0.0
                                                    text_style: { font_size: 12.0 }
                                                    fn get_color(self) -> vec4 {
                                                        return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                    }
                                                }
                                            }

                                            delete_btn = <Button> {
                                                width: Fit, height: 32
                                                padding: {left: 12, right: 12}
                                                text: "Delete"

                                                draw_bg: {
                                                    instance hover: 0.0
                                                    instance dark_mode: 0.0
                                                    instance border_radius: 6.0
                                                    fn pixel(self) -> vec4 {
                                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                        let base = mix(vec4(0.95, 0.95, 0.96, 1.0), (SLATE_700), self.dark_mode);
                                                        let hover_color = mix(vec4(0.98, 0.85, 0.85, 1.0), vec4(0.7, 0.3, 0.3, 1.0), self.dark_mode);
                                                        sdf.fill(mix(base, hover_color, self.hover));
                                                        return sdf.result;
                                                    }
                                                }

                                                draw_text: {
                                                    instance hover: 0.0
                                                    instance dark_mode: 0.0
                                                    text_style: { font_size: 12.0 }
                                                    fn get_color(self) -> vec4 {
                                                        let normal = mix((MOXIN_TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                                        let hover_color = mix(vec4(0.8, 0.2, 0.2, 1.0), vec4(1.0, 0.4, 0.4, 1.0), self.dark_mode);
                                                        return mix(normal, hover_color, self.hover);
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                    } // End library_page

                    // ============ Voice Clone Page ============
                    clone_page = <View> {
                        width: Fill, height: Fill
                        flow: Down
                        spacing: 20
                        visible: false  // Hidden by default

                        // Page header
                        clone_header = <View> {
                            width: Fill, height: Fit
                            flow: Right
                            align: {y: 0.5}
                            spacing: 20

                            clone_title_section = <View> {
                                width: Fit, height: Fit
                                flow: Right
                                spacing: 20
                                align: {y: 0.5}

                                clone_title = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 24.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((MOXIN_TEXT_PRIMARY), (MOXIN_TEXT_PRIMARY_DARK), self.dark_mode);
                                        }
                                    }
                                    text: "音色克隆"
                                }

                                // Mode selector (Quick/Advanced)
                                mode_selector = <View> {
                                    width: Fit, height: Fit
                                    flow: Right
                                    spacing: 0
                                    padding: {left: 4, right: 4, top: 4, bottom: 4}
                                    show_bg: true
                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        instance border_radius: 8.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                            let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                            sdf.fill(bg);
                                            return sdf.result;
                                        }
                                    }

                                    quick_mode_btn = <Button> {
                                        width: Fit, height: 32
                                        padding: {left: 16, right: 16}
                                        text: "快速模式"

                                        draw_bg: {
                                            instance hover: 0.0
                                            instance active: 1.0
                                            instance border_radius: 6.0
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
                                            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                            fn get_color(self) -> vec4 {
                                                let normal = vec4(0.6, 0.6, 0.65, 1.0);
                                                let active = (MOXIN_PRIMARY);
                                                return mix(normal, active, self.active);
                                            }
                                        }
                                    }

                                    advanced_mode_btn = <Button> {
                                        width: Fit, height: 32
                                        padding: {left: 16, right: 16}
                                        text: "高级模式"

                                        draw_bg: {
                                            instance hover: 0.0
                                            instance active: 0.0
                                            instance border_radius: 6.0
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
                                            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                            fn get_color(self) -> vec4 {
                                                let normal = vec4(0.6, 0.6, 0.65, 1.0);
                                                let active = (MOXIN_PRIMARY);
                                                return mix(normal, active, self.active);
                                            }
                                        }
                                    }
                                }
                            }

                            <View> { width: Fill, height: 1 }  // Spacer

                            // Create task button
                            create_task_btn = <Button> {
                                width: Fit, height: 44
                                padding: {left: 24, right: 24}
                                text: "➕ 创建任务"

                                draw_bg: {
                                    instance hover: 0.0
                                    instance border_radius: 10.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        let base = (MOXIN_PRIMARY);
                                        let hover_color = (MOXIN_PRIMARY_LIGHT);
                                        sdf.fill(mix(base, hover_color, self.hover));
                                        return sdf.result;
                                    }
                                }

                                draw_text: {
                                    text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                    fn get_color(self) -> vec4 {
                                        return (WHITE);
                                    }
                                }
                            }
                        }

                        // Empty state (shown when no tasks)
                        clone_empty_state = <View> {
                                    width: Fill, height: 400
                                    align: {x: 0.5, y: 0.5}
                                    flow: Down
                                    spacing: 16
                                    visible: false  // Will be shown when no tasks

                                    clone_empty_icon = <Label> {
                                        width: Fit, height: Fit
                                        draw_text: {
                                            text_style: { font_size: 64.0 }
                                            fn get_color(self) -> vec4 {
                                                return vec4(0.6, 0.6, 0.65, 1.0);
                                            }
                                        }
                                        text: "📋"
                                    }

                                    clone_empty_text = <Label> {
                                        width: Fit, height: Fit
                                        draw_text: {
                                            instance dark_mode: 0.0
                                            text_style: { font_size: 15.0 }
                                            fn get_color(self) -> vec4 {
                                                return mix((MOXIN_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
                                            }
                                        }
                                        text: "暂无训练任务，点击「创建任务」开始"
                                    }
                                }

                                // Task list using PortalList
                                task_portal_list = <PortalList> {
                                    width: Fill, height: Fill
                                    flow: Down
                                    spacing: 12

                                    TaskCard = <RoundedView> {
                                        width: Fill, height: Fit
                                        padding: {left: 20, right: 20, top: 16, bottom: 16}
                                        flow: Down
                                        spacing: 12

                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            instance hover: 0.0
                                            instance border_radius: 12.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                                let hover_bg = mix(vec4(0.98, 0.98, 0.99, 1.0), (SLATE_700), self.dark_mode);
                                                sdf.fill(mix(bg, hover_bg, self.hover));
                                                let border = mix((MOXIN_BORDER_LIGHT), (SLATE_700), self.dark_mode);
                                                sdf.stroke(border, 1.0);
                                                return sdf.result;
                                            }
                                        }

                                        top_row = <View> {
                                            width: Fill, height: Fit
                                            flow: Right
                                            align: {y: 0.5}
                                            spacing: 8

                                            task_name = <Label> {
                                                width: Fit, height: Fit
                                                draw_text: {
                                                    instance dark_mode: 0.0
                                                    text_style: <FONT_SEMIBOLD>{ font_size: 15.0 }
                                                    fn get_color(self) -> vec4 {
                                                        return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                    }
                                                }
                                                text: "Task Name"
                                            }

                                            status_badge = <View> {
                                                width: Fit, height: Fit
                                                padding: {left: 6, right: 6, top: 2, bottom: 2}
                                                show_bg: true
                                                draw_bg: {
                                                    instance status_color: vec4(0.16, 0.65, 0.37, 1.0)
                                                    instance border_radius: 4.0
                                                    fn pixel(self) -> vec4 {
                                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                        sdf.fill(self.status_color);
                                                        return sdf.result;
                                                    }
                                                }

                                                status_label = <Label> {
                                                    width: Fit, height: Fit
                                                    draw_text: {
                                                        text_style: <FONT_SEMIBOLD>{ font_size: 10.0 }
                                                        fn get_color(self) -> vec4 {
                                                            return (WHITE);
                                                        }
                                                    }
                                                    text: "Completed"
                                                }
                                            }

                                            <View> { width: Fill, height: 1 }
                                        }

                                        progress_row = <View> {
                                            width: Fill, height: Fit
                                            flow: Down
                                            spacing: 4
                                            visible: false

                                            progress_text = <Label> {
                                                width: Fit, height: Fit
                                                draw_text: {
                                                    instance dark_mode: 0.0
                                                    text_style: { font_size: 12.0 }
                                                    fn get_color(self) -> vec4 {
                                                        return mix((MOXIN_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                    }
                                                }
                                                text: "Progress: 65%"
                                            }

                                            progress_bar = <View> {
                                                width: Fill, height: 6
                                                show_bg: true
                                                draw_bg: {
                                                    instance dark_mode: 0.0
                                                    instance progress: 0.65
                                                    instance border_radius: 3.0
                                                    fn pixel(self) -> vec4 {
                                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                        let bg = mix((SLATE_200), (SLATE_700), self.dark_mode);
                                                        sdf.fill(bg);
                                                        let progress_width = self.rect_size.x * self.progress;
                                                        sdf.box(0., 0., progress_width, self.rect_size.y, self.border_radius);
                                                        sdf.fill((MOXIN_PRIMARY));
                                                        return sdf.result;
                                                    }
                                                }
                                            }
                                        }

                                        bottom_row = <View> {
                                            width: Fill, height: Fit
                                            flow: Right
                                            align: {y: 0.5}

                                            task_meta = <View> {
                                                width: Fill, height: Fit
                                                flow: Right
                                                spacing: 16

                                                created_time = <Label> {
                                                    width: Fit, height: Fit
                                                    draw_text: {
                                                        instance dark_mode: 0.0
                                                        text_style: { font_size: 12.0 }
                                                        fn get_color(self) -> vec4 {
                                                            return mix((MOXIN_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
                                                        }
                                                    }
                                                    text: "2024-01-15 10:30"
                                                }
                                            }

                                            actions = <View> {
                                                width: Fit, height: Fit
                                                flow: Right
                                                spacing: 8

                                                view_btn = <Button> {
                                                    width: Fit, height: 32
                                                    padding: {left: 12, right: 12}
                                                    text: "查看"

                                                    draw_bg: {
                                                        instance hover: 0.0
                                                        instance dark_mode: 0.0
                                                        instance border_radius: 6.0
                                                        fn pixel(self) -> vec4 {
                                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                            let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                                            let hover_color = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                                            sdf.fill(mix(base, hover_color, self.hover));
                                                            return sdf.result;
                                                        }
                                                    }

                                                    draw_text: {
                                                        instance dark_mode: 0.0
                                                        text_style: { font_size: 12.0 }
                                                        fn get_color(self) -> vec4 {
                                                            return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                        }
                                                    }
                                                }

                                                cancel_btn = <Button> {
                                                    width: Fit, height: 32
                                                    padding: {left: 12, right: 12}
                                                    text: "取消"
                                                    visible: false

                                                    draw_bg: {
                                                        instance hover: 0.0
                                                        instance dark_mode: 0.0
                                                        instance border_radius: 6.0
                                                        fn pixel(self) -> vec4 {
                                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                            let base = mix(vec4(0.95, 0.95, 0.96, 1.0), (SLATE_700), self.dark_mode);
                                                            let hover_color = mix(vec4(0.98, 0.85, 0.85, 1.0), vec4(0.7, 0.3, 0.3, 1.0), self.dark_mode);
                                                            sdf.fill(mix(base, hover_color, self.hover));
                                                            return sdf.result;
                                                        }
                                                    }

                                                    draw_text: {
                                                        instance hover: 0.0
                                                        instance dark_mode: 0.0
                                                        text_style: { font_size: 12.0 }
                                                        fn get_color(self) -> vec4 {
                                                            let normal = mix((MOXIN_TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                                            let hover_color = mix(vec4(0.8, 0.2, 0.2, 1.0), vec4(1.0, 0.4, 0.4, 1.0), self.dark_mode);
                                                            return mix(normal, hover_color, self.hover);
                                                        }
                                                    }
                                                }

                                                delete_btn = <Button> {
                                                    width: Fit, height: 32
                                                    padding: {left: 12, right: 12}
                                                    text: "删除"
                                                    visible: false

                                                    draw_bg: {
                                                        instance hover: 0.0
                                                        instance dark_mode: 0.0
                                                        instance border_radius: 6.0
                                                        fn pixel(self) -> vec4 {
                                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                            let base = mix(vec4(0.95, 0.93, 0.93, 1.0), (SLATE_700), self.dark_mode);
                                                            let hover_color = mix(vec4(0.98, 0.80, 0.80, 1.0), vec4(0.65, 0.25, 0.25, 1.0), self.dark_mode);
                                                            sdf.fill(mix(base, hover_color, self.hover));
                                                            return sdf.result;
                                                        }
                                                    }

                                                    draw_text: {
                                                        instance hover: 0.0
                                                        instance dark_mode: 0.0
                                                        text_style: { font_size: 12.0 }
                                                        fn get_color(self) -> vec4 {
                                                            let normal = mix(vec4(0.75, 0.2, 0.2, 1.0), vec4(1.0, 0.5, 0.5, 1.0), self.dark_mode);
                                                            let hover_color = mix(vec4(0.8, 0.1, 0.1, 1.0), vec4(1.0, 0.3, 0.3, 1.0), self.dark_mode);
                                                            return mix(normal, hover_color, self.hover);
                                                        }
                                                    }
                                                }
                                            }
                                        }
                                    }
                                }
                    } // End clone_page

                    // Task Detail page
                    task_detail_page = <View> {
                        width: Fill, height: Fill
                        flow: Down
                        spacing: 20
                        padding: {left: 24, right: 24, top: 24, bottom: 24}
                        visible: false

                        // Header: back button + task name + status badge + cancel button
                        detail_header = <View> {
                            width: Fill, height: Fit
                            flow: Right
                            spacing: 16
                            align: {y: 0.5}

                            back_btn = <Button> {
                                width: Fit, height: 36
                                padding: {left: 16, right: 16}
                                text: "← 返回"

                                draw_bg: {
                                    instance hover: 0.0
                                    instance border_radius: 8.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        let bg = mix((SLATE_100), (SLATE_700), 0.0);
                                        let border = mix((SLATE_300), (SLATE_500), 0.0);
                                        sdf.fill(bg);
                                        sdf.stroke(border, 1.0);
                                        return sdf.result;
                                    }
                                }
                                draw_text: {
                                    text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                    fn get_color(self) -> vec4 {
                                        return (MOXIN_TEXT_PRIMARY);
                                    }
                                }
                            }

                            detail_task_name = <Label> {
                                width: Fit, height: Fit
                                draw_text: {
                                    text_style: <FONT_SEMIBOLD>{ font_size: 20.0 }
                                    fn get_color(self) -> vec4 {
                                        return (MOXIN_TEXT_PRIMARY);
                                    }
                                }
                                text: "任务详情"
                            }

                            detail_status_badge = <RoundedView> {
                                width: Fit, height: Fit
                                padding: {left: 12, right: 12, top: 4, bottom: 4}
                                draw_bg: {
                                    instance border_radius: 12.0
                                    instance status_color: 0.0  // 0=pending,1=running,2=done,3=failed
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        let pending = vec4(0.8, 0.8, 0.85, 1.0);
                                        let running = vec4(0.38, 0.40, 0.95, 0.15);
                                        let done = vec4(0.05, 0.65, 0.35, 0.15);
                                        let failed = vec4(0.9, 0.2, 0.2, 0.15);
                                        let c = mix(
                                            mix(pending, running, clamp(self.status_color - 0.0, 0.0, 1.0)),
                                            mix(done, failed, clamp(self.status_color - 2.0, 0.0, 1.0)),
                                            clamp(self.status_color - 1.0, 0.0, 1.0)
                                        );
                                        sdf.fill(c);
                                        return sdf.result;
                                    }
                                }

                                detail_status_label = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: {
                                        text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                                        fn get_color(self) -> vec4 {
                                            return (MOXIN_TEXT_SECONDARY);
                                        }
                                    }
                                    text: ""
                                }
                            }

                            <View> { width: Fill, height: 1 }  // Spacer

                            detail_cancel_btn = <Button> {
                                width: Fit, height: 36
                                padding: {left: 16, right: 16}
                                text: "取消任务"

                                draw_bg: {
                                    instance hover: 0.0
                                    instance border_radius: 8.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        let base = vec4(0.9, 0.2, 0.2, 1.0);
                                        let hov = vec4(1.0, 0.3, 0.3, 1.0);
                                        sdf.fill(mix(base, hov, self.hover));
                                        return sdf.result;
                                    }
                                }
                                draw_text: {
                                    text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                    fn get_color(self) -> vec4 { return (WHITE); }
                                }
                            }
                        }

                        // Task info card
                        detail_info_card = <RoundedView> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 12
                            padding: {left: 20, right: 20, top: 16, bottom: 16}
                            draw_bg: {
                                instance border_radius: 12.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                    sdf.fill((WHITE));
                                    sdf.stroke(vec4(0.0, 0.0, 0.0, 0.05), 1.0);
                                    return sdf.result;
                                }
                            }

                            detail_info_title = <Label> {
                                width: Fill, height: Fit
                                draw_text: {
                                    text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                    fn get_color(self) -> vec4 { return (MOXIN_TEXT_PRIMARY); }
                                }
                                text: "任务信息"
                            }

                            detail_times_row = <View> {
                                flow: Right
                                spacing: 40
                                width: Fill, height: Fit

                                detail_created_section = <View> {
                                    flow: Down, spacing: 4, width: Fit, height: Fit
                                    detail_created_title = <Label> {
                                        width: Fit, height: Fit
                                        draw_text: {
                                            text_style: { font_size: 11.0 }
                                            fn get_color(self) -> vec4 { return (MOXIN_TEXT_MUTED); }
                                        }
                                        text: "创建时间"
                                    }
                                    detail_created_at = <Label> {
                                        width: Fit, height: Fit
                                        draw_text: {
                                            text_style: { font_size: 13.0 }
                                            fn get_color(self) -> vec4 { return (MOXIN_TEXT_PRIMARY); }
                                        }
                                        text: "-"
                                    }
                                }

                                detail_started_section = <View> {
                                    flow: Down, spacing: 4, width: Fit, height: Fit
                                    detail_started_title = <Label> {
                                        width: Fit, height: Fit
                                        draw_text: {
                                            text_style: { font_size: 11.0 }
                                            fn get_color(self) -> vec4 { return (MOXIN_TEXT_MUTED); }
                                        }
                                        text: "开始时间"
                                    }
                                    detail_started_at = <Label> {
                                        width: Fit, height: Fit
                                        draw_text: {
                                            text_style: { font_size: 13.0 }
                                            fn get_color(self) -> vec4 { return (MOXIN_TEXT_PRIMARY); }
                                        }
                                        text: "-"
                                    }
                                }

                                detail_completed_section = <View> {
                                    flow: Down, spacing: 4, width: Fit, height: Fit
                                    detail_completed_title = <Label> {
                                        width: Fit, height: Fit
                                        draw_text: {
                                            text_style: { font_size: 11.0 }
                                            fn get_color(self) -> vec4 { return (MOXIN_TEXT_MUTED); }
                                        }
                                        text: "完成时间"
                                    }
                                    detail_completed_at = <Label> {
                                        width: Fit, height: Fit
                                        draw_text: {
                                            text_style: { font_size: 13.0 }
                                            fn get_color(self) -> vec4 { return (MOXIN_TEXT_PRIMARY); }
                                        }
                                        text: "-"
                                    }
                                }
                            }
                        }

                        // Progress card with 8-stage list
                        detail_progress_card = <RoundedView> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 12
                            padding: {left: 20, right: 20, top: 16, bottom: 16}
                            draw_bg: {
                                instance border_radius: 12.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                    sdf.fill((WHITE));
                                    sdf.stroke(vec4(0.0, 0.0, 0.0, 0.05), 1.0);
                                    return sdf.result;
                                }
                            }

                            detail_progress_title = <Label> {
                                width: Fill, height: Fit
                                draw_text: {
                                    text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                    fn get_color(self) -> vec4 { return (MOXIN_TEXT_PRIMARY); }
                                }
                                text: "训练进度"
                            }

                            // Overall progress bar row
                            detail_overall_row = <View> {
                                flow: Right, spacing: 12, align: {y: 0.5}
                                width: Fill, height: Fit

                                detail_progress_bar = <View> {
                                    width: Fill, height: 8
                                    show_bg: true
                                    draw_bg: {
                                        instance progress: 0.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 4.0);
                                            sdf.fill(vec4(0.9, 0.9, 0.95, 1.0));
                                            let pw = self.rect_size.x * self.progress;
                                            sdf.box(0., 0., pw, self.rect_size.y, 4.0);
                                            sdf.fill((MOXIN_PRIMARY));
                                            return sdf.result;
                                        }
                                    }
                                }

                                detail_progress_text = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: {
                                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                        fn get_color(self) -> vec4 { return (MOXIN_PRIMARY); }
                                    }
                                    text: "0%"
                                }
                            }

                            // Stage rows (8 stages)
                            stage_1_row = <View> {
                                flow: Right, spacing: 12, align: {y: 0.5}
                                width: Fill, height: Fit, padding: {top: 4, bottom: 4}
                                stage_1_dot = <RoundedView> {
                                    width: 16, height: 16
                                    draw_bg: {
                                        instance border_radius: 8.0
                                        instance dot_color: 0.0  // 0=pending,1=running,2=done
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.circle(8.0, 8.0, 7.0);
                                            let pending = vec4(0.8, 0.8, 0.85, 1.0);
                                            let running = vec4(0.38, 0.40, 0.95, 1.0);
                                            let done = vec4(0.1, 0.75, 0.4, 1.0);
                                            let c = mix(mix(pending, running, clamp(self.dot_color, 0.0, 1.0)), done, clamp(self.dot_color - 1.0, 0.0, 1.0));
                                            sdf.fill(c);
                                            return sdf.result;
                                        }
                                    }
                                }
                                stage_1_name = <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOXIN_TEXT_PRIMARY); } }
                                    text: "音频切片"
                                }
                                stage_1_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOXIN_PRIMARY); } }
                                    text: ""
                                }
                            }

                            stage_2_row = <View> {
                                flow: Right, spacing: 12, align: {y: 0.5}
                                width: Fill, height: Fit, padding: {top: 4, bottom: 4}
                                stage_2_dot = <RoundedView> {
                                    width: 16, height: 16
                                    draw_bg: {
                                        instance border_radius: 8.0
                                        instance dot_color: 0.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.circle(8.0, 8.0, 7.0);
                                            let pending = vec4(0.8, 0.8, 0.85, 1.0);
                                            let running = vec4(0.38, 0.40, 0.95, 1.0);
                                            let done = vec4(0.1, 0.75, 0.4, 1.0);
                                            let c = mix(mix(pending, running, clamp(self.dot_color, 0.0, 1.0)), done, clamp(self.dot_color - 1.0, 0.0, 1.0));
                                            sdf.fill(c);
                                            return sdf.result;
                                        }
                                    }
                                }
                                stage_2_name = <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOXIN_TEXT_PRIMARY); } }
                                    text: "语音识别"
                                }
                                stage_2_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOXIN_PRIMARY); } }
                                    text: ""
                                }
                            }

                            stage_3_row = <View> {
                                flow: Right, spacing: 12, align: {y: 0.5}
                                width: Fill, height: Fit, padding: {top: 4, bottom: 4}
                                stage_3_dot = <RoundedView> {
                                    width: 16, height: 16
                                    draw_bg: {
                                        instance border_radius: 8.0
                                        instance dot_color: 0.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.circle(8.0, 8.0, 7.0);
                                            let pending = vec4(0.8, 0.8, 0.85, 1.0);
                                            let running = vec4(0.38, 0.40, 0.95, 1.0);
                                            let done = vec4(0.1, 0.75, 0.4, 1.0);
                                            let c = mix(mix(pending, running, clamp(self.dot_color, 0.0, 1.0)), done, clamp(self.dot_color - 1.0, 0.0, 1.0));
                                            sdf.fill(c);
                                            return sdf.result;
                                        }
                                    }
                                }
                                stage_3_name = <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOXIN_TEXT_PRIMARY); } }
                                    text: "文本特征"
                                }
                                stage_3_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOXIN_PRIMARY); } }
                                    text: ""
                                }
                            }

                            stage_4_row = <View> {
                                flow: Right, spacing: 12, align: {y: 0.5}
                                width: Fill, height: Fit, padding: {top: 4, bottom: 4}
                                stage_4_dot = <RoundedView> {
                                    width: 16, height: 16
                                    draw_bg: {
                                        instance border_radius: 8.0
                                        instance dot_color: 0.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.circle(8.0, 8.0, 7.0);
                                            let pending = vec4(0.8, 0.8, 0.85, 1.0);
                                            let running = vec4(0.38, 0.40, 0.95, 1.0);
                                            let done = vec4(0.1, 0.75, 0.4, 1.0);
                                            let c = mix(mix(pending, running, clamp(self.dot_color, 0.0, 1.0)), done, clamp(self.dot_color - 1.0, 0.0, 1.0));
                                            sdf.fill(c);
                                            return sdf.result;
                                        }
                                    }
                                }
                                stage_4_name = <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOXIN_TEXT_PRIMARY); } }
                                    text: "HuBERT特征"
                                }
                                stage_4_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOXIN_PRIMARY); } }
                                    text: ""
                                }
                            }

                            stage_5_row = <View> {
                                flow: Right, spacing: 12, align: {y: 0.5}
                                width: Fill, height: Fit, padding: {top: 4, bottom: 4}
                                stage_5_dot = <RoundedView> {
                                    width: 16, height: 16
                                    draw_bg: {
                                        instance border_radius: 8.0
                                        instance dot_color: 0.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.circle(8.0, 8.0, 7.0);
                                            let pending = vec4(0.8, 0.8, 0.85, 1.0);
                                            let running = vec4(0.38, 0.40, 0.95, 1.0);
                                            let done = vec4(0.1, 0.75, 0.4, 1.0);
                                            let c = mix(mix(pending, running, clamp(self.dot_color, 0.0, 1.0)), done, clamp(self.dot_color - 1.0, 0.0, 1.0));
                                            sdf.fill(c);
                                            return sdf.result;
                                        }
                                    }
                                }
                                stage_5_name = <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOXIN_TEXT_PRIMARY); } }
                                    text: "语义Token"
                                }
                                stage_5_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOXIN_PRIMARY); } }
                                    text: ""
                                }
                            }

                            stage_6_row = <View> {
                                flow: Right, spacing: 12, align: {y: 0.5}
                                width: Fill, height: Fit, padding: {top: 4, bottom: 4}
                                stage_6_dot = <RoundedView> {
                                    width: 16, height: 16
                                    draw_bg: {
                                        instance border_radius: 8.0
                                        instance dot_color: 0.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.circle(8.0, 8.0, 7.0);
                                            let pending = vec4(0.8, 0.8, 0.85, 1.0);
                                            let running = vec4(0.38, 0.40, 0.95, 1.0);
                                            let done = vec4(0.1, 0.75, 0.4, 1.0);
                                            let c = mix(mix(pending, running, clamp(self.dot_color, 0.0, 1.0)), done, clamp(self.dot_color - 1.0, 0.0, 1.0));
                                            sdf.fill(c);
                                            return sdf.result;
                                        }
                                    }
                                }
                                stage_6_name = <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOXIN_TEXT_PRIMARY); } }
                                    text: "SoVITS训练"
                                }
                                stage_6_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOXIN_PRIMARY); } }
                                    text: ""
                                }
                            }

                            stage_7_row = <View> {
                                flow: Right, spacing: 12, align: {y: 0.5}
                                width: Fill, height: Fit, padding: {top: 4, bottom: 4}
                                stage_7_dot = <RoundedView> {
                                    width: 16, height: 16
                                    draw_bg: {
                                        instance border_radius: 8.0
                                        instance dot_color: 0.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.circle(8.0, 8.0, 7.0);
                                            let pending = vec4(0.8, 0.8, 0.85, 1.0);
                                            let running = vec4(0.38, 0.40, 0.95, 1.0);
                                            let done = vec4(0.1, 0.75, 0.4, 1.0);
                                            let c = mix(mix(pending, running, clamp(self.dot_color, 0.0, 1.0)), done, clamp(self.dot_color - 1.0, 0.0, 1.0));
                                            sdf.fill(c);
                                            return sdf.result;
                                        }
                                    }
                                }
                                stage_7_name = <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOXIN_TEXT_PRIMARY); } }
                                    text: "GPT训练"
                                }
                                stage_7_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOXIN_PRIMARY); } }
                                    text: ""
                                }
                            }

                            stage_8_row = <View> {
                                flow: Right, spacing: 12, align: {y: 0.5}
                                width: Fill, height: Fit, padding: {top: 4, bottom: 4}
                                stage_8_dot = <RoundedView> {
                                    width: 16, height: 16
                                    draw_bg: {
                                        instance border_radius: 8.0
                                        instance dot_color: 0.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.circle(8.0, 8.0, 7.0);
                                            let pending = vec4(0.8, 0.8, 0.85, 1.0);
                                            let running = vec4(0.38, 0.40, 0.95, 1.0);
                                            let done = vec4(0.1, 0.75, 0.4, 1.0);
                                            let c = mix(mix(pending, running, clamp(self.dot_color, 0.0, 1.0)), done, clamp(self.dot_color - 1.0, 0.0, 1.0));
                                            sdf.fill(c);
                                            return sdf.result;
                                        }
                                    }
                                }
                                stage_8_name = <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOXIN_TEXT_PRIMARY); } }
                                    text: "推理测试"
                                }
                                stage_8_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOXIN_PRIMARY); } }
                                    text: ""
                                }
                            }

                            detail_message_label = <Label> {
                                width: Fill, height: Fit
                                draw_text: {
                                    text_style: { font_size: 12.0 }
                                    fn get_color(self) -> vec4 { return (MOXIN_TEXT_SECONDARY); }
                                }
                                text: ""
                            }
                        }
                    } // End task_detail_page

                } // End content_area
            } // End left_column

            // Splitter handle for resizing - hidden in Moxin.tts style
            splitter = <Splitter> {
                visible: false
                width: 0
            }

            // Right Panel: System Log - hidden in Moxin.tts style
            log_section = <View> {
                width: 0, height: Fill
                visible: false
                flow: Right
                align: {y: 0.0}

                // Toggle button column
                toggle_column = <View> {
                    width: Fit, height: Fill
                    show_bg: true
                    draw_bg: {
                        instance dark_mode: 0.0
                        fn pixel(self) -> vec4 {
                            return mix((MOXIN_BG_PRIMARY), (SLATE_800), self.dark_mode);
                        }
                    }
                    align: {x: 0.5, y: 0.0}
                    padding: {left: 4, right: 4, top: 12}

                    toggle_log_btn = <Button> {
                        width: Fit, height: Fit
                        padding: {left: 6, right: 6, top: 8, bottom: 8}
                        text: ">"

                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_BOLD>{ font_size: 10.0 }
                            fn get_color(self) -> vec4 {
                                return mix((MOXIN_TEXT_MUTED), (SLATE_400), self.dark_mode);
                            }
                        }
                        draw_bg: {
                            instance hover: 0.0
                            instance pressed: 0.0
                            instance dark_mode: 0.0
                            instance border_radius: 4.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let base = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                let hover_color = mix((SLATE_300), (SLATE_500), self.dark_mode);
                                let pressed_color = mix((SLATE_400), (SLATE_400), self.dark_mode);
                                let color = mix(mix(base, hover_color, self.hover), pressed_color, self.pressed);
                                sdf.fill(color);
                                return sdf.result;
                            }
                        }
                    }
                }

                // Log content panel with border - Moxin.tts card style
                log_content_column = <RoundedView> {
                    width: Fill, height: Fill
                    draw_bg: {
                        instance dark_mode: 0.0
                        instance border_radius: 12.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                            // Moxin.tts style: white card
                            let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                            sdf.fill(bg);
                            let border = mix(vec4(0.0, 0.0, 0.0, 0.05), vec4(1.0, 1.0, 1.0, 0.1), self.dark_mode);
                            sdf.stroke(border, 1.0);
                            return sdf.result;
                        }
                    }
                    flow: Down

                    log_header = <View> {
                        width: Fill, height: Fit
                        flow: Down
                        show_bg: true
                        draw_bg: {
                            instance dark_mode: 0.0
                            fn pixel(self) -> vec4 {
                                // Bottom border only
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.rect(0.0, self.rect_size.y - 1.0, self.rect_size.x, 1.0);
                                let border = mix((MOXIN_BORDER_LIGHT), (SLATE_700), self.dark_mode);
                                sdf.fill(border);
                                return sdf.result;
                            }
                        }

                        log_title_row = <View> {
                            width: Fill, height: Fit
                            padding: {left: 16, right: 16, top: 14, bottom: 14}
                            flow: Right
                            align: {x: 0.0, y: 0.5}

                            log_title_label = <Label> {
                                text: "System Log"
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                            }

                            <View> { width: Fill, height: 1 }

                            clear_log_btn = <Button> {
                                width: Fit, height: 26
                                padding: {left: 10, right: 10}
                                text: "Clear"
                                draw_bg: {
                                    instance dark_mode: 0.0
                                    instance hover: 0.0
                                    instance border_radius: 6.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                        let hover_color = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                        sdf.fill(mix(base, hover_color, self.hover));
                                        return sdf.result;
                                    }
                                }
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: { font_size: 11.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((MOXIN_TEXT_SECONDARY), (SLATE_300), self.dark_mode);
                                    }
                                }
                            }
                        }
                    }

                    log_scroll = <ScrollYView> {
                        width: Fill, height: Fill
                        flow: Down
                        scroll_bars: <ScrollBars> {
                            show_scroll_x: false
                            show_scroll_y: true
                        }

                        log_content_wrapper = <View> {
                            width: Fill, height: Fit
                            padding: { left: 14, right: 14, top: 10, bottom: 10 }
                            flow: Down

                            log_content = <Markdown> {
                                width: Fill, height: Fit
                                font_size: 11.0
                                font_color: (GRAY_600)
                                paragraph_spacing: 6

                                draw_normal: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_REGULAR>{ font_size: 11.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((SLATE_600), (TEXT_SECONDARY_DARK), self.dark_mode);
                                    }
                                }
                                draw_bold: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((SLATE_700), (TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                            }
                        }
                    }
                }
            }
            } // End main_content

            // Bottom audio player bar - Moxin.tts style
            audio_player_bar = <View> {
                width: Fill, height: 90
                flow: Right
                align: {x: 0.5, y: 0.5}
                padding: {left: 24, right: 24, top: 8, bottom: 8}
                spacing: 0
                visible: false

                show_bg: true
            draw_bg: {
                instance dark_mode: 0.0
                fn pixel(self) -> vec4 {
                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                    // Top border
                    sdf.rect(0.0, 0.0, self.rect_size.x, 1.0);
                    let border = mix((MOXIN_BORDER_LIGHT), (SLATE_700), self.dark_mode);
                    sdf.fill(border);
                    // Background - Moxin.tts style white
                    sdf.rect(0.0, 1.0, self.rect_size.x, self.rect_size.y - 1.0);
                    let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                    sdf.fill(bg);
                    return sdf.result;
                }
            }

            // Left: Voice info (fixed width for balance)
            voice_info = <View> {
                width: 220, height: Fill
                flow: Right
                align: {x: 0.0, y: 0.5}
                spacing: 12

                // Voice avatar - Moxin.tts style
                voice_avatar = <RoundedView> {
                    width: 48, height: 48
                    align: {x: 0.5, y: 0.5}
                    draw_bg: {
                        instance border_radius: 10.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                            sdf.fill((MOXIN_PRIMARY));
                            return sdf.result;
                        }
                    }

                    avatar_initial = <Label> {
                        width: Fill, height: Fill
                        align: {x: 0.4, y: 0.7}
                        draw_text: {
                            text_style: <FONT_BOLD>{ font_size: 18.0 }
                            fn get_color(self) -> vec4 {
                                return (WHITE);
                            }
                        }
                        text: "豆"
                    }
                }

                // Voice name
                voice_name_container = <View> {
                    width: Fit, height: Fit
                    flow: Down
                    spacing: 2

                    current_voice_name = <Label> {
                        width: Fit, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                            fn get_color(self) -> vec4 {
                                return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                            }
                        }
                        text: "Doubao"
                    }

                    status_label = <Label> {
                        width: Fit, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: { font_size: 12.0 }
                            fn get_color(self) -> vec4 {
                                return mix((MOXIN_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
                            }
                        }
                        text: "Ready"
                    }
                }
            }

            // Center: Playback controls (fills space)
            playback_controls = <View> {
                width: Fill, height: Fill
                flow: Down
                align: {x: 0.5, y: 0.5}
                spacing: 6

                // Control buttons row - centered
                controls_row = <View> {
                    width: Fill, height: Fit
                    flow: Right
                    align: {x: 0.5, y: 0.5}
                    spacing: 12

                    // Play/Pause button - Moxin.tts style
                    play_btn = <PlayButton> {
                        text: ""
                        draw_bg: {
                            instance is_playing: 0.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                let center = self.rect_size * 0.5;
                                sdf.circle(center.x, center.y, 17.0);
                                sdf.fill((MOXIN_PRIMARY));
                                if self.is_playing > 0.5 {
                                    // Pause icon (two vertical bars)
                                    sdf.rect(13.0, 12.0, 3.0, 12.0);
                                    sdf.fill((WHITE));
                                    sdf.rect(20.0, 12.0, 3.0, 12.0);
                                    sdf.fill((WHITE));
                                } else {
                                    // Play icon (triangle)
                                    sdf.move_to(14.0, 11.0);
                                    sdf.line_to(26.0, 18.0);
                                    sdf.line_to(14.0, 25.0);
                                    sdf.close_path();
                                    sdf.fill((WHITE));
                                }
                                return sdf.result;
                            }
                        }
                    }
                }

                // Progress bar row - centered with max width constraint
                progress_row = <View> {
                    width: 350, height: Fit
                    flow: Right
                    align: {y: 0.5}
                    spacing: 8

                    current_time = <Label> {
                        width: Fit, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: { font_size: 11.0 }
                            fn get_color(self) -> vec4 {
                                return mix((MOXIN_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
                            }
                        }
                        text: "00:00"
                    }

                    // Progress bar container
                    progress_bar_container = <View> {
                        width: Fill, height: 4
                        margin: {top: 2}

                        // Progress bar
                        progress_bar = <View> {
                            width: Fill, height: Fill
                            show_bg: true
                            draw_bg: {
                                instance dark_mode: 0.0
                                instance progress: 0.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    // Background track
                                    sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 2.0);
                                    let track_color = mix((GRAY_200), (GRAY_700), self.dark_mode);
                                    sdf.fill(track_color);
                                    // Progress fill
                                    let progress_width = self.rect_size.x * self.progress;
                                    sdf.box(0.0, 0.0, progress_width, self.rect_size.y, 2.0);
                                    sdf.fill((PRIMARY_500));
                                    return sdf.result;
                                }
                            }
                        }
                    }

                    total_time = <Label> {
                        width: Fit, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: { font_size: 11.0 }
                            fn get_color(self) -> vec4 {
                                return mix((MOXIN_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
                            }
                        }
                        text: "00:00"
                    }
                }
            }

            // Right: Download/share buttons (fixed width for balance) - Moxin.tts style
            download_section = <View> {
                width: 220, height: Fill
                flow: Right
                spacing: 10
                align: {x: 1.0, y: 0.5}

                download_btn = <Button> {
                    width: Fit, height: 40
                    padding: {left: 24, right: 24}
                    text: "Download"

                    draw_bg: {
                        instance hover: 0.0
                        instance pressed: 0.0

                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 8.0);

                            // Moxin.tts style: primary color outline button
                            let base = vec4(0.39, 0.40, 0.95, 0.08);
                            let hover_color = vec4(0.39, 0.40, 0.95, 0.15);
                            let pressed_color = vec4(0.39, 0.40, 0.95, 0.25);
                            let border = (MOXIN_PRIMARY);

                            let color = mix(base, hover_color, self.hover);
                            let color = mix(color, pressed_color, self.pressed);

                            sdf.fill(color);
                            sdf.stroke(border, 1.5);
                            return sdf.result;
                        }
                    }

                    draw_text: {
                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                        fn get_color(self) -> vec4 {
                            return (MOXIN_PRIMARY);
                        }
                    }
                }

                share_btn = <Button> {
                    width: Fit, height: 40
                    padding: {left: 24, right: 24}
                    text: "Share"

                    draw_bg: {
                        instance hover: 0.0
                        instance pressed: 0.0

                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 8.0);

                            let base = vec4(0.39, 0.40, 0.95, 0.08);
                            let hover_color = vec4(0.39, 0.40, 0.95, 0.15);
                            let pressed_color = vec4(0.39, 0.40, 0.95, 0.25);
                            let border = (MOXIN_PRIMARY);

                            let color = mix(base, hover_color, self.hover);
                            let color = mix(color, pressed_color, self.pressed);

                            sdf.fill(color);
                            sdf.stroke(border, 1.5);
                            return sdf.result;
                        }
                    }

                    draw_text: {
                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                        fn get_color(self) -> vec4 {
                            return (MOXIN_PRIMARY);
                        }
                    }
                }
            }
            } // End audio_player_bar
        } // End content_wrapper
        } // End app_layout

        // Voice clone modal (overlay)
        voice_clone_modal = <VoiceCloneModal> {}

        // Confirm delete modal (overlay)
        confirm_delete_modal = <ConfirmDeleteModal> {}

        // Confirm cancel task modal (overlay)
        confirm_cancel_modal = <ConfirmCancelModal> {}

        // Share target modal
        share_modal = <View> {
            width: Fill, height: Fill
            flow: Overlay
            align: {x: 0.5, y: 0.5}
            visible: false

            share_backdrop = <View> {
                width: Fill, height: Fill
                show_bg: true
                draw_bg: {
                    fn pixel(self) -> vec4 {
                        return vec4(0.0, 0.0, 0.0, 0.45);
                    }
                }
            }

            share_dialog = <RoundedView> {
                width: 460, height: Fit
                flow: Down
                spacing: 0

                draw_bg: {
                    instance dark_mode: 0.0
                    instance border_radius: 12.0
                    fn pixel(self) -> vec4 {
                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                        let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                        sdf.fill(bg);
                        return sdf.result;
                    }
                }

                share_header = <View> {
                    width: Fill, height: Fit
                    padding: {left: 20, right: 20, top: 18, bottom: 12}
                    flow: Down
                    spacing: 6

                    share_title = <Label> {
                        width: Fill, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_BOLD>{ font_size: 17.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                            }
                        }
                        text: "分享音频"
                    }

                    share_subtitle = <Label> {
                        width: Fill, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: { font_size: 12.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                            }
                        }
                        text: "选择目标应用进行分享"
                    }
                }

                share_actions = <View> {
                    width: Fill, height: Fit
                    flow: Down
                    spacing: 8
                    padding: {left: 20, right: 20, top: 0, bottom: 14}

                    share_system_btn = <Button> {
                        width: Fill, height: 38
                        text: "系统打开"
                        draw_bg: {
                            instance dark_mode: 0.0
                            instance hover: 0.0
                            instance border_radius: 8.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                let hover_bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                sdf.fill(mix(bg, hover_bg, self.hover));
                                return sdf.result;
                            }
                        }
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                            }
                        }
                    }

                    share_capcut_btn = <Button> {
                        width: Fill, height: 38
                        text: "分享到剪映"
                        draw_bg: {
                            instance dark_mode: 0.0
                            instance hover: 0.0
                            instance border_radius: 8.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                let hover_bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                sdf.fill(mix(bg, hover_bg, self.hover));
                                return sdf.result;
                            }
                        }
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                            }
                        }
                    }

                    share_premiere_btn = <Button> {
                        width: Fill, height: 38
                        text: "分享到 Premiere Pro"
                        draw_bg: {
                            instance dark_mode: 0.0
                            instance hover: 0.0
                            instance border_radius: 8.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                let hover_bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                sdf.fill(mix(bg, hover_bg, self.hover));
                                return sdf.result;
                            }
                        }
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                            }
                        }
                    }

                    share_wechat_btn = <Button> {
                        width: Fill, height: 38
                        text: "分享到微信"
                        draw_bg: {
                            instance dark_mode: 0.0
                            instance hover: 0.0
                            instance border_radius: 8.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                let hover_bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                sdf.fill(mix(bg, hover_bg, self.hover));
                                return sdf.result;
                            }
                        }
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                            }
                        }
                    }

                    share_finder_btn = <Button> {
                        width: Fill, height: 38
                        text: "在访达中显示"
                        draw_bg: {
                            instance dark_mode: 0.0
                            instance hover: 0.0
                            instance border_radius: 8.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                let hover_bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                sdf.fill(mix(bg, hover_bg, self.hover));
                                return sdf.result;
                            }
                        }
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                            }
                        }
                    }
                }

                share_footer = <View> {
                    width: Fill, height: Fit
                    padding: {left: 20, right: 20, top: 0, bottom: 16}
                    flow: Right
                    align: {x: 1.0, y: 0.5}

                    share_cancel_btn = <Button> {
                        width: Fit, height: 34
                        padding: {left: 16, right: 16}
                        text: "取消"
                        draw_bg: {
                            instance dark_mode: 0.0
                            instance hover: 0.0
                            instance border_radius: 8.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                let hover_bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                sdf.fill(mix(bg, hover_bg, self.hover));
                                return sdf.result;
                            }
                        }
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                            }
                        }
                    }
                }
            }
        }

        // Voice picker modal (overlay) - ElevenLabs style
        voice_picker_modal = <View> {
            width: Fill, height: Fill
            flow: Overlay
            align: {x: 0.5, y: 0.5}
            visible: false

            // Semi-transparent backdrop
            voice_picker_backdrop = <View> {
                width: Fill, height: Fill
                show_bg: true
                draw_bg: {
                    fn pixel(self) -> vec4 {
                        return vec4(0.0, 0.0, 0.0, 0.5);
                    }
                }
            }

            // Modal dialog
            voice_picker_dialog = <RoundedView> {
                width: 520, height: 640
                flow: Down
                spacing: 0

                draw_bg: {
                    instance dark_mode: 0.0
                    instance border_radius: 16.0
                    fn pixel(self) -> vec4 {
                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                        let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                        sdf.fill(bg);
                        return sdf.result;
                    }
                }

                // Header
                voice_picker_header = <View> {
                    width: Fill, height: 64
                    flow: Right
                    padding: {left: 20, right: 16, top: 16, bottom: 14}
                    align: {y: 0.5}
                    spacing: 12

                    voice_picker_back_btn = <Button> {
                        width: 32, height: 32
                        text: "←"
                        draw_bg: {
                            instance hover: 0.0
                            fn pixel(self) -> vec4 { return vec4(0.0, 0.0, 0.0, 0.0); }
                        }
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: { font_size: 18.0 }
                            fn get_color(self) -> vec4 {
                                return mix(vec4(0.4, 0.4, 0.45, 1.0), vec4(0.75, 0.75, 0.8, 1.0), self.dark_mode);
                            }
                        }
                    }

                    voice_picker_title = <Label> {
                        width: Fit, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 16.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                            }
                        }
                        text: "Select a voice"
                    }
                }

                // Tabs: Built-in | My Voices
                voice_picker_tabs = <View> {
                    width: Fill, height: 46
                    flow: Right
                    padding: {left: 20, right: 20}
                    spacing: 16
                    show_bg: true
                    draw_bg: {
                        instance dark_mode: 0.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.rect(0.0, self.rect_size.y - 1.0, self.rect_size.x, 1.0);
                            let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                            sdf.fill(border);
                            return sdf.result;
                        }
                    }

                    voice_explore_tab = <Button> {
                        width: Fit, height: Fill
                        padding: {left: 8, right: 8}
                        text: "Built-in"
                        draw_bg: {
                            instance active: 1.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.rect(0.0, self.rect_size.y - 2.0, self.rect_size.x, 2.0);
                                let underline = mix(vec4(0.0, 0.0, 0.0, 0.0), (MOXIN_PRIMARY), self.active);
                                sdf.fill(underline);
                                return sdf.result;
                            }
                        }
                        draw_text: {
                            instance active: 1.0
                            instance dark_mode: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                            fn get_color(self) -> vec4 {
                                let normal = mix(vec4(0.5, 0.5, 0.55, 1.0), vec4(0.62, 0.62, 0.68, 1.0), self.dark_mode);
                                let active = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                return mix(normal, active, self.active);
                            }
                        }
                    }

                    voice_my_voices_tab = <Button> {
                        width: Fit, height: Fill
                        padding: {left: 8, right: 8}
                        text: "My Voices"
                        draw_bg: {
                            instance active: 0.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.rect(0.0, self.rect_size.y - 2.0, self.rect_size.x, 2.0);
                                let underline = mix(vec4(0.0, 0.0, 0.0, 0.0), (MOXIN_PRIMARY), self.active);
                                sdf.fill(underline);
                                return sdf.result;
                            }
                        }
                        draw_text: {
                            instance active: 0.0
                            instance dark_mode: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                            fn get_color(self) -> vec4 {
                                let normal = mix(vec4(0.5, 0.5, 0.55, 1.0), vec4(0.62, 0.62, 0.68, 1.0), self.dark_mode);
                                let active = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                return mix(normal, active, self.active);
                            }
                        }
                    }
                }

                // Search bar
                voice_search_row = <View> {
                    width: Fill, height: 60
                    padding: {left: 20, right: 20, top: 12, bottom: 6}

                    voice_search_input = <TextInput> {
                        width: Fill, height: 42
                        empty_text: "Start typing to search..."
                        text: ""
                        draw_bg: {
                            instance dark_mode: 0.0
                            instance focus: 0.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 12.0);
                                let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                sdf.fill(bg);
                                let border_normal = mix((SLATE_300), (SLATE_600), self.dark_mode);
                                let border_focused = (MOXIN_PRIMARY);
                                let border = mix(border_normal, border_focused, self.focus);
                                sdf.stroke(border, mix(1.0, 1.5, self.focus));
                                return sdf.result;
                            }
                        }
                        draw_text: {
                            instance dark_mode: 0.0
                            instance empty: 0.0
                            text_style: { font_size: 13.0 }
                            fn get_color(self) -> vec4 {
                                let text_color = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                let placeholder = mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                return mix(text_color, placeholder, self.empty);
                            }
                        }
                        draw_cursor: {
                            instance focus: 0.0
                            instance blink: 0.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 0.5);
                                let alpha = self.focus * self.blink;
                                sdf.fill(vec4(0.39, 0.40, 0.95, alpha));
                                return sdf.result;
                            }
                        }
                        draw_selection: {
                            instance focus: 0.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 1.0);
                                sdf.fill(mix(vec4(0.39, 0.40, 0.95, 0.18), vec4(0.39, 0.40, 0.95, 0.30), self.focus));
                                return sdf.result;
                            }
                        }
                    }
                }

                // Filter row
                voice_filter_row = <View> {
                    width: Fill, height: 44
                    flow: Right
                    padding: {left: 20, right: 20, top: 2, bottom: 10}
                    spacing: 10

                    filter_language_wrap = <View> {
                        width: 104, height: 34
                        flow: Overlay
                        align: {x: 0.0, y: 0.5}
                        filter_language_dd = <VoicePickerFilterDropDown> { width: Fill, height: Fill }
                        filter_language_label = <Label> {
                            width: Fill, height: Fill
                            align: {x: 0.18, y: 0.55}
                            draw_text: {
                                instance dark_mode: 0.0
                                text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                                fn get_color(self) -> vec4 {
                                    return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                }
                            }
                            text: "语言"
                        }
                    }

                    filter_gender_wrap = <View> {
                        width: 100, height: 34
                        flow: Overlay
                        align: {x: 0.0, y: 0.5}
                        filter_gender_dd = <VoicePickerFilterDropDown> { width: Fill, height: Fill }
                        filter_gender_label = <Label> {
                            width: Fill, height: Fill
                            align: {x: 0.20, y: 0.55}
                            draw_text: {
                                instance dark_mode: 0.0
                                text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                                fn get_color(self) -> vec4 {
                                    return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                }
                            }
                            text: "性别"
                        }
                    }

                    filter_age_wrap = <View> {
                        width: 92, height: 34
                        flow: Overlay
                        align: {x: 0.0, y: 0.5}
                        filter_age_dd = <VoicePickerFilterDropDown> { width: Fill, height: Fill }
                        filter_age_label = <Label> {
                            width: Fill, height: Fill
                            align: {x: 0.22, y: 0.55}
                            draw_text: {
                                instance dark_mode: 0.0
                                text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                                fn get_color(self) -> vec4 {
                                    return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                }
                            }
                            text: "年龄"
                        }
                    }

                    filter_domain_wrap = <View> {
                        width: 96, height: 34
                        flow: Overlay
                        align: {x: 0.0, y: 0.5}
                        filter_domain_dd = <VoicePickerFilterDropDown> { width: Fill, height: Fill }
                        filter_domain_label = <Label> {
                            width: Fill, height: Fill
                            align: {x: 0.22, y: 0.55}
                            draw_text: {
                                instance dark_mode: 0.0
                                text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                                fn get_color(self) -> vec4 {
                                    return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                }
                            }
                            text: "风格"
                        }
                    }
                }

                // Voice list (scrollable)
                voice_picker_list = <PortalList> {
                    width: Fill, height: Fill
                    flow: Down

                    VoicePickerItem = <View> {
                        width: Fill, height: 96
                        margin: {left: 12, right: 12, top: 8}
                        padding: {left: 14, right: 14, top: 12, bottom: 12}
                        flow: Right
                        align: {y: 0.5}
                        spacing: 14
                        cursor: Hand

                        show_bg: true
                        draw_bg: {
                            instance dark_mode: 0.0
                            instance hover: 0.0
                            instance selected: 0.0
                            instance border_radius: 12.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let base = mix((WHITE), (SLATE_800), self.dark_mode);
                                let hover_color = mix((SLATE_50), (SLATE_700), self.dark_mode);
                                let selected_color = mix((PRIMARY_50), (PRIMARY_900), self.dark_mode);
                                let border = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                let color = mix(base, hover_color, self.hover);
                                let color = mix(color, selected_color, self.selected);
                                sdf.fill(color);
                                sdf.stroke(border, 1.0);
                                return sdf.result;
                            }
                        }

                        picker_avatar = <RoundedView> {
                            width: 50, height: 50
                            align: {x: 0.5, y: 0.5}
                            draw_bg: {
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.circle(25.0, 25.0, 25.0);
                                    sdf.fill((PRIMARY_500));
                                    return sdf.result;
                                }
                            }
                            picker_initial = <Label> {
                                width: Fill, height: Fill
                                align: {x: 0.3, y: 0.6}
                                draw_text: {
                                    text_style: <FONT_SEMIBOLD>{ font_size: 19.0 }
                                    fn get_color(self) -> vec4 { return (WHITE); }
                                }
                                text: "D"
                            }
                        }

                        picker_info = <View> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 8

                            picker_name = <Label> {
                                width: Fill, height: 24
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 15.0 }
                                    wrap: Ellipsis
                                    fn get_color(self) -> vec4 {
                                        return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "Voice Name"
                            }
                            picker_desc = <Label> {
                                width: Fill, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: { font_size: 13.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "Voice description"
                            }
                        }

                        picker_play_btn = <View> {
                            width: 34, height: 34
                            align: {x: 0.5, y: 0.5}
                            cursor: Hand
                            show_bg: true
                            draw_bg: {
                                instance dark_mode: 0.0
                                instance hover: 0.0
                                instance playing: 0.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.circle(17.0, 17.0, 17.0);
                                    let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                    let hover_bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                    let bg = mix(base, hover_bg, self.hover);
                                    let icon = mix((SLATE_600), (SLATE_200), self.dark_mode);
                                    sdf.fill(bg);
                                    if self.playing > 0.5 {
                                        sdf.rect(12.0, 11.0, 3.0, 12.0);
                                        sdf.fill(icon);
                                        sdf.rect(19.0, 11.0, 3.0, 12.0);
                                        sdf.fill(icon);
                                    } else {
                                        sdf.move_to(13.0, 11.0);
                                        sdf.line_to(23.0, 17.0);
                                        sdf.line_to(13.0, 23.0);
                                        sdf.close_path();
                                        sdf.fill(icon);
                                    }
                                    return sdf.result;
                                }
                            }
                        }
                    }
                }

                voice_picker_empty = <Label> {
                    width: Fill, height: Fit
                    margin: {left: 20, right: 20, top: 24}
                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: { font_size: 12.0 }
                        fn get_color(self) -> vec4 {
                            return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                        }
                    }
                    text: "No voices found with current filters."
                }
            }
        }

        // Model picker modal (overlay) - ElevenLabs style
        model_picker_modal = <View> {
            width: Fill, height: Fill
            flow: Overlay
            align: {x: 0.5, y: 0.5}
            visible: false

            model_picker_backdrop = <View> {
                width: Fill, height: Fill
                show_bg: true
                draw_bg: {
                    fn pixel(self) -> vec4 {
                        return vec4(0.0, 0.0, 0.0, 0.5);
                    }
                }
            }

            model_picker_dialog = <RoundedView> {
                width: 560, height: 600
                flow: Down
                spacing: 0
                draw_bg: {
                    instance dark_mode: 0.0
                    instance border_radius: 16.0
                    fn pixel(self) -> vec4 {
                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                        let bg = mix((WHITE), (SLATE_900), self.dark_mode);
                        sdf.fill(bg);
                        return sdf.result;
                    }
                }

                model_picker_header = <View> {
                    width: Fill, height: 56
                    flow: Right
                    padding: {left: 20, right: 16, top: 14, bottom: 14}
                    align: {y: 0.5}
                    spacing: 12
                    show_bg: true
                    draw_bg: {
                        instance dark_mode: 0.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.rect(0.0, self.rect_size.y - 1.0, self.rect_size.x, 1.0);
                            let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                            sdf.fill(border);
                            return sdf.result;
                        }
                    }

                    model_picker_back_btn = <Button> {
                        width: 32, height: 32
                        text: "←"
                        draw_bg: {
                            instance hover: 0.0
                            fn pixel(self) -> vec4 { return vec4(0.0, 0.0, 0.0, 0.0); }
                        }
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: { font_size: 18.0 }
                            fn get_color(self) -> vec4 {
                                return mix(vec4(0.35, 0.35, 0.40, 1.0), vec4(0.72, 0.72, 0.78, 1.0), self.dark_mode);
                            }
                        }
                    }

                    model_picker_title = <Label> {
                        width: Fit, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 16.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                            }
                        }
                        text: "Select a model"
                    }
                }

                model_picker_list = <PortalList> {
                    width: Fill, height: Fill
                    flow: Down

                    ModelPickerCard = <View> {
                        width: Fill, height: Fit
                        margin: {left: 16, right: 16, top: 10}
                        padding: {left: 16, right: 16, top: 14, bottom: 14}
                        flow: Down
                        spacing: 10
                        cursor: Hand
                        show_bg: true
                        draw_bg: {
                            instance dark_mode: 0.0
                            instance hover: 0.0
                            instance selected: 0.0
                            instance border_radius: 12.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                let base = mix((WHITE), (SLATE_800), self.dark_mode);
                                let hover_color = mix((SLATE_50), (SLATE_700), self.dark_mode);
                                let selected_color = mix((PRIMARY_50), (PRIMARY_900), self.dark_mode);
                                let bg = mix(base, hover_color, self.hover);
                                let bg = mix(bg, selected_color, self.selected);
                                let border = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                sdf.fill(bg);
                                sdf.stroke(border, 1.0);
                                return sdf.result;
                            }
                        }

                        model_top = <View> {
                            width: Fill, height: Fit
                            flow: Right
                            align: {y: 0.5}
                            spacing: 8

                            model_name = <Label> {
                                width: Fit, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 15.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "Model Name"
                            }

                            <View> { width: Fill, height: 1 }

                            model_badge = <RoundedView> {
                                width: Fit, height: Fit
                                visible: false
                                padding: {left: 10, right: 10, top: 4, bottom: 4}
                                draw_bg: {
                                    instance dark_mode: 0.0
                                    border_radius: 10.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                        sdf.fill(bg);
                                        return sdf.result;
                                    }
                                }
                                model_badge_label = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                        }
                                    }
                                    text: "Available"
                                }
                            }

                            model_check = <View> {
                                width: 24, height: 24
                                show_bg: true
                                draw_bg: {
                                    instance selected: 0.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.circle(12.0, 12.0, 10.0);
                                        sdf.stroke(mix((SLATE_400), (MOXIN_PRIMARY), self.selected), 1.5);
                                        sdf.move_to(7.0, 12.0);
                                        sdf.line_to(10.5, 15.0);
                                        sdf.line_to(17.0, 8.5);
                                        sdf.stroke(vec4(0.39, 0.40, 0.95, self.selected), 1.8);
                                        return sdf.result;
                                    }
                                }
                            }
                        }

                        model_desc = <Label> {
                            width: Fill, height: Fit
                            draw_text: {
                                instance dark_mode: 0.0
                                text_style: { font_size: 12.0 }
                                fn get_color(self) -> vec4 {
                                    return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                }
                            }
                            text: "Model description"
                        }

                        model_tags = <View> {
                            width: Fill, height: Fit
                            flow: Right
                            spacing: 8

                            tag_1 = <ModelTagChip> {}
                            tag_2 = <ModelTagChip> {}
                            tag_3 = <ModelTagChip> {}
                        }
                    }
                }

                model_picker_footer = <View> {
                    width: Fill, height: 52
                    align: {x: 0.5, y: 0.5}
                    show_bg: true
                    draw_bg: {
                        instance dark_mode: 0.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.rect(0.0, 0.0, self.rect_size.x, 1.0);
                            let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                            sdf.fill(border);
                            return sdf.result;
                        }
                    }

                    model_picker_footer_label = <Label> {
                        width: Fit, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                            }
                        }
                        text: "Show all models"
                    }
                }
            }
        }

        // Global settings modal (overlay)
        global_settings_modal = <View> {
            width: Fill, height: Fill
            flow: Overlay
            align: {x: 0.5, y: 0.5}
            visible: false

            // Semi-transparent backdrop
            settings_backdrop = <View> {
                width: Fill, height: Fill
                show_bg: true
                draw_bg: {
                    fn pixel(self) -> vec4 {
                        return vec4(0.0, 0.0, 0.0, 0.5);
                    }
                }
            }

            // Settings dialog
            settings_dialog = <RoundedView> {
                width: 400, height: 480
                flow: Down
                spacing: 0

                draw_bg: {
                    instance dark_mode: 0.0
                    instance border_radius: 16.0
                    fn pixel(self) -> vec4 {
                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                        let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                        sdf.fill(bg);
                        return sdf.result;
                    }
                }

                // Header
                settings_header = <View> {
                    width: Fill, height: 60
                    flow: Right
                    padding: {left: 24, right: 24, top: 20, bottom: 16}
                    align: {y: 0.5}

                    settings_title = <Label> {
                        width: Fill, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_BOLD>{ font_size: 18.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                            }
                        }
                        text: "⚙️ Settings"
                    }

                    settings_close_btn = <Button> {
                        width: 32, height: 32
                        text: "X"
                        draw_bg: {
                            instance dark_mode: 0.0
                            instance hover: 0.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.circle(16.0, 16.0, 16.0);
                                let light_hover = vec4(0.0, 0.0, 0.0, 0.10);
                                let dark_hover = vec4(1.0, 1.0, 1.0, 0.14);
                                let bg = mix(vec4(0.0, 0.0, 0.0, 0.0), mix(light_hover, dark_hover, self.dark_mode), self.hover);
                                sdf.fill(bg);
                                return sdf.result;
                            }
                        }
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_BOLD>{ font_size: 16.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                            }
                        }
                    }
                }

                // Settings content
                settings_content = <View> {
                    width: Fill, height: Fill
                    flow: Down
                    padding: {left: 24, right: 24, top: 8, bottom: 24}
                    spacing: 24

                    // Language setting
                    language_section = <View> {
                        width: Fill, height: Fit
                        flow: Down
                        spacing: 12

                        language_title = <Label> {
                            width: Fit, height: Fit
                            draw_text: {
                                instance dark_mode: 0.0
                                text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                fn get_color(self) -> vec4 {
                                    return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                }
                            }
                            text: "🌐 Language"
                        }

                        language_options = <View> {
                            width: Fill, height: Fit
                            flow: Right
                            spacing: 12

                            lang_en_option = <Button> {
                                width: Fit, height: 36
                                padding: {left: 16, right: 16}
                                text: "English"
                                draw_bg: {
                                    instance active: 1.0
                                    instance dark_mode: 0.0
                                    instance border_radius: 8.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        let normal = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                        let active_color = (MOXIN_PRIMARY);
                                        sdf.fill(mix(normal, active_color, self.active));
                                        return sdf.result;
                                    }
                                }
                                draw_text: {
                                    instance active: 1.0
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                    fn get_color(self) -> vec4 {
                                        let normal = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                        return mix(normal, (WHITE), self.active);
                                    }
                                }
                            }

                            lang_zh_option = <Button> {
                                width: Fit, height: 36
                                padding: {left: 16, right: 16}
                                text: "中文"
                                draw_bg: {
                                    instance active: 0.0
                                    instance dark_mode: 0.0
                                    instance border_radius: 8.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        let normal = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                        let active_color = (MOXIN_PRIMARY);
                                        sdf.fill(mix(normal, active_color, self.active));
                                        return sdf.result;
                                    }
                                }
                                draw_text: {
                                    instance active: 0.0
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                    fn get_color(self) -> vec4 {
                                        let normal = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                        return mix(normal, (WHITE), self.active);
                                    }
                                }
                            }
                        }
                    }

                    // Theme setting
                    theme_section = <View> {
                        width: Fill, height: Fit
                        flow: Down
                        spacing: 12

                        theme_title = <Label> {
                            width: Fit, height: Fit
                            draw_text: {
                                instance dark_mode: 0.0
                                text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                fn get_color(self) -> vec4 {
                                    return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                }
                            }
                            text: "🎨 Theme"
                        }

                        theme_options = <View> {
                            width: Fill, height: Fit
                            flow: Right
                            spacing: 12

                            theme_light_option = <Button> {
                                width: Fit, height: 36
                                padding: {left: 16, right: 16}
                                text: "☀️ Light"
                                draw_bg: {
                                    instance active: 1.0
                                    instance dark_mode: 0.0
                                    instance border_radius: 8.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        let normal = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                        let active_color = (MOXIN_PRIMARY);
                                        sdf.fill(mix(normal, active_color, self.active));
                                        return sdf.result;
                                    }
                                }
                                draw_text: {
                                    instance active: 1.0
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                    fn get_color(self) -> vec4 {
                                        let normal = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                        return mix(normal, (WHITE), self.active);
                                    }
                                }
                            }

                            theme_dark_option = <Button> {
                                width: Fit, height: 36
                                padding: {left: 16, right: 16}
                                text: "🌙 Dark"
                                draw_bg: {
                                    instance active: 0.0
                                    instance dark_mode: 0.0
                                    instance border_radius: 8.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        let normal = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                        let active_color = (MOXIN_PRIMARY);
                                        sdf.fill(mix(normal, active_color, self.active));
                                        return sdf.result;
                                    }
                                }
                                draw_text: {
                                    instance active: 0.0
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                    fn get_color(self) -> vec4 {
                                        let normal = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                        return mix(normal, (WHITE), self.active);
                                    }
                                }
                            }
                        }
                    }

                    // About section
                    about_section = <View> {
                        width: Fill, height: Fit
                        flow: Down
                        spacing: 8

                        about_title = <Label> {
                            width: Fit, height: Fit
                            draw_text: {
                                instance dark_mode: 0.0
                                text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                fn get_color(self) -> vec4 {
                                    return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                }
                            }
                            text: "ℹ️ About"
                        }

                        about_version = <Label> {
                            width: Fit, height: Fit
                            draw_text: {
                                instance dark_mode: 0.0
                                text_style: { font_size: 13.0 }
                                fn get_color(self) -> vec4 {
                                    return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                }
                            }
                            text: "Moxin TTS v0.1.0"
                        }

                        about_engine = <Label> {
                            width: Fit, height: Fit
                            draw_text: {
                                instance dark_mode: 0.0
                                text_style: { font_size: 12.0 }
                                fn get_color(self) -> vec4 {
                                    return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                }
                            }
                            text: "Powered by GPT-SoVITS v2"
                        }
                    }
                }
            }
        }

        // Toast notification (top center overlay)
        toast_overlay = <View> {
            width: Fill, height: Fill
            align: {x: 0.5, y: 0.0}
            padding: {top: 80}

            download_toast = <Toast> {}
        }

        // Loading overlay - shown while dataflow is initializing
        loading_overlay = <View> {
            width: Fill, height: Fill
            align: {x: 0.5, y: 0.5}
            visible: true

            show_bg: true
            draw_bg: {
                fn pixel(self) -> vec4 {
                    // Match sidebar gradient style
                    let gradient = mix(
                        vec4(0.157, 0.196, 0.255, 1.0),  // #283041
                        vec4(0.122, 0.153, 0.200, 1.0),  // #1f2733
                        self.pos.y
                    );
                    return gradient;
                }
            }

            loading_content = <View> {
                width: Fit, height: Fit
                flow: Down
                spacing: 24
                align: {x: 0.5, y: 0.5}

                // App logo/name
                loading_logo = <View> {
                    width: Fit, height: Fit
                    flow: Down
                    spacing: 12
                    align: {x: 0.5}

                    loading_title = <Label> {
                        width: Fit, height: Fit
                        draw_text: {
                            text_style: <FONT_BOLD>{ font_size: 28.0 }
                            fn get_color(self) -> vec4 {
                                return (WHITE);
                            }
                        }
                        text: "Moxin TTS"
                    }

                    loading_subtitle = <Label> {
                        width: Fit, height: Fit
                        draw_text: {
                            text_style: { font_size: 13.0 }
                            fn get_color(self) -> vec4 {
                                return vec4(0.7, 0.7, 0.75, 1.0);
                            }
                        }
                        text: "Voice Cloning & Text-to-Speech"
                    }
                }

                // Spinner area
                loading_spinner_area = <View> {
                    width: Fit, height: Fit
                    align: {x: 0.5}

                    loading_spinner = <View> {
                        width: 40, height: 40
                        show_bg: true
                        draw_bg: {
                            instance phase: 0.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                let cx = self.rect_size.x * 0.5;
                                let cy = self.rect_size.y * 0.5;
                                let radius = min(cx, cy) - 4.0;
                                let pi = 3.141592653589793;

                                // Background track ring
                                sdf.circle(cx, cy, radius);
                                sdf.stroke(vec4(1.0, 1.0, 1.0, 0.15), 3.0);

                                // Spinning arc (270 degrees)
                                let start = self.phase * 2.0 * pi;
                                let end = start + pi * 1.5;
                                sdf.arc_round_caps(cx, cy, radius, start, end, 3.0);
                                sdf.fill(vec4(0.39, 0.40, 0.95, 1.0));

                                return sdf.result;
                            }
                        }
                    }
                }

                // Status text
                loading_status = <Label> {
                    width: Fit, height: Fit
                    draw_text: {
                        text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                        fn get_color(self) -> vec4 {
                            return vec4(0.8, 0.8, 0.85, 1.0);
                        }
                    }
                    text: "Initializing..."
                }

                loading_detail = <Label> {
                    width: Fit, height: Fit
                    draw_text: {
                        text_style: { font_size: 12.0 }
                        fn get_color(self) -> vec4 {
                            return vec4(0.55, 0.55, 0.6, 1.0);
                        }
                    }
                    text: "Starting TTS dataflow engine"
                }
            }
        }
    }
}

#[derive(Live, LiveHook, Widget)]
pub struct TTSScreen {
    #[deref]
    view: View,

    #[rust]
    tts_status: TTSStatus,

    #[rust]
    audio_player: Option<TTSPlayer>,

    #[rust]
    dora: Option<DoraIntegration>,

    #[rust]
    update_timer: Timer,

    #[rust]
    dark_mode: f64,

    // UI Logic states
    #[rust]
    log_panel_width: f64,
    #[rust]
    log_panel_collapsed: bool,
    #[rust]
    splitter_dragging: bool,
    #[rust]
    log_entries: Vec<String>,
    #[rust]
    logs_initialized: bool,
    #[rust]
    audio_playing_time: f64,

    // Stored audio for playback/download (not auto-play)
    #[rust]
    stored_audio_samples: Vec<f32>,
    #[rust]
    stored_audio_sample_rate: u32,
    #[rust]
    processed_audio_samples: Vec<f32>,

    // Current voice name for display
    #[rust]
    current_voice_name: String,
    #[rust]
    selected_voice_id: Option<String>,
    #[rust]
    generated_voice_id: Option<String>,
    #[rust]
    pending_generation_voice_id: Option<String>,
    #[rust]
    pending_generation_text: Option<String>,
    #[rust]
    pending_generation_model_id: Option<String>,
    #[rust]
    pending_generation_model_name: Option<String>,
    #[rust]
    pending_generation_speed: f64,
    #[rust]
    pending_generation_pitch: f64,
    #[rust]
    pending_generation_volume: f64,
    #[rust]
    has_generated_audio: bool,

    // Preview player for reference audio
    #[rust]
    preview_player: Option<TTSPlayer>,
    #[rust]
    preview_playing_voice_id: Option<String>,

    // Toast notification state
    #[rust]
    toast_timer: Timer,
    #[rust]
    toast_visible: bool,
    #[rust]
    toast_message: String,

    // Delete confirmation state
    #[rust]
    pending_delete_voice_id: Option<String>,
    #[rust]
    pending_delete_voice_name: Option<String>,

    // Cancel task confirmation state
    #[rust]
    pending_cancel_task_id: Option<String>,
    #[rust]
    pending_cancel_task_name: Option<String>,

    // Current page state
    #[rust]
    current_page: AppPage,

    // Voice Library page state
    #[rust]
    library_voices: Vec<Voice>,
    #[rust]
    library_search_query: String,
    #[rust]
    library_category_filter: VoiceFilter,
    #[rust]
    library_language_filter: LanguageFilter,
    #[rust]
    library_age_filter: u8, // bitmask: 0b01 = Adult, 0b10 = Youth
    #[rust]
    library_style_filter: u8, // bitmask: 0b01 = Sweet, 0b10 = Magnetic
    #[rust]
    library_trait_filter: u8, // bitmask: 0b01 = Professional, 0b10 = Character
    #[rust]
    library_loading: bool,
    #[rust]
    library_card_areas: Vec<(usize, Area, Area, Area)>, // (voice_idx, card_area, preview_btn_area, delete_btn_area)

    // Voice Clone page state
    #[rust]
    clone_tasks: Vec<CloneTask>,
    #[rust]
    clone_loading: bool,
    #[rust]
    clone_card_areas: Vec<(usize, Area, Area, Area, Area)>, // (task_idx, card_area, view_btn_area, cancel_btn_area, delete_btn_area)
    
    // Task Detail page state
    #[rust]
    current_task_id: Option<String>,

    // Current clone mode (Quick/Advanced)
    #[rust]
    current_clone_mode: CloneMode,

    // Training executor for background task execution
    #[rust]
    training_executor: Option<TrainingExecutor>,

    #[rust]
    dora_started: bool,

    #[rust]
    loading_dismissed: bool,

    #[rust]
    spinner_phase: f64,

    // Right controls panel state: 0 = Voice Management, 1 = Settings, 2 = History
    #[rust]
    controls_panel_tab: u8,
    
    // TTS parameters
    #[rust]
    tts_speed: f64,
    #[rust]
    tts_pitch: f64,
    #[rust]
    tts_volume: f64,
    #[rust]
    tts_slider_dragging: Option<TtsParamSliderKind>,
    
    // Global settings modal
    #[rust]
    global_settings_visible: bool,
    #[rust]
    app_language: String, // "en" or "zh"

    // Model picker modal
    #[rust]
    model_picker_visible: bool,
    #[rust]
    model_options: Vec<TtsModelOption>,
    #[rust]
    selected_tts_model_id: Option<String>,
    #[rust]
    model_picker_card_areas: Vec<(usize, Area)>, // (model_idx, card_area)
    
    // Voice picker (inline in settings panel)
    #[rust]
    voice_picker_search: String,
    #[rust]
    voice_picker_tab: u8, // 0 = Built-in, 1 = My Voices
    #[rust]
    voice_picker_language_filter: LanguageFilter,
    #[rust]
    voice_picker_gender_filter: VoiceFilter,
    #[rust]
    voice_picker_age_filter: u8, // bitmask: 0b01 = Adult, 0b10 = Youth
    #[rust]
    voice_picker_style_filter: u8, // bitmask: 0b01 = Sweet, 0b10 = Magnetic
    #[rust]
    voice_picker_trait_filter: u8, // bitmask: 0b01 = Professional, 0b10 = Character
    #[rust]
    voice_picker_item_areas: Vec<(usize, Area, Area)>, // (item_idx, item_area, play_btn_area)
    #[rust]
    voice_picker_active_voice_id: Option<String>, // current focused voice in picker (selection/preview)

    // TTS generation history
    #[rust]
    tts_history: Vec<TtsHistoryEntry>,
    #[rust]
    history_item_areas: Vec<(usize, Area, Area, Area, Area, Area, Area)>, // (idx, card, play, use, download, share, delete)
    #[rust]
    share_modal_visible: bool,
    #[rust]
    pending_share_source: Option<ShareSource>,
}

// Import CloneTask and CloneTaskStatus from task_persistence
use crate::task_persistence::{CloneTask, CloneTaskStatus};

impl Widget for TTSScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // IMPORTANT: Use capture_actions to consume child widget events and prevent
        // them from propagating to sibling screens (like moxin-debate)
        let actions = cx.capture_actions(|cx| {
            self.view.handle_event(cx, event, scope);
        });

        // Initialize audio player
        if self.audio_player.is_none() {
            self.audio_player = Some(TTSPlayer::new());
        }

        // Initialize log bridge and timer
        if !self.logs_initialized {
            log_bridge::init();
            self.logs_initialized = true;
            // Start timer for polling
            self.update_timer = cx.start_interval(0.1);
            // Initialize stored audio sample rate (PrimeSpeech uses 32000)
            self.stored_audio_sample_rate = 32000;
            self.processed_audio_samples = Vec::new();
            // Initialize voice name
            self.current_voice_name = "Doubao".to_string();
            self.selected_voice_id = Some("Doubao".to_string());
            self.generated_voice_id = None;
            self.pending_generation_voice_id = None;
            self.pending_generation_text = None;
            self.pending_generation_model_id = None;
            self.pending_generation_model_name = None;
            self.pending_generation_speed = 1.0;
            self.pending_generation_pitch = 0.0;
            self.pending_generation_volume = 100.0;
            self.has_generated_audio = false;
            // Initialize current page
            self.current_page = AppPage::TextToSpeech;
            
            // Initialize voice library state
            self.library_voices = Vec::new();
            self.library_search_query = String::new();
            self.library_category_filter = VoiceFilter::All;
            self.library_language_filter = LanguageFilter::All;
            self.library_age_filter = 0;
            self.library_style_filter = 0;
            self.library_trait_filter = 0;
            self.library_loading = false;
            self.library_card_areas = Vec::new();

            // Initialize model picker state
            self.model_picker_visible = false;
            self.model_options = get_project_tts_models();
            self.selected_tts_model_id = self.model_options.first().map(|m| m.id.clone());
            self.model_picker_card_areas = Vec::new();

            // Initialize voice picker state
            self.voice_picker_search = String::new();
            self.voice_picker_tab = 0;
            self.voice_picker_language_filter = LanguageFilter::All;
            self.voice_picker_gender_filter = VoiceFilter::All;
            self.voice_picker_age_filter = 0;
            self.voice_picker_style_filter = 0;
            self.voice_picker_trait_filter = 0;
            self.voice_picker_item_areas = Vec::new();
            self.voice_picker_active_voice_id = self.selected_voice_id.clone();

            self.tts_history = tts_history::load_history();
            self.history_item_areas = Vec::new();
            self.share_modal_visible = false;
            self.pending_share_source = None;

            // Initialize TTS parameters
            self.tts_speed = 1.0;
            self.tts_pitch = 0.0;
            self.tts_volume = 100.0;
            self.tts_slider_dragging = None;
            self.controls_panel_tab = 1;
            self.apply_controls_panel_tab_visibility(cx);
            self.update_settings_tabs(cx);
            self.update_sidebar_nav_states(cx);

            // Initialize global settings state (default locale is zh)
            self.global_settings_visible = false;
            self.app_language = i18n::get_locale();
            if i18n::set_locale(&self.app_language).is_err() {
                self.app_language = "zh".to_string();
                let _ = i18n::set_locale("zh");
            }
            
            // Initialize clone tasks state
            self.clone_tasks = Vec::new();
            self.clone_loading = false;
            self.clone_card_areas = Vec::new();
            self.current_clone_mode = CloneMode::Express;
            self.training_executor = Some(TrainingExecutor::new());

            // Mark any interrupted (Processing) tasks as Failed
            let _ = task_persistence::mark_stale_tasks_as_failed();

            // Load initial data
            self.load_voice_library(cx);
            self.load_clone_tasks(cx);
            self.sync_selected_model_ui(cx);
            self.update_voice_picker_controls(cx);
            self.sync_selected_voice_ui(cx);
            self.update_history_display(cx);
            self.update_tts_param_controls(cx);
            self.update_language_options(cx);
            self.update_theme_options(cx);
            self.apply_localization(cx);
            self.apply_dark_mode(cx);
            
            // Add initial log entries
            self.log_entries
                .push("[INFO] [tts] Moxin TTS initialized".to_string());
            self.log_entries
                .push("[INFO] [tts] Default voice: Doubao (GPT-SoVITS)".to_string());
            self.log_entries
                .push(format!(
                    "[INFO] [tts] Model catalog loaded: {} model(s)",
                    self.model_options.len()
                ));
            self.log_entries
                .push("[INFO] [tts] Click 'Start' to connect to Moxin bridge".to_string());
            // Update log display immediately
            self.update_log_display(cx);

            // Collapse log panel by default
            self.log_panel_collapsed = true;
            self.log_panel_width = 320.0;
            self.view
                .view(ids!(content_wrapper.main_content.log_section))
                .apply_over(cx, live! { width: Fit });
            self.view
                .view(ids!(
                    content_wrapper.main_content.log_section.log_content_column
                ))
                .set_visible(cx, false);
            self.view
                .button(ids!(
                    content_wrapper
                        .main_content
                        .log_section
                        .toggle_column
                        .toggle_log_btn
                ))
                .set_text(cx, "<");
            self.view
                .view(ids!(content_wrapper.main_content.splitter))
                .apply_over(cx, live! { width: 0 });
        }

        // Initialize Dora and auto-start dataflow
        if self.dora.is_none() {
            let dora = DoraIntegration::new();
            self.dora = Some(dora);
        }

        // NOTE: Dataflow auto-start is DISABLED to fix connection issues.
        // Users must start dataflow MANUALLY before running the app:
        //   1. dora up (if daemon not running)
        //   2. dora start apps/moxin-voice/dataflow/tts.yml
        //   3. Wait for "Running" status with 6 nodes
        //   4. cargo run -p moxin-voice
        //
        // This fixes the "Failed to connect to Dora" error that occurs when
        // the app tries to manage dataflow internally.
        if !self.dora_started {
            self.dora_started = true;
            // Auto-start dataflow on app launch
            self.auto_start_dataflow(cx);
        }

        // Pass SharedDoraState to voice clone modal
        if let Some(ref dora) = self.dora {
            let shared_state = dora.shared_dora_state().clone();
            self.view
                .voice_clone_modal(ids!(voice_clone_modal))
                .set_shared_dora_state(shared_state);
        }

        // Handle toast timer (auto-hide after delay)
        if self.toast_timer.is_event(event).is_some() {
            self.hide_toast(cx);
        }

        // Poll for audio and logs
        if self.update_timer.is_event(event).is_some() {
            // Poll Dora Audio - store audio samples instead of auto-playing
            if let Some(dora) = &self.dora {
                if dora.is_running() {
                    let shared = dora.shared_dora_state();
                    let chunks = shared.audio.drain();
                    if !chunks.is_empty() {
                        for audio in chunks {
                            self.stored_audio_samples.extend(&audio.samples);
                            self.stored_audio_sample_rate = audio.sample_rate;
                        }
                        self.rebuild_processed_audio_samples();
                        // Transition to Ready state - user must click Play
                        if self.tts_status == TTSStatus::Generating {
                            let sample_count = self.effective_audio_samples().len();
                            let duration_secs = if self.stored_audio_sample_rate > 0 {
                                sample_count as f32 / self.stored_audio_sample_rate as f32
                            } else {
                                0.0
                            };
                            self.add_log(
                                cx,
                                &format!(
                                    "[INFO] [tts] Audio generated: {} samples, {:.1}s duration",
                                    sample_count, duration_secs
                                ),
                            );
                            self.tts_status = TTSStatus::Ready;
                            self.has_generated_audio = true;
                            let generated_voice_id = self
                                .pending_generation_voice_id
                                .clone()
                                .or_else(|| self.selected_voice_id.clone());
                            if let Some(voice_id) = generated_voice_id {
                                self.apply_generated_voice_to_player_bar(cx, &voice_id, None);
                            }
                            self.append_current_generation_to_history(cx);
                            self.clear_pending_generation_snapshot();
                            self.set_generate_button_loading(cx, false);
                            self.update_player_bar(cx);
                        }
                    }
                }
            }

            // Update playback progress and check if finished
            if self.tts_status == TTSStatus::Playing {
                if let Some(player) = &self.audio_player {
                    // Check if playback has actually finished (buffer empty)
                    if player.check_playback_finished() {
                        // Audio finished - reset to Ready state
                        self.tts_status = TTSStatus::Ready;
                        self.audio_playing_time = 0.0;
                        self.update_playback_progress(cx);
                        self.update_player_bar(cx);
                        self.add_log(cx, "[INFO] [tts] Playback completed");
                    } else if player.is_playing() {
                        // Still playing - update playback time and progress bar
                        self.audio_playing_time += 0.1;
                        self.update_playback_progress(cx);
                    }
                    // If paused (is_playing=false but not finished), do nothing - keep current time
                }
            }

            // Check if preview playback has finished
            if self.preview_playing_voice_id.is_some() {
                if let Some(player) = &self.preview_player {
                    if player.check_playback_finished() {
                        // Preview finished - reset preview state
                        self.preview_playing_voice_id = None;
                        let voice_selector = self.view.voice_selector(ids!(
                            content_wrapper
                                .main_content
                                .left_column
                                .content_area
                                .tts_page
                                .cards_container
                                .controls_panel
                                .settings_panel
                                .voice_section
                                .voice_selector
                        ));
                        voice_selector.set_preview_playing(cx, None);
                        self.update_voice_picker_controls(cx);
                        self.add_log(cx, "[INFO] [tts] Preview playback finished");
                    }
                }
            }

            // Poll TrainingExecutor for task queue execution
            if let Some(ref mut executor) = self.training_executor {
                if let Some(event) = executor.poll() {
                    use crate::training_executor::ExecutorEvent;
                    match event {
                        ExecutorEvent::TaskCompleted {
                            task_id,
                            task_name,
                            gpt_weights,
                            sovits_weights,
                            reference_audio,
                            reference_text,
                        } => {
                            self.add_log(cx, &format!("[INFO] [executor] Task completed: {}", task_id));

                            // Register the trained voice so it appears in Voice Library
                            // and the voice selector.
                            use crate::voice_data::{Voice, VoiceCategory, VoiceSource};
                            use crate::voice_persistence;
                            let new_voice = Voice {
                                id: task_id.clone(),
                                name: task_name.clone(),
                                description: format!("Trained voice - {}", task_name),
                                category: VoiceCategory::Character,
                                language: "zh".to_string(),
                                preview_audio: Some(reference_audio.to_string_lossy().to_string()),
                                source: VoiceSource::Trained,
                                reference_audio_path: Some(reference_audio.to_string_lossy().to_string()),
                                prompt_text: Some(reference_text.clone()),
                                gpt_weights: Some(gpt_weights.to_string_lossy().to_string()),
                                sovits_weights: Some(sovits_weights.to_string_lossy().to_string()),
                                created_at: Some(
                                    std::time::SystemTime::now()
                                        .duration_since(std::time::UNIX_EPOCH)
                                        .map(|d| d.as_secs())
                                        .unwrap_or(0),
                                ),
                            };

                            // Save to disk (ignore duplicate errors — voice may already exist)
                            let _ = voice_persistence::add_custom_voice(new_voice.clone());

                            // Add/update in-memory library list and refresh Library page
                            if let Some(existing) = self
                                .library_voices
                                .iter_mut()
                                .find(|v| v.id == new_voice.id)
                            {
                                *existing = new_voice.clone();
                            } else {
                                self.library_voices.push(new_voice.clone());
                            }
                            self.update_library_display(cx);
                            self.update_voice_picker_controls(cx);

                            // Add to voice selector
                            let voice_selector = self.view.voice_selector(ids!(
                                content_wrapper
                                    .main_content
                                    .left_column
                                    .content_area
                                    .tts_page
                                    .cards_container
                                    .controls_panel
                                    .settings_panel
                                    .voice_section
                                    .voice_selector
                            ));
                            voice_selector.reload_voices(cx);

                            // Refresh clone task list and detail
                            self.refresh_clone_tasks(cx);
                            if self.current_task_id.as_deref() == Some(&task_id) {
                                self.refresh_task_detail(cx);
                            }

                            self.show_toast(cx, &format!("音色「{}」训练完成，已添加到语音库", task_name));
                        }
                        ExecutorEvent::TaskFailed(task_id, err) => {
                            self.add_log(cx, &format!("[ERROR] [executor] Task failed: {} - {}", task_id, err));
                            self.refresh_clone_tasks(cx);
                            if self.current_task_id.as_deref() == Some(&task_id) {
                                self.refresh_task_detail(cx);
                            }
                        }
                        ExecutorEvent::ProgressUpdated(task_id, _progress) => {
                            // Sync updated task data (progress, sub_step, etc.) into the
                            // in-memory list so the clone-page card redraws correctly.
                            if let Some(fresh) = task_persistence::get_task(&task_id) {
                                if let Some(existing) = self.clone_tasks.iter_mut().find(|t| t.id == task_id) {
                                    *existing = fresh;
                                }
                            }
                            self.view.redraw(cx);
                            // Refresh detail page if currently showing this task
                            if self.current_page == AppPage::TaskDetail
                                && self.current_task_id.as_deref() == Some(&task_id)
                            {
                                self.refresh_task_detail(cx);
                            }
                        }
                    }
                }
            }

            // Poll Logs from log_bridge
            let logs = log_bridge::poll_logs();
            if !logs.is_empty() {
                for log_msg in logs {
                    self.log_entries.push(log_msg.format());
                }
                self.update_log_display(cx);
            }

            // Update loading overlay
            if !self.loading_dismissed {
                // Animate spinner
                self.spinner_phase += 0.03;
                if self.spinner_phase > 1.0 {
                    self.spinner_phase -= 1.0;
                }
                self.view.view(ids!(loading_overlay.loading_content.loading_spinner_area.loading_spinner))
                    .apply_over(cx, live! { draw_bg: { phase: (self.spinner_phase) } });

                // Update status text based on dora state
                let is_running = self.dora.as_ref().map(|d| d.is_running()).unwrap_or(false);
                if is_running {
                    self.view.label(ids!(loading_overlay.loading_content.loading_status))
                        .set_text(cx, self.tr("已连接", "Connected"));
                    self.view.label(ids!(loading_overlay.loading_content.loading_detail))
                        .set_text(cx, self.tr("TTS 引擎已就绪", "TTS engine ready"));
                    // Dismiss loading overlay
                    self.loading_dismissed = true;
                    self.view.view(ids!(loading_overlay)).set_visible(cx, false);
                    self.view.redraw(cx);
                    self.add_log(cx, "[INFO] [tts] Dataflow connected, UI ready");
                } else {
                    self.view.label(ids!(loading_overlay.loading_content.loading_status))
                        .set_text(cx, self.tr("连接中...", "Connecting..."));
                    self.view.label(ids!(loading_overlay.loading_content.loading_detail))
                        .set_text(cx, self.tr("正在启动 TTS 数据流引擎", "Starting TTS dataflow engine"));
                }

                self.view.redraw(cx);
            }
        }

        // MoxinHero actions not used in Moxin UI (dataflow auto-starts)

        // Handle navigation button clicks
        if self
            .view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_tts))
            .clicked(&actions)
        {
            self.controls_panel_tab = 1;
            self.update_settings_tabs(cx);
            self.apply_controls_panel_tab_visibility(cx);
            self.switch_page(cx, AppPage::TextToSpeech);
            self.update_sidebar_nav_states(cx);
        }

        if self
            .view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_library))
            .clicked(&actions)
        {
            // Reset library filters when entering the page to avoid stale empty states.
            self.library_category_filter = VoiceFilter::All;
            self.library_age_filter = 0;
            self.library_style_filter = 0;
            self.library_trait_filter = 0;
            self.library_language_filter = LanguageFilter::All;
            self.library_search_query.clear();
            self.view
                .text_input(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.search_input))
                .set_text(cx, "");
            // Ensure library content is refreshed whenever user opens Voice Library.
            self.load_voice_library(cx);
            self.switch_page(cx, AppPage::VoiceLibrary);
        }

        if self
            .view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_clone))
            .clicked(&actions)
        {
            self.switch_page(cx, AppPage::VoiceClone);
        }

        if self
            .view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_history))
            .clicked(&actions)
        {
            self.controls_panel_tab = 2;
            self.update_settings_tabs(cx);
            self.apply_controls_panel_tab_visibility(cx);
            self.update_history_display(cx);
            self.switch_page(cx, AppPage::TextToSpeech);
            self.update_sidebar_nav_states(cx);
        }

        // Handle global settings button click
        if self
            .view
            .button(ids!(app_layout.sidebar.sidebar_footer.global_settings_btn))
            .clicked(&actions)
        {
            self.global_settings_visible = true;
            self.view.view(ids!(global_settings_modal)).set_visible(cx, true);
            self.update_language_options(cx);
            self.update_theme_options(cx);
            self.apply_localization(cx);
            self.view.redraw(cx);
        }

        // Handle Settings/History tab clicks
        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_tabs.voice_management_tab_btn))
            .clicked(&actions)
        {
            self.controls_panel_tab = 0;
            self.update_settings_tabs(cx);
            self.apply_controls_panel_tab_visibility(cx);
            self.update_sidebar_nav_states(cx);
            self.view.redraw(cx);
        }

        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_tabs.settings_tab_btn))
            .clicked(&actions)
        {
            self.controls_panel_tab = 1;
            self.update_settings_tabs(cx);
            self.apply_controls_panel_tab_visibility(cx);
            self.update_sidebar_nav_states(cx);
            self.view.redraw(cx);
        }

        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_tabs.history_tab_btn))
            .clicked(&actions)
        {
            self.controls_panel_tab = 2;
            self.update_settings_tabs(cx);
            self.apply_controls_panel_tab_visibility(cx);
            self.update_sidebar_nav_states(cx);
            self.update_history_display(cx);
            self.view.redraw(cx);
        }

        if self
            .view
            .button(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .controls_panel
                    .history_panel
                    .history_header
                    .clear_history_btn
            ))
            .clicked(&actions)
        {
            self.clear_tts_history(cx);
        }

        // Handle model picker button click
        if self
            .view
            .button(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .model_row
                    .model_picker_btn
            ))
            .clicked(&actions)
        {
            self.model_options = get_project_tts_models();
            if self.selected_tts_model_id.is_none() {
                self.selected_tts_model_id = self.model_options.first().map(|m| m.id.clone());
            }
            self.model_picker_visible = true;
            self.view.view(ids!(model_picker_modal)).set_visible(cx, true);
            self.update_model_picker_controls(cx);
            self.view.redraw(cx);
        }

        // Handle model picker close button
        if self
            .view
            .button(ids!(model_picker_modal.model_picker_dialog.model_picker_header.model_picker_back_btn))
            .clicked(&actions)
        {
            self.model_picker_visible = false;
            self.view.view(ids!(model_picker_modal)).set_visible(cx, false);
            self.view.redraw(cx);
        }

        // Handle model picker backdrop click
        if self
            .view
            .view(ids!(model_picker_modal.model_picker_backdrop))
            .finger_up(&actions)
            .is_some()
        {
            self.model_picker_visible = false;
            self.view.view(ids!(model_picker_modal)).set_visible(cx, false);
            self.view.redraw(cx);
        }

        // Handle TTS parameter sliders (drag + click-to-jump)
        let speed_slider_area = self
            .view
            .view(ids!(
                content_wrapper.main_content.left_column.content_area.tts_page.cards_container
                    .input_section.bottom_bar.param_controls.speed_row.speed_slider_row.speed_slider
            ))
            .area();
        self.handle_tts_param_slider_event(cx, event, speed_slider_area, TtsParamSliderKind::Speed);

        let pitch_slider_area = self
            .view
            .view(ids!(
                content_wrapper.main_content.left_column.content_area.tts_page.cards_container
                    .input_section.bottom_bar.param_controls.pitch_row.pitch_slider_row.pitch_slider
            ))
            .area();
        self.handle_tts_param_slider_event(cx, event, pitch_slider_area, TtsParamSliderKind::Pitch);

        let volume_slider_area = self
            .view
            .view(ids!(
                content_wrapper.main_content.left_column.content_area.tts_page.cards_container
                    .input_section.bottom_bar.param_controls.volume_row.volume_slider_row.volume_slider
            ))
            .area();
        self.handle_tts_param_slider_event(cx, event, volume_slider_area, TtsParamSliderKind::Volume);

        // Handle voice picker "select voice" click (legacy button + inline selected voice button).
        let legacy_voice_picker_btn_clicked = self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.voice_row.voice_picker_btn))
            .clicked(&actions);
        let inline_selected_voice_btn_clicked = self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.select_voice_row.selected_voice_btn))
            .clicked(&actions);
        if legacy_voice_picker_btn_clicked || inline_selected_voice_btn_clicked {
            // Always refresh so My Voices immediately reflects newly cloned voices.
            self.load_voice_library(cx);
            self.voice_picker_tab = 0;
            self.voice_picker_search.clear();
            self.voice_picker_language_filter = LanguageFilter::All;
            self.clear_voice_picker_tag_filters();
            self.voice_picker_active_voice_id = self.selected_voice_id.clone();
            self.update_voice_picker_controls(cx);
            self.view.redraw(cx);
        }

        // Voice picker chips (multi-select; male/female are mutually exclusive)
        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_gender.gender_male_btn))
            .clicked(&actions)
        {
            let was_active = self.voice_picker_gender_filter == VoiceFilter::Male;
            self.voice_picker_gender_filter = if was_active { VoiceFilter::All } else { VoiceFilter::Male };
            self.update_voice_picker_controls(cx);
        }

        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_gender.gender_female_btn))
            .clicked(&actions)
        {
            let was_active = self.voice_picker_gender_filter == VoiceFilter::Female;
            self.voice_picker_gender_filter = if was_active { VoiceFilter::All } else { VoiceFilter::Female };
            self.update_voice_picker_controls(cx);
        }

        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_style.style_sweet_btn))
            .clicked(&actions)
        {
            const SWEET_BIT: u8 = 0b01;
            if self.voice_picker_style_filter & SWEET_BIT != 0 {
                self.voice_picker_style_filter &= !SWEET_BIT;
            } else {
                self.voice_picker_style_filter |= SWEET_BIT;
            }
            self.update_voice_picker_controls(cx);
        }

        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_style.style_magnetic_btn))
            .clicked(&actions)
        {
            const MAGNETIC_BIT: u8 = 0b10;
            if self.voice_picker_style_filter & MAGNETIC_BIT != 0 {
                self.voice_picker_style_filter &= !MAGNETIC_BIT;
            } else {
                self.voice_picker_style_filter |= MAGNETIC_BIT;
            }
            self.update_voice_picker_controls(cx);
        }

        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_gender.age_adult_btn))
            .clicked(&actions)
        {
            const ADULT_BIT: u8 = 0b01;
            if self.voice_picker_age_filter & ADULT_BIT != 0 {
                self.voice_picker_age_filter &= !ADULT_BIT;
            } else {
                self.voice_picker_age_filter |= ADULT_BIT;
            }
            self.update_voice_picker_controls(cx);
        }

        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_gender.age_youth_btn))
            .clicked(&actions)
        {
            const YOUTH_BIT: u8 = 0b10;
            if self.voice_picker_age_filter & YOUTH_BIT != 0 {
                self.voice_picker_age_filter &= !YOUTH_BIT;
            } else {
                self.voice_picker_age_filter |= YOUTH_BIT;
            }
            self.update_voice_picker_controls(cx);
        }

        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_trait.trait_prof_btn))
            .clicked(&actions)
        {
            const PROF_BIT: u8 = 0b01;
            if self.voice_picker_trait_filter & PROF_BIT != 0 {
                self.voice_picker_trait_filter &= !PROF_BIT;
            } else {
                self.voice_picker_trait_filter |= PROF_BIT;
            }
            self.update_voice_picker_controls(cx);
        }

        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_trait.trait_character_btn))
            .clicked(&actions)
        {
            const CHARACTER_BIT: u8 = 0b10;
            if self.voice_picker_trait_filter & CHARACTER_BIT != 0 {
                self.voice_picker_trait_filter &= !CHARACTER_BIT;
            } else {
                self.voice_picker_trait_filter |= CHARACTER_BIT;
            }
            self.update_voice_picker_controls(cx);
        }

        // Handle global settings modal close button
        if self
            .view
            .button(ids!(global_settings_modal.settings_dialog.settings_header.settings_close_btn))
            .clicked(&actions)
        {
            self.global_settings_visible = false;
            self.view.view(ids!(global_settings_modal)).set_visible(cx, false);
            self.view.redraw(cx);
        }

        // Handle global settings backdrop click
        if self
            .view
            .view(ids!(global_settings_modal.settings_backdrop))
            .finger_up(&actions)
            .is_some()
        {
            self.global_settings_visible = false;
            self.view.view(ids!(global_settings_modal)).set_visible(cx, false);
            self.view.redraw(cx);
        }

        // Handle language selection in global settings
        if self
            .view
            .button(ids!(global_settings_modal.settings_dialog.settings_content.language_section.language_options.lang_en_option))
            .clicked(&actions)
        {
            self.app_language = "en".to_string();
            let _ = i18n::set_locale("en");
            self.update_language_options(cx);
            self.apply_localization(cx);
        }

        if self
            .view
            .button(ids!(global_settings_modal.settings_dialog.settings_content.language_section.language_options.lang_zh_option))
            .clicked(&actions)
        {
            self.app_language = "zh".to_string();
            let _ = i18n::set_locale("zh");
            self.update_language_options(cx);
            self.apply_localization(cx);
        }

        // Handle theme selection in global settings
        if self
            .view
            .button(ids!(global_settings_modal.settings_dialog.settings_content.theme_section.theme_options.theme_light_option))
            .clicked(&actions)
        {
            self.dark_mode = 0.0;
            self.update_theme_options(cx);
            self.apply_dark_mode(cx);
        }

        if self
            .view
            .button(ids!(global_settings_modal.settings_dialog.settings_content.theme_section.theme_options.theme_dark_option))
            .clicked(&actions)
        {
            self.dark_mode = 1.0;
            self.update_theme_options(cx);
            self.apply_dark_mode(cx);
        }

        // Handle share modal controls
        if self.share_modal_visible {
            if self
                .view
                .button(ids!(share_modal.share_dialog.share_footer.share_cancel_btn))
                .clicked(&actions)
            {
                self.close_share_modal(cx);
            }
            if self
                .view
                .view(ids!(share_modal.share_backdrop))
                .finger_up(&actions)
                .is_some()
            {
                self.close_share_modal(cx);
            }
            if self
                .view
                .button(ids!(share_modal.share_dialog.share_actions.share_system_btn))
                .clicked(&actions)
            {
                self.share_audio_to_target(cx, ShareTarget::System);
            }
            if self
                .view
                .button(ids!(share_modal.share_dialog.share_actions.share_capcut_btn))
                .clicked(&actions)
            {
                self.share_audio_to_target(cx, ShareTarget::CapCut);
            }
            if self
                .view
                .button(ids!(share_modal.share_dialog.share_actions.share_premiere_btn))
                .clicked(&actions)
            {
                self.share_audio_to_target(cx, ShareTarget::Premiere);
            }
            if self
                .view
                .button(ids!(share_modal.share_dialog.share_actions.share_wechat_btn))
                .clicked(&actions)
            {
                self.share_audio_to_target(cx, ShareTarget::WeChat);
            }
            if self
                .view
                .button(ids!(share_modal.share_dialog.share_actions.share_finder_btn))
                .clicked(&actions)
            {
                self.share_audio_to_target(cx, ShareTarget::Finder);
            }
        }

        // Handle Voice Library page buttons
        if self
            .view
            .button(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .library_page
                    .library_header
                    .refresh_btn
            ))
            .clicked(&actions)
        {
            self.refresh_voice_library(cx);
        }

        // Handle Voice Library category tags
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_gender.filter_male_btn)).clicked(&actions) {
            let was_active = self.library_category_filter == VoiceFilter::Male;
            self.library_category_filter = if was_active { VoiceFilter::All } else { VoiceFilter::Male };
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_gender.filter_female_btn)).clicked(&actions) {
            let was_active = self.library_category_filter == VoiceFilter::Female;
            self.library_category_filter = if was_active { VoiceFilter::All } else { VoiceFilter::Female };
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_gender.age_adult_btn)).clicked(&actions) {
            const ADULT_BIT: u8 = 0b01;
            if self.library_age_filter & ADULT_BIT != 0 {
                self.library_age_filter &= !ADULT_BIT;
            } else {
                self.library_age_filter |= ADULT_BIT;
            }
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_gender.age_youth_btn)).clicked(&actions) {
            const YOUTH_BIT: u8 = 0b10;
            if self.library_age_filter & YOUTH_BIT != 0 {
                self.library_age_filter &= !YOUTH_BIT;
            } else {
                self.library_age_filter |= YOUTH_BIT;
            }
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_style.style_sweet_btn)).clicked(&actions) {
            const SWEET_BIT: u8 = 0b01;
            if self.library_style_filter & SWEET_BIT != 0 {
                self.library_style_filter &= !SWEET_BIT;
            } else {
                self.library_style_filter |= SWEET_BIT;
            }
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_style.style_magnetic_btn)).clicked(&actions) {
            const MAGNETIC_BIT: u8 = 0b10;
            if self.library_style_filter & MAGNETIC_BIT != 0 {
                self.library_style_filter &= !MAGNETIC_BIT;
            } else {
                self.library_style_filter |= MAGNETIC_BIT;
            }
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_trait.trait_prof_btn)).clicked(&actions) {
            const PROF_BIT: u8 = 0b01;
            if self.library_trait_filter & PROF_BIT != 0 {
                self.library_trait_filter &= !PROF_BIT;
            } else {
                self.library_trait_filter |= PROF_BIT;
            }
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_trait.trait_character_btn)).clicked(&actions) {
            const CHARACTER_BIT: u8 = 0b10;
            if self.library_trait_filter & CHARACTER_BIT != 0 {
                self.library_trait_filter &= !CHARACTER_BIT;
            } else {
                self.library_trait_filter |= CHARACTER_BIT;
            }
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }

        // Handle Voice Library language filter buttons
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.language_filter.lang_all_btn)).clicked(&actions) {
            self.library_language_filter = LanguageFilter::All;
            self.update_language_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.language_filter.lang_zh_btn)).clicked(&actions) {
            self.library_language_filter = LanguageFilter::Chinese;
            self.update_language_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.language_filter.lang_en_btn)).clicked(&actions) {
            self.library_language_filter = LanguageFilter::English;
            self.update_language_filter_buttons(cx);
            self.update_library_display(cx);
        }

        // Handle Voice Library search input
        if let Some(search_text) = self
            .view
            .text_input(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .library_page
                    .library_header
                    .search_input
            ))
            .changed(&actions)
        {
            self.library_search_query = search_text;
            self.update_library_display(cx);
            self.add_log(cx, &format!("[INFO] [library] Search query: {}", self.library_search_query));
        }

        // Handle Voice Clone page mode selector buttons
        if self.view.button(ids!(
            content_wrapper.main_content.left_column.content_area
            .clone_page.clone_header.clone_title_section.mode_selector.quick_mode_btn
        )).clicked(&actions) {
            self.current_clone_mode = CloneMode::Express;
            // Update quick_mode_btn active state
            self.view.button(ids!(
                content_wrapper.main_content.left_column.content_area
                .clone_page.clone_header.clone_title_section.mode_selector.quick_mode_btn
            )).apply_over(cx, live! { draw_bg: { active: 1.0 } draw_text: { active: 1.0 } });
            self.view.button(ids!(
                content_wrapper.main_content.left_column.content_area
                .clone_page.clone_header.clone_title_section.mode_selector.advanced_mode_btn
            )).apply_over(cx, live! { draw_bg: { active: 0.0 } draw_text: { active: 0.0 } });
        }

        if self.view.button(ids!(
            content_wrapper.main_content.left_column.content_area
            .clone_page.clone_header.clone_title_section.mode_selector.advanced_mode_btn
        )).clicked(&actions) {
            self.current_clone_mode = CloneMode::Pro;
            // Update advanced_mode_btn active state
            self.view.button(ids!(
                content_wrapper.main_content.left_column.content_area
                .clone_page.clone_header.clone_title_section.mode_selector.quick_mode_btn
            )).apply_over(cx, live! { draw_bg: { active: 0.0 } draw_text: { active: 0.0 } });
            self.view.button(ids!(
                content_wrapper.main_content.left_column.content_area
                .clone_page.clone_header.clone_title_section.mode_selector.advanced_mode_btn
            )).apply_over(cx, live! { draw_bg: { active: 1.0 } draw_text: { active: 1.0 } });
        }

        // Handle Voice Clone page buttons
        if self
            .view
            .button(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .clone_page
                    .clone_header
                    .create_task_btn
            ))
            .clicked(&actions)
        {
            // Show the voice clone modal in the current mode
            let mode = self.current_clone_mode;
            self.view
                .voice_clone_modal(ids!(voice_clone_modal))
                .show_with_mode(cx, mode);
            self.add_log(cx, "[INFO] [clone] Opening create task dialog...");
        }

        // Handle Task Detail page buttons
        if self.view.button(ids!(
            content_wrapper.main_content.left_column.content_area
            .task_detail_page.detail_header.back_btn
        )).clicked(&actions) {
            self.switch_page(cx, AppPage::VoiceClone);
        }

        if self.view.button(ids!(
            content_wrapper.main_content.left_column.content_area
            .task_detail_page.detail_header.detail_cancel_btn
        )).clicked(&actions) {
            if let Some(task_id) = self.current_task_id.clone() {
                self.cancel_clone_task(cx, task_id.clone());
                self.refresh_task_detail(cx);
            }
        }

        // Handle confirm delete modal buttons
        if self
            .view
            .button(ids!(confirm_delete_modal.dialog.footer.confirm_btn))
            .clicked(&actions)
        {
            // User confirmed deletion
            if let Some(voice_id) = self.pending_delete_voice_id.take() {
                self.delete_voice(cx, voice_id);
                // Hide dialog
                self.view
                    .view(ids!(confirm_delete_modal))
                    .set_visible(cx, false);
            }
            self.pending_delete_voice_name = None;
        }

        if self
            .view
            .button(ids!(confirm_delete_modal.dialog.footer.cancel_btn))
            .clicked(&actions)
        {
            // User cancelled deletion
            self.pending_delete_voice_id = None;
            self.pending_delete_voice_name = None;
            // Hide dialog
            self.view
                .view(ids!(confirm_delete_modal))
                .set_visible(cx, false);
            self.add_log(cx, "[INFO] [library] Delete cancelled");
        }

        // Handle Voice Library card button clicks using stored areas
        let filtered_voices = self.get_filtered_voices();
        for (voice_idx, _card_area, preview_btn_area, delete_btn_area) in self.library_card_areas.clone() {
            if voice_idx >= filtered_voices.len() {
                continue;
            }
            
            let voice = &filtered_voices[voice_idx];
            
            // Check preview button click
            match event.hits(cx, preview_btn_area) {
                Hit::FingerUp(fe) if fe.was_tap() => {
                    self.preview_voice(cx, voice.id.clone());
                }
                _ => {}
            }
            
            // Check delete button click (only for custom voices)
            if voice.source != crate::voice_data::VoiceSource::Builtin {
                match event.hits(cx, delete_btn_area) {
                    Hit::FingerUp(fe) if fe.was_tap() => {
                        // Show confirmation dialog
                        self.pending_delete_voice_id = Some(voice.id.clone());
                        self.pending_delete_voice_name = Some(voice.name.clone());
                        
                        // Update dialog with voice name
                        self.view
                            .label(ids!(confirm_delete_modal.dialog.header.voice_name))
                            .set_text(cx, &format!("\"{}\"", voice.name));
                        
                        // Show dialog
                        self.view
                            .view(ids!(confirm_delete_modal))
                            .set_visible(cx, true);
                        
                        self.add_log(cx, &format!("[INFO] [library] Requesting delete confirmation for: {}", voice.name));
                    }
                    _ => {}
                }
            }
        }

        // Handle Voice Picker item interactions
        if self.controls_panel_tab == 1 {
            let picker_voices = self.get_voice_picker_voices();
            for (item_idx, item_area, play_btn_area) in self.voice_picker_item_areas.clone() {
                if item_idx >= picker_voices.len() {
                    continue;
                }

                let voice = &picker_voices[item_idx];
                let mut handled_play_tap = false;

                // 1) Play button tap: preview only, do not close picker.
                match event.hits(cx, play_btn_area) {
                    Hit::FingerUp(fe) if fe.was_tap() => {
                        self.voice_picker_active_voice_id = Some(voice.id.clone());
                        self.preview_voice(cx, voice.id.clone());
                        self.update_voice_picker_controls(cx);
                        handled_play_tap = true;
                    }
                    _ => {}
                }
                if handled_play_tap {
                    continue;
                }

                // 2) Row tap (outside play button): confirm selection and close picker.
                match event.hits(cx, item_area) {
                    Hit::FingerUp(fe) if fe.was_tap() => {
                        // Guard against edge overlap to avoid accidental close on play-button taps.
                        if play_btn_area.rect(cx).contains(fe.abs) {
                            continue;
                        }
                        self.select_voice(cx, voice.clone());
                        self.voice_picker_active_voice_id = Some(voice.id.clone());
                        self.update_voice_picker_controls(cx);
                    }
                    _ => {}
                }
            }
        }

        // Handle history card interactions
        if self.controls_panel_tab == 2 {
            for (item_idx, card_area, play_area, use_area, download_area, share_area, delete_area) in
                self.history_item_areas.clone()
            {
                if item_idx >= self.tts_history.len() {
                    continue;
                }
                let entry_id = self.tts_history[item_idx].id.clone();
                let mut handled = false;

                match event.hits(cx, play_area) {
                    Hit::FingerUp(fe) if fe.was_tap() => {
                        self.load_history_entry_into_player(cx, &entry_id);
                        handled = true;
                    }
                    _ => {}
                }
                if handled {
                    continue;
                }

                match event.hits(cx, use_area) {
                    Hit::FingerUp(fe) if fe.was_tap() => {
                        self.reuse_history_entry(cx, &entry_id);
                        handled = true;
                    }
                    _ => {}
                }
                if handled {
                    continue;
                }

                match event.hits(cx, download_area) {
                    Hit::FingerUp(fe) if fe.was_tap() => {
                        self.download_history_entry(cx, &entry_id);
                        handled = true;
                    }
                    _ => {}
                }
                if handled {
                    continue;
                }

                match event.hits(cx, share_area) {
                    Hit::FingerUp(fe) if fe.was_tap() => {
                        self.open_share_modal(cx, ShareSource::History(entry_id.clone()));
                        handled = true;
                    }
                    _ => {}
                }
                if handled {
                    continue;
                }

                match event.hits(cx, delete_area) {
                    Hit::FingerUp(fe) if fe.was_tap() => {
                        self.delete_history_entry(cx, &entry_id);
                        handled = true;
                    }
                    _ => {}
                }
                if handled {
                    continue;
                }

                match event.hits(cx, card_area) {
                    Hit::FingerUp(fe) if fe.was_tap() => {
                        self.load_history_entry_into_player(cx, &entry_id);
                    }
                    _ => {}
                }
            }
        }

        // Handle Model Picker item interactions
        if self.model_picker_visible {
            for (model_idx, card_area) in self.model_picker_card_areas.clone() {
                if model_idx >= self.model_options.len() {
                    continue;
                }
                let model = self.model_options[model_idx].clone();
                match event.hits(cx, card_area) {
                    Hit::FingerUp(fe) if fe.was_tap() => {
                        self.select_tts_model(cx, &model.id);
                        self.model_picker_visible = false;
                        self.view.view(ids!(model_picker_modal)).set_visible(cx, false);
                        self.view.redraw(cx);
                    }
                    _ => {}
                }
            }
        }

        // Handle Clone Task card button clicks using stored areas
        for (task_idx, _card_area, view_btn_area, cancel_btn_area, delete_btn_area) in self.clone_card_areas.clone() {
            if task_idx >= self.clone_tasks.len() {
                continue;
            }

            let task = &self.clone_tasks[task_idx];
            let task_id = task.id.clone();
            let task_name = task.name.clone();
            let task_status = task.status.clone();

            // Check view button click
            match event.hits(cx, view_btn_area) {
                Hit::FingerUp(fe) if fe.was_tap() => {
                    self.view_clone_task(cx, task_id.clone());
                }
                _ => {}
            }

            // Check cancel/stop button click
            if task_status == CloneTaskStatus::Pending || task_status == CloneTaskStatus::Processing {
                match event.hits(cx, cancel_btn_area) {
                    Hit::FingerUp(fe) if fe.was_tap() => {
                        if task_status == CloneTaskStatus::Processing {
                            // Stop the running training
                            if let Some(ref mut executor) = self.training_executor {
                                executor.cancel_current();
                            }
                            let _ = task_persistence::update_task_status(
                                &task_id,
                                CloneTaskStatus::Cancelled,
                                None,
                                None,
                            );
                            self.load_clone_tasks(cx);
                        } else {
                            // Pending task — confirm cancel
                            self.show_cancel_task_confirmation(cx, task_id.clone(), task_name.clone());
                        }
                    }
                    _ => {}
                }
            }

            // Check delete button click (only for terminal states)
            if matches!(task_status, CloneTaskStatus::Completed | CloneTaskStatus::Failed | CloneTaskStatus::Cancelled) {
                match event.hits(cx, delete_btn_area) {
                    Hit::FingerUp(fe) if fe.was_tap() => {
                        let _ = task_persistence::delete_task(&task_id);
                        // If we were viewing this task in the detail page, go back
                        if self.current_task_id.as_deref() == Some(&task_id) {
                            self.current_task_id = None;
                            self.switch_page(cx, AppPage::VoiceClone);
                        }
                        self.load_clone_tasks(cx);
                    }
                    _ => {}
                }
            }
        }

        for action in &actions {

            // Handle voice selector actions
            match action.as_widget_action().cast() {
                VoiceSelectorAction::VoiceSelected(voice_id) => {
                    if let Some(voice) = self
                        .library_voices
                        .iter()
                        .find(|v| v.id == voice_id)
                        .cloned()
                    {
                        self.select_voice(cx, voice);
                    } else {
                        self.selected_voice_id = Some(voice_id.clone());
                        self.sync_selected_voice_ui(cx);
                        self.add_log(cx, &format!("[INFO] [tts] Voice selected: {}", voice_id));
                    }
                }
                VoiceSelectorAction::PreviewRequested(voice_id) => {
                    self.handle_preview_request(cx, &voice_id);
                }
                VoiceSelectorAction::RequestStartDora => {
                    // Dataflow auto-starts in Moxin UI; show status if not ready yet
                    let is_running = self.dora.as_ref().map(|d| d.is_running()).unwrap_or(false);
                    if !is_running {
                        self.show_toast(
                            cx,
                            self.tr(
                                "数据流仍在连接中，请稍候...",
                                "Dataflow is still connecting, please wait...",
                            ),
                        );
                    }
                }
                VoiceSelectorAction::CloneVoiceClicked => {
                    // Show the voice clone modal
                    self.view
                        .voice_clone_modal(ids!(voice_clone_modal))
                        .show(cx);
                    self.add_log(cx, "[INFO] [tts] Opening voice clone dialog...");
                }
                VoiceSelectorAction::RequestDeleteConfirmation(voice_id, voice_name) => {
                    // Show confirmation dialog
                    self.pending_delete_voice_id = Some(voice_id);
                    self.pending_delete_voice_name = Some(voice_name.clone());

                    // Update dialog with voice name
                    self.view
                        .label(ids!(confirm_delete_modal.dialog.header.voice_name))
                        .set_text(cx, &format!("\"{}\"", voice_name));

                    // Update dark mode
                    self.update_delete_modal_dark_mode(cx);

                    // Show modal
                    self.view
                        .view(ids!(confirm_delete_modal))
                        .set_visible(cx, true);
                    self.view.redraw(cx);
                }
                VoiceSelectorAction::DeleteVoiceClicked(voice_id) => {
                    self.delete_voice(cx, voice_id.clone());
                    self.add_log(cx, &format!("[INFO] [tts] Deleted voice: {}", voice_id));
                }
                VoiceSelectorAction::FilterCategoryChanged(_) | VoiceSelectorAction::FilterLanguageChanged(_) => {}
                VoiceSelectorAction::None => {}
            }

            // Handle voice clone modal actions
            match action.as_widget_action().cast() {
                VoiceCloneModalAction::TaskCreated(task) => {
                    // Add to in-memory list, hide empty state, navigate to task detail
                    self.clone_tasks.push(task.clone());
                    self.update_clone_display(cx);
                    self.add_log(cx, &format!("[INFO] [clone] Task created: {}", task.name));
                    let task_id = task.id.clone();
                    self.open_task_detail(cx, task_id);
                }
                VoiceCloneModalAction::VoiceCreated(voice) => {
                    // Sync into in-memory library so My Voices updates immediately.
                    if let Some(existing) = self.library_voices.iter_mut().find(|v| v.id == voice.id) {
                        *existing = voice.clone();
                    } else {
                        self.library_voices.push(voice.clone());
                    }
                    self.update_library_display(cx);
                    self.update_voice_picker_controls(cx);

                    // Keep hidden voice selector in sync for backward compatibility.
                    let voice_selector = self.view.voice_selector(ids!(
                        content_wrapper
                            .main_content
                            .left_column
                            .content_area
                            .tts_page
                            .cards_container
                            .controls_panel
                            .settings_panel
                            .voice_section
                            .voice_selector
                    ));
                    voice_selector.reload_voices(cx);
                    self.add_log(
                        cx,
                        &format!("[INFO] [tts] Voice '{}' created successfully!", voice.name),
                    );

                    // Show toast notification
                    self.show_toast(
                        cx,
                        &format!("Custom voice '{}' created successfully!", voice.name),
                    );
                }
                VoiceCloneModalAction::SendAudioToAsr {
                    samples,
                    sample_rate,
                    language,
                    ..
                } => {
                    // Parent screen handles sending audio to ASR via dora
                    self.add_log(
                        cx,
                        &format!("[INFO] [tts] Sending {} samples to ASR...", samples.len()),
                    );

                    if let Some(ref dora) = self.dora {
                        if dora.is_running() {
                            if dora.send_audio(samples, sample_rate, language) {
                                self.add_log(cx, "[INFO] [tts] Audio sent to ASR successfully");
                            } else {
                                self.add_log(cx, "[ERROR] [tts] Failed to send audio to ASR");
                            }
                        } else {
                            self.add_log(
                                cx,
                                "[ERROR] [tts] Dataflow is not running - cannot send audio to ASR",
                            );
                        }
                    } else {
                        self.add_log(cx, "[ERROR] [tts] Dora integration not initialized");
                    }
                }
                VoiceCloneModalAction::Closed => {
                    // Modal closed, nothing to do
                }
                VoiceCloneModalAction::None => {}
            }

            // Handle confirm delete modal buttons
            if self
                .view
                .button(ids!(confirm_delete_modal.dialog.footer.confirm_btn))
                .clicked(&actions)
            {
                // User confirmed - proceed with deletion
                if let Some(voice_id) = self.pending_delete_voice_id.take() {
                    self.pending_delete_voice_name = None;

                    self.delete_voice(cx, voice_id.clone());
                    self.add_log(
                        cx,
                        &format!("[INFO] [tts] Voice '{}' deleted successfully", voice_id),
                    );
                }
                // Hide modal
                self.view
                    .view(ids!(confirm_delete_modal))
                    .set_visible(cx, false);
                self.view.redraw(cx);
            }

            if self
                .view
                .button(ids!(confirm_delete_modal.dialog.footer.cancel_btn))
                .clicked(&actions)
            {
                // User cancelled - just hide dialog
                self.pending_delete_voice_id = None;
                self.pending_delete_voice_name = None;
                self.view
                    .view(ids!(confirm_delete_modal))
                    .set_visible(cx, false);
                self.view.redraw(cx);
            }

            // Click backdrop to close dialog
            if self
                .view
                .view(ids!(confirm_delete_modal.backdrop))
                .finger_up(&actions)
                .is_some()
            {
                self.pending_delete_voice_id = None;
                self.pending_delete_voice_name = None;
                self.view
                    .view(ids!(confirm_delete_modal))
                    .set_visible(cx, false);
                self.view.redraw(cx);
            }
        }

        // Handle confirm cancel task modal
        {
            if self
                .view
                .button(ids!(confirm_cancel_modal.dialog.footer.confirm_btn))
                .clicked(&actions)
            {
                // User confirmed - cancel the task
                if let Some(task_id) = self.pending_cancel_task_id.take() {
                    self.cancel_clone_task(cx, task_id);
                }
                self.pending_cancel_task_name = None;
                // Hide modal
                self.view
                    .view(ids!(confirm_cancel_modal))
                    .set_visible(cx, false);
                self.view.redraw(cx);
            }

            if self
                .view
                .button(ids!(confirm_cancel_modal.dialog.footer.back_btn))
                .clicked(&actions)
            {
                // User cancelled - just hide dialog
                self.pending_cancel_task_id = None;
                self.pending_cancel_task_name = None;
                self.view
                    .view(ids!(confirm_cancel_modal))
                    .set_visible(cx, false);
                self.add_log(cx, "[INFO] [clone] Cancel task cancelled");
                self.view.redraw(cx);
            }

            // Click backdrop to close dialog
            if self
                .view
                .view(ids!(confirm_cancel_modal.backdrop))
                .finger_up(&actions)
                .is_some()
            {
                self.pending_cancel_task_id = None;
                self.pending_cancel_task_name = None;
                self.view
                    .view(ids!(confirm_cancel_modal))
                    .set_visible(cx, false);
                self.view.redraw(cx);
            }
        }

        // Handle text input changes
        if self
            .view
            .text_input(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .input_container
                    .text_input
            ))
            .changed(&actions)
            .is_some()
        {
            self.update_char_count(cx);
        }

        // Handle generate button
        if self
            .view
            .button(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .generate_section
                    .generate_btn
            ))
            .clicked(&actions)
        {
            // Check if dora is running and bridges are ready
            if let Some(ref dora) = self.dora {
                if !dora.is_running() {
                    // Not started yet
                    self.show_toast(
                        cx,
                        self.tr(
                            "请先点击“Start Moxin”按钮初始化数据流",
                            "Please click 'Start Moxin' button first to initialize the dataflow",
                        ),
                    );
                } else {
                    // Check if bridges are ready (expected: 4 bridges)
                    let bridge_count = dora.shared_dora_state().status.read().active_bridges.len();
                    if bridge_count < 4 {
                        // Starting but not ready
                        self.show_toast(
                            cx,
                            &if self.is_english() {
                                format!(
                                    "Dataflow is starting ({}/4 bridges connected), please wait...",
                                    bridge_count
                                )
                            } else {
                                format!("数据流启动中（已连接 {}/4 个桥接），请稍候...", bridge_count)
                            },
                        );
                        self.add_log(cx, &format!("[WARN] [tts] Bridges not ready yet: {}/4", bridge_count));
                    } else {
                        // Ready to generate
                        self.generate_speech(cx);
                    }
                }
            } else {
                self.show_toast(
                    cx,
                    self.tr("Dora 集成尚未初始化", "Dora integration not initialized"),
                );
            }
        }

        // Handle play button in audio player bar
        if self
            .view
            .button(ids!(
                content_wrapper
                    .audio_player_bar
                    .playback_controls
                    .controls_row
                    .play_btn
            ))
            .clicked(&actions)
        {
            self.toggle_playback(cx);
        }

        // Handle stop button in audio player bar
        if self
            .view
            .button(ids!(
                content_wrapper
                    .audio_player_bar
                    .playback_controls
                    .controls_row
                    .stop_btn
            ))
            .clicked(&actions)
        {
            self.stop_playback(cx);
        }

        // Handle download button in audio player bar
        if self
            .view
            .button(ids!(
                content_wrapper
                    .audio_player_bar
                    .download_section
                    .download_btn
            ))
            .clicked(&actions)
        {
            self.download_audio(cx);
        }

        // Handle share button in audio player bar
        if self
            .view
            .button(ids!(
                content_wrapper
                    .audio_player_bar
                    .download_section
                    .share_btn
            ))
            .clicked(&actions)
        {
            self.open_share_modal(cx, ShareSource::CurrentAudio);
        }

        // Handle clear logs
        if self
            .view
            .button(ids!(
                main_content
                    .log_section
                    .log_content_column
                    .log_header
                    .log_title_row
                    .clear_log_btn
            ))
            .clicked(&actions)
        {
            self.log_entries.clear();
            self.update_log_display(cx);
        }

        // Handle toggle log panel button
        if self
            .view
            .button(ids!(
                content_wrapper
                    .main_content
                    .log_section
                    .toggle_column
                    .toggle_log_btn
            ))
            .clicked(&actions)
        {
            self.toggle_log_panel(cx);
        }

        // Handle splitter
        let splitter = self.view.view(ids!(content_wrapper.main_content.splitter));
        match event.hits(cx, splitter.area()) {
            Hit::FingerDown(_) => {
                self.splitter_dragging = true;
            }
            Hit::FingerMove(fm) => {
                if self.splitter_dragging {
                    self.resize_log_panel(cx, fm.abs.x);
                }
            }
            Hit::FingerUp(_) => {
                self.splitter_dragging = false;
            }
            _ => {}
        }
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        // Clear card areas before redrawing
        self.library_card_areas.clear();
        self.clone_card_areas.clear();
        self.model_picker_card_areas.clear();
        self.voice_picker_item_areas.clear();
        self.history_item_areas.clear();

        // Get UIDs of our PortalLists using full paths to avoid name collisions
        let voice_list_uid = self.view.portal_list(ids!(
            content_wrapper.main_content.left_column.content_area.library_page.voice_list
        )).widget_uid();
        let task_list_uid = self.view.portal_list(ids!(
            content_wrapper.main_content.left_column.content_area.clone_page.task_portal_list
        )).widget_uid();
        let voice_picker_list_uid = self
            .view
            .portal_list(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_picker_list))
            .widget_uid();
        let model_picker_list_uid = self
            .view
            .portal_list(ids!(model_picker_modal.model_picker_dialog.model_picker_list))
            .widget_uid();
        let history_list_uid = self
            .view
            .portal_list(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .controls_panel
                    .history_panel
                    .history_list
            ))
            .widget_uid();

        // Draw PortalLists
        while let Some(item) = self.view.draw_walk(cx, scope, walk).step() {
            if let Some(mut list) = item.as_portal_list().borrow_mut() {
                let list_id = list.widget_uid();
                if list_id == voice_list_uid {
                    // Render voice cards
                    let filtered_voices = self.get_filtered_voices();
                    list.set_item_range(cx, 0, filtered_voices.len());

                    while let Some(item_id) = list.next_visible_item(cx) {
                        if item_id < filtered_voices.len() {
                            let voice = &filtered_voices[item_id];
                            let initial = voice.name.chars().next().unwrap_or('?').to_string();
                            let name = voice.name.clone();
                            let language = match voice.language.as_str() {
                                "zh" => self.tr("中文", "Chinese").to_string(),
                                "en" => self.tr("英文", "English").to_string(),
                                _ => voice.language.clone(),
                            };
                            let source = voice.source.clone();
                            let type_text = match source {
                                crate::voice_data::VoiceSource::Builtin => self.tr("内置", "Built-in"),
                                crate::voice_data::VoiceSource::Custom => self.tr("自定义", "Custom"),
                                crate::voice_data::VoiceSource::Trained => self.tr("训练", "Trained"),
                            };
                            let is_custom = source != crate::voice_data::VoiceSource::Builtin;
                            let dark_mode = self.dark_mode;

                            let card = list.item(cx, item_id, live_id!(VoiceCard));

                            // Set voice data
                            card.label(ids!(avatar.avatar_initial)).set_text(cx, &initial);
                            card.label(ids!(voice_info.voice_name)).set_text(cx, &name);
                            card.label(ids!(voice_info.voice_meta.voice_language)).set_text(cx, &language);
                            card.label(ids!(voice_info.voice_meta.voice_type)).set_text(cx, type_text);
                            card.button(ids!(actions.preview_btn))
                                .set_text(cx, self.tr("预览", "Preview"));
                            card.button(ids!(actions.delete_btn))
                                .set_text(cx, self.tr("删除", "Delete"));

                            // Apply dark mode
                            card.apply_over(cx, live! {
                                draw_bg: { dark_mode: (dark_mode) }
                            });

                            // Show delete button only for custom/trained voices
                            card.button(ids!(actions.delete_btn)).set_visible(cx, is_custom);

                            // Draw the card
                            card.draw_all(cx, &mut Scope::empty());

                            // Store card areas for hit testing (valid after draw_all)
                            let card_area = card.area();
                            let preview_area = card.button(ids!(actions.preview_btn)).area();
                            let delete_area = card.button(ids!(actions.delete_btn)).area();
                            self.library_card_areas.push((item_id, card_area, preview_area, delete_area));
                        }
                    }
                } else if list_id == task_list_uid {
                    // Render task cards
                    let task_count = self.clone_tasks.len();
                    list.set_item_range(cx, 0, task_count);

                    while let Some(item_id) = list.next_visible_item(cx) {
                        if item_id < task_count {
                            // Clone data before borrowing card
                            let task_name = self.clone_tasks[item_id].name.clone();
                            let task_status = self.clone_tasks[item_id].status.clone();
                            let task_progress = self.clone_tasks[item_id].progress;
                            let task_created = self.clone_tasks[item_id].created_at.clone();
                            let dark_mode = self.dark_mode;

                            let (status_text, status_color) = match task_status {
                                CloneTaskStatus::Completed => (self.tr("已完成", "Completed"), vec4(0.16, 0.65, 0.37, 1.0)),
                                CloneTaskStatus::Processing => (self.tr("处理中", "Processing"), vec4(0.39, 0.40, 0.95, 1.0)),
                                CloneTaskStatus::Pending => (self.tr("排队中", "Pending"), vec4(0.6, 0.6, 0.65, 1.0)),
                                CloneTaskStatus::Failed => (self.tr("失败", "Failed"), vec4(0.8, 0.2, 0.2, 1.0)),
                                CloneTaskStatus::Cancelled => (self.tr("已取消", "Cancelled"), vec4(0.5, 0.5, 0.5, 1.0)),
                            };

                            let card = list.item(cx, item_id, live_id!(TaskCard));

                            card.label(ids!(top_row.task_name)).set_text(cx, &task_name);
                            card.label(ids!(top_row.status_badge.status_label)).set_text(cx, status_text);
                            card.view(ids!(top_row.status_badge)).apply_over(cx, live! {
                                draw_bg: { status_color: (status_color) }
                            });

                            let show_progress = task_status == CloneTaskStatus::Processing;
                            card.view(ids!(progress_row)).set_visible(cx, show_progress);
                            if show_progress {
                                let progress_text = if self.is_english() {
                                    format!("Progress: {:.0}%", task_progress * 100.0)
                                } else {
                                    format!("进度：{:.0}%", task_progress * 100.0)
                                };
                                card.label(ids!(progress_row.progress_text)).set_text(cx, &progress_text);
                                card.view(ids!(progress_row.progress_bar)).apply_over(cx, live! {
                                    draw_bg: { progress: (task_progress as f64) }
                                });
                            }

                            card.label(ids!(bottom_row.task_meta.created_time)).set_text(cx, &task_created);

                            // Show cancel/stop for pending/processing; show delete for terminal states
                            let can_cancel = task_status == CloneTaskStatus::Pending;
                            let can_stop = task_status == CloneTaskStatus::Processing;
                            let can_delete = matches!(task_status, CloneTaskStatus::Completed | CloneTaskStatus::Failed | CloneTaskStatus::Cancelled);

                            // Reuse cancel_btn for both Pending (Cancel) and Processing (Stop)
                            card.button(ids!(bottom_row.actions.cancel_btn)).set_visible(cx, can_cancel || can_stop);
                            if can_stop {
                                card.button(ids!(bottom_row.actions.cancel_btn))
                                    .set_text(cx, self.tr("停止", "Stop"));
                            } else {
                                card.button(ids!(bottom_row.actions.cancel_btn))
                                    .set_text(cx, self.tr("取消", "Cancel"));
                            }
                            card.button(ids!(bottom_row.actions.delete_btn)).set_visible(cx, can_delete);
                            card.button(ids!(bottom_row.actions.view_btn))
                                .set_text(cx, self.tr("查看", "View"));
                            card.button(ids!(bottom_row.actions.delete_btn))
                                .set_text(cx, self.tr("删除", "Delete"));

                            card.apply_over(cx, live! {
                                draw_bg: { dark_mode: (dark_mode) }
                            });

                            // Draw the card
                            card.draw_all(cx, &mut Scope::empty());

                            // Store card areas for hit testing (valid after draw_all)
                            let card_area = card.area();
                            let view_area = card.button(ids!(bottom_row.actions.view_btn)).area();
                            let cancel_area = card.button(ids!(bottom_row.actions.cancel_btn)).area();
                            let delete_area = card.button(ids!(bottom_row.actions.delete_btn)).area();
                            self.clone_card_areas.push((item_id, card_area, view_area, cancel_area, delete_area));
                        }
                    }
                } else if list_id == voice_picker_list_uid {
                    // Render voice picker entries (Built-in/My Voices + search/filters)
                    let picker_voices = self.get_voice_picker_voices();
                    list.set_item_range(cx, 0, picker_voices.len());

                    while let Some(item_id) = list.next_visible_item(cx) {
                        if item_id < picker_voices.len() {
                            let voice = &picker_voices[item_id];
                            let name = Self::single_line_text(&voice.name);
                            let desc = self.localized_voice_description(voice);
                            let initial = name.chars().next().unwrap_or('?').to_string();
                            let highlighted_voice_id = self
                                .voice_picker_active_voice_id
                                .as_ref()
                                .or(self.selected_voice_id.as_ref());
                            let is_highlighted = highlighted_voice_id == Some(&voice.id);
                            let selected = if is_highlighted { 1.0 } else { 0.0 };
                            let is_playing =
                                self.preview_playing_voice_id.as_ref() == Some(&voice.id);
                            let playing = if is_playing { 1.0 } else { 0.0 };
                            let dark_mode = self.dark_mode;

                            let item = list.item(cx, item_id, live_id!(VoicePickerItem));
                            item.label(ids!(picker_avatar.picker_initial))
                                .set_text(cx, &initial);
                            item.label(ids!(picker_info.picker_name)).set_text(cx, &name);
                            item.label(ids!(picker_info.picker_desc)).set_text(cx, &desc);

                            item.apply_over(
                                cx,
                                live! {
                                    draw_bg: { dark_mode: (dark_mode), selected: (selected) }
                                },
                            );
                            item.label(ids!(picker_info.picker_name)).apply_over(
                                cx,
                                live! {
                                    draw_text: { dark_mode: (dark_mode) }
                                },
                            );
                            item.label(ids!(picker_info.picker_desc)).apply_over(
                                cx,
                                live! {
                                    draw_text: { dark_mode: (dark_mode) }
                                },
                            );
                            item.view(ids!(picker_play_btn)).apply_over(
                                cx,
                                live! {
                                    draw_bg: { dark_mode: (dark_mode), playing: (playing) }
                                },
                            );

                            item.draw_all(cx, &mut Scope::empty());

                            let item_area = item.area();
                            let play_area = item.view(ids!(picker_play_btn)).area();
                            self.voice_picker_item_areas
                                .push((item_id, item_area, play_area));
                        }
                    }
                } else if list_id == history_list_uid {
                    // Render TTS generation history cards
                    let history_count = self.tts_history.len();
                    list.set_item_range(cx, 0, history_count);

                    while let Some(item_id) = list.next_visible_item(cx) {
                        if item_id < history_count {
                            let entry = &self.tts_history[item_id];
                            let dark_mode = self.dark_mode;
                            let created_text = self.format_history_time(entry.created_at);
                            let duration_text = Self::format_duration(entry.duration_secs);
                            let model_name = entry
                                .model_name
                                .clone()
                                .unwrap_or_else(|| self.tr("默认模型", "Default Model").to_string());

                            let card = list.item(cx, item_id, live_id!(HistoryCard));
                            card.label(ids!(top_row.left_info.voice_name))
                                .set_text(cx, &entry.voice_name);
                            card.label(ids!(top_row.left_info.created_time))
                                .set_text(cx, &created_text);
                            card.label(ids!(text_preview))
                                .set_text(cx, &entry.text_preview);
                            card.label(ids!(meta_row.model_name))
                                .set_text(cx, &model_name);
                            card.label(ids!(meta_row.duration))
                                .set_text(cx, &duration_text);

                            card.button(ids!(actions_row.play_btn))
                                .set_text(cx, self.tr("播放", "Play"));
                            card.button(ids!(actions_row.use_btn))
                                .set_text(cx, self.tr("复用", "Reuse"));
                            card.button(ids!(actions_row.download_btn))
                                .set_text(cx, self.tr("下载", "Download"));
                            card.button(ids!(actions_row.share_btn))
                                .set_text(cx, self.tr("分享", "Share"));
                            card.button(ids!(actions_row.delete_btn))
                                .set_text(cx, self.tr("删除", "Delete"));

                            card.apply_over(
                                cx,
                                live! {
                                    draw_bg: { dark_mode: (dark_mode) }
                                },
                            );
                            card.label(ids!(top_row.left_info.voice_name)).apply_over(
                                cx,
                                live! { draw_text: { dark_mode: (dark_mode) } },
                            );
                            card.label(ids!(top_row.left_info.created_time)).apply_over(
                                cx,
                                live! { draw_text: { dark_mode: (dark_mode) } },
                            );
                            card.label(ids!(text_preview)).apply_over(
                                cx,
                                live! { draw_text: { dark_mode: (dark_mode) } },
                            );
                            card.label(ids!(meta_row.model_name)).apply_over(
                                cx,
                                live! { draw_text: { dark_mode: (dark_mode) } },
                            );
                            card.label(ids!(meta_row.duration)).apply_over(
                                cx,
                                live! { draw_text: { dark_mode: (dark_mode) } },
                            );
                            card.button(ids!(actions_row.use_btn)).apply_over(
                                cx,
                                live! {
                                    draw_bg: { dark_mode: (dark_mode) }
                                    draw_text: { dark_mode: (dark_mode) }
                                },
                            );
                            card.button(ids!(actions_row.download_btn)).apply_over(
                                cx,
                                live! {
                                    draw_bg: { dark_mode: (dark_mode) }
                                    draw_text: { dark_mode: (dark_mode) }
                                },
                            );
                            card.button(ids!(actions_row.share_btn)).apply_over(
                                cx,
                                live! {
                                    draw_bg: { dark_mode: (dark_mode) }
                                    draw_text: { dark_mode: (dark_mode) }
                                },
                            );
                            card.button(ids!(actions_row.delete_btn)).apply_over(
                                cx,
                                live! {
                                    draw_bg: { dark_mode: (dark_mode) }
                                    draw_text: { dark_mode: (dark_mode) }
                                },
                            );

                            card.draw_all(cx, &mut Scope::empty());

                            let card_area = card.area();
                            let play_area = card.button(ids!(actions_row.play_btn)).area();
                            let use_area = card.button(ids!(actions_row.use_btn)).area();
                            let download_area = card.button(ids!(actions_row.download_btn)).area();
                            let share_area = card.button(ids!(actions_row.share_btn)).area();
                            let delete_area = card.button(ids!(actions_row.delete_btn)).area();
                            self.history_item_areas.push((
                                item_id,
                                card_area,
                                play_area,
                                use_area,
                                download_area,
                                share_area,
                                delete_area,
                            ));
                        }
                    }
                } else if list_id == model_picker_list_uid {
                    // Render model picker entries
                    let model_count = self.model_options.len();
                    list.set_item_range(cx, 0, model_count);

                    while let Some(item_id) = list.next_visible_item(cx) {
                        if item_id < model_count {
                            let model = &self.model_options[item_id];
                            let is_selected =
                                self.selected_tts_model_id.as_ref() == Some(&model.id);
                            let selected = if is_selected { 1.0 } else { 0.0 };
                            let dark_mode = self.dark_mode;

                            let card = list.item(cx, item_id, live_id!(ModelPickerCard));
                            card.label(ids!(model_top.model_name))
                                .set_text(cx, &model.name);
                            card.label(ids!(model_desc))
                                .set_text(cx, &self.localized_model_description(model));

                            let localized_badge = self.localized_model_badge(model);
                            let badge_visible = localized_badge
                                .as_ref()
                                .map(|s| !s.trim().is_empty())
                                .unwrap_or(false);
                            card.view(ids!(model_top.model_badge))
                                .set_visible(cx, badge_visible);
                            if let Some(badge) = &localized_badge {
                                card.label(ids!(model_top.model_badge.model_badge_label))
                                    .set_text(cx, badge);
                            }

                            let localized_tags = self.localized_model_tags(model);
                            let tag_1 = localized_tags.get(0).cloned().unwrap_or_default();
                            let tag_2 = localized_tags.get(1).cloned().unwrap_or_default();
                            let tag_3 = localized_tags.get(2).cloned().unwrap_or_default();

                            card.view(ids!(model_tags.tag_1))
                                .set_visible(cx, !tag_1.is_empty());
                            card.view(ids!(model_tags.tag_2))
                                .set_visible(cx, !tag_2.is_empty());
                            card.view(ids!(model_tags.tag_3))
                                .set_visible(cx, !tag_3.is_empty());

                            if !tag_1.is_empty() {
                                card.label(ids!(model_tags.tag_1.tag_label))
                                    .set_text(cx, &tag_1);
                            }
                            if !tag_2.is_empty() {
                                card.label(ids!(model_tags.tag_2.tag_label))
                                    .set_text(cx, &tag_2);
                            }
                            if !tag_3.is_empty() {
                                card.label(ids!(model_tags.tag_3.tag_label))
                                    .set_text(cx, &tag_3);
                            }

                            card.apply_over(
                                cx,
                                live! {
                                    draw_bg: { dark_mode: (dark_mode), selected: (selected) }
                                },
                            );
                            card.label(ids!(model_top.model_name)).apply_over(
                                cx,
                                live! { draw_text: { dark_mode: (dark_mode) } },
                            );
                            card.label(ids!(model_desc)).apply_over(
                                cx,
                                live! { draw_text: { dark_mode: (dark_mode) } },
                            );
                            card.view(ids!(model_top.model_badge)).apply_over(
                                cx,
                                live! { draw_bg: { dark_mode: (dark_mode) } },
                            );
                            card.label(ids!(model_top.model_badge.model_badge_label))
                                .apply_over(
                                    cx,
                                    live! { draw_text: { dark_mode: (dark_mode) } },
                                );
                            card.view(ids!(model_top.model_check)).apply_over(
                                cx,
                                live! { draw_bg: { selected: (selected) } },
                            );

                            card.draw_all(cx, &mut Scope::empty());

                            let card_area = card.area();
                            self.model_picker_card_areas.push((item_id, card_area));
                        }
                    }
                }
            }
        }

        DrawStep::done()
    }
}

impl TTSScreen {
    fn add_log(&mut self, cx: &mut Cx, message: &str) {
        self.log_entries.push(message.to_string());
        self.update_log_display(cx);
    }

    fn is_english(&self) -> bool {
        self.app_language == "en"
    }

    fn tr<'a>(&self, zh: &'a str, en: &'a str) -> &'a str {
        if self.is_english() {
            en
        } else {
            zh
        }
    }

    fn localized_model_description(&self, model: &TtsModelOption) -> String {
        if self.is_english() {
            return model.description.clone();
        }
        match model.id.as_str() {
            "primespeech-gsv2" => "当前项目生产可用的 TTS 流水线，支持内置音色和克隆音色。".to_string(),
            _ => model.description.clone(),
        }
    }

    fn localized_model_badge(&self, model: &TtsModelOption) -> Option<String> {
        let badge = model.badge.as_ref()?;
        if self.is_english() {
            return Some(badge.clone());
        }
        let mapped = match badge.as_str() {
            "Available" => "可用",
            _ => badge.as_str(),
        };
        Some(mapped.to_string())
    }

    fn localized_model_tags(&self, model: &TtsModelOption) -> Vec<String> {
        if self.is_english() {
            return model.tag_labels.clone();
        }
        model
            .tag_labels
            .iter()
            .map(|tag| match tag.as_str() {
                "Chinese" => "中文".to_string(),
                "English" => "英文".to_string(),
                "Voice Clone" => "音色克隆".to_string(),
                _ => tag.clone(),
            })
            .collect()
    }

    fn localized_voice_description(&self, voice: &Voice) -> String {
        if self.is_english() {
            return voice.description.clone();
        }

        let builtin_zh = match voice.id.as_str() {
            "Doubao" => Some("中文 · 混合风格，自然且富有表现力"),
            "Luo Xiang" => Some("中文男声 · 法学教授风格，表达清晰，富有思辨"),
            "Yang Mi" => Some("中文女声 · 演员风格，甜美自然"),
            "Zhou Jielun" => Some("中文男声 · 歌手风格，辨识度高"),
            "Ma Yun" => Some("中文男声 · 创业者风格，自信有力"),
            "Chen Yifan" => Some("中文男声 · 分析师风格，专业稳重"),
            "Zhao Daniu" => Some("中文男声 · 播客主持风格，叙述感强"),
            "BYS" => Some("中文 · 日常风格，轻松亲切"),
            "Ma Baoguo" => Some("中文男声 · 武术大师风格，个性鲜明"),
            "Shen Yi" => Some("中文男声 · 教授风格，分析性强"),
            "Maple" => Some("英文女声 · 讲述风格，温暖柔和"),
            "Cove" => Some("英文男声 · 解说风格，清晰专业"),
            "Ellen" => Some("英文女声 · 脱口秀主持风格，节奏活跃"),
            "Juniper" => Some("英文女声 · 旁白风格，平稳舒缓"),
            "Trump" => Some("英文男声 · 个性化说话风格"),
            _ => None,
        };
        if let Some(desc) = builtin_zh {
            return desc.to_string();
        }

        if let Some(rest) = voice.description.strip_prefix("Custom voice - ") {
            return format!("自定义音色 - {}", rest);
        }
        if let Some(rest) = voice.description.strip_prefix("Trained voice - ") {
            return format!("训练音色 - {}", rest);
        }

        let mut desc = voice.description.clone();
        let replacements = [
            ("Chinese", "中文"),
            ("English", "英文"),
            ("male", "男声"),
            ("female", "女声"),
            ("mixed style", "混合风格"),
            ("natural and expressive", "自然且富有表现力"),
            ("law professor", "法学教授"),
            ("articulate and thoughtful", "表达清晰且有思辨"),
            ("actress", "演员"),
            ("sweet and charming", "甜美且亲和"),
            ("singer", "歌手"),
            ("unique and distinctive", "个性鲜明"),
            ("entrepreneur", "创业者"),
            ("confident speaker", "表达自信"),
            ("analyst", "分析师"),
            ("professional tone", "专业语气"),
            ("podcast host", "播客主持"),
            ("engaging narrator", "叙述感强"),
            ("casual and friendly", "轻松亲切"),
            ("martial arts master", "武术大师"),
            ("professor", "教授"),
            ("analytical tone", "分析性语气"),
            ("storyteller", "讲述风格"),
            ("warm and gentle", "温暖柔和"),
            ("commentator", "解说风格"),
            ("clear and professional", "清晰专业"),
            ("talk show host", "脱口秀主持"),
            ("energetic", "富有活力"),
            ("narrator", "旁白风格"),
            ("calm and soothing", "平稳舒缓"),
            ("distinctive speaking style", "个性化说话风格"),
        ];
        for (en, zh) in replacements {
            desc = desc.replace(en, zh);
        }
        desc
    }

    fn apply_localization(&mut self, cx: &mut Cx) {
        let en = self.is_english();

        self.view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_tts))
            .set_text(cx, self.tr("📝 文本转语音", "📝 Text to Speech"));
        self.view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_library))
            .set_text(cx, self.tr("🎤 音色库", "🎤 Voice Library"));
        self.view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_clone))
            .set_text(cx, self.tr("📋 音色克隆", "📋 Voice Clone"));
        self.view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_history))
            .set_text(cx, self.tr("🕘 历史", "🕘 History"));
        self.view
            .label(ids!(app_layout.sidebar.sidebar_footer.user_details.user_name))
            .set_text(cx, self.tr("用户", "User"));

        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.tts_page.page_header.page_title
            ))
            .set_text(cx, self.tr("文本转语音", "Text to Speech"));
        self.view
            .text_input(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .input_container
                    .text_input
            ))
            .apply_over(
                cx,
                live! {
                    empty_text: (if en { "Enter text to convert to speech..." } else { "请输入要转换的文本..." })
                },
            );
        self.view
            .button(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .controls_panel
                    .settings_tabs
                    .voice_management_tab_btn
            ))
            .set_text(cx, self.tr("音色管理", "Voice"));
        self.view
            .button(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .controls_panel
                    .settings_tabs
                    .settings_tab_btn
            ))
            .set_text(cx, self.tr("参数", "Controls"));
        self.view
            .button(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .controls_panel
                    .settings_tabs
                    .history_tab_btn
            ))
            .set_text(cx, self.tr("历史", "History"));
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .controls_panel
                    .settings_panel
                    .voice_row
                    .voice_label
            ))
            .set_text(cx, self.tr("音色", "Voice"));
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .model_row
                    .model_label
            ))
            .set_text(cx, self.tr("模型", "Model"));
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .speed_row
                    .speed_header
                    .speed_label
            ))
            .set_text(cx, self.tr("语速", "Speed"));
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .speed_row
                    .speed_slider_row
                    .speed_min_slot
                    .slower_label
            ))
            .set_text(cx, self.tr("更慢", "Slower"));
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .speed_row
                    .speed_slider_row
                    .speed_max_slot
                    .faster_label
            ))
            .set_text(cx, self.tr("更快", "Faster"));
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .pitch_row
                    .pitch_header
                    .pitch_label
            ))
            .set_text(cx, self.tr("音调", "Pitch"));
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .pitch_row
                    .pitch_slider_row
                    .pitch_min_slot
                    .lower_label
            ))
            .set_text(cx, self.tr("更低", "Lower"));
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .pitch_row
                    .pitch_slider_row
                    .pitch_max_slot
                    .higher_label
            ))
            .set_text(cx, self.tr("更高", "Higher"));
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .volume_row
                    .volume_header
                    .volume_label
            ))
            .set_text(cx, self.tr("音量", "Volume"));
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .volume_row
                    .volume_slider_row
                    .volume_min_slot
                    .quiet_label
            ))
            .set_text(cx, self.tr("更轻", "Quiet"));
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .volume_row
                    .volume_slider_row
                    .volume_max_slot
                    .loud_label
            ))
            .set_text(cx, self.tr("更响", "Loud"));
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .controls_panel
                    .history_panel
                    .history_empty
                    .history_empty_text
            ))
            .set_text(cx, self.tr("暂无生成历史", "No generation history yet"));

        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .library_page
                    .library_header
                    .title_and_tags
                    .library_title
            ))
            .set_text(cx, self.tr("音色库", "Voice Library"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_gender.row_label
            ))
            .set_text(cx, self.tr("性别年龄", "Gender/Age"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_gender.filter_male_btn
            ))
            .set_text(cx, self.tr("男声", "Male"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_gender.filter_female_btn
            ))
            .set_text(cx, self.tr("女声", "Female"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_gender.age_adult_btn
            ))
            .set_text(cx, self.tr("成年", "Adult"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_gender.age_youth_btn
            ))
            .set_text(cx, self.tr("青年", "Youth"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_style.row_label
            ))
            .set_text(cx, self.tr("风格", "Style"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_style.style_sweet_btn
            ))
            .set_text(cx, self.tr("甜美", "Sweet"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_style.style_magnetic_btn
            ))
            .set_text(cx, self.tr("磁性", "Magnetic"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_trait.row_label
            ))
            .set_text(cx, self.tr("声音特质", "Traits"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_trait.trait_prof_btn
            ))
            .set_text(cx, self.tr("专业播音", "Pro Voice"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_trait.trait_character_btn
            ))
            .set_text(cx, self.tr("特色人物", "Character"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.language_filter.lang_all_btn
            ))
            .set_text(cx, self.tr("全部语言", "All Lang"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.language_filter.lang_zh_btn
            ))
            .set_text(cx, self.tr("中文", "Chinese"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.language_filter.lang_en_btn
            ))
            .set_text(cx, self.tr("英文", "English"));
        self.view
            .text_input(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.search_input
            ))
            .apply_over(
                cx,
                live! {
                    empty_text: (if en { "Search voices..." } else { "搜索音色..." })
                },
            );
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.refresh_btn
            ))
            .set_text(cx, self.tr("刷新", "Refresh"));

        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.clone_page.clone_header.clone_title_section.clone_title
            ))
            .set_text(cx, self.tr("音色克隆", "Voice Clone"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.clone_page.clone_header.clone_title_section.mode_selector.quick_mode_btn
            ))
            .set_text(cx, self.tr("快速模式", "Quick Mode"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.clone_page.clone_header.clone_title_section.mode_selector.advanced_mode_btn
            ))
            .set_text(cx, self.tr("高级模式", "Advanced Mode"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.clone_page.clone_header.create_task_btn
            ))
            .set_text(cx, self.tr("➕ 创建任务", "➕ Create Task"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.clone_page.clone_empty_state.clone_empty_text
            ))
            .set_text(
                cx,
                self.tr(
                    "暂无训练任务，点击「创建任务」开始",
                    "No training tasks yet. Click \"Create Task\" to begin.",
                ),
            );

        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_header.back_btn
            ))
            .set_text(cx, self.tr("← 返回", "← Back"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_header.detail_task_name
            ))
            .set_text(cx, self.tr("任务详情", "Task Detail"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_header.detail_cancel_btn
            ))
            .set_text(cx, self.tr("取消任务", "Cancel Task"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_info_card.detail_info_title
            ))
            .set_text(cx, self.tr("任务信息", "Task Info"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_info_card.detail_times_row.detail_created_section.detail_created_title
            ))
            .set_text(cx, self.tr("创建时间", "Created"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_info_card.detail_times_row.detail_started_section.detail_started_title
            ))
            .set_text(cx, self.tr("开始时间", "Started"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_info_card.detail_times_row.detail_completed_section.detail_completed_title
            ))
            .set_text(cx, self.tr("完成时间", "Completed"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.detail_progress_title
            ))
            .set_text(cx, self.tr("训练进度", "Training Progress"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_1_row.stage_1_name
            ))
            .set_text(cx, self.tr("音频切片", "Audio Slicing"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_2_row.stage_2_name
            ))
            .set_text(cx, self.tr("语音识别", "ASR"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_3_row.stage_3_name
            ))
            .set_text(cx, self.tr("文本特征", "Text Features"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_4_row.stage_4_name
            ))
            .set_text(cx, self.tr("HuBERT特征", "HuBERT Features"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_5_row.stage_5_name
            ))
            .set_text(cx, self.tr("语义Token", "Semantic Tokens"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_6_row.stage_6_name
            ))
            .set_text(cx, self.tr("SoVITS训练", "SoVITS Training"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_7_row.stage_7_name
            ))
            .set_text(cx, self.tr("GPT训练", "GPT Training"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_8_row.stage_8_name
            ))
            .set_text(cx, self.tr("推理测试", "Inference Test"));

        self.view
            .label(ids!(
                content_wrapper.main_content.log_section.log_content_column.log_title_row.log_title_label
            ))
            .set_text(cx, self.tr("系统日志", "System Log"));
        self.view
            .button(ids!(
                content_wrapper.main_content.log_section.log_content_column.log_title_row.clear_log_btn
            ))
            .set_text(cx, self.tr("清空", "Clear"));

        self.view
            .button(ids!(content_wrapper.audio_player_bar.download_section.download_btn))
            .set_text(cx, self.tr("下载", "Download"));
        self.view
            .button(ids!(content_wrapper.audio_player_bar.download_section.share_btn))
            .set_text(cx, self.tr("分享", "Share"));

        self.view
            .label(ids!(share_modal.share_dialog.share_header.share_title))
            .set_text(cx, self.tr("分享音频", "Share audio"));
        self.view
            .label(ids!(share_modal.share_dialog.share_header.share_subtitle))
            .set_text(
                cx,
                self.tr(
                    "选择目标应用进行分享",
                    "Choose a target app to share this audio",
                ),
            );
        self.view
            .button(ids!(share_modal.share_dialog.share_actions.share_system_btn))
            .set_text(cx, self.tr("系统打开", "Open with system app"));
        self.view
            .button(ids!(share_modal.share_dialog.share_actions.share_capcut_btn))
            .set_text(cx, self.tr("打开剪映", "Open CapCut (manual import)"));
        self.view
            .button(ids!(share_modal.share_dialog.share_actions.share_premiere_btn))
            .set_text(cx, self.tr("分享到 Premiere Pro", "Share to Premiere Pro"));
        self.view
            .button(ids!(share_modal.share_dialog.share_actions.share_wechat_btn))
            .set_text(cx, self.tr("打开微信", "Open WeChat (manual send)"));
        self.view
            .button(ids!(share_modal.share_dialog.share_actions.share_finder_btn))
            .set_text(cx, self.tr("在访达中显示", "Reveal in Finder"));
        self.view
            .button(ids!(share_modal.share_dialog.share_footer.share_cancel_btn))
            .set_text(cx, self.tr("取消", "Cancel"));

        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.select_voice_row.select_voice_title))
            .set_text(cx, self.tr("选择音色", "Select Voice"));
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_gender.tag_group_label))
            .set_text(cx, self.tr("性别年龄", "Gender/Age"));
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_style.tag_group_label))
            .set_text(cx, self.tr("风格", "Style"));
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_trait.tag_group_label))
            .set_text(cx, self.tr("声音特质", "Traits"));
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_gender.gender_male_btn))
            .set_text(cx, self.tr("男声", "Male"));
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_gender.gender_female_btn))
            .set_text(cx, self.tr("女声", "Female"));
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_gender.age_adult_btn))
            .set_text(cx, self.tr("成年", "Adult"));
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_gender.age_youth_btn))
            .set_text(cx, self.tr("青年", "Youth"));
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_style.style_sweet_btn))
            .set_text(cx, self.tr("甜美", "Sweet"));
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_style.style_magnetic_btn))
            .set_text(cx, self.tr("磁性", "Magnetic"));
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_trait.trait_prof_btn))
            .set_text(cx, self.tr("专业播音", "Pro Voice"));
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_trait.trait_character_btn))
            .set_text(cx, self.tr("特色人物", "Character"));

        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_header.settings_title))
            .set_text(cx, self.tr("⚙️ 设置", "⚙️ Settings"));
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.language_section.language_title))
            .set_text(cx, self.tr("🌐 语言", "🌐 Language"));
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.theme_section.theme_title))
            .set_text(cx, self.tr("🎨 主题", "🎨 Theme"));
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.about_section.about_title))
            .set_text(cx, self.tr("ℹ️ 关于", "ℹ️ About"));
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.about_section.about_engine))
            .set_text(cx, self.tr("基于 GPT-SoVITS v2", "Powered by GPT-SoVITS v2"));
        self.view
            .button(ids!(global_settings_modal.settings_dialog.settings_content.language_section.language_options.lang_en_option))
            .set_text(cx, "English");
        self.view
            .button(ids!(global_settings_modal.settings_dialog.settings_content.language_section.language_options.lang_zh_option))
            .set_text(cx, if en { "Chinese" } else { "中文" });
        self.view
            .button(ids!(global_settings_modal.settings_dialog.settings_content.theme_section.theme_options.theme_light_option))
            .set_text(cx, self.tr("☀️ 浅色", "☀️ Light"));
        self.view
            .button(ids!(global_settings_modal.settings_dialog.settings_content.theme_section.theme_options.theme_dark_option))
            .set_text(cx, self.tr("🌙 深色", "🌙 Dark"));

        self.view
            .label(ids!(loading_overlay.loading_content.loading_subtitle))
            .set_text(cx, self.tr("音色克隆与文本转语音", "Voice Cloning & Text-to-Speech"));
        self.view
            .label(ids!(loading_overlay.loading_content.loading_status))
            .set_text(cx, self.tr("初始化中...", "Initializing..."));
        self.view
            .label(ids!(loading_overlay.loading_content.loading_detail))
            .set_text(cx, self.tr("正在启动 TTS 数据流引擎", "Starting TTS dataflow engine"));

        self.view
            .label(ids!(confirm_delete_modal.dialog.header.title))
            .set_text(cx, self.tr("删除音色？", "Delete Voice?"));
        self.view
            .label(ids!(confirm_delete_modal.dialog.header.message))
            .set_text(cx, self.tr("此操作不可撤销。", "This action cannot be undone."));
        self.view
            .button(ids!(confirm_delete_modal.dialog.footer.cancel_btn))
            .set_text(cx, self.tr("取消", "Cancel"));
        self.view
            .button(ids!(confirm_delete_modal.dialog.footer.confirm_btn))
            .set_text(cx, self.tr("删除", "Delete"));
        self.view
            .label(ids!(confirm_cancel_modal.dialog.header.title))
            .set_text(cx, self.tr("取消任务？", "Cancel Task?"));
        self.view
            .label(ids!(confirm_cancel_modal.dialog.header.message))
            .set_text(
                cx,
                self.tr(
                    "任务将被停止且无法恢复。",
                    "The task will be stopped and cannot be resumed.",
                ),
            );
        self.view
            .button(ids!(confirm_cancel_modal.dialog.footer.back_btn))
            .set_text(cx, self.tr("返回", "Go Back"));
        self.view
            .button(ids!(confirm_cancel_modal.dialog.footer.confirm_btn))
            .set_text(cx, self.tr("取消任务", "Cancel Task"));

        self.update_char_count(cx);
        self.set_generate_button_loading(cx, self.tts_status == TTSStatus::Generating);
        self.update_player_bar(cx);
        self.update_library_display(cx);
        self.update_history_display(cx);
        self.sync_selected_voice_ui(cx);
        self.sync_selected_model_ui(cx);
        self.update_voice_picker_controls(cx);
        self.update_model_picker_controls(cx);
    }

    fn apply_controls_panel_tab_visibility(&mut self, cx: &mut Cx) {
        let show_voice = self.controls_panel_tab == 0;
        let show_settings = self.controls_panel_tab == 1;
        let show_history = self.controls_panel_tab == 2;

        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.voice_management_panel))
            .set_visible(cx, show_voice);
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel))
            .set_visible(cx, show_settings);
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.history_panel))
            .set_visible(cx, show_history);
        self.apply_tts_history_mode_layout(cx);
    }

    fn apply_tts_history_mode_layout(&mut self, cx: &mut Cx) {
        let history_mode =
            self.current_page == AppPage::TextToSpeech && self.controls_panel_tab == 2;

        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.tts_page.page_header))
            .set_visible(cx, !history_mode);
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.input_section))
            .set_visible(cx, !history_mode);
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_tabs))
            .set_visible(cx, !history_mode);

        if history_mode {
            self.view
                .view(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel))
                .apply_over(cx, live! { width: Fill });
        } else {
            self.view
                .view(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel))
                .apply_over(cx, live! { width: 320 });
        }
    }

    fn update_sidebar_nav_states(&mut self, cx: &mut Cx) {
        let tts_context = self.current_page == AppPage::TextToSpeech;
        let tts_active = if tts_context && self.controls_panel_tab != 2 { 1.0 } else { 0.0 };
        let history_active = if tts_context && self.controls_panel_tab == 2 { 1.0 } else { 0.0 };
        let library_active = if self.current_page == AppPage::VoiceLibrary { 1.0 } else { 0.0 };
        let clone_active = if self.current_page == AppPage::VoiceClone || self.current_page == AppPage::TaskDetail {
            1.0
        } else {
            0.0
        };

        self.view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_tts))
            .apply_over(
                cx,
                live! {
                    draw_bg: { active: (tts_active) }
                    draw_text: { active: (tts_active) }
                },
            );
        self.view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_history))
            .apply_over(
                cx,
                live! {
                    draw_bg: { active: (history_active) }
                    draw_text: { active: (history_active) }
                },
            );
        self.view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_library))
            .apply_over(
                cx,
                live! {
                    draw_bg: { active: (library_active) }
                    draw_text: { active: (library_active) }
                },
            );
        self.view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_clone))
            .apply_over(
                cx,
                live! {
                    draw_bg: { active: (clone_active) }
                    draw_text: { active: (clone_active) }
                },
            );
    }

    /// Switch to a different page and update UI accordingly
    fn switch_page(&mut self, cx: &mut Cx, page: AppPage) {
        if self.current_page == page {
            self.update_sidebar_nav_states(cx);
            return; // Already on this page
        }

        self.current_page = page;
        self.add_log(cx, &format!("[INFO] [ui] Switching to {:?} page", page));
        self.update_sidebar_nav_states(cx);

        // Show/hide page content based on current_page
        let show_tts = page == AppPage::TextToSpeech;
        let show_library = page == AppPage::VoiceLibrary;
        let show_clone = page == AppPage::VoiceClone;
        let show_detail = page == AppPage::TaskDetail;

        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
            ))
            .set_visible(cx, show_tts);

        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .library_page
            ))
            .set_visible(cx, show_library);

        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .clone_page
            ))
            .set_visible(cx, show_clone);

        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .task_detail_page
            ))
            .set_visible(cx, show_detail);

        // Show audio player bar only after first successful generation on TTS page
        self.view
            .view(ids!(content_wrapper.audio_player_bar))
            .set_visible(cx, show_tts && self.has_generated_audio);

        if show_library {
            self.update_library_display(cx);
        }

        self.apply_tts_history_mode_layout(cx);
        self.view.redraw(cx);
    }

    fn update_delete_modal_dark_mode(&mut self, cx: &mut Cx) {
        let dark_mode = self.dark_mode;
        self.view
            .view(ids!(confirm_delete_modal.dialog))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .label(ids!(confirm_delete_modal.dialog.header.title))
            .apply_over(
                cx,
                live! {
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .label(ids!(confirm_delete_modal.dialog.header.voice_name))
            .apply_over(
                cx,
                live! {
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .label(ids!(confirm_delete_modal.dialog.header.message))
            .apply_over(
                cx,
                live! {
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .button(ids!(confirm_delete_modal.dialog.footer.cancel_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .button(ids!(confirm_delete_modal.dialog.footer.confirm_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                },
            );
    }

    fn update_char_count(&mut self, cx: &mut Cx) {
        let text = self
            .view
            .text_input(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .input_container
                    .text_input
            ))
            .text();
        let count = text.chars().count();
        let label = if self.is_english() {
            format!("{} / 5,000 characters", count)
        } else {
            format!("{} / 5,000 字符", count)
        };
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .char_count
            ))
            .set_text(cx, &label);
    }

    fn set_generate_button_loading(&mut self, cx: &mut Cx, loading: bool) {
        // Update button text
        let button_text = if loading {
            self.tr("生成中...", "Generating...")
        } else {
            self.tr("生成语音", "Generate Speech")
        };
        self.view
            .button(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .generate_section
                    .generate_btn
            ))
            .set_text(cx, button_text);

        // Show/hide spinner
        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .generate_section
                    .generate_spinner
            ))
            .set_visible(cx, loading);

        // Start/stop spinner animation
        if loading {
            self.view
                .view(ids!(
                    content_wrapper
                        .main_content
                        .left_column
                        .content_area
                        .tts_page
                        .cards_container
                        .input_section
                        .bottom_bar
                        .generate_section
                        .generate_spinner
                ))
                .animator_play(cx, ids!(spin.on));
        } else {
            self.view
                .view(ids!(
                    content_wrapper
                        .main_content
                        .left_column
                        .content_area
                        .tts_page
                        .cards_container
                        .input_section
                        .bottom_bar
                        .generate_section
                        .generate_spinner
                ))
                .animator_play(cx, ids!(spin.off));
        }

        self.view.redraw(cx);
    }

    fn clear_pending_generation_snapshot(&mut self) {
        self.pending_generation_voice_id = None;
        self.pending_generation_text = None;
        self.pending_generation_model_id = None;
        self.pending_generation_model_name = None;
        self.pending_generation_speed = self.tts_speed;
        self.pending_generation_pitch = self.tts_pitch;
        self.pending_generation_volume = self.tts_volume;
    }

    fn update_audio_player_visibility(&mut self, cx: &mut Cx) {
        let show_player =
            self.current_page == AppPage::TextToSpeech && self.has_generated_audio;
        self.view
            .view(ids!(content_wrapper.audio_player_bar))
            .set_visible(cx, show_player);
    }

    fn set_player_bar_voice_labels(&mut self, cx: &mut Cx, voice_name: &str) {
        let initial = voice_name.chars().next().unwrap_or('?').to_string();
        self.current_voice_name = voice_name.to_string();

        self.view
            .label(ids!(
                content_wrapper
                    .audio_player_bar
                    .voice_info
                    .voice_name_container
                    .current_voice_name
            ))
            .set_text(cx, voice_name);
        self.view
            .label(ids!(
                content_wrapper
                    .audio_player_bar
                    .voice_info
                    .voice_avatar
                    .avatar_initial
            ))
            .set_text(cx, &initial);
    }

    fn apply_voice_to_player_bar(
        &mut self,
        cx: &mut Cx,
        voice_id: &str,
        fallback_name: Option<&str>,
    ) {
        let voice_name = self
            .library_voices
            .iter()
            .find(|v| v.id == voice_id)
            .map(|v| v.name.clone())
            .or_else(|| fallback_name.map(|s| s.to_string()))
            .unwrap_or_else(|| voice_id.to_string());
        self.set_player_bar_voice_labels(cx, &voice_name);
    }

    fn apply_generated_voice_to_player_bar(
        &mut self,
        cx: &mut Cx,
        voice_id: &str,
        fallback_name: Option<&str>,
    ) {
        self.generated_voice_id = Some(voice_id.to_string());
        self.apply_voice_to_player_bar(cx, voice_id, fallback_name);
    }

    fn update_player_bar(&mut self, cx: &mut Cx) {
        self.update_audio_player_visibility(cx);

        // Update status label
        let status_text = match self.tts_status {
            TTSStatus::Idle => self.tr("就绪", "Ready"),
            TTSStatus::Generating => self.tr("生成中...", "Generating..."),
            TTSStatus::Playing => self.tr("播放中", "Playing"),
            TTSStatus::Ready => self.tr("音频已生成", "Audio Ready"),
            TTSStatus::Error(ref msg) => msg.as_str(),
        };
        self.view
            .label(ids!(
                content_wrapper
                    .audio_player_bar
                    .voice_info
                    .voice_name_container
                    .status_label
            ))
            .set_text(cx, status_text);

        let audio_len = self.effective_audio_samples().len();
        let has_playable_audio = self.has_generated_audio
            && audio_len > 0
            && self.stored_audio_sample_rate > 0;
        let controls_enabled =
            has_playable_audio && self.tts_status != TTSStatus::Generating;

        self.view
            .button(ids!(
                content_wrapper
                    .audio_player_bar
                    .playback_controls
                    .controls_row
                    .play_btn
            ))
            .set_enabled(cx, controls_enabled);
        self.view
            .button(ids!(
                content_wrapper
                    .audio_player_bar
                    .download_section
                    .download_btn
            ))
            .set_enabled(cx, controls_enabled);
        self.view
            .button(ids!(
                content_wrapper
                    .audio_player_bar
                    .download_section
                    .share_btn
            ))
            .set_enabled(cx, controls_enabled);

        // Update play button state
        let is_playing = self.tts_status == TTSStatus::Playing;
        self.view
            .button(ids!(
                content_wrapper
                    .audio_player_bar
                    .playback_controls
                    .controls_row
                    .play_btn
            ))
            .apply_over(
                cx,
                live! {
                    draw_bg: { is_playing: (if is_playing { 1.0 } else { 0.0 }) }
                },
            );

        // Update total time
        if has_playable_audio {
            let duration_secs = audio_len as f32 / self.stored_audio_sample_rate as f32;
            let mins = (duration_secs / 60.0) as u32;
            let secs = (duration_secs % 60.0) as u32;
            let time_str = format!("{:02}:{:02}", mins, secs);
            self.view
                .label(ids!(
                    content_wrapper
                        .audio_player_bar
                        .playback_controls
                        .progress_row
                        .total_time
                ))
                .set_text(cx, &time_str);
        } else {
            self.view
                .label(ids!(
                    content_wrapper
                        .audio_player_bar
                        .playback_controls
                        .progress_row
                        .current_time
                ))
                .set_text(cx, "00:00");
            self.view
                .label(ids!(
                    content_wrapper
                        .audio_player_bar
                        .playback_controls
                        .progress_row
                        .total_time
                ))
                .set_text(cx, "00:00");
            self.view
                .view(ids!(
                    content_wrapper
                        .audio_player_bar
                        .playback_controls
                        .progress_row
                        .progress_bar_container
                        .progress_bar
                ))
                .apply_over(
                    cx,
                    live! {
                        draw_bg: { progress: 0.0 }
                    },
                );
        }

        self.view.redraw(cx);
    }

    fn format_duration(duration_secs: f32) -> String {
        let total = duration_secs.max(0.0).round() as u32;
        let mins = total / 60;
        let secs = total % 60;
        format!("{:02}:{:02}", mins, secs)
    }

    fn format_history_time(&self, created_at: u64) -> String {
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(created_at);
        let diff = now.saturating_sub(created_at);

        if self.is_english() {
            if diff < 60 {
                "Just now".to_string()
            } else if diff < 3600 {
                format!("{}m ago", diff / 60)
            } else if diff < 86400 {
                format!("{}h ago", diff / 3600)
            } else {
                format!("{}d ago", diff / 86400)
            }
        } else if diff < 60 {
            "刚刚".to_string()
        } else if diff < 3600 {
            format!("{} 分钟前", diff / 60)
        } else if diff < 86400 {
            format!("{} 小时前", diff / 3600)
        } else {
            format!("{} 天前", diff / 86400)
        }
    }

    fn history_text_preview(text: &str) -> String {
        let trimmed = text.trim();
        if trimmed.is_empty() {
            return String::new();
        }
        let max_chars = 90usize;
        if trimmed.chars().count() <= max_chars {
            trimmed.to_string()
        } else {
            let cutoff = trimmed
                .char_indices()
                .nth(max_chars)
                .map(|(idx, _)| idx)
                .unwrap_or(trimmed.len());
            format!("{}...", &trimmed[..cutoff])
        }
    }

    fn persist_tts_history(&mut self, cx: &mut Cx) {
        if let Err(e) = tts_history::save_history(&self.tts_history) {
            self.add_log(cx, &format!("[WARN] [history] Failed to save history: {}", e));
        }
    }

    fn append_current_generation_to_history(&mut self, cx: &mut Cx) {
        let samples = self.effective_audio_samples().to_vec();
        if samples.is_empty() || self.stored_audio_sample_rate == 0 {
            return;
        }

        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default();
        let created_at = now.as_secs();
        let entry_id = format!("gen-{}-{}", created_at, now.subsec_nanos());
        let audio_file = format!("{}.wav", entry_id);
        let audio_path = tts_history::history_audio_path(&audio_file);

        if let Err(e) = tts_history::ensure_history_storage() {
            self.add_log(
                cx,
                &format!("[WARN] [history] Failed to prepare storage: {}", e),
            );
            return;
        }
        if let Err(e) =
            Self::write_wav_file_with_sample_rate(&audio_path, &samples, self.stored_audio_sample_rate)
        {
            self.add_log(
                cx,
                &format!("[WARN] [history] Failed to save audio snapshot: {}", e),
            );
            return;
        }

        let voice_id = self
            .pending_generation_voice_id
            .clone()
            .or_else(|| self.selected_voice_id.clone())
            .unwrap_or_else(|| "unknown".to_string());
        let voice_name = self
            .library_voices
            .iter()
            .find(|v| v.id == voice_id)
            .map(|v| v.name.clone())
            .unwrap_or_else(|| self.current_voice_name.clone());

        let text = self
            .pending_generation_text
            .clone()
            .unwrap_or_else(|| {
                self.view
                    .text_input(ids!(
                        content_wrapper
                            .main_content
                            .left_column
                            .content_area
                            .tts_page
                            .cards_container
                            .input_section
                            .input_container
                            .text_input
                    ))
                    .text()
            });

        let duration_secs = samples.len() as f32 / self.stored_audio_sample_rate as f32;
        let entry = TtsHistoryEntry {
            id: entry_id,
            created_at,
            text: text.clone(),
            text_preview: Self::history_text_preview(&text),
            voice_id,
            voice_name,
            model_id: self.pending_generation_model_id.clone(),
            model_name: self.pending_generation_model_name.clone(),
            duration_secs,
            sample_rate: self.stored_audio_sample_rate,
            sample_count: samples.len(),
            speed: self.pending_generation_speed,
            pitch: self.pending_generation_pitch,
            volume: self.pending_generation_volume,
            audio_file,
        };

        self.tts_history.insert(0, entry);

        if self.tts_history.len() > tts_history::DEFAULT_MAX_HISTORY_ITEMS {
            let removed = self.tts_history.split_off(tts_history::DEFAULT_MAX_HISTORY_ITEMS);
            for old in removed {
                let _ = tts_history::delete_audio_file(&old.audio_file);
            }
        }

        self.persist_tts_history(cx);
        self.update_history_display(cx);
    }

    fn update_history_display(&mut self, cx: &mut Cx) {
        let count = self.tts_history.len();
        let is_empty = count == 0;
        let count_text = if self.is_english() {
            format!("{} item{}", count, if count == 1 { "" } else { "s" })
        } else {
            format!("{} 条记录", count)
        };

        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .controls_panel
                    .history_panel
                    .history_header
                    .history_count
            ))
            .set_text(cx, &count_text);
        self.view
            .button(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .controls_panel
                    .history_panel
                    .history_header
                    .clear_history_btn
            ))
            .set_text(cx, self.tr("清空", "Clear"));
        self.view
            .button(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .controls_panel
                    .history_panel
                    .history_header
                    .clear_history_btn
            ))
            .set_visible(cx, !is_empty);
        self.view
            .portal_list(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .controls_panel
                    .history_panel
                    .history_list
            ))
            .set_visible(cx, !is_empty);
        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .controls_panel
                    .history_panel
                    .history_empty
            ))
            .set_visible(cx, is_empty);
        self.view.redraw(cx);
    }

    fn load_history_entry_into_player(&mut self, cx: &mut Cx, entry_id: &str) {
        if self.tts_status == TTSStatus::Generating {
            self.show_toast(
                cx,
                self.tr("生成中，请稍候再操作历史记录", "Generation in progress, please wait"),
            );
            return;
        }

        let entry = match self.tts_history.iter().find(|h| h.id == entry_id).cloned() {
            Some(v) => v,
            None => return,
        };

        let audio_path = tts_history::history_audio_path(&entry.audio_file);
        if !audio_path.exists() {
            self.add_log(
                cx,
                &format!(
                    "[WARN] [history] Audio file missing for entry {}: {:?}",
                    entry.id, audio_path
                ),
            );
            self.show_toast(
                cx,
                self.tr("历史音频文件不存在", "History audio file not found"),
            );
            return;
        }

        if let Some(player) = &self.audio_player {
            player.stop();
        }

        match self.load_wav_file(&audio_path) {
            Ok(samples) => {
                self.stored_audio_samples = samples;
                self.processed_audio_samples.clear();
                self.stored_audio_sample_rate = entry.sample_rate.max(1);
                self.audio_playing_time = 0.0;
                self.tts_status = TTSStatus::Ready;
                self.has_generated_audio = true;

                self.apply_generated_voice_to_player_bar(
                    cx,
                    &entry.voice_id,
                    Some(&entry.voice_name),
                );
                self.update_playback_progress(cx);
                self.update_player_bar(cx);
                self.add_log(
                    cx,
                    &format!("[INFO] [history] Loaded entry for playback: {}", entry.id),
                );
            }
            Err(e) => {
                self.add_log(
                    cx,
                    &format!("[ERROR] [history] Failed to load audio for {}: {}", entry.id, e),
                );
                self.show_toast(
                    cx,
                    self.tr("加载历史音频失败", "Failed to load history audio"),
                );
            }
        }
    }

    fn reuse_history_entry(&mut self, cx: &mut Cx, entry_id: &str) {
        let entry = match self.tts_history.iter().find(|h| h.id == entry_id).cloned() {
            Some(v) => v,
            None => return,
        };

        self.view
            .text_input(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .input_container
                    .text_input
            ))
            .set_text(cx, &entry.text);
        self.update_char_count(cx);

        if let Some(voice) = self
            .library_voices
            .iter()
            .find(|v| v.id == entry.voice_id)
            .cloned()
        {
            self.select_voice(cx, voice);
        } else {
            self.selected_voice_id = Some(entry.voice_id.clone());
            self.sync_selected_voice_ui(cx);
        }

        if let Some(model_id) = entry.model_id.as_ref() {
            self.select_tts_model(cx, model_id);
        }

        self.tts_speed = entry.speed;
        self.tts_pitch = entry.pitch;
        self.tts_volume = entry.volume;
        self.update_tts_param_controls(cx);

        self.show_toast(
            cx,
            self.tr("已将历史记录填充到编辑区", "History entry loaded into editor"),
        );
    }

    fn download_history_entry(&mut self, cx: &mut Cx, entry_id: &str) {
        let entry = match self.tts_history.iter().find(|h| h.id == entry_id).cloned() {
            Some(v) => v,
            None => return,
        };

        let src = tts_history::history_audio_path(&entry.audio_file);
        if !src.exists() {
            self.show_toast(
                cx,
                self.tr("历史音频文件不存在", "History audio file not found"),
            );
            return;
        }

        let safe_voice: String = entry
            .voice_id
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let filename = format!("tts_history_{}_{}.wav", entry.created_at, safe_voice);
        let target = if let Some(home) = dirs::home_dir() {
            let downloads = home.join("Downloads");
            if downloads.exists() {
                downloads.join(&filename)
            } else {
                PathBuf::from(&filename)
            }
        } else {
            PathBuf::from(&filename)
        };

        match std::fs::copy(&src, &target) {
            Ok(_) => {
                self.add_log(
                    cx,
                    &format!(
                        "[INFO] [history] Exported history audio to {}",
                        target.display()
                    ),
                );
                self.show_toast(
                    cx,
                    self.tr("下载成功！", "Downloaded successfully!"),
                );
            }
            Err(e) => {
                self.add_log(
                    cx,
                    &format!("[ERROR] [history] Failed to export history audio: {}", e),
                );
                self.show_toast(
                    cx,
                    self.tr("下载失败", "Download failed"),
                );
            }
        }
    }

    fn delete_history_entry(&mut self, cx: &mut Cx, entry_id: &str) {
        if let Some(index) = self.tts_history.iter().position(|h| h.id == entry_id) {
            let removed = self.tts_history.remove(index);
            let _ = tts_history::delete_audio_file(&removed.audio_file);
            self.persist_tts_history(cx);
            self.update_history_display(cx);
            self.show_toast(cx, self.tr("历史记录已删除", "History item deleted"));
        }
    }

    fn clear_tts_history(&mut self, cx: &mut Cx) {
        for entry in &self.tts_history {
            let _ = tts_history::delete_audio_file(&entry.audio_file);
        }
        self.tts_history.clear();
        self.persist_tts_history(cx);
        self.update_history_display(cx);
        self.show_toast(cx, self.tr("已清空历史记录", "History cleared"));
    }

    fn update_playback_progress(&mut self, cx: &mut Cx) {
        // Calculate total duration and current position
        let audio_len = self.effective_audio_samples().len();
        if audio_len == 0 || self.stored_audio_sample_rate == 0 {
            return;
        }

        let total_duration = audio_len as f32 / self.stored_audio_sample_rate as f32;
        let current_time = self.audio_playing_time as f32;
        let progress = (current_time / total_duration).min(1.0).max(0.0);

        // Update current time label
        let mins = (current_time / 60.0) as u32;
        let secs = (current_time % 60.0) as u32;
        let time_str = format!("{:02}:{:02}", mins, secs);
        self.view
            .label(ids!(
                content_wrapper
                    .audio_player_bar
                    .playback_controls
                    .progress_row
                    .current_time
            ))
            .set_text(cx, &time_str);

        // Update progress bar
        self.view
            .view(ids!(
                content_wrapper
                    .audio_player_bar
                    .playback_controls
                    .progress_row
                    .progress_bar_container
                    .progress_bar
            ))
            .apply_over(
                cx,
                live! {
                    draw_bg: { progress: (progress as f64) }
                },
            );

        self.view.redraw(cx);
    }

    fn handle_preview_request(&mut self, cx: &mut Cx, voice_id: &str) {
        use crate::voice_data::VoiceSource;
        use crate::voice_persistence;

        // Get the voice selector to check preview audio path
        let voice_selector = self.view.voice_selector(ids!(
            content_wrapper
                .main_content
                .left_column
                .content_area
                .tts_page
                .cards_container
                .controls_panel
                .settings_panel
                .voice_section
                .voice_selector
        ));

        // Check if we're stopping a preview
        if self.preview_playing_voice_id.as_ref() == Some(&voice_id.to_string()) {
            // Stop preview
            if let Some(player) = &self.preview_player {
                player.stop();
            }
            self.preview_playing_voice_id = None;
            voice_selector.set_preview_playing(cx, None);
            self.add_log(cx, &format!("[INFO] [tts] Stopped preview: {}", voice_id));
            return;
        }

        // Stop any currently playing preview
        if let Some(player) = &self.preview_player {
            player.stop();
        }

        // Get voice info
        let voice = match voice_selector.get_voice(voice_id) {
            Some(v) => v,
            None => {
                self.add_log(cx, &format!("[ERROR] [tts] Voice not found: {}", voice_id));
                return;
            }
        };

        // Build the audio path based on voice source
        let audio_path = if voice.source == VoiceSource::Custom {
            // Custom voice: use reference_audio_path from persistence
            match voice_persistence::get_reference_audio_path(&voice) {
                Some(path) => path,
                None => {
                    self.add_log(
                        cx,
                        &format!(
                            "[ERROR] [tts] Custom voice has no reference audio: {}",
                            voice_id
                        ),
                    );
                    return;
                }
            }
        } else {
            // Built-in voice: use preview_audio from models directory
            let preview_file = match &voice.preview_audio {
                Some(f) => f.clone(),
                None => {
                    self.add_log(
                        cx,
                        &format!("[WARN] [tts] No preview audio for: {}", voice_id),
                    );
                    return;
                }
            };

            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(".dora")
                .join("models")
                .join("primespeech")
                .join("moyoyo")
                .join("ref_audios")
                .join(&preview_file)
        };

        if !audio_path.exists() {
            self.add_log(
                cx,
                &format!("[ERROR] [tts] Preview audio not found: {:?}", audio_path),
            );
            return;
        }

        // Load WAV file
        match self.load_wav_file(&audio_path) {
            Ok(samples) => {
                // Initialize preview player if needed
                if self.preview_player.is_none() {
                    self.preview_player = Some(TTSPlayer::new());
                }

                // Play the audio
                if let Some(player) = &self.preview_player {
                    player.write_audio(&samples);
                    player.resume();
                }

                self.preview_playing_voice_id = Some(voice_id.to_string());
                voice_selector.set_preview_playing(cx, Some(voice_id.to_string()));
                self.add_log(
                    cx,
                    &format!(
                        "[INFO] [tts] Playing preview: {} ({:.1}s)",
                        voice_id,
                        samples.len() as f32 / 32000.0
                    ),
                );
            }
            Err(e) => {
                self.add_log(cx, &format!("[ERROR] [tts] Failed to load preview: {}", e));
            }
        }
    }

    fn load_wav_file(&self, path: &PathBuf) -> Result<Vec<f32>, String> {
        let reader = WavReader::open(path).map_err(|e| format!("Failed to open WAV: {}", e))?;
        let spec = reader.spec();
        let sample_rate = spec.sample_rate;
        let channels = spec.channels as usize;

        // Read samples based on format
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => {
                let bits = spec.bits_per_sample;
                let max_val = (1 << (bits - 1)) as f32;
                reader
                    .into_samples::<i32>()
                    .filter_map(Result::ok)
                    .map(|s| s as f32 / max_val)
                    .collect()
            }
            hound::SampleFormat::Float => reader
                .into_samples::<f32>()
                .filter_map(Result::ok)
                .collect(),
        };

        // Convert to mono if stereo
        let mono_samples: Vec<f32> = if channels > 1 {
            samples
                .chunks(channels)
                .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                .collect()
        } else {
            samples
        };

        // Resample to 32000 Hz if needed (TTSPlayer expects 32000 Hz)
        let target_rate = 32000;
        let resampled = if sample_rate != target_rate {
            let ratio = target_rate as f32 / sample_rate as f32;
            let new_len = (mono_samples.len() as f32 * ratio) as usize;
            let mut result = Vec::with_capacity(new_len);
            for i in 0..new_len {
                let src_idx = i as f32 / ratio;
                let idx = src_idx as usize;
                let frac = src_idx - idx as f32;
                let s1 = mono_samples.get(idx).copied().unwrap_or(0.0);
                let s2 = mono_samples.get(idx + 1).copied().unwrap_or(s1);
                result.push(s1 + (s2 - s1) * frac);
            }
            result
        } else {
            mono_samples
        };

        Ok(resampled)
    }

    fn show_toast(&mut self, cx: &mut Cx, message: &str) {
        self.toast_message = message.to_string();
        self.toast_visible = true;

        // Update toast label
        self.view
            .label(ids!(toast_overlay.download_toast.toast_content.toast_label))
            .set_text(cx, message);

        // Show toast
        self.view
            .view(ids!(toast_overlay.download_toast))
            .set_visible(cx, true);

        // Start timer to auto-hide after 3 seconds
        self.toast_timer = cx.start_timeout(3.0);

        self.view.redraw(cx);
    }

    fn hide_toast(&mut self, cx: &mut Cx) {
        self.toast_visible = false;
        self.view
            .view(ids!(toast_overlay.download_toast))
            .set_visible(cx, false);
        self.view.redraw(cx);
    }

    fn update_log_display(&mut self, cx: &mut Cx) {
        let log_text = if self.log_entries.is_empty() {
            self.tr("*暂无日志*", "*No log entries*").to_string()
        } else {
            self.log_entries
                .iter()
                .map(|s| s.as_str())
                .collect::<Vec<_>>()
                .join("\n\n")
        };

        self.view
            .markdown(ids!(
                main_content
                    .log_section
                    .log_content_column
                    .log_scroll
                    .log_content_wrapper
                    .log_content
            ))
            .set_text(cx, &log_text);
        self.view.redraw(cx);
    }

    fn toggle_log_panel(&mut self, cx: &mut Cx) {
        self.log_panel_collapsed = !self.log_panel_collapsed;

        if self.log_panel_width == 0.0 {
            self.log_panel_width = 320.0;
        }

        if self.log_panel_collapsed {
            self.view
                .view(ids!(content_wrapper.main_content.log_section))
                .apply_over(cx, live! { width: Fit });
            self.view
                .view(ids!(
                    content_wrapper.main_content.log_section.log_content_column
                ))
                .set_visible(cx, false);
            self.view
                .button(ids!(
                    content_wrapper
                        .main_content
                        .log_section
                        .toggle_column
                        .toggle_log_btn
                ))
                .set_text(cx, "<");
            self.view
                .view(ids!(content_wrapper.main_content.splitter))
                .apply_over(cx, live! { width: 0 });
        } else {
            let width = self.log_panel_width;
            self.view
                .view(ids!(content_wrapper.main_content.log_section))
                .apply_over(cx, live! { width: (width) });
            self.view
                .view(ids!(
                    content_wrapper.main_content.log_section.log_content_column
                ))
                .set_visible(cx, true);
            self.view
                .button(ids!(
                    content_wrapper
                        .main_content
                        .log_section
                        .toggle_column
                        .toggle_log_btn
                ))
                .set_text(cx, ">");
            self.view
                .view(ids!(content_wrapper.main_content.splitter))
                .apply_over(cx, live! { width: 16 });
        }

        self.view.redraw(cx);
    }

    fn resize_log_panel(&mut self, cx: &mut Cx, abs_x: f64) {
        let container_rect = self.view.area().rect(cx);
        let padding = 16.0;
        let new_log_width = (container_rect.pos.x + container_rect.size.x - abs_x - padding)
            .max(150.0)
            .min(container_rect.size.x - 400.0);

        self.log_panel_width = new_log_width;

        self.view
            .view(ids!(content_wrapper.main_content.log_section))
            .apply_over(cx, live! { width: (new_log_width) });

        self.view.redraw(cx);
    }

    fn auto_start_dataflow(&mut self, cx: &mut Cx) {
        let should_start = self.dora.as_ref().map(|d| !d.is_running()).unwrap_or(false);
        if !should_start {
            return;
        }

        let dataflow_path = std::env::var("MOXIN_DATAFLOW_PATH")
            .map(PathBuf::from)
            .unwrap_or_else(|_| PathBuf::from("apps/moxin-voice/dataflow/tts.yml"));
        if !dataflow_path.exists() {
            self.add_log(
                cx,
                &format!(
                    "[ERROR] [tts] Dataflow file not found: {}",
                    dataflow_path.display()
                ),
            );
            return;
        }

        // Stop any external TTS dataflows first to avoid conflicts
        self.add_log(cx, "[INFO] [tts] Stopping any existing dataflows...");
        let _ = std::process::Command::new("dora")
            .args(["list"])
            .output()
            .map(|output| {
                let output_str = String::from_utf8_lossy(&output.stdout);
                for line in output_str.lines() {
                    if line.contains("Running") {
                        // Extract UUID (first field)
                        let parts: Vec<&str> = line.split_whitespace().collect();
                        if let Some(uuid) = parts.first() {
                            let _ = std::process::Command::new("dora")
                                .args(["stop", uuid])
                                .output();
                        }
                    }
                }
            });

        // Wait a bit for dataflows to stop
        std::thread::sleep(std::time::Duration::from_secs(2));

        self.add_log(cx, "[INFO] [tts] Auto-starting TTS dataflow...");

        if let Some(dora) = &mut self.dora {
            dora.start_dataflow(dataflow_path);
        }

        self.add_log(cx, "[INFO] [tts] Dataflow started, connecting...");
    }

    fn stop_dora(&mut self, cx: &mut Cx) {
        if self.dora.is_none() {
            return;
        }

        self.add_log(cx, "[INFO] [tts] Stopping TTS dataflow...");

        if let Some(dora) = &mut self.dora {
            dora.stop_dataflow();
        }

        self.add_log(cx, "[INFO] [tts] Dataflow stopped");
    }

    fn generate_speech(&mut self, cx: &mut Cx) {
        // Check if Dora is connected
        let is_running = self.dora.as_ref().map(|d| d.is_running()).unwrap_or(false);
        if !is_running {
            self.add_log(
                cx,
                "[WARN] [tts] Bridge not connected. Please start Moxin first.",
            );
            return;
        }

        let text = self
            .view
            .text_input(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .input_container
                    .text_input
            ))
            .text();
        if text.is_empty() {
            self.add_log(
                cx,
                "[WARN] [tts] Please enter some text to convert to speech.",
            );
            return;
        }

        let log_text = match text.char_indices().nth(50) {
            Some((idx, _)) => format!("{}...", &text[..idx]),
            None => text.clone(),
        };
        self.add_log(
            cx,
            &format!("[INFO] [tts] Generating speech for: '{}'", log_text),
        );

        let selected_model = self
            .selected_tts_model_id
            .as_ref()
            .and_then(|id| self.model_options.iter().find(|m| &m.id == id))
            .cloned()
            .or_else(|| self.model_options.first().cloned());
        if let Some(model) = selected_model.as_ref() {
            self.add_log(
                cx,
                &format!("[INFO] [tts] Using model: {} ({})", model.id, model.name),
            );
        }

        let mut voice_id = self
            .selected_voice_id
            .clone()
            .unwrap_or_else(|| "Doubao".to_string());
        let mut voice_info = self
            .library_voices
            .iter()
            .find(|v| v.id == voice_id)
            .cloned();

        // If selected voice is missing (e.g. deleted), fall back to first available built-in voice.
        if voice_info.is_none() {
            if let Some(fallback) = self
                .library_voices
                .iter()
                .find(|v| v.source == crate::voice_data::VoiceSource::Builtin)
                .cloned()
            {
                voice_id = fallback.id.clone();
                voice_info = Some(fallback.clone());
                self.selected_voice_id = Some(voice_id.clone());
                self.sync_selected_voice_ui(cx);
            }
        }

        let previous_generated_voice_id = self.generated_voice_id.clone();
        let previous_voice_name = self.current_voice_name.clone();
        let pending_voice_name = voice_info.as_ref().map(|v| v.name.clone());

        self.add_log(cx, &format!("[INFO] [tts] Using voice: {}", voice_id));
        self.pending_generation_voice_id = Some(voice_id.clone());
        self.pending_generation_text = Some(text.clone());
        self.pending_generation_model_id = selected_model.as_ref().map(|m| m.id.clone());
        self.pending_generation_model_name = selected_model.as_ref().map(|m| m.name.clone());
        self.pending_generation_speed = self.tts_speed;
        self.pending_generation_pitch = self.tts_pitch;
        self.pending_generation_volume = self.tts_volume;
        self.add_log(cx, "========== VOICE DEBUG START ==========");

        // Debug: log voice source
        if let Some(ref v) = voice_info {
            self.add_log(cx, &format!("[DEBUG] [tts] Voice source: {:?}", v.source));
            self.add_log(cx, &format!("[DEBUG] [tts] Has GPT weights: {}", v.gpt_weights.is_some()));
            self.add_log(cx, &format!("[DEBUG] [tts] Has SoVITS weights: {}", v.sovits_weights.is_some()));
        } else {
            self.add_log(cx, "[DEBUG] [tts] Voice info is None - voice not found in library");
        }
        self.add_log(cx, "========== VOICE DEBUG END ==========");

        // Clear previous audio
        self.stored_audio_samples.clear();
        self.processed_audio_samples.clear();
        self.stored_audio_sample_rate = 32000;

        self.tts_status = TTSStatus::Generating;
        self.set_generate_button_loading(cx, true);
        self.apply_voice_to_player_bar(cx, &voice_id, pending_voice_name.as_deref());
        self.update_player_bar(cx);

        // For PrimeSpeech, encode voice selection in prompt using VOICE: prefix
        // The dora-primespeech node will parse this format
        // For custom voices, use extended format: VOICE:CUSTOM|<ref_audio_path>|<prompt_text>|<language>|<text>
        // For trained voices, use: VOICE:TRAINED|<gpt_weights>|<sovits_weights>|<ref_audio>|<prompt_text>|<language>|<text>
        let prompt = if let Some(voice) = voice_info {
            if voice.source == crate::voice_data::VoiceSource::Trained {
                // Trained voice (Pro Mode) - need to send model weights, reference audio, and prompt text
                if let (Some(gpt_weights), Some(sovits_weights), Some(ref_audio), Some(prompt_text)) =
                    (&voice.gpt_weights, &voice.sovits_weights, &voice.reference_audio_path, &voice.prompt_text)
                {
                    self.add_log(
                        cx,
                        &format!("[INFO] [tts] Using trained voice with custom models: {}", voice.name),
                    );
                    self.add_log(
                        cx,
                        &format!("[INFO] [tts] GPT: {}", gpt_weights),
                    );
                    self.add_log(
                        cx,
                        &format!("[INFO] [tts] SoVITS: {}", sovits_weights),
                    );

                    // VOICE:TRAINED|<gpt_weights>|<sovits_weights>|<ref_audio>|<prompt_text>|<language>|<text>
                    format!(
                        "VOICE:TRAINED|{}|{}|{}|{}|{}|{}",
                        gpt_weights, sovits_weights, ref_audio, prompt_text, voice.language, text
                    )
                } else {
                    self.add_log(
                        cx,
                        "[WARN] [tts] Trained voice missing model weights or ref audio, using default",
                    );
                    format!("VOICE:Doubao|{}", text)
                }
            } else if voice.source == crate::voice_data::VoiceSource::Custom {
                // Custom voice (Express Mode) - need to send reference audio path and prompt text
                if let (Some(ref_audio), Some(prompt_text)) =
                    (&voice.reference_audio_path, &voice.prompt_text)
                {
                    // Get absolute path for reference audio
                    let ref_audio_path = crate::voice_persistence::get_reference_audio_path(&voice)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_else(|| ref_audio.clone());

                    self.add_log(
                        cx,
                        &format!("[INFO] [tts] Custom voice ref audio: {}", ref_audio_path),
                    );

                    // Extended format for custom voices (zero-shot)
                    // VOICE:CUSTOM|<ref_audio_path>|<prompt_text>|<language>|<text_to_speak>
                    format!(
                        "VOICE:CUSTOM|{}|{}|{}|{}",
                        ref_audio_path, prompt_text, voice.language, text
                    )
                } else {
                    self.add_log(
                        cx,
                        "[WARN] [tts] Custom voice missing ref audio or prompt text, using default",
                    );
                    format!("VOICE:Doubao|{}", text)
                }
            } else {
                // Built-in voice - use simple format
                format!("VOICE:{}|{}", voice_id, text)
            }
        } else {
            // Voice not found, use default
            format!("VOICE:{}|{}", voice_id, text)
        };

        // Debug: log the formatted prompt (use char boundary safe truncation)
        let prompt_preview = if prompt.chars().count() > 100 {
            let end: usize = prompt.char_indices().nth(100).map(|(i, _)| i).unwrap_or(prompt.len());
            format!("{}...", &prompt[..end])
        } else {
            prompt.clone()
        };
        self.add_log(cx, &format!("[DEBUG] Sending prompt: {}", prompt_preview));

        let payload = serde_json::json!({
            "prompt": prompt,
            "speed": self.pending_generation_speed,
            "pitch": self.pending_generation_pitch,
            "volume": self.pending_generation_volume,
        });
        let payload_text = payload.to_string();

        self.add_log(
            cx,
            &format!(
                "[DEBUG] [tts] Params snapshot: speed={:.2}, pitch={:+.1}, volume={:.0}%",
                self.pending_generation_speed,
                self.pending_generation_pitch,
                self.pending_generation_volume
            ),
        );

        // Send prompt payload to dora.
        let send_result = self
            .dora
            .as_ref()
            .map(|d| d.send_prompt(&payload_text))
            .unwrap_or(false);

        if send_result {
            self.add_log(cx, "[INFO] [tts] Prompt sent to TTS engine");
        } else {
            self.add_log(cx, "[ERROR] [tts] Failed to send prompt to Dora");
            self.clear_pending_generation_snapshot();
            self.tts_status = TTSStatus::Error("Failed to send prompt".to_string());
            if self.has_generated_audio {
                if let Some(prev_voice_id) = previous_generated_voice_id.as_deref() {
                    self.apply_voice_to_player_bar(cx, prev_voice_id, Some(&previous_voice_name));
                } else {
                    self.set_player_bar_voice_labels(cx, &previous_voice_name);
                }
            }
            self.set_generate_button_loading(cx, false);
            self.update_player_bar(cx);
        }

        if let Some(player) = &self.audio_player {
            player.stop();
        }
    }

    fn toggle_playback(&mut self, cx: &mut Cx) {
        if self.tts_status == TTSStatus::Generating {
            self.add_log(
                cx,
                "[INFO] [tts] Playback is disabled while generating new audio",
            );
            self.update_player_bar(cx);
            return;
        }

        if self.tts_status == TTSStatus::Playing {
            // Pause
            if let Some(player) = &self.audio_player {
                player.pause();
            }
            self.tts_status = TTSStatus::Ready;
            self.add_log(cx, &format!("[INFO] [tts] Playback paused at {:.1}s", self.audio_playing_time));
        } else {
            let playback_samples = self.effective_audio_samples().to_vec();
            if playback_samples.is_empty() {
                self.add_log(cx, "[WARN] [tts] No audio to play");
                self.update_player_bar(cx);
                return;
            }

            // Check if we're resuming from a paused state or starting fresh
            let total_duration = playback_samples.len() as f64 / self.stored_audio_sample_rate as f64;
            let is_resuming = self.audio_playing_time > 0.1
                && self.audio_playing_time < (total_duration - 0.1);

            if let Some(player) = &self.audio_player {
                // Always stop and clear buffer first to avoid audio overlap
                player.stop();

                if is_resuming {
                    // Resume from paused position - write remaining audio from current position
                    let current_sample_index = (self.audio_playing_time * self.stored_audio_sample_rate as f64) as usize;
                    if current_sample_index < playback_samples.len() {
                        let remaining_samples = &playback_samples[current_sample_index..];
                        player.write_audio(remaining_samples);
                        player.resume();
                        self.add_log(cx, &format!("[INFO] [tts] Resuming playback from {:.1}s", self.audio_playing_time));
                    }
                } else {
                    // Start from beginning
                    player.write_audio(&playback_samples);
                    player.resume();
                    self.audio_playing_time = 0.0;
                    self.update_playback_progress(cx);
                    self.add_log(cx, "[INFO] [tts] Playing audio...");
                }
            }
            self.tts_status = TTSStatus::Playing;
        }
        self.update_player_bar(cx);
    }

    fn stop_playback(&mut self, cx: &mut Cx) {
        if let Some(player) = &self.audio_player {
            player.stop();
        }
        if self.tts_status == TTSStatus::Playing {
            self.tts_status = TTSStatus::Ready;
            self.add_log(cx, "[INFO] [tts] Playback stopped");
        }
        // Reset progress
        self.view
            .label(ids!(
                content_wrapper
                    .audio_player_bar
                    .playback_controls
                    .progress_row
                    .current_time
            ))
            .set_text(cx, "00:00");
        self.update_player_bar(cx);
    }

    fn open_share_modal(&mut self, cx: &mut Cx, source: ShareSource) {
        if matches!(source, ShareSource::CurrentAudio) {
            if self.tts_status == TTSStatus::Generating {
                self.add_log(cx, "[INFO] [share] Share is disabled while generating new audio");
                self.show_toast(
                    cx,
                    self.tr("生成中，暂不可分享", "Sharing is disabled while generating"),
                );
                return;
            }
            if self.effective_audio_samples().is_empty() {
                self.add_log(cx, "[WARN] [share] No audio available to share");
                self.show_toast(cx, self.tr("暂无可分享音频", "No audio available to share"));
                return;
            }
        }

        self.pending_share_source = Some(source);
        self.share_modal_visible = true;
        self.view.view(ids!(share_modal)).set_visible(cx, true);
        self.view.redraw(cx);
    }

    fn close_share_modal(&mut self, cx: &mut Cx) {
        self.share_modal_visible = false;
        self.pending_share_source = None;
        self.view.view(ids!(share_modal)).set_visible(cx, false);
        self.view.redraw(cx);
    }

    fn share_audio_to_target(&mut self, cx: &mut Cx, target: ShareTarget) {
        let source = match self.pending_share_source.clone() {
            Some(source) => source,
            None => {
                self.add_log(cx, "[WARN] [share] No pending share source");
                return;
            }
        };

        let share_file = match source {
            ShareSource::CurrentAudio => self.prepare_current_audio_share_file(cx),
            ShareSource::History(entry_id) => self.prepare_history_audio_share_file(cx, &entry_id),
        };

        let share_file = match share_file {
            Some(path) => path,
            None => return,
        };

        let shared = Self::launch_share_target(&share_file, target);
        if shared {
            self.add_log(
                cx,
                &format!("[INFO] [share] Shared audio via {}: {}", Self::share_target_key(target), share_file.display()),
            );
            let success_toast = match target {
                ShareTarget::CapCut => self.tr(
                    "已打开剪映并在访达定位音频，请拖拽或手动导入",
                    "CapCut opened and file revealed. Drag it into CapCut or import manually",
                ),
                ShareTarget::WeChat => self.tr(
                    "已打开微信并在访达定位音频，请拖拽到会话发送",
                    "WeChat opened and file revealed. Drag the file into a chat to send",
                ),
                _ => self.tr(
                    "已打开分享目标，请在目标应用中继续发送",
                    "Share target opened. Continue sending in the target app",
                ),
            };
            self.show_toast(cx, success_toast);
            self.close_share_modal(cx);
        } else {
            self.add_log(
                cx,
                &format!(
                    "[ERROR] [share] Failed to launch share target {} for {}",
                    Self::share_target_key(target),
                    share_file.display()
                ),
            );
            self.show_toast(cx, self.tr("分享失败，请检查目标应用", "Share failed, please check target app"));
        }
    }

    fn prepare_current_audio_share_file(&mut self, cx: &mut Cx) -> Option<PathBuf> {
        let samples = self.effective_audio_samples().to_vec();
        if samples.is_empty() || self.stored_audio_sample_rate == 0 {
            self.add_log(cx, "[WARN] [share] Current audio is empty");
            self.show_toast(cx, self.tr("暂无可分享音频", "No audio available to share"));
            return None;
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let safe_voice = Self::sanitize_file_component(&self.current_voice_name);
        let filename = format!("tts_share_{}_{}.wav", safe_voice, timestamp);
        let share_path = Self::export_path_for_filename(&filename);

        match self.write_wav_file(&share_path, &samples) {
            Ok(_) => Some(share_path),
            Err(e) => {
                self.add_log(
                    cx,
                    &format!("[ERROR] [share] Failed to export current audio: {}", e),
                );
                self.show_toast(
                    cx,
                    self.tr("导出分享文件失败", "Failed to export audio for sharing"),
                );
                None
            }
        }
    }

    fn prepare_history_audio_share_file(&mut self, cx: &mut Cx, entry_id: &str) -> Option<PathBuf> {
        let entry = match self.tts_history.iter().find(|h| h.id == entry_id).cloned() {
            Some(v) => v,
            None => {
                self.add_log(
                    cx,
                    &format!("[WARN] [share] History entry not found: {}", entry_id),
                );
                return None;
            }
        };

        let src = tts_history::history_audio_path(&entry.audio_file);
        if !src.exists() {
            self.show_toast(
                cx,
                self.tr("历史音频文件不存在", "History audio file not found"),
            );
            return None;
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(entry.created_at);
        let safe_voice = Self::sanitize_file_component(&entry.voice_name);
        let filename = format!("tts_history_share_{}_{}.wav", timestamp, safe_voice);
        let target = Self::export_path_for_filename(&filename);

        match std::fs::copy(&src, &target) {
            Ok(_) => Some(target),
            Err(e) => {
                self.add_log(
                    cx,
                    &format!(
                        "[ERROR] [share] Failed to export history audio {}: {}",
                        entry.id, e
                    ),
                );
                self.show_toast(
                    cx,
                    self.tr("导出历史音频失败", "Failed to export history audio"),
                );
                None
            }
        }
    }

    fn sanitize_file_component(value: &str) -> String {
        let sanitized: String = value
            .chars()
            .map(|c| if c.is_ascii_alphanumeric() { c } else { '_' })
            .collect();
        let trimmed = sanitized.trim_matches('_');
        if trimmed.is_empty() {
            "audio".to_string()
        } else {
            trimmed.to_string()
        }
    }

    fn export_path_for_filename(filename: &str) -> PathBuf {
        if let Some(home) = dirs::home_dir() {
            let downloads = home.join("Downloads");
            if downloads.exists() {
                return downloads.join(filename);
            }
        }
        PathBuf::from(filename)
    }

    fn share_target_key(target: ShareTarget) -> &'static str {
        match target {
            ShareTarget::System => "system",
            ShareTarget::CapCut => "capcut",
            ShareTarget::Premiere => "premiere",
            ShareTarget::WeChat => "wechat",
            ShareTarget::Finder => "finder",
        }
    }

    fn command_succeeds(command: &mut std::process::Command) -> bool {
        command.status().map(|status| status.success()).unwrap_or(false)
    }

    fn open_path_with_system(path: &PathBuf) -> bool {
        #[cfg(target_os = "macos")]
        {
            let mut cmd = std::process::Command::new("open");
            cmd.arg(path);
            return Self::command_succeeds(&mut cmd);
        }
        #[cfg(target_os = "linux")]
        {
            let mut cmd = std::process::Command::new("xdg-open");
            cmd.arg(path);
            return Self::command_succeeds(&mut cmd);
        }
        #[cfg(target_os = "windows")]
        {
            let mut cmd = std::process::Command::new("cmd");
            cmd.arg("/C").arg("start").arg("").arg(path);
            return Self::command_succeeds(&mut cmd);
        }
        #[allow(unreachable_code)]
        false
    }

    fn reveal_in_file_manager(path: &PathBuf) -> bool {
        #[cfg(target_os = "macos")]
        {
            let mut cmd = std::process::Command::new("open");
            cmd.arg("-R").arg(path);
            return Self::command_succeeds(&mut cmd);
        }
        #[cfg(target_os = "linux")]
        {
            if let Some(parent) = path.parent() {
                let mut cmd = std::process::Command::new("xdg-open");
                cmd.arg(parent);
                return Self::command_succeeds(&mut cmd);
            }
            return false;
        }
        #[cfg(target_os = "windows")]
        {
            let mut cmd = std::process::Command::new("explorer");
            cmd.arg("/select,").arg(path);
            return Self::command_succeeds(&mut cmd);
        }
        #[allow(unreachable_code)]
        false
    }

    fn open_macos_app(path: &PathBuf, app_name: &str) -> bool {
        #[cfg(target_os = "macos")]
        {
            let mut cmd = std::process::Command::new("open");
            cmd.arg("-a").arg(app_name).arg(path);
            return Self::command_succeeds(&mut cmd);
        }
        #[allow(unreachable_code)]
        false
    }

    fn open_with_macos_candidates(path: &PathBuf, candidates: &[&str]) -> bool {
        for app in candidates {
            if Self::open_macos_app(path, app) {
                return true;
            }
        }
        false
    }

    fn open_macos_app_only(app_name: &str) -> bool {
        #[cfg(target_os = "macos")]
        {
            let mut cmd = std::process::Command::new("open");
            cmd.arg("-a").arg(app_name);
            return Self::command_succeeds(&mut cmd);
        }
        #[allow(unreachable_code)]
        false
    }

    fn open_macos_app_only_with_candidates(candidates: &[&str]) -> bool {
        for app in candidates {
            if Self::open_macos_app_only(app) {
                return true;
            }
        }
        false
    }

    fn launch_share_target(path: &PathBuf, target: ShareTarget) -> bool {
        #[cfg(target_os = "macos")]
        {
            return match target {
                ShareTarget::System => Self::open_path_with_system(path),
                ShareTarget::Finder => Self::reveal_in_file_manager(path),
                ShareTarget::CapCut => {
                    let opened_app =
                        Self::open_macos_app_only_with_candidates(&["CapCut", "JianyingPro"]);
                    let revealed_file = Self::reveal_in_file_manager(path);
                    opened_app || revealed_file
                }
                ShareTarget::Premiere => {
                    Self::open_with_macos_candidates(
                        path,
                        &[
                            "Adobe Premiere Pro 2026",
                            "Adobe Premiere Pro 2025",
                            "Adobe Premiere Pro 2024",
                            "Adobe Premiere Pro",
                        ],
                    ) || Self::open_path_with_system(path)
                }
                ShareTarget::WeChat => {
                    let opened_app =
                        Self::open_macos_app_only_with_candidates(&["WeChat", "wechat", "微信"]);
                    let revealed_file = Self::reveal_in_file_manager(path);
                    opened_app || revealed_file
                }
            };
        }

        #[cfg(not(target_os = "macos"))]
        {
            return match target {
                ShareTarget::Finder => Self::reveal_in_file_manager(path),
                _ => Self::open_path_with_system(path),
            };
        }
    }

    fn download_audio(&mut self, cx: &mut Cx) {
        if self.tts_status == TTSStatus::Generating {
            self.add_log(cx, "[INFO] [tts] Download is disabled while generating new audio");
            self.update_player_bar(cx);
            return;
        }

        let export_samples = self.effective_audio_samples().to_vec();
        if export_samples.is_empty() {
            self.add_log(cx, "[WARN] [tts] No audio to download");
            return;
        }

        // Generate filename with timestamp
        let timestamp = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let filename = format!("tts_output_{}.wav", timestamp);

        // Get downloads folder or current directory
        let download_path = if let Some(home) = dirs::home_dir() {
            let downloads = home.join("Downloads");
            if downloads.exists() {
                downloads.join(&filename)
            } else {
                PathBuf::from(&filename)
            }
        } else {
            PathBuf::from(&filename)
        };

        // Write WAV file
        match self.write_wav_file(&download_path, &export_samples) {
            Ok(_) => {
                self.add_log(
                    cx,
                    &format!("[INFO] [tts] Audio saved to: {}", download_path.display()),
                );
                // Show success toast
                self.show_toast(cx, self.tr("下载成功！", "Downloaded successfully!"));
            }
            Err(e) => {
                self.add_log(cx, &format!("[ERROR] [tts] Failed to save audio: {}", e));
            }
        }
    }

    fn write_wav_file(&self, path: &PathBuf, samples: &[f32]) -> std::io::Result<()> {
        Self::write_wav_file_with_sample_rate(path, samples, self.stored_audio_sample_rate)
    }

    fn write_wav_file_with_sample_rate(
        path: &PathBuf,
        samples: &[f32],
        sample_rate: u32,
    ) -> std::io::Result<()> {
        use std::io::Write;

        let num_channels: u16 = 1;
        let bits_per_sample: u16 = 16;
        let byte_rate = sample_rate * (num_channels as u32) * (bits_per_sample as u32) / 8;
        let block_align: u16 = num_channels * bits_per_sample / 8;
        let data_size = (samples.len() * 2) as u32;
        let file_size = 36 + data_size;

        let mut file = std::fs::File::create(path)?;

        // RIFF header
        file.write_all(b"RIFF")?;
        file.write_all(&file_size.to_le_bytes())?;
        file.write_all(b"WAVE")?;

        // fmt chunk
        file.write_all(b"fmt ")?;
        file.write_all(&16u32.to_le_bytes())?;
        file.write_all(&1u16.to_le_bytes())?;
        file.write_all(&num_channels.to_le_bytes())?;
        file.write_all(&sample_rate.to_le_bytes())?;
        file.write_all(&byte_rate.to_le_bytes())?;
        file.write_all(&block_align.to_le_bytes())?;
        file.write_all(&bits_per_sample.to_le_bytes())?;

        // data chunk
        file.write_all(b"data")?;
        file.write_all(&data_size.to_le_bytes())?;

        // Convert f32 samples to i16 and write
        for &sample in samples {
            let clamped = sample.max(-1.0).min(1.0);
            let i16_sample = (clamped * 32767.0) as i16;
            file.write_all(&i16_sample.to_le_bytes())?;
        }

        Ok(())
    }

    // ============ Voice Library Methods ============

    /// Load voice library from disk
    fn load_voice_library(&mut self, cx: &mut Cx) {
        self.library_loading = true;
        self.add_log(cx, "[INFO] [library] Loading voice library...");

        // Load builtin voices
        let mut voices = crate::voice_data::get_builtin_voices();
        let builtin_count = voices.len();

        // Load custom/trained voices from disk
        let custom_voices = crate::voice_persistence::load_custom_voices();
        let custom_count = custom_voices.len();
        voices.extend(custom_voices);

        self.library_voices = voices;
        self.library_loading = false;

        self.add_log(cx, &format!(
            "[INFO] [library] Loaded {} voices ({} builtin, {} custom/trained)",
            self.library_voices.len(), builtin_count, custom_count
        ));
        self.update_library_display(cx);
        self.sync_selected_voice_ui(cx);
        self.update_voice_picker_controls(cx);
    }

    /// Refresh voice library
    fn refresh_voice_library(&mut self, cx: &mut Cx) {
        self.load_voice_library(cx);
        self.show_toast(cx, self.tr("音色库已刷新", "Voice library refreshed"));
    }

    /// Update category filter button states
    fn update_category_filter_buttons(&mut self, cx: &mut Cx) {
        let male_active = if self.library_category_filter == VoiceFilter::Male { 1.0 } else { 0.0 };
        let female_active = if self.library_category_filter == VoiceFilter::Female { 1.0 } else { 0.0 };
        let adult_active = if self.library_age_filter & 0b01 != 0 { 1.0 } else { 0.0 };
        let youth_active = if self.library_age_filter & 0b10 != 0 { 1.0 } else { 0.0 };
        let sweet_active = if self.library_style_filter & 0b01 != 0 { 1.0 } else { 0.0 };
        let magnetic_active = if self.library_style_filter & 0b10 != 0 { 1.0 } else { 0.0 };
        let prof_active = if self.library_trait_filter & 0b01 != 0 { 1.0 } else { 0.0 };
        let character_active = if self.library_trait_filter & 0b10 != 0 { 1.0 } else { 0.0 };

        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_gender.filter_male_btn))
            .apply_over(cx, live! { draw_bg: { active: (male_active) } draw_text: { active: (male_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_gender.filter_female_btn))
            .apply_over(cx, live! { draw_bg: { active: (female_active) } draw_text: { active: (female_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_gender.age_adult_btn))
            .apply_over(cx, live! { draw_bg: { active: (adult_active) } draw_text: { active: (adult_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_gender.age_youth_btn))
            .apply_over(cx, live! { draw_bg: { active: (youth_active) } draw_text: { active: (youth_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_style.style_sweet_btn))
            .apply_over(cx, live! { draw_bg: { active: (sweet_active) } draw_text: { active: (sweet_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_style.style_magnetic_btn))
            .apply_over(cx, live! { draw_bg: { active: (magnetic_active) } draw_text: { active: (magnetic_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_trait.trait_prof_btn))
            .apply_over(cx, live! { draw_bg: { active: (prof_active) } draw_text: { active: (prof_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.category_filter.row_trait.trait_character_btn))
            .apply_over(cx, live! { draw_bg: { active: (character_active) } draw_text: { active: (character_active) } });
    }

    /// Update language filter button states
    fn update_language_filter_buttons(&mut self, cx: &mut Cx) {
        let all_active = if self.library_language_filter == LanguageFilter::All { 1.0 } else { 0.0 };
        let zh_active = if self.library_language_filter == LanguageFilter::Chinese { 1.0 } else { 0.0 };
        let en_active = if self.library_language_filter == LanguageFilter::English { 1.0 } else { 0.0 };

        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.language_filter.lang_all_btn))
            .apply_over(cx, live! { draw_bg: { active: (all_active) } draw_text: { active: (all_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.language_filter.lang_zh_btn))
            .apply_over(cx, live! { draw_bg: { active: (zh_active) } draw_text: { active: (zh_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.language_filter.lang_en_btn))
            .apply_over(cx, live! { draw_bg: { active: (en_active) } draw_text: { active: (en_active) } });
    }

    /// Update right-side controls tab states
    fn update_settings_tabs(&mut self, cx: &mut Cx) {
        let voice_active = if self.controls_panel_tab == 0 { 1.0 } else { 0.0 };
        let settings_active = if self.controls_panel_tab == 1 { 1.0 } else { 0.0 };
        let history_active = if self.controls_panel_tab == 2 { 1.0 } else { 0.0 };
        let dark_mode = self.dark_mode;

        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_tabs.voice_management_tab_btn))
            .apply_over(cx, live! {
                draw_bg: { active: (voice_active) }
                draw_text: { active: (voice_active), dark_mode: (dark_mode) }
            });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_tabs.settings_tab_btn))
            .apply_over(cx, live! {
                draw_bg: { active: (settings_active) }
                draw_text: { active: (settings_active), dark_mode: (dark_mode) }
            });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_tabs.history_tab_btn))
            .apply_over(cx, live! {
                draw_bg: { active: (history_active) }
                draw_text: { active: (history_active), dark_mode: (dark_mode) }
            });
    }

    /// Update language options in global settings
    fn update_language_options(&mut self, cx: &mut Cx) {
        let en_active = if self.app_language == "en" { 1.0 } else { 0.0 };
        let zh_active = if self.app_language == "zh" { 1.0 } else { 0.0 };
        let dark_mode = self.dark_mode;

        self.view.button(ids!(global_settings_modal.settings_dialog.settings_content.language_section.language_options.lang_en_option))
            .apply_over(cx, live! {
                draw_bg: { active: (en_active), dark_mode: (dark_mode) }
                draw_text: { active: (en_active), dark_mode: (dark_mode) }
            });
        self.view.button(ids!(global_settings_modal.settings_dialog.settings_content.language_section.language_options.lang_zh_option))
            .apply_over(cx, live! {
                draw_bg: { active: (zh_active), dark_mode: (dark_mode) }
                draw_text: { active: (zh_active), dark_mode: (dark_mode) }
            });
        self.view.redraw(cx);
    }

    /// Update theme options in global settings
    fn update_theme_options(&mut self, cx: &mut Cx) {
        let light_active = if self.dark_mode < 0.5 { 1.0 } else { 0.0 };
        let dark_active = if self.dark_mode >= 0.5 { 1.0 } else { 0.0 };
        let dark_mode = self.dark_mode;

        self.view.button(ids!(global_settings_modal.settings_dialog.settings_content.theme_section.theme_options.theme_light_option))
            .apply_over(cx, live! {
                draw_bg: { active: (light_active), dark_mode: (dark_mode) }
                draw_text: { active: (light_active), dark_mode: (dark_mode) }
            });
        self.view.button(ids!(global_settings_modal.settings_dialog.settings_content.theme_section.theme_options.theme_dark_option))
            .apply_over(cx, live! {
                draw_bg: { active: (dark_active), dark_mode: (dark_mode) }
                draw_text: { active: (dark_active), dark_mode: (dark_mode) }
            });
        self.view.redraw(cx);
    }

    fn update_tts_param_controls(&mut self, cx: &mut Cx) {
        self.tts_speed = self.tts_speed.clamp(0.5, 2.0);
        self.tts_pitch = self.tts_pitch.clamp(-12.0, 12.0);
        self.tts_volume = self.tts_volume.clamp(0.0, 100.0);

        let pitch_text = if self.tts_pitch.abs() < 0.05 {
            "0".to_string()
        } else {
            format!("{:+.1}", self.tts_pitch)
        };

        let speed_progress = ((self.tts_speed - 0.5) / 1.5).clamp(0.0, 1.0);
        let pitch_progress = ((self.tts_pitch + 12.0) / 24.0).clamp(0.0, 1.0);
        let volume_progress = (self.tts_volume / 100.0).clamp(0.0, 1.0);
        let dark_mode = self.dark_mode;
        let speed_dragging = if self.tts_slider_dragging == Some(TtsParamSliderKind::Speed) {
            1.0
        } else {
            0.0
        };
        let pitch_dragging = if self.tts_slider_dragging == Some(TtsParamSliderKind::Pitch) {
            1.0
        } else {
            0.0
        };
        let volume_dragging = if self.tts_slider_dragging == Some(TtsParamSliderKind::Volume) {
            1.0
        } else {
            0.0
        };

        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .speed_row
                    .speed_header
                    .speed_value
            ))
            .set_text(cx, &format!("{:.2}x", self.tts_speed));
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .pitch_row
                    .pitch_header
                    .pitch_value
            ))
            .set_text(cx, &pitch_text);
        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .volume_row
                    .volume_header
                    .volume_value
            ))
            .set_text(cx, &format!("{:.0}%", self.tts_volume));

        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .speed_row
                    .speed_slider_row
                    .speed_slider
            ))
            .apply_over(
                cx,
                live! {
                    draw_bg: {
                        progress: (speed_progress),
                        dark_mode: (dark_mode),
                        dragging: (speed_dragging)
                    }
                },
            );
        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .pitch_row
                    .pitch_slider_row
                    .pitch_slider
            ))
            .apply_over(
                cx,
                live! {
                    draw_bg: {
                        progress: (pitch_progress),
                        dark_mode: (dark_mode),
                        dragging: (pitch_dragging)
                    }
                },
            );
        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .param_controls
                    .volume_row
                    .volume_slider_row
                    .volume_slider
            ))
            .apply_over(
                cx,
                live! {
                    draw_bg: {
                        progress: (volume_progress),
                        dark_mode: (dark_mode),
                        dragging: (volume_dragging)
                    }
                },
            );

        self.view.redraw(cx);
    }

    fn update_tts_param_by_slider_ratio(
        &mut self,
        cx: &mut Cx,
        kind: TtsParamSliderKind,
        ratio: f64,
    ) {
        let ratio = ratio.clamp(0.0, 1.0);
        let changed = match kind {
            TtsParamSliderKind::Speed => {
                let next = (0.5 + ratio * 1.5).clamp(0.5, 2.0);
                let snapped = (next * 100.0).round() / 100.0;
                if (snapped - self.tts_speed).abs() < 0.0001 {
                    false
                } else {
                    self.tts_speed = snapped;
                    true
                }
            }
            TtsParamSliderKind::Pitch => {
                let next = (-12.0 + ratio * 24.0).clamp(-12.0, 12.0);
                let snapped = (next * 10.0).round() / 10.0;
                if (snapped - self.tts_pitch).abs() < 0.0001 {
                    false
                } else {
                    self.tts_pitch = snapped;
                    true
                }
            }
            TtsParamSliderKind::Volume => {
                let next = (ratio * 100.0).clamp(0.0, 100.0);
                let snapped = next.round();
                if (snapped - self.tts_volume).abs() < 0.0001 {
                    false
                } else {
                    self.tts_volume = snapped;
                    true
                }
            }
        };

        if changed {
            self.update_tts_param_controls(cx);
            return;
        }

        self.update_tts_param_controls(cx);
    }

    fn slider_ratio_from_rect(abs_x: f64, rect: Rect) -> f64 {
        if rect.size.x <= 1.0 {
            return 0.0;
        }
        ((abs_x - rect.pos.x) / rect.size.x).clamp(0.0, 1.0)
    }

    fn handle_tts_param_slider_event(
        &mut self,
        cx: &mut Cx,
        event: &Event,
        slider_area: Area,
        kind: TtsParamSliderKind,
    ) {
        match event.hits_with_capture_overload(cx, slider_area, true) {
            Hit::FingerHoverOver(_) => {
                cx.set_cursor(MouseCursor::Hand);
            }
            Hit::FingerDown(fe) if fe.device.is_primary_hit() => {
                self.tts_slider_dragging = Some(kind);
                let ratio = Self::slider_ratio_from_rect(fe.abs.x, fe.rect);
                self.update_tts_param_by_slider_ratio(cx, kind, ratio);
            }
            Hit::FingerMove(fe) => {
                if self.tts_slider_dragging == Some(kind) {
                    let ratio = Self::slider_ratio_from_rect(fe.abs.x, fe.rect);
                    self.update_tts_param_by_slider_ratio(cx, kind, ratio);
                }
            }
            Hit::FingerUp(fe) if fe.is_primary_hit() => {
                if self.tts_slider_dragging == Some(kind) {
                    let ratio = Self::slider_ratio_from_rect(fe.abs.x, fe.rect);
                    self.update_tts_param_by_slider_ratio(cx, kind, ratio);
                    self.tts_slider_dragging = None;
                    self.update_tts_param_controls(cx);
                }
            }
            _ => {}
        }
    }

    fn effective_audio_samples(&self) -> &[f32] {
        if self.stored_audio_samples.is_empty() {
            &self.stored_audio_samples
        } else if self.processed_audio_samples.is_empty() {
            &self.stored_audio_samples
        } else {
            &self.processed_audio_samples
        }
    }

    fn rebuild_processed_audio_samples(&mut self) {
        if self.stored_audio_samples.is_empty() {
            self.processed_audio_samples.clear();
            return;
        }

        // Generated audio should remain immutable after completion.
        // Speed/Pitch/Volume changes only apply to the next synthesis request.
        self.processed_audio_samples = self.stored_audio_samples.clone();
    }

    /// Apply dark mode to the entire UI
    fn apply_dark_mode(&mut self, cx: &mut Cx) {
        let dark_mode = self.dark_mode;
        
        // Apply to main layout
        self.view.view(ids!(content_wrapper)).apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });

        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .page_header
                    .page_title
            ))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });

        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
            ))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });

        self.view
            .text_input(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .input_container
                    .text_input
            ))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });

        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
            ))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });

        self.view
            .label(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .input_section
                    .bottom_bar
                    .char_count
            ))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        
        // Apply to settings panel
        self.view.view(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });

        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .tts_page
                    .cards_container
                    .controls_panel
                    .settings_tabs
            ))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });

        self.view
            .view(ids!(content_wrapper.audio_player_bar))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        
        // Apply to global settings modal
        self.view.view(ids!(global_settings_modal.settings_dialog))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        
        // Apply to model picker modal
        self.view.view(ids!(model_picker_modal.model_picker_dialog))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });

        // Voice picker container itself has no draw_bg.dark_mode field.
        // Per-widget dark mode is applied in update_voice_picker_controls().

        // Apply to settings tabs and modal controls that depend on dark_mode
        self.update_tts_param_controls(cx);
        self.update_settings_tabs(cx);
        self.update_language_options(cx);
        self.update_theme_options(cx);
        self.update_voice_picker_controls(cx);
        self.update_model_picker_controls(cx);
        self.update_delete_modal_dark_mode(cx);
        self.view
            .view(ids!(confirm_cancel_modal.dialog))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(confirm_cancel_modal.dialog.header.title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(confirm_cancel_modal.dialog.header.task_name))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(confirm_cancel_modal.dialog.header.message))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .button(ids!(confirm_cancel_modal.dialog.footer.back_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .button(ids!(confirm_cancel_modal.dialog.footer.confirm_btn))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .view(ids!(share_modal.share_dialog))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(share_modal.share_dialog.share_header.share_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(share_modal.share_dialog.share_header.share_subtitle))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .button(ids!(share_modal.share_dialog.share_actions.share_system_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .button(ids!(share_modal.share_dialog.share_actions.share_capcut_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .button(ids!(share_modal.share_dialog.share_actions.share_premiere_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .button(ids!(share_modal.share_dialog.share_actions.share_wechat_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .button(ids!(share_modal.share_dialog.share_actions.share_finder_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .button(ids!(share_modal.share_dialog.share_footer.share_cancel_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .button(ids!(global_settings_modal.settings_dialog.settings_header.settings_close_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_header.settings_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.language_section.language_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.theme_section.theme_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.about_section.about_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.about_section.about_version))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.about_section.about_engine))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        
        self.view.redraw(cx);
    }

    fn get_voice_picker_voices(&self) -> Vec<Voice> {
        use crate::voice_data::VoiceSource;

        self.library_voices
            .iter()
            .filter(|v| match self.voice_picker_tab {
                0 => true,
                1 => v.source != VoiceSource::Builtin,
                _ => true,
            })
            .filter(|v| v.matches_language(&self.voice_picker_language_filter))
            .filter(|v| v.matches_filter(&self.voice_picker_gender_filter))
            .filter(|v| self.matches_voice_age_filter(v))
            .filter(|v| self.matches_voice_style_filter(v))
            .filter(|v| self.matches_voice_trait_filter(v))
            .filter(|v| {
                if self.voice_picker_search.trim().is_empty() {
                    true
                } else {
                    v.matches_search(&self.voice_picker_search)
                }
            })
            .cloned()
            .collect()
    }

    fn voice_text(voice: &Voice) -> String {
        format!("{} {}", voice.name, voice.description).to_lowercase()
    }

    fn single_line_text(text: &str) -> String {
        // Normalize hidden newlines/tabs from persisted voice names to keep UI single-line.
        let collapsed = text.split_whitespace().collect::<Vec<_>>().join(" ");
        collapsed.replace(" )", ")")
    }

    fn infer_voice_age_group(voice: &Voice) -> u8 {
        match voice.id.as_str() {
            // More energetic/young sounding built-in presets.
            "Yang Mi" | "Zhou Jielun" | "Ellen" => return 2,
            _ => {}
        }

        let text = Self::voice_text(voice);
        if text.contains("young")
            || text.contains("youth")
            || text.contains("teen")
            || text.contains("child")
            || text.contains("kid")
            || text.contains("boy")
            || text.contains("girl")
            || text.contains("energetic")
            || text.contains("青年")
            || text.contains("年轻")
            || text.contains("学生")
        {
            2
        } else {
            1
        }
    }

    fn infer_voice_style(voice: &Voice) -> u8 {
        match voice.id.as_str() {
            // Sweet / warm / gentle oriented built-in presets.
            "Yang Mi" | "Maple" | "Juniper" | "Ellen" => return 1,
            // Stronger / deeper / distinctive built-in presets.
            "Luo Xiang" | "Zhou Jielun" | "Ma Yun" | "Chen Yifan" | "Zhao Daniu"
            | "Ma Baoguo" | "Shen Yi" | "Cove" | "Trump" => return 2,
            _ => {}
        }

        let text = Self::voice_text(voice);
        if text.contains("sweet")
            || text.contains("charming")
            || text.contains("gentle")
            || text.contains("warm")
            || text.contains("soft")
            || text.contains("soothing")
            || text.contains("friendly")
            || text.contains("甜美")
            || text.contains("温柔")
            || text.contains("柔和")
        {
            1
        } else {
            2
        }
    }

    fn infer_voice_trait(voice: &Voice) -> u8 {
        use crate::voice_data::VoiceCategory;

        match voice.id.as_str() {
            // Professional broadcast/commentary leaning presets.
            "Luo Xiang" | "Chen Yifan" | "Shen Yi" | "Cove" | "Zhao Daniu" => return 1,
            // Highly distinctive character/persona presets.
            "Doubao" | "BYS" | "Ma Baoguo" | "Trump" => return 2,
            _ => {}
        }

        if voice.category == VoiceCategory::Character {
            return 2;
        }

        let text = Self::voice_text(voice);
        if text.contains("professional")
            || text.contains("professor")
            || text.contains("analyst")
            || text.contains("commentator")
            || text.contains("播音")
            || text.contains("解说")
            || text.contains("主持")
        {
            1
        } else if text.contains("distinctive")
            || text.contains("character")
            || text.contains("个性")
            || text.contains("特色")
            || text.contains("martial")
        {
            2
        } else {
            // Keep unmatched voices in the professional bucket so filters stay useful.
            1
        }
    }

    fn selected_voice_trait_labels(&self, voice: &Voice) -> (Option<String>, String, String) {
        let text = Self::voice_text(voice);
        let gender = if text.contains("child")
            || text.contains("kid")
            || text.contains("boy")
            || text.contains("girl")
        {
            Some(self.tr("童声", "Child").to_string())
        } else {
            match voice.category {
                crate::voice_data::VoiceCategory::Male => Some(self.tr("男声", "Male").to_string()),
                crate::voice_data::VoiceCategory::Female => Some(self.tr("女声", "Female").to_string()),
                crate::voice_data::VoiceCategory::Character => None,
            }
        };
        let age = match Self::infer_voice_age_group(voice) {
            2 => self.tr("青年音", "Youth").to_string(),
            _ => self.tr("成年音", "Adult").to_string(),
        };
        let style = match Self::infer_voice_style(voice) {
            1 => self.tr("甜美", "Sweet").to_string(),
            _ => self.tr("磁性", "Magnetic").to_string(),
        };

        (gender, age, style)
    }

    fn matches_voice_age_filter(&self, voice: &Voice) -> bool {
        if self.voice_picker_age_filter == 0 {
            return true;
        }
        let age_bit = match Self::infer_voice_age_group(voice) {
            1 => 0b01, // Adult
            2 => 0b10, // Youth
            _ => 0,
        };
        age_bit != 0 && (self.voice_picker_age_filter & age_bit) != 0
    }

    fn matches_voice_style_filter(&self, voice: &Voice) -> bool {
        if self.voice_picker_style_filter == 0 {
            return true;
        }
        let style_bit = match Self::infer_voice_style(voice) {
            1 => 0b01, // Sweet
            2 => 0b10, // Magnetic
            _ => 0,
        };
        style_bit != 0 && (self.voice_picker_style_filter & style_bit) != 0
    }

    fn matches_voice_trait_filter(&self, voice: &Voice) -> bool {
        if self.voice_picker_trait_filter == 0 {
            return true;
        }
        let trait_bit = match Self::infer_voice_trait(voice) {
            1 => 0b01, // Professional
            2 => 0b10, // Character
            _ => 0,
        };
        trait_bit != 0 && (self.voice_picker_trait_filter & trait_bit) != 0
    }

    fn clear_voice_picker_tag_filters(&mut self) {
        self.voice_picker_gender_filter = VoiceFilter::All;
        self.voice_picker_age_filter = 0;
        self.voice_picker_style_filter = 0;
        self.voice_picker_trait_filter = 0;
    }

    fn update_voice_picker_controls(&mut self, cx: &mut Cx) {
        let dark_mode = self.dark_mode;
        let male_active = if self.voice_picker_gender_filter == VoiceFilter::Male { 1.0 } else { 0.0 };
        let female_active = if self.voice_picker_gender_filter == VoiceFilter::Female { 1.0 } else { 0.0 };
        let adult_active = if self.voice_picker_age_filter & 0b01 != 0 { 1.0 } else { 0.0 };
        let youth_active = if self.voice_picker_age_filter & 0b10 != 0 { 1.0 } else { 0.0 };
        let sweet_active = if self.voice_picker_style_filter & 0b01 != 0 { 1.0 } else { 0.0 };
        let magnetic_active = if self.voice_picker_style_filter & 0b10 != 0 { 1.0 } else { 0.0 };
        let prof_active = if self.voice_picker_trait_filter & 0b01 != 0 { 1.0 } else { 0.0 };
        let character_active = if self.voice_picker_trait_filter & 0b10 != 0 { 1.0 } else { 0.0 };

        let active_voice_id = self
            .voice_picker_active_voice_id
            .as_ref()
            .or(self.selected_voice_id.as_ref());
        let active_voice_name = active_voice_id
            .and_then(|id| self.library_voices.iter().find(|v| &v.id == id))
            .map(|v| Self::single_line_text(&v.name))
            .unwrap_or_else(|| self.tr("请选择", "Select").to_string());

        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.select_voice_row.selected_voice_btn))
            .set_text(cx, &active_voice_name);

        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.select_voice_row.selected_voice_btn))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.select_voice_row.select_voice_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_gender.tag_group_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_style.tag_group_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_trait.tag_group_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });

        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_gender.gender_male_btn))
            .apply_over(cx, live! { draw_bg: { active: (male_active), dark_mode: (dark_mode) } draw_text: { active: (male_active), dark_mode: (dark_mode) } });
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_gender.gender_female_btn))
            .apply_over(cx, live! { draw_bg: { active: (female_active), dark_mode: (dark_mode) } draw_text: { active: (female_active), dark_mode: (dark_mode) } });
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_gender.age_adult_btn))
            .apply_over(cx, live! { draw_bg: { active: (adult_active), dark_mode: (dark_mode) } draw_text: { active: (adult_active), dark_mode: (dark_mode) } });
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_gender.age_youth_btn))
            .apply_over(cx, live! { draw_bg: { active: (youth_active), dark_mode: (dark_mode) } draw_text: { active: (youth_active), dark_mode: (dark_mode) } });
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_style.style_sweet_btn))
            .apply_over(cx, live! { draw_bg: { active: (sweet_active), dark_mode: (dark_mode) } draw_text: { active: (sweet_active), dark_mode: (dark_mode) } });
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_style.style_magnetic_btn))
            .apply_over(cx, live! { draw_bg: { active: (magnetic_active), dark_mode: (dark_mode) } draw_text: { active: (magnetic_active), dark_mode: (dark_mode) } });
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_trait.trait_prof_btn))
            .apply_over(cx, live! { draw_bg: { active: (prof_active), dark_mode: (dark_mode) } draw_text: { active: (prof_active), dark_mode: (dark_mode) } });
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_filter_card.tag_row_trait.trait_character_btn))
            .apply_over(cx, live! { draw_bg: { active: (character_active), dark_mode: (dark_mode) } draw_text: { active: (character_active), dark_mode: (dark_mode) } });

        let filtered = self.get_voice_picker_voices();
        let is_empty = filtered.is_empty();
        let any_filter_active = self.voice_picker_gender_filter != VoiceFilter::All
            || self.voice_picker_age_filter != 0
            || self.voice_picker_style_filter != 0
            || self.voice_picker_trait_filter != 0;
        let empty_text = if any_filter_active {
            self.tr("暂无符合标签的音色。", "No voices match current tags.")
        } else {
            self.tr("暂无可用音色。", "No voices available.")
        };

        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_picker_empty))
            .set_text(cx, empty_text);
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_picker_empty))
            .set_visible(cx, is_empty);

        self.view.redraw(cx);
    }

    fn sync_selected_model_ui(&mut self, cx: &mut Cx) {
        if self.model_options.is_empty() {
            self.selected_tts_model_id = None;
            self.view
                .button(ids!(
                    content_wrapper
                        .main_content
                        .left_column
                        .content_area
                        .tts_page
                        .cards_container
                        .input_section
                        .bottom_bar
                        .model_row
                        .model_picker_btn
                ))
                .set_text(cx, self.tr("🔮 暂无可用模型", "🔮 No model available"));
            return;
        }

        let selected_model = self
            .selected_tts_model_id
            .as_ref()
            .and_then(|id| self.model_options.iter().find(|m| &m.id == id))
            .cloned()
            .or_else(|| self.model_options.first().cloned());

        if let Some(model) = selected_model {
            self.selected_tts_model_id = Some(model.id.clone());
            self.view
                .button(ids!(
                    content_wrapper
                        .main_content
                        .left_column
                        .content_area
                        .tts_page
                        .cards_container
                        .input_section
                        .bottom_bar
                        .model_row
                        .model_picker_btn
                ))
                .set_text(cx, &format!("🔮 {}", model.name));
        }
    }

    fn update_model_picker_controls(&mut self, cx: &mut Cx) {
        self.sync_selected_model_ui(cx);

        let dark_mode = self.dark_mode;
        let model_count = self.model_options.len();
        let footer_text = if self.is_english() {
            if model_count == 1 {
                "1 model available in this project".to_string()
            } else {
                format!("{model_count} models available in this project")
            }
        } else {
            format!("当前项目共 {model_count} 个可用模型")
        };

        self.view
            .label(ids!(model_picker_modal.model_picker_dialog.model_picker_header.model_picker_title))
            .set_text(cx, self.tr("选择模型", "Select a model"));
        self.view
            .label(ids!(model_picker_modal.model_picker_dialog.model_picker_footer.model_picker_footer_label))
            .set_text(cx, &footer_text);

        self.view
            .view(ids!(model_picker_modal.model_picker_dialog))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .view(ids!(model_picker_modal.model_picker_dialog.model_picker_header))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(model_picker_modal.model_picker_dialog.model_picker_header.model_picker_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .button(ids!(model_picker_modal.model_picker_dialog.model_picker_header.model_picker_back_btn))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .view(ids!(model_picker_modal.model_picker_dialog.model_picker_footer))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(model_picker_modal.model_picker_dialog.model_picker_footer.model_picker_footer_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });

        self.view.redraw(cx);
    }

    fn select_tts_model(&mut self, cx: &mut Cx, model_id: &str) {
        if let Some(model) = self.model_options.iter().find(|m| m.id == model_id).cloned() {
            self.selected_tts_model_id = Some(model.id.clone());
            self.sync_selected_model_ui(cx);
            self.update_model_picker_controls(cx);
            self.add_log(
                cx,
                &format!("[INFO] [tts] Model selected: {} ({})", model.id, model.name),
            );
        }
    }

    fn sync_selected_voice_ui(&mut self, cx: &mut Cx) {
        let selected = self
            .selected_voice_id
            .as_ref()
            .and_then(|id| self.library_voices.iter().find(|v| &v.id == id).cloned())
            .or_else(|| {
                self.library_voices
                    .iter()
                    .find(|v| v.source == crate::voice_data::VoiceSource::Builtin)
                    .cloned()
            })
            .or_else(|| self.library_voices.first().cloned());

        if let Some(voice) = selected {
            self.selected_voice_id = Some(voice.id.clone());
            if self.voice_picker_active_voice_id.is_none() {
                self.voice_picker_active_voice_id = Some(voice.id.clone());
            }
            let (gender_label, age_label, style_label) = self.selected_voice_trait_labels(&voice);
            let source_tag = match voice.source {
                crate::voice_data::VoiceSource::Builtin => self.tr("内置", "Built-in"),
                _ => self.tr("我的音色", "My Voice"),
            };
            let picker_text = format!("🎤 {} ({})", voice.name, source_tag);

            self.view
                .button(ids!(
                    content_wrapper
                        .main_content
                        .left_column
                        .content_area
                        .tts_page
                        .cards_container
                        .controls_panel
                        .settings_panel
                        .voice_row
                        .voice_picker_btn
                ))
                .set_text(cx, &picker_text);
            self.view
                .view(ids!(
                    content_wrapper
                        .main_content
                        .left_column
                        .content_area
                        .tts_page
                        .cards_container
                        .controls_panel
                        .settings_panel
                        .voice_row
                        .voice_tags_row
                ))
                .set_visible(cx, true);
            self.view
                .view(ids!(
                    content_wrapper
                        .main_content
                        .left_column
                        .content_area
                        .tts_page
                        .cards_container
                        .controls_panel
                        .settings_panel
                        .voice_row
                        .voice_tags_row
                        .gender_badge
                ))
                .set_visible(cx, gender_label.is_some());
            if let Some(gender_label) = gender_label {
                self.view
                    .label(ids!(
                        content_wrapper
                            .main_content
                            .left_column
                            .content_area
                            .tts_page
                            .cards_container
                            .controls_panel
                            .settings_panel
                            .voice_row
                            .voice_tags_row
                            .gender_badge
                            .gender_badge_label
                    ))
                    .set_text(cx, &gender_label);
            }
            self.view
                .label(ids!(
                    content_wrapper
                        .main_content
                        .left_column
                        .content_area
                        .tts_page
                        .cards_container
                        .controls_panel
                        .settings_panel
                        .voice_row
                        .voice_tags_row
                        .age_badge
                        .age_badge_label
                ))
                .set_text(cx, &age_label);
            self.view
                .label(ids!(
                    content_wrapper
                        .main_content
                        .left_column
                        .content_area
                        .tts_page
                        .cards_container
                        .controls_panel
                        .settings_panel
                        .voice_row
                        .voice_tags_row
                        .style_badge
                        .style_badge_label
                ))
                .set_text(cx, &style_label);
            let dark_mode = self.dark_mode;
            self.view.view(ids!(
                content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.voice_row.voice_tags_row.gender_badge
            )).apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
            self.view.label(ids!(
                content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.voice_row.voice_tags_row.gender_badge.gender_badge_label
            )).apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
            self.view.view(ids!(
                content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.voice_row.voice_tags_row.age_badge
            )).apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
            self.view.label(ids!(
                content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.voice_row.voice_tags_row.age_badge.age_badge_label
            )).apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
            self.view.view(ids!(
                content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.voice_row.voice_tags_row.style_badge
            )).apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
            self.view.label(ids!(
                content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.voice_row.voice_tags_row.style_badge.style_badge_label
            )).apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        } else {
            self.selected_voice_id = None;
            self.voice_picker_active_voice_id = None;
            self.view
                .button(ids!(
                    content_wrapper
                        .main_content
                        .left_column
                        .content_area
                        .tts_page
                        .cards_container
                        .controls_panel
                        .settings_panel
                        .voice_row
                        .voice_picker_btn
                ))
                .set_text(cx, self.tr("🎤 选择音色", "🎤 Select a voice"));
            self.view
                .view(ids!(
                    content_wrapper
                        .main_content
                        .left_column
                        .content_area
                        .tts_page
                        .cards_container
                        .controls_panel
                        .settings_panel
                        .voice_row
                        .voice_tags_row
                ))
                .set_visible(cx, false);
        }
    }

    fn select_voice(&mut self, cx: &mut Cx, voice: Voice) {
        self.selected_voice_id = Some(voice.id.clone());
        self.voice_picker_active_voice_id = Some(voice.id.clone());
        self.sync_selected_voice_ui(cx);
        self.update_voice_picker_controls(cx);
        self.add_log(
            cx,
            &format!("[INFO] [tts] Voice selected: {} ({})", voice.id, voice.name),
        );
    }

    fn matches_library_age_filter(&self, voice: &Voice) -> bool {
        if self.library_age_filter == 0 {
            return true;
        }
        let age_bit = match Self::infer_voice_age_group(voice) {
            1 => 0b01, // Adult
            2 => 0b10, // Youth
            _ => 0,
        };
        age_bit != 0 && (self.library_age_filter & age_bit) != 0
    }

    fn matches_library_style_filter(&self, voice: &Voice) -> bool {
        if self.library_style_filter == 0 {
            return true;
        }
        let style_bit = match Self::infer_voice_style(voice) {
            1 => 0b01, // Sweet
            2 => 0b10, // Magnetic
            _ => 0,
        };
        style_bit != 0 && (self.library_style_filter & style_bit) != 0
    }

    fn matches_library_trait_filter(&self, voice: &Voice) -> bool {
        if self.library_trait_filter == 0 {
            return true;
        }
        let trait_bit = match Self::infer_voice_trait(voice) {
            1 => 0b01, // Professional
            2 => 0b10, // Character
            _ => 0,
        };
        trait_bit != 0 && (self.library_trait_filter & trait_bit) != 0
    }

    /// Filter voices based on category tags, language, and search query
    fn get_filtered_voices(&self) -> Vec<Voice> {
            self.library_voices
                .iter()
            .filter(|v| v.matches_filter(&self.library_category_filter))
            .filter(|v| self.matches_library_age_filter(v))
            .filter(|v| self.matches_library_style_filter(v))
            .filter(|v| self.matches_library_trait_filter(v))
            .filter(|v| v.matches_language(&self.library_language_filter))
                .filter(|v| {
                if self.library_search_query.is_empty() {
                    true
                } else {
                    let query = self.library_search_query.to_lowercase();
                    v.name.to_lowercase().contains(&query)
                        || v.language.to_lowercase().contains(&query)
                }
                })
                .cloned()
                .collect()
    }

    /// Update library display
    fn update_library_display(&mut self, cx: &mut Cx) {
        self.update_category_filter_buttons(cx);
        self.update_language_filter_buttons(cx);

        let filtered = self.get_filtered_voices();
        let total_count = self.library_voices.len();
        let filtered_count = filtered.len();
        
        // Update empty state visibility
        let is_empty = filtered.is_empty();
        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .library_page
                    .empty_state
            ))
            .set_visible(cx, is_empty);

        // Update empty state text based on whether it's a search result or truly empty
        if is_empty {
            let empty_text = if self.library_search_query.is_empty() {
                self.tr("暂无音色，点击「刷新」加载", "No voices yet. Click \"Refresh\" to load.")
            } else {
                self.tr("未找到匹配的音色", "No matching voices found.")
            };
            self.view
                .label(ids!(
                    content_wrapper
                        .main_content
                        .left_column
                        .content_area
                        .library_page
                        .empty_state
                        .empty_text
                ))
                .set_text(cx, empty_text);
        }

        // Log the display status
        if self.library_search_query.is_empty() {
            self.add_log(cx, &format!("[INFO] [library] Displaying {} voices", filtered_count));
        } else {
            self.add_log(cx, &format!(
                "[INFO] [library] Search results: {} of {} voices match '{}'",
                filtered_count, total_count, self.library_search_query
            ));
        }

        // Show/hide voice_list vs empty_state
        self.view
            .portal_list(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .library_page
                    .voice_list
            ))
            .set_visible(cx, !is_empty);

        // Force full redraw so PortalList re-renders with updated filter
        self.redraw(cx);
    }

    /// Delete a voice
    fn delete_voice(&mut self, cx: &mut Cx, voice_id: String) {
        self.add_log(cx, &format!("[INFO] [library] Deleting voice: {}", voice_id));

        // Check if it's a custom/trained voice (only those can be deleted from disk)
        let is_custom = self.library_voices.iter()
            .find(|v| v.id == voice_id)
            .map(|v| v.source != crate::voice_data::VoiceSource::Builtin)
            .unwrap_or(false);

        // Remove from in-memory list
        self.library_voices.retain(|v| v.id != voice_id);

        // Delete from disk if custom/trained
        if is_custom {
            if let Err(e) = crate::voice_persistence::remove_custom_voice(&voice_id) {
                self.add_log(cx, &format!("[WARN] [library] Failed to remove from disk: {}", e));
            }
        }

        if self.selected_voice_id.as_deref() == Some(&voice_id) {
            self.selected_voice_id = None;
        }

        self.update_library_display(cx);
        self.sync_selected_voice_ui(cx);
        self.update_voice_picker_controls(cx);
        self.show_toast(cx, self.tr("音色已删除", "Voice deleted successfully"));
    }

    /// Preview a voice from the library
    fn preview_voice(&mut self, cx: &mut Cx, voice_id: String) {
        self.add_log(cx, &format!("[INFO] [library] Previewing voice: {}", voice_id));

        // Toggle: stop if already playing this voice
        if self.preview_playing_voice_id.as_deref() == Some(&voice_id) {
            if let Some(player) = &self.preview_player {
                player.stop();
            }
            self.preview_playing_voice_id = None;
            self.update_voice_picker_controls(cx);
            return;
        }

        // Stop any currently playing preview
        if let Some(player) = &self.preview_player {
            player.stop();
        }

        // Find the voice in library
        let voice = match self.library_voices.iter().find(|v| v.id == voice_id) {
            Some(v) => v.clone(),
            None => {
                self.add_log(cx, &format!("[WARN] [library] Voice not found: {}", voice_id));
                return;
            }
        };

        // Build audio path based on voice source
        use crate::voice_data::VoiceSource;
        use crate::voice_persistence;
        let audio_path = if voice.source == VoiceSource::Custom || voice.source == VoiceSource::Trained {
            match voice_persistence::get_reference_audio_path(&voice) {
                Some(path) => path,
                None => {
                    self.add_log(cx, &format!("[WARN] [library] No reference audio for custom voice: {}", voice_id));
                    return;
                }
            }
        } else {
            let preview_file = match &voice.preview_audio {
                Some(f) => f.clone(),
                None => {
                    self.add_log(cx, &format!("[WARN] [library] No preview audio for: {}", voice_id));
                    return;
                }
            };
            let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
            home.join(".dora")
                .join("models")
                .join("primespeech")
                .join("moyoyo")
                .join("ref_audios")
                .join(&preview_file)
        };

        if !audio_path.exists() {
            self.add_log(cx, &format!("[WARN] [library] Preview audio file not found: {:?}", audio_path));
            return;
        }

        // Load and play WAV file
        match self.load_wav_file(&audio_path) {
            Ok(samples) => {
                if self.preview_player.is_none() {
                    self.preview_player = Some(TTSPlayer::new());
                }
                if let Some(player) = &self.preview_player {
                    player.write_audio(&samples);
                    player.resume();
                }
                self.preview_playing_voice_id = Some(voice_id.clone());
                self.update_voice_picker_controls(cx);
                self.add_log(cx, &format!("[INFO] [library] Playing preview: {}", voice_id));
            }
            Err(e) => {
                self.add_log(cx, &format!("[ERROR] [library] Failed to load preview audio: {}", e));
            }
        }
    }

    // ============ Voice Clone Methods ============

    /// Load clone tasks
    fn load_clone_tasks(&mut self, cx: &mut Cx) {
        self.clone_loading = true;
        self.add_log(cx, "[INFO] [clone] Loading clone tasks...");
        
        // Load tasks from disk
        self.clone_tasks = task_persistence::load_clone_tasks();
        
        self.clone_loading = false;
        self.add_log(cx, &format!("[INFO] [clone] Loaded {} tasks", self.clone_tasks.len()));
        self.update_clone_display(cx);
    }

    /// Refresh clone tasks
    fn refresh_clone_tasks(&mut self, cx: &mut Cx) {
        self.load_clone_tasks(cx);
    }

    /// Update clone display
    fn update_clone_display(&mut self, cx: &mut Cx) {
        let is_empty = self.clone_tasks.is_empty();
        
        // Update empty state visibility
        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .clone_page
                    .clone_empty_state
            ))
            .set_visible(cx, is_empty);
        
        // Log task statistics
        if !is_empty {
            let completed = self.clone_tasks.iter().filter(|t| t.status == CloneTaskStatus::Completed).count();
            let processing = self.clone_tasks.iter().filter(|t| t.status == CloneTaskStatus::Processing).count();
            let pending = self.clone_tasks.iter().filter(|t| t.status == CloneTaskStatus::Pending).count();
            let failed = self.clone_tasks.iter().filter(|t| t.status == CloneTaskStatus::Failed).count();
            let cancelled = self.clone_tasks.iter().filter(|t| t.status == CloneTaskStatus::Cancelled).count();
            
            self.add_log(cx, &format!(
                "[INFO] [clone] Tasks: {} total ({} completed, {} processing, {} pending, {} failed, {} cancelled)",
                self.clone_tasks.len(), completed, processing, pending, failed, cancelled
            ));
        } else {
            self.add_log(cx, "[INFO] [clone] No tasks found");
        }
        
        // TODO: Update task cards dynamically using PortalList
    }

    /// Show cancel task confirmation dialog
    fn show_cancel_task_confirmation(&mut self, cx: &mut Cx, task_id: String, task_name: String) {
        self.pending_cancel_task_id = Some(task_id.clone());
        self.pending_cancel_task_name = Some(task_name.clone());
        
        // Update dialog with task name
        self.view
            .label(ids!(confirm_cancel_modal.dialog.header.task_name))
            .set_text(cx, &format!("\"{}\"", task_name));
        
        // Show dialog
        self.view
            .view(ids!(confirm_cancel_modal))
            .set_visible(cx, true);
        
        self.add_log(cx, &format!("[INFO] [clone] Requesting cancel confirmation for: {}", task_name));
    }

    /// Cancel a clone task (called after confirmation)
    fn cancel_clone_task(&mut self, cx: &mut Cx, task_id: String) {
        self.add_log(cx, &format!("[INFO] [clone] Cancelling task: {}", task_id));
        let cancelled_message = self.tr("任务已由用户取消", "Task cancelled by user").to_string();
        
        // Find and update task status
        if let Some(task) = self.clone_tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = CloneTaskStatus::Cancelled;
            task.message = Some(cancelled_message);
            
            // Save to disk
            if let Err(e) = task_persistence::save_clone_tasks(&self.clone_tasks) {
                self.add_log(cx, &format!("[ERROR] [clone] Failed to save tasks: {}", e));
            } else {
                self.add_log(cx, "[INFO] [clone] Task status saved to disk");
            }
        }
        
        // TODO: Cancel actual training process
        
        self.update_clone_display(cx);
        self.show_toast(cx, self.tr("任务已取消", "Task cancelled"));
    }

    /// View clone task details - now opens the task detail page
    fn view_clone_task(&mut self, cx: &mut Cx, task_id: String) {
        self.add_log(cx, &format!("[INFO] [clone] Viewing task: {}", task_id));
        self.open_task_detail(cx, task_id);
    }

    /// Open the task detail page for a given task ID
    fn open_task_detail(&mut self, cx: &mut Cx, task_id: String) {
        self.current_task_id = Some(task_id);
        self.refresh_task_detail(cx);
        self.switch_page(cx, AppPage::TaskDetail);
    }

    /// Refresh the task detail page UI from persistence
    fn refresh_task_detail(&mut self, cx: &mut Cx) {
        let Some(ref task_id) = self.current_task_id.clone() else { return; };

        // Try to get fresh data from persistence
        let task = if let Some(t) = task_persistence::get_task(task_id) {
            // Update in-memory list too
            if let Some(existing) = self.clone_tasks.iter_mut().find(|t| t.id == *task_id) {
                *existing = t.clone();
            }
            t
        } else if let Some(t) = self.clone_tasks.iter().find(|t| t.id == *task_id).cloned() {
            t
        } else {
            return;
        };

        // Update task name
        self.view.label(ids!(
            content_wrapper.main_content.left_column.content_area
            .task_detail_page.detail_header.detail_task_name
        )).set_text(cx, &task.name);

        // Update status badge text
        let (status_text, _status_val) = match task.status {
            CloneTaskStatus::Pending => ("待执行", 0.0_f64),
            CloneTaskStatus::Processing => ("训练中", 1.0),
            CloneTaskStatus::Completed => ("已完成", 2.0),
            CloneTaskStatus::Failed => ("失败", 3.0),
            CloneTaskStatus::Cancelled => ("已取消", 3.0),
        };
        self.view.label(ids!(
            content_wrapper.main_content.left_column.content_area
            .task_detail_page.detail_header.detail_status_badge.detail_status_label
        )).set_text(cx, status_text);

        // Show cancel button only for Pending/Processing tasks
        let can_cancel = matches!(task.status, CloneTaskStatus::Pending | CloneTaskStatus::Processing);
        self.view.button(ids!(
            content_wrapper.main_content.left_column.content_area
            .task_detail_page.detail_header.detail_cancel_btn
        )).set_visible(cx, can_cancel);

        // Update time info
        self.view.label(ids!(
            content_wrapper.main_content.left_column.content_area
            .task_detail_page.detail_info_card.detail_times_row
            .detail_created_section.detail_created_at
        )).set_text(cx, &task.created_at);

        self.view.label(ids!(
            content_wrapper.main_content.left_column.content_area
            .task_detail_page.detail_info_card.detail_times_row
            .detail_started_section.detail_started_at
        )).set_text(cx, task.started_at.as_deref().unwrap_or("-"));

        self.view.label(ids!(
            content_wrapper.main_content.left_column.content_area
            .task_detail_page.detail_info_card.detail_times_row
            .detail_completed_section.detail_completed_at
        )).set_text(cx, task.completed_at.as_deref().unwrap_or("-"));

        // Update overall progress bar
        let pct = task.progress;
        let pct_text = format!("{:.0}%", pct * 100.0);
        self.view.view(ids!(
            content_wrapper.main_content.left_column.content_area
            .task_detail_page.detail_progress_card.detail_overall_row.detail_progress_bar
        )).apply_over(cx, live! { draw_bg: { progress: (pct) } });
        self.view.label(ids!(
            content_wrapper.main_content.left_column.content_area
            .task_detail_page.detail_progress_card.detail_overall_row.detail_progress_text
        )).set_text(cx, &pct_text);

        // Update 8 stage dots
        // dot_color: 0.0 = pending (gray), 1.0 = running (purple), 2.0 = done (green)
        //
        // Python sends current_step 1..=7 (1-indexed) when a stage STARTS.
        // So when current_step=N, steps 1..N-1 are complete and step N is running.
        // In the 8-slot UI (0-indexed) the running dot is at index current_step - 1.
        // The 8th dot (推理测试) is only lit on COMPLETE (current_step set to 8).
        let current_step = task.current_step.unwrap_or(0) as usize;
        let is_completed = matches!(task.status, CloneTaskStatus::Completed);
        let is_processing = matches!(task.status, CloneTaskStatus::Processing);
        // 0-indexed slot currently running; clamp so it never exceeds 7
        let running_idx = current_step.saturating_sub(1).min(7);

        // Sub-epoch progress for GPT/SoVITS stages
        let sub_step = task.sub_step.unwrap_or(0) as usize;
        let sub_total = task.sub_total.unwrap_or(0) as usize;

        // Python stage 6 = GPT training → UI index 5 (SoVITS训练) … but that's a
        // name mismatch.  We keep the linear mapping; the stage name display will
        // still be useful progress feedback.
        // Stages 5 and 6 (0-indexed) are the long training stages that have sub-progress.
        let long_training_stages = [5usize, 6usize]; // SoVITS训练 / GPT训练 slots

        let stage_names = ["stage_1", "stage_2", "stage_3", "stage_4", "stage_5", "stage_6", "stage_7", "stage_8"];
        for (i, _name) in stage_names.iter().enumerate() {
            let dot_color: f64 = if is_completed {
                2.0  // all green when training complete
            } else if i < running_idx {
                2.0  // completed stages
            } else if i == running_idx && is_processing {
                1.0  // currently running stage
            } else {
                0.0  // pending
            };

            // Show "X/Y epochs" for long training stages, otherwise show nothing
            let pct_label = if i == running_idx && is_processing && long_training_stages.contains(&i) && sub_total > 0 {
                format!("{}/{} epochs", sub_step, sub_total)
            } else {
                String::new()
            };

            // Apply to each stage row's dot and pct label
            match i {
                0 => {
                    self.view.view(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_1_row.stage_1_dot))
                        .apply_over(cx, live! { draw_bg: { dot_color: (dot_color) } });
                    self.view.label(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_1_row.stage_1_pct))
                        .set_text(cx, &pct_label);
                }
                1 => {
                    self.view.view(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_2_row.stage_2_dot))
                        .apply_over(cx, live! { draw_bg: { dot_color: (dot_color) } });
                    self.view.label(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_2_row.stage_2_pct))
                        .set_text(cx, &pct_label);
                }
                2 => {
                    self.view.view(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_3_row.stage_3_dot))
                        .apply_over(cx, live! { draw_bg: { dot_color: (dot_color) } });
                    self.view.label(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_3_row.stage_3_pct))
                        .set_text(cx, &pct_label);
                }
                3 => {
                    self.view.view(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_4_row.stage_4_dot))
                        .apply_over(cx, live! { draw_bg: { dot_color: (dot_color) } });
                    self.view.label(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_4_row.stage_4_pct))
                        .set_text(cx, &pct_label);
                }
                4 => {
                    self.view.view(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_5_row.stage_5_dot))
                        .apply_over(cx, live! { draw_bg: { dot_color: (dot_color) } });
                    self.view.label(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_5_row.stage_5_pct))
                        .set_text(cx, &pct_label);
                }
                5 => {
                    self.view.view(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_6_row.stage_6_dot))
                        .apply_over(cx, live! { draw_bg: { dot_color: (dot_color) } });
                    self.view.label(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_6_row.stage_6_pct))
                        .set_text(cx, &pct_label);
                }
                6 => {
                    self.view.view(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_7_row.stage_7_dot))
                        .apply_over(cx, live! { draw_bg: { dot_color: (dot_color) } });
                    self.view.label(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_7_row.stage_7_pct))
                        .set_text(cx, &pct_label);
                }
                7 => {
                    self.view.view(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_8_row.stage_8_dot))
                        .apply_over(cx, live! { draw_bg: { dot_color: (dot_color) } });
                    self.view.label(ids!(content_wrapper.main_content.left_column.content_area.task_detail_page.detail_progress_card.stage_8_row.stage_8_pct))
                        .set_text(cx, &pct_label);
                }
                _ => {}
            }
        }

        // Update message label
        self.view.label(ids!(
            content_wrapper.main_content.left_column.content_area
            .task_detail_page.detail_progress_card.detail_message_label
        )).set_text(cx, task.message.as_deref().unwrap_or(""));

        self.view.redraw(cx);
    }
}

impl TTSScreenRef {
    pub fn update_dark_mode(&self, cx: &mut Cx, dark_mode: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.dark_mode = dark_mode;
            inner.apply_dark_mode(cx);

            // Apply dark mode to voice selector
            inner
                .view
                .voice_selector(ids!(
                    content_wrapper
                        .main_content
                        .left_column
                        .content_area
                        .tts_page
                        .cards_container
                        .controls_panel
                        .settings_panel
                        .voice_section
                        .voice_selector
                ))
                .update_dark_mode(cx, dark_mode);

            // Apply dark mode to log markdown
            let log_markdown = inner.view.markdown(ids!(
                content_wrapper
                    .main_content
                    .log_section
                    .log_content_column
                    .log_scroll
                    .log_content_wrapper
                    .log_content
            ));
            log_markdown.apply_over(
                cx,
                live! {
                    draw_normal: { dark_mode: (dark_mode) }
                    draw_bold: { dark_mode: (dark_mode) }
                },
            );

            // Apply dark mode to voice clone modal
            inner
                .view
                .voice_clone_modal(ids!(voice_clone_modal))
                .update_dark_mode(cx, dark_mode);

            inner.view.redraw(cx);
        }
    }
}
