//! Screen - Moxin.tts style interface with sidebar layout
//! This is a variant of the TTS screen with a sidebar navigation similar to Moxin.tts

use crate::app_preferences::{self, AppPreferences};
use crate::audio_player::{
    default_input_device_name, default_output_device_name, list_input_devices, list_output_devices,
    TTSPlayer,
};
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
use std::sync::Arc;
use std::fs::OpenOptions;
use std::fs;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::sync::mpsc::{self, Receiver};
use std::thread;
use std::time::{Duration, SystemTime, UNIX_EPOCH};

/// Current page in the application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppPage {
    #[default]
    TextToSpeech,
    VoiceLibrary,
    VoiceClone,
    TaskDetail,
    UserSettings,
    Translation,
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

#[derive(Clone, Debug)]
enum DownloadSource {
    CurrentAudio,
    History(String),
}

#[derive(Clone, Copy, Debug)]
enum DownloadFormat {
    Mp3,
    Wav,
}

#[derive(Clone, Copy, Debug)]
enum ShareTarget {
    System,
    CapCut,
    Premiere,
    WeChat,
    Finder,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
enum RuntimeInitState {
    #[default]
    Idle,
    Running,
    Ready,
    Failed,
}

#[derive(Debug)]
enum RuntimeInitEvent {
    Stage { status: String, detail: String },
    DoneOk,
    DoneErr(String),
}

#[derive(Debug)]
enum DoraStartupEvent {
    Stage(String),
    Ready,
    Failed(String),
}

#[derive(Debug)]
enum QwenModelDownloadEvent {
    Stage(String),
    DoneOk,
    DoneErr(String),
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
    // Qwen3-only mode: only one backend available.
    // PrimeSpeech MLX entry removed. See doc/REFACTOR_QWEN3_ONLY.md to restore.
    vec![
        TtsModelOption {
            id: "qwen3_tts_mlx".to_string(),
            name: "Qwen3 TTS MLX".to_string(),
            description: "Qwen3 TTS MLX inference engine. Supports preset voices and zero-shot voice clone. Fast and lightweight on Apple Silicon.".to_string(),
            tag_labels: vec![
                "Chinese".to_string(),
                "English".to_string(),
                "Zero-shot".to_string(),
            ],
            badge: None,
        },
    ]
}

const TTS_INPUT_MAX_CHARS: usize = 1000;

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
            width: 520.0
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

    SettingsBodyLabel = <Label> {
        width: Fill, height: Fit
        draw_text: {
            instance dark_mode: 0.0
            text_style: { font_size: 12.0 }
            fn get_color(self) -> vec4 {
                let light = vec4(0.26, 0.29, 0.34, 1.0);
                let dark = vec4(0.84, 0.87, 0.92, 1.0);
                return mix(light, dark, self.dark_mode);
            }
        }
        text: "-"
    }

    SettingsSectionTitle = <Label> {
        width: Fit, height: Fit
        draw_text: {
            instance dark_mode: 0.0
            text_style: <FONT_SEMIBOLD>{ font_size: 15.0 }
            fn get_color(self) -> vec4 {
                let light = vec4(0.10, 0.14, 0.20, 1.0);
                let dark = vec4(0.92, 0.94, 0.98, 1.0);
                return mix(light, dark, self.dark_mode);
            }
        }
        text: "Section"
    }

    SettingsActionBtn = <Button> {
        width: Fit, height: 34
        padding: {left: 12, right: 12}
        draw_bg: {
            instance dark_mode: 0.0
            instance active: 0.0
            instance hover: 0.0
            instance pressed: 0.0
            instance border_radius: 8.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                let base = mix(vec4(0.90, 0.93, 0.97, 1.0), vec4(0.28, 0.33, 0.42, 1.0), self.dark_mode);
                let hover = mix(vec4(0.84, 0.89, 0.96, 1.0), vec4(0.32, 0.38, 0.48, 1.0), self.dark_mode);
                let pressed = mix(vec4(0.78, 0.84, 0.94, 1.0), vec4(0.35, 0.42, 0.53, 1.0), self.dark_mode);
                let active = mix((PRIMARY_500), (PRIMARY_400), self.dark_mode);
                let border = mix(vec4(0.57, 0.65, 0.77, 1.0), vec4(0.46, 0.55, 0.70, 1.0), self.dark_mode);
                let idle_color = mix(mix(base, hover, self.hover), pressed, self.pressed);
                sdf.fill(mix(idle_color, active, self.active));
                sdf.stroke(border, 1.0);
                return sdf.result;
            }
        }
        draw_text: {
            instance dark_mode: 0.0
            instance active: 0.0
            text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
            fn get_color(self) -> vec4 {
                let normal_light = vec4(0.16, 0.19, 0.23, 1.0);
                let normal_dark = vec4(0.89, 0.92, 0.96, 1.0);
                let normal = mix(normal_light, normal_dark, self.dark_mode);
                return mix(normal, (WHITE), self.active);
            }
        }
        text: "Action"
    }

    SettingsTabBtn = <Button> {
        width: Fit, height: 34
        padding: {left: 12, right: 12}
        draw_bg: {
            instance active: 0.0
            instance hover: 0.0
            instance pressed: 0.0
            instance dark_mode: 0.0
            instance border_radius: 9.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                let base = mix(vec4(0.93, 0.95, 0.98, 1.0), vec4(0.18, 0.22, 0.30, 1.0), self.dark_mode);
                let hover = mix(vec4(0.89, 0.93, 0.99, 1.0), vec4(0.22, 0.27, 0.36, 1.0), self.dark_mode);
                let pressed = mix(vec4(0.83, 0.89, 0.98, 1.0), vec4(0.26, 0.32, 0.41, 1.0), self.dark_mode);
                let active = mix(vec4(0.87, 0.93, 1.0, 1.0), vec4(0.22, 0.28, 0.43, 1.0), self.dark_mode);
                let border = mix(vec4(0.74, 0.80, 0.90, 1.0), vec4(0.36, 0.43, 0.56, 1.0), self.dark_mode);
                let idle = mix(mix(base, hover, self.hover), pressed, self.pressed);
                sdf.fill(mix(idle, active, self.active));
                sdf.stroke(border, 1.0);
                return sdf.result;
            }
        }
        draw_text: {
            instance active: 0.0
            instance dark_mode: 0.0
            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
            fn get_color(self) -> vec4 {
                let normal = mix(vec4(0.34, 0.38, 0.44, 1.0), vec4(0.68, 0.72, 0.78, 1.0), self.dark_mode);
                let active = mix(vec4(0.16, 0.35, 0.82, 1.0), vec4(0.52, 0.70, 1.0, 1.0), self.dark_mode);
                return mix(normal, active, self.active);
            }
        }
        text: "Tab"
    }

    SettingsDevicePopupMenu = <PopupMenu> {
        width: 640.0
        draw_bg: {
            instance dark_mode: 0.0
            border_size: 1.0
            border_radius: 8.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                let border = mix((SLATE_300), (SLATE_600), self.dark_mode);
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
                    let hover = mix((SLATE_100), (SLATE_700), self.dark_mode);
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

    SettingsDeviceDropDown = <DropDown> {
        width: Fill, height: 38
        padding: {left: 12, right: 34, top: 8, bottom: 8}
        margin: {top: 2, bottom: 2}
        popup_menu_position: BelowInput
        popup_menu: <SettingsDevicePopupMenu> {}
        draw_bg: {
            instance dark_mode: 0.0
            border_radius: 8.0
            border_size: 1.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                let bg = mix(vec4(0.96, 0.97, 0.99, 1.0), vec4(0.22, 0.27, 0.36, 1.0), self.dark_mode);
                let border = mix(vec4(0.64, 0.71, 0.82, 1.0), vec4(0.42, 0.50, 0.63, 1.0), self.dark_mode);
                sdf.fill(bg);
                sdf.stroke(border, self.border_size);
                return sdf.result;
            }
        }
        draw_text: {
            instance dark_mode: 0.0
            text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
            fn get_color(self) -> vec4 {
                return mix(vec4(0.14, 0.18, 0.24, 1.0), vec4(0.90, 0.93, 0.97, 1.0), self.dark_mode);
            }
        }
    }

    SettingsTextInput = <TextInput> {
        width: Fill, height: 38
        padding: {left: 12, right: 12, top: 9, bottom: 9}
        draw_bg: {
            instance dark_mode: 0.0
            instance border_radius: 8.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                let bg = mix(vec4(0.96, 0.97, 0.99, 1.0), vec4(0.20, 0.24, 0.33, 1.0), self.dark_mode);
                let border = mix(vec4(0.66, 0.73, 0.84, 1.0), vec4(0.40, 0.47, 0.61, 1.0), self.dark_mode);
                sdf.fill(bg);
                sdf.stroke(border, 1.0);
                return sdf.result;
            }
        }
        draw_text: {
            instance dark_mode: 0.0
            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
            fn get_color(self) -> vec4 {
                return mix(vec4(0.14, 0.18, 0.24, 1.0), vec4(0.92, 0.94, 0.98, 1.0), self.dark_mode);
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
    }

    SettingsCard = <RoundedView> {
        width: Fill, height: Fit
        flow: Down
        spacing: 12
        padding: {left: 18, right: 18, top: 16, bottom: 16}
        margin: {bottom: 6}
        draw_bg: {
            instance dark_mode: 0.0
            instance border_radius: 10.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0., 1., self.rect_size.x, self.rect_size.y - 1., 11.0);
                sdf.fill(vec4(0., 0., 0., 0.03));
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y - 2., 10.0);
                let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                sdf.fill(bg);
                return sdf.result;
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
        width: Fill, height: 40
        padding: {left: 20, right: 16}
        align: {x: 0.0, y: 0.5}

        draw_bg: {
            instance hover: 0.0
            instance active: 0.0
            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                // Subtle background for hover and active
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 6.0);
                let normal = vec4(0.0, 0.0, 0.0, 0.0);
                let hover_color = vec4(1.0, 1.0, 1.0, 0.06);
                let active_color = vec4(1.0, 1.0, 1.0, 0.10);
                let bg = mix(normal, hover_color, self.hover);
                let bg = mix(bg, active_color, self.active);
                sdf.fill(bg);
                // Left accent bar when active
                sdf.rect(0., 10., 3., self.rect_size.y - 20.);
                let bar_color = mix(vec4(0.0, 0.0, 0.0, 0.0), (MOXIN_PRIMARY_LIGHT), self.active);
                sdf.fill(bar_color);
                return sdf.result;
            }
        }

        draw_text: {
            instance active: 0.0
            text_style: <FONT_MEDIUM>{ font_size: 13.5 }
            fn get_color(self) -> vec4 {
                let normal = vec4(1.0, 1.0, 1.0, 0.55);
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
                    flow: Down
                    padding: {left: 24, right: 24, top: 28, bottom: 24}
                    align: {x: 0.0, y: 0.5}

                    logo_section = <View> {
                        width: Fill, height: Fit
                        flow: Down
                        align: {x: 0.0, y: 0.5}

                        logo_text = <Label> {
                            width: Fit, height: Fit
                            draw_text: {
                                text_style: <FONT_BOLD>{ font_size: 17.0 }
                                fn get_color(self) -> vec4 { return vec4(1.0, 1.0, 1.0, 0.95); }
                            }
                            text: "Moxin Voice"
                        }

                        logo_subtitle = <Label> {
                            width: Fit, height: Fit
                            margin: {top: -6}
                            draw_text: {
                                text_style: <FONT_REGULAR>{ font_size: 11.0 }
                                fn get_color(self) -> vec4 { return vec4(1.0, 1.0, 1.0, 0.35); }
                            }
                            text: "OminiX MLX"
                        }
                    }
                }

                // Sidebar Navigation
                sidebar_nav = <View> {
                    width: Fill, height: Fill
                    flow: Down
                    padding: {left: 12, right: 12, top: 8, bottom: 16}
                    spacing: 2

                    nav_tts = <NavItem> {
                        text: "Text to Speech"
                        draw_bg: { active: 1.0 }
                        draw_text: { active: 1.0 }
                    }

                    nav_library = <NavItem> {
                        text: "Voice Library"
                    }

                    nav_clone = <NavItem> {
                        text: "Voice Clone"
                    }

                    nav_history = <NavItem> {
                        text: "History"
                    }

                    nav_translation = <NavItem> {
                        text: "实时翻译"
                    }
                }

                // Sidebar Footer: User Info
                sidebar_footer = <View> {
                    width: Fill, height: Fit
                    flow: Right
                    padding: {left: 16, right: 14, top: 14, bottom: 14}
                    spacing: 10
                    align: {y: 0.5}
                    cursor: Hand

                    show_bg: true
                    draw_bg: {
                        instance active: 0.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            // Top separator line only
                            sdf.rect(16.0, 0.0, self.rect_size.x - 32.0, 1.0);
                            sdf.fill(vec4(1.0, 1.0, 1.0, 0.07));
                            // Hover fill
                            sdf.box(0.0, 1.0, self.rect_size.x, self.rect_size.y - 1.0, 0.0);
                            sdf.fill(mix(vec4(0.0, 0.0, 0.0, 0.0), vec4(1.0, 1.0, 1.0, 0.06), self.active));
                            return sdf.result;
                        }
                    }

                    user_avatar = <RoundedView> {
                        width: 32, height: 32
                        align: {x: 0.5, y: 0.5}
                        draw_bg: {
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.circle(self.rect_size.x * 0.5, self.rect_size.y * 0.5, 16.0);
                                sdf.fill((MOXIN_PRIMARY));
                                return sdf.result;
                            }
                        }

                        avatar_letter = <Label> {
                            width: Fill, height: Fill
                            padding: {left: 0.0, right: 0.0, top: 4.0, bottom: 0.0}
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
                padding: { left: 40, right: 40, top: 28, bottom: 16 }

            // Left column - we keep this structure for compatibility but it now contains everything
            left_column = <View> {
                width: Fill, height: Fill
                flow: Down
                spacing: 24
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
                        spacing: 12
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
                                text_style: <FONT_SEMIBOLD>{ font_size: 18.0 }
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
                            instance border_radius: 10.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 1., self.rect_size.x, self.rect_size.y - 1., 11.0);
                                sdf.fill(vec4(0., 0., 0., 0.03));
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y - 2., 10.0);
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
                        input_container = <ScrollYView> {
                            width: Fill, height: Fill
                            flow: Down
                            padding: {left: 24, right: 24, top: 24, bottom: 16}

                            text_input = <TextInput> {
                                width: Fill, height: Fit
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
                                    text: "0 / 1,000 字符"
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
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.box(0., 1., self.rect_size.x, self.rect_size.y - 1., 11.0);
                                sdf.fill(vec4(0., 0., 0., 0.03));
                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y - 2., 10.0);
                                let bg = mix((WHITE), (SLATE_900), self.dark_mode);
                                sdf.fill(bg);
                                return sdf.result;
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
                                        width: Fill, height: Fit
                                        margin: {left: 0, right: 0, top: 0, bottom: 0}
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
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                // Background
                                                sdf.rect(0., 0., self.rect_size.x, self.rect_size.y);
                                                let base = mix((WHITE), (SLATE_800), self.dark_mode);
                                                let hover_color = mix((SLATE_50), (SLATE_700), self.dark_mode);
                                                let selected_color = mix((PRIMARY_50), (PRIMARY_900), self.dark_mode);
                                                let color = mix(base, hover_color, self.hover);
                                                let color = mix(color, selected_color, self.selected);
                                                sdf.fill(color);
                                                // iOS-style bottom divider (inset after avatar)
                                                sdf.rect(66., self.rect_size.y - 1.0, self.rect_size.x - 66., 1.0);
                                                let divider = mix(vec4(0.0, 0.0, 0.0, 0.14), vec4(1.0, 1.0, 1.0, 0.14), self.dark_mode);
                                                sdf.fill(divider);
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
                                                padding: {left: 0.0, right: 0.0, top: 4.0, bottom: 0.0}
                                                align: {x: 0.5, y: 0.5}
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
                                                width: Fill, height: Fit
                                                padding: {top: 4, bottom: 2}
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

                                voice_picker_empty_container = <View> {
                                    width: Fill, height: Fit
                                    visible: false

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
                                    margin: {bottom: 6}
                                    flow: Down
                                    spacing: 8

                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        instance hover: 0.0
                                        instance border_radius: 10.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 1., self.rect_size.x, self.rect_size.y - 1., 11.0);
                                            sdf.fill(vec4(0., 0., 0., 0.03));
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y - 2., 10.0);
                                            let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                            let hover_bg = mix(vec4(0.98, 0.98, 0.99, 1.0), (SLATE_700), self.dark_mode);
                                            sdf.fill(mix(bg, hover_bg, self.hover));
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
                                            text: "Qwen3 TTS MLX"
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
                            flow: Down
                            spacing: 12

                            // Title row: library_title on left, controls on right
                            title_and_tags = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                align: {y: 0.5}
                                spacing: 16

                                library_title = <Label> {
                                    width: Fill, height: Fit
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 18.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((MOXIN_TEXT_PRIMARY), (MOXIN_TEXT_PRIMARY_DARK), self.dark_mode);
                                        }
                                    }
                                    text: "音色库"
                                }

                                // Search box
                                search_input = <TextInput> {
                                    width: 200, height: 40
                                    padding: {left: 12, right: 12, top: 10, bottom: 10}
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
                                        instance focus: 0.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, self.border_radius);
                                            sdf.fill(vec4(
                                                MOXIN_PRIMARY.x,
                                                MOXIN_PRIMARY.y,
                                                MOXIN_PRIMARY.z,
                                                self.focus
                                            ));
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

                                // Refresh button (rightmost)
                                refresh_btn = <Button> {
                                    width: Fit, height: 40
                                    padding: {left: 20, right: 20}
                                    text: "刷新"

                                    draw_bg: {
                                        instance hover: 0.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 8.0);
                                            let base = (MOXIN_PRIMARY);
                                            let hover_color = (MOXIN_PRIMARY_LIGHT);
                                            sdf.fill(mix(base, hover_color, self.hover));
                                            return sdf.result;
                                        }
                                    }

                                    draw_text: {
                                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                        fn get_color(self) -> vec4 {
                                            return vec4(1.0, 1.0, 1.0, 1.0);
                                        }
                                    }
                                }
                            }

                            // Category filter row
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
                                        margin: {bottom: 6}
                                        flow: Right
                                        spacing: 16
                                        align: {y: 0.5}

                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            instance hover: 0.0
                                            instance border_radius: 10.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 1., self.rect_size.x, self.rect_size.y - 1., 11.0);
                                                sdf.fill(vec4(0., 0., 0., 0.03));
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y - 2., 10.0);
                                                let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                                let hover_bg = mix(vec4(0.98, 0.98, 0.99, 1.0), (SLATE_700), self.dark_mode);
                                                sdf.fill(mix(bg, hover_bg, self.hover));
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
                                                width: Fill, height: Fill
                                                padding: {left: 0.0, right: 0.0, top: 4.0, bottom: 0.0}
                                                align: {x: 0.5, y: 0.5}
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
                        spacing: 12
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
                                        text_style: <FONT_SEMIBOLD>{ font_size: 18.0 }
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
                                text: "创建任务"

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
                                        margin: {bottom: 6}
                                        flow: Down
                                        spacing: 12

                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            instance hover: 0.0
                                            instance border_radius: 10.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 1., self.rect_size.x, self.rect_size.y - 1., 11.0);
                                                sdf.fill(vec4(0., 0., 0., 0.03));
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y - 2., 10.0);
                                                let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                                let hover_bg = mix(vec4(0.98, 0.98, 0.99, 1.0), (SLATE_700), self.dark_mode);
                                                sdf.fill(mix(bg, hover_bg, self.hover));
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
                            margin: {bottom: 6}
                            draw_bg: {
                                instance border_radius: 10.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 1., self.rect_size.x, self.rect_size.y - 1., 11.0);
                                    sdf.fill(vec4(0., 0., 0., 0.03));
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y - 2., 10.0);
                                    sdf.fill((WHITE));
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
                            margin: {bottom: 6}
                            draw_bg: {
                                instance border_radius: 10.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 1., self.rect_size.x, self.rect_size.y - 1., 11.0);
                                    sdf.fill(vec4(0., 0., 0., 0.03));
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y - 2., 10.0);
                                    sdf.fill((WHITE));
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

                    // User & Settings page
                    user_settings_page = <View> {
                        width: Fill, height: Fill
                        flow: Down
                        spacing: 18
                        padding: {left: 8, right: 8, top: 4, bottom: 16}
                        visible: false

                        user_settings_header = <View> {
                            width: Fill, height: Fit
                            flow: Right
                            align: {y: 0.5}

                            user_settings_title = <Label> {
                                width: Fit, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 18.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "用户设置"
                            }
                        }

                        settings_tab_bar = <View> {
                            width: Fill, height: Fit
                            flow: Right
                            spacing: 10

                            tab_profile_btn = <SettingsTabBtn> { text: "通用" }
                            tab_app_btn = <SettingsTabBtn> { text: "语音" }
                            tab_runtime_btn = <SettingsTabBtn> { text: "系统" }
                        }

                        settings_scroll = <ScrollYView> {
                            width: Fill, height: Fill
                            flow: Down
                            scroll_bars: <ScrollBars> {
                                show_scroll_x: false
                                show_scroll_y: true
                            }

                            settings_scroll_content = <View> {
                                width: Fill, height: Fit
                                flow: Down
                                spacing: 14
                                padding: {left: 6, right: 6, bottom: 24}

                        profile_panel = <View> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 14
                            visible: true

                        app_settings_card = <SettingsCard> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 16
                            padding: {left: 18, right: 18, top: 16, bottom: 16}
                            draw_bg: {
                                instance dark_mode: 0.0
                                instance border_radius: 12.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                    let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                    let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                                    sdf.fill(bg);
                                    sdf.stroke(border, 1.0);
                                    return sdf.result;
                                }
                            }

                            app_settings_title = <Label> {
                                width: Fit, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 15.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "通用设置"
                            }

                            language_section = <View> {
                                width: Fill, height: Fit
                                flow: Down
                                spacing: 10

                                language_title = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                        }
                                    }
                                    text: "语言"
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
                                                sdf.fill(mix(normal, (MOXIN_PRIMARY), self.active));
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
                                                sdf.fill(mix(normal, (MOXIN_PRIMARY), self.active));
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

                            theme_section = <View> {
                                width: Fill, height: Fit
                                flow: Down
                                spacing: 10

                                theme_title = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                        }
                                    }
                                    text: "主题"
                                }

                                theme_options = <View> {
                                    width: Fill, height: Fit
                                    flow: Right
                                    spacing: 12

                                    theme_light_option = <Button> {
                                        width: Fit, height: 36
                                        padding: {left: 16, right: 16}
                                        text: "☀️ 浅色"
                                        draw_bg: {
                                            instance active: 1.0
                                            instance dark_mode: 0.0
                                            instance border_radius: 8.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                let normal = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                                sdf.fill(mix(normal, (MOXIN_PRIMARY), self.active));
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
                                        text: "🌙 深色"
                                        draw_bg: {
                                            instance active: 0.0
                                            instance dark_mode: 0.0
                                            instance border_radius: 8.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                let normal = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                                sdf.fill(mix(normal, (MOXIN_PRIMARY), self.active));
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
                        }

                        profile_card = <SettingsCard> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 14
                            padding: {left: 18, right: 18, top: 16, bottom: 16}
                            draw_bg: {
                                instance dark_mode: 0.0
                                instance border_radius: 12.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                    let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                    let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                                    sdf.fill(bg);
                                    sdf.stroke(border, 1.0);
                                    return sdf.result;
                                }
                            }

                            profile_title = <Label> {
                                width: Fit, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 15.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "个人资料"
                            }

                            profile_body = <View> {
                                width: Fill, height: Fit
                                flow: Down
                                spacing: 10
                                align: {x: 0.0, y: 0.0}

                                profile_form = <View> {
                                    width: Fill, height: Fit
                                    flow: Down
                                    spacing: 10

                                    name_row = <View> {
                                        width: Fill, height: Fit
                                        flow: Down
                                        spacing: 6

                                        name_label = <Label> {
                                            width: Fit, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: { font_size: 12.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "用户名"
                                        }

                                        name_input = <SettingsTextInput> {
                                            width: Fill, height: 36
                                            empty_text: "输入用户名"
                                        }
                                    }
                                }
                            }

                            profile_actions = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                align: {x: 1.0}

                                save_profile_btn = <Button> {
                                    width: Fit, height: 36
                                    padding: {left: 16, right: 16}
                                    text: "保存"
                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        instance hover: 0.0
                                        instance border_radius: 8.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                            let base = mix((PRIMARY_500), (PRIMARY_400), self.dark_mode);
                                            let hover_color = mix((PRIMARY_600), (PRIMARY_300), self.dark_mode);
                                            sdf.fill(mix(base, hover_color, self.hover));
                                            return sdf.result;
                                        }
                                    }
                                    draw_text: {
                                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                        fn get_color(self) -> vec4 { return (WHITE); }
                                    }
                                }
                            }
                        }

                        } // End profile_panel

                        app_panel = <View> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 14
                            visible: false

                        defaults_card = <SettingsCard> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 10
                            padding: {left: 18, right: 18, top: 14, bottom: 14}
                            draw_bg: {
                                instance dark_mode: 0.0
                                instance border_radius: 12.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                    let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                    let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                                    sdf.fill(bg);
                                    sdf.stroke(border, 1.0);
                                    return sdf.result;
                                }
                            }

                            defaults_title = <Label> {
                                width: Fit, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "推理默认参数"
                            }

                            defaults_voice_label = <Label> {
                                width: Fill, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: { font_size: 12.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "默认音色: Doubao"
                            }

                            defaults_row = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                spacing: 10

                                speed_col = <View> {
                                    width: Fill, height: Fit
                                    flow: Down
                                    spacing: 6

                                    speed_label = <SettingsBodyLabel> {
                                        width: Fit, height: Fit
                                        text: "语速"
                                    }
                                    speed_input = <SettingsTextInput> { width: Fill, height: 38 empty_text: "1.0" }
                                }

                                pitch_col = <View> {
                                    width: Fill, height: Fit
                                    flow: Down
                                    spacing: 6

                                    pitch_label = <SettingsBodyLabel> {
                                        width: Fit, height: Fit
                                        text: "音高"
                                    }
                                    pitch_input = <SettingsTextInput> { width: Fill, height: 38 empty_text: "0.0" }
                                }

                                volume_col = <View> {
                                    width: Fill, height: Fit
                                    flow: Down
                                    spacing: 6

                                    volume_label = <SettingsBodyLabel> {
                                        width: Fit, height: Fit
                                        text: "音量"
                                    }
                                    volume_input = <SettingsTextInput> { width: Fill, height: 38 empty_text: "100" }
                                }
                            }

                            defaults_actions = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                spacing: 10

                                save_defaults_btn = <SettingsActionBtn> {
                                    width: Fit, height: 34
                                    padding: {left: 12, right: 12}
                                    text: "保存默认"
                                }
                                apply_defaults_now_btn = <SettingsActionBtn> {
                                    width: Fit, height: 34
                                    padding: {left: 12, right: 12}
                                    text: "应用到当前"
                                }
                            }
                        }

                        devices_card = <SettingsCard> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 12
                            padding: {left: 18, right: 18, top: 14, bottom: 14}
                            draw_bg: {
                                instance dark_mode: 0.0
                                instance border_radius: 12.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                    let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                    let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                                    sdf.fill(bg);
                                    sdf.stroke(border, 1.0);
                                    return sdf.result;
                                }
                            }

                            devices_header = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                align: {y: 0.5}
                                spacing: 8
                                devices_title = <SettingsSectionTitle> { width: Fill, height: Fit text: "音频设备" }
                                refresh_devices_btn = <SettingsActionBtn> {
                                    width: Fit, height: 34
                                    padding: {left: 12, right: 12}
                                    text: "刷新设备"
                                }
                            }

                            input_pick_row = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                spacing: 12
                                align: {y: 0.5}
                                input_pick_label = <SettingsBodyLabel> {
                                    width: 76, height: Fit
                                    text: "输入"
                                }
                                input_device_dropdown = <SettingsDeviceDropDown> {
                                    width: Fill, height: 38
                                }
                            }

                            output_pick_row = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                spacing: 12
                                align: {y: 0.5}
                                output_pick_label = <SettingsBodyLabel> {
                                    width: 76, height: Fit
                                    text: "输出"
                                }
                                output_device_dropdown = <SettingsDeviceDropDown> {
                                    width: Fill, height: 38
                                }
                            }
                        }

                        } // End app_panel

                        runtime_panel = <View> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 14
                            visible: false

                        runtime_card = <SettingsCard> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 8
                            padding: {left: 18, right: 18, top: 14, bottom: 14}
                            draw_bg: {
                                instance dark_mode: 0.0
                                instance border_radius: 12.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                    let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                    let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                                    sdf.fill(bg);
                                    sdf.stroke(border, 1.0);
                                    return sdf.result;
                                }
                            }

                            runtime_title = <Label> {
                                width: Fit, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "运行状态"
                            }

                            dora_status = <SettingsBodyLabel> { width: Fill, height: Fit text: "Dora: -" }
                            asr_status = <SettingsBodyLabel> { width: Fill, height: Fit text: "ASR: -" }
                            tts_status = <SettingsBodyLabel> { width: Fill, height: Fit text: "TTS: -" }

                            runtime_refresh_btn = <SettingsActionBtn> {
                                width: Fit, height: 34
                                padding: {left: 12, right: 12}
                                text: "刷新状态"
                            }
                        }

                        paths_card = <SettingsCard> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 8
                            padding: {left: 18, right: 18, top: 14, bottom: 14}
                            draw_bg: {
                                instance dark_mode: 0.0
                                instance border_radius: 12.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                    let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                    let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                                    sdf.fill(bg);
                                    sdf.stroke(border, 1.0);
                                    return sdf.result;
                                }
                            }

                            paths_title = <SettingsSectionTitle> { width: Fit, height: Fit text: "本地路径与资源" }
                            model_path_label = <SettingsBodyLabel> { width: Fill, height: Fit text: "Models: -" }
                            log_path_label = <SettingsBodyLabel> { width: Fill, height: Fit text: "Logs: -" }
                            workspace_path_label = <SettingsBodyLabel> { width: Fill, height: Fit text: "Workspace: -" }

                            path_actions = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                spacing: 8
                                open_model_dir_btn = <SettingsActionBtn> { width: Fit, height: 34 padding: {left: 10, right: 10} text: "打开模型目录" }
                                open_log_dir_btn = <SettingsActionBtn> { width: Fit, height: 34 padding: {left: 10, right: 10} text: "打开日志目录" }
                                open_workspace_dir_btn = <SettingsActionBtn> { width: Fit, height: 34 padding: {left: 10, right: 10} text: "打开工作目录" }
                            }

                            clear_cache_btn = <SettingsActionBtn> {
                                width: Fit, height: 34
                                padding: {left: 12, right: 12}
                                text: "清理缓存"
                            }
                        }

                        privacy_card = <SettingsCard> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 8
                            padding: {left: 18, right: 18, top: 14, bottom: 14}
                            draw_bg: {
                                instance dark_mode: 0.0
                                instance border_radius: 12.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                    let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                    let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                                    sdf.fill(bg);
                                    sdf.stroke(border, 1.0);
                                    return sdf.result;
                                }
                            }

                            privacy_title = <SettingsSectionTitle> { width: Fit, height: Fit text: "隐私与数据" }
                            retention_pick_row = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                spacing: 12
                                align: {y: 0.5}
                                retention_pick_label = <SettingsBodyLabel> {
                                    width: 110, height: Fit
                                    text: "历史保留"
                                }
                                retention_dropdown = <SettingsDeviceDropDown> {
                                    width: Fill, height: 38
                                }
                            }

                            privacy_actions = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                spacing: 8
                                clear_tts_history_btn = <SettingsActionBtn> { width: Fit, height: 34 padding: {left: 10, right: 10} text: "清空 TTS 历史" }
                                clear_training_artifacts_btn = <SettingsActionBtn> { width: Fit, height: 34 padding: {left: 10, right: 10} text: "清理训练中间产物" }
                            }
                        }

                        experiments_card = <SettingsCard> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 8
                            padding: {left: 18, right: 18, top: 14, bottom: 14}
                            draw_bg: {
                                instance dark_mode: 0.0
                                instance border_radius: 12.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                    let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                    let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                                    sdf.fill(bg);
                                    sdf.stroke(border, 1.0);
                                    return sdf.result;
                                }
                            }

                            experiments_title = <SettingsSectionTitle> { width: Fit, height: Fit text: "实验功能" }

                            zero_shot_backend_pick_row = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                spacing: 12
                                align: {y: 0.5}
                                zero_shot_backend_pick_label = <SettingsBodyLabel> {
                                    width: 110, height: Fit
                                    text: "Zero-shot 后端"
                                }
                                zero_shot_backend_dropdown = <SettingsDeviceDropDown> {
                                    width: Fill, height: 38
                                }
                            }

                            backend_pick_row = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                spacing: 12
                                align: {y: 0.5}
                                backend_pick_label = <SettingsBodyLabel> {
                                    width: 110, height: Fit
                                    text: "训练后端"
                                }
                                training_backend_dropdown = <SettingsDeviceDropDown> {
                                    width: Fill, height: 38
                                }
                            }

                            debug_pick_row = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                spacing: 12
                                align: {y: 0.5}
                                debug_pick_label = <SettingsBodyLabel> {
                                    width: 110, height: Fit
                                    text: "Debug 日志"
                                }
                                debug_logs_dropdown = <SettingsDeviceDropDown> {
                                    width: Fill, height: 38
                                }
                            }

                            qwen_status_row = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                spacing: 12
                                align: {y: 0.5}
                                qwen_status_label = <SettingsBodyLabel> {
                                    width: 110, height: Fit
                                    text: "Qwen 模型"
                                }
                                qwen_status_value = <SettingsBodyLabel> {
                                    width: Fill, height: Fit
                                    text: "未就绪"
                                }
                            }
                        }

                        about_card = <SettingsCard> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 8
                            padding: {left: 18, right: 18, top: 14, bottom: 14}
                            draw_bg: {
                                instance dark_mode: 0.0
                                instance border_radius: 12.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                    let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                    let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                                    sdf.fill(bg);
                                    sdf.stroke(border, 1.0);
                                    return sdf.result;
                                }
                            }

                            about_section_title = <SettingsSectionTitle> { width: Fit, height: Fit text: "关于" }

                            about_version_label = <SettingsBodyLabel> {
                                width: Fill, height: Fit
                                text: "Moxin Voice v0.1.0"
                            }

                            about_engine_label = <SettingsBodyLabel> {
                                width: Fill, height: Fit
                                text: "Powered by OminiX MLX · Qwen3-TTS-MLX"
                            }

                            about_ominix_label = <SettingsBodyLabel> {
                                width: Fill, height: Fit
                                text: "github.com/OminiX-ai/OminiX-MLX"
                            }
                        }

                        } // End runtime_panel

                        data_panel = <View> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 14
                            visible: false
                        } // End data_panel
                            } // End settings_scroll_content
                        } // End settings_scroll
                    } // End user_settings_page

                    // ============ 实时翻译 Page ============
                    translation_page = <View> {
                        width: Fill, height: Fill
                        flow: Down
                        spacing: 0
                        visible: false

                        // Page header
                        page_header = <View> {
                            width: Fill, height: Fit
                            flow: Right
                            align: {y: 0.5}
                            padding: {bottom: 16}

                            page_title = <Label> {
                                width: Fit, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 18.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "实时翻译"
                            }

                            <View> { width: Fill, height: 1 }

                            // Status badge (hidden when not running)
                            translation_status_badge = <View> {
                                width: Fit, height: Fit
                                flow: Right
                                spacing: 6
                                align: {y: 0.5}
                                padding: {left: 10, right: 10, top: 4, bottom: 4}
                                visible: false
                                show_bg: true
                                draw_bg: {
                                    instance dark_mode: 0.0
                                    instance border_radius: 12.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        sdf.fill(mix(vec4(0.098, 0.725, 0.506, 0.15), vec4(0.098, 0.725, 0.506, 0.22), self.dark_mode));
                                        return sdf.result;
                                    }
                                }

                                translation_status_dot = <View> {
                                    width: 8, height: 8
                                    show_bg: true
                                    draw_bg: {
                                        instance border_radius: 4.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.circle(4.0, 4.0, 3.5);
                                            sdf.fill(vec4(0.098, 0.725, 0.506, 1.0));
                                            return sdf.result;
                                        }
                                    }
                                }

                                translation_status_text = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_MEDIUM>{ font_size: 11.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix(vec4(0.098, 0.725, 0.506, 1.0), vec4(0.42, 0.90, 0.68, 1.0), self.dark_mode);
                                        }
                                    }
                                    text: "运行中"
                                }
                            }
                        }

                        // Body: Settings panel and Running panel stacked via Overlay
                        translation_body = <View> {
                            width: Fill, height: Fill
                            flow: Overlay

                            // ── 设置面板（启动前）─────────────────────────────
                            translation_settings_panel = <View> {
                                width: Fill, height: Fill
                                flow: Down
                                spacing: 12
                                visible: true

                                // ── 设置卡片组 ──────────────────────────────────
                                settings_card = <RoundedView> {
                                    width: Fill, height: Fit
                                    flow: Down
                                    spacing: 0
                                    padding: 0
                                    show_bg: true
                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        instance border_radius: 10.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                            let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                            let border = mix((SLATE_200), (SLATE_700), self.dark_mode);
                                            sdf.fill(bg);
                                            sdf.stroke(border, 1.0);
                                            return sdf.result;
                                        }
                                    }

                                    // 输入源
                                    setting_row_source = <View> {
                                        width: Fill, height: 52
                                        flow: Right
                                        align: {y: 0.5}
                                        padding: {left: 16, right: 16}
                                        spacing: 12

                                        translation_source_label = <Label> {
                                            width: 90, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_MEDIUM>{ font_size: 13.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "输入源"
                                        }

                                        translation_source_dropdown = <SettingsDeviceDropDown> {
                                            width: Fill, height: 32
                                            labels: ["系统默认麦克风"]
                                            values: ["default"]
                                        }
                                    }

                                    // 分隔线
                                    <View> {
                                        width: Fill, height: 1
                                        margin: {left: 16, right: 16}
                                        show_bg: true
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            fn pixel(self) -> vec4 {
                                                return mix((SLATE_100), (SLATE_700), self.dark_mode);
                                            }
                                        }
                                    }

                                    // 输入语言
                                    setting_row_src_lang = <View> {
                                        width: Fill, height: 52
                                        flow: Right
                                        align: {y: 0.5}
                                        padding: {left: 16, right: 16}
                                        spacing: 12

                                        translation_src_lang_label = <Label> {
                                            width: 90, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_MEDIUM>{ font_size: 13.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "输入语言"
                                        }

                                        src_lang_dropdown = <SettingsDeviceDropDown> {
                                            width: Fill, height: 32
                                            labels: ["中文", "英语", "日语", "法语"]
                                            values: ["zh", "en", "ja", "fr"]
                                        }
                                    }

                                    // 分隔线
                                    <View> {
                                        width: Fill, height: 1
                                        margin: {left: 16, right: 16}
                                        show_bg: true
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            fn pixel(self) -> vec4 {
                                                return mix((SLATE_100), (SLATE_700), self.dark_mode);
                                            }
                                        }
                                    }

                                    // 目标语言
                                    setting_row_tgt_lang = <View> {
                                        width: Fill, height: 52
                                        flow: Right
                                        align: {y: 0.5}
                                        padding: {left: 16, right: 16}
                                        spacing: 12

                                        translation_tgt_lang_label = <Label> {
                                            width: 90, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_MEDIUM>{ font_size: 13.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "目标语言"
                                        }

                                        tgt_lang_dropdown = <SettingsDeviceDropDown> {
                                            width: Fill, height: 32
                                            labels: ["英语", "中文", "日语", "法语"]
                                            values: ["en", "zh", "ja", "fr"]
                                        }
                                    }

                                    // 分隔线
                                    <View> {
                                        width: Fill, height: 1
                                        margin: {left: 16, right: 16}
                                        show_bg: true
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            fn pixel(self) -> vec4 {
                                                return mix((SLATE_100), (SLATE_700), self.dark_mode);
                                            }
                                        }
                                    }

                                    // 浮窗样式
                                    setting_row_overlay = <View> {
                                        width: Fill, height: 52
                                        flow: Right
                                        align: {y: 0.5}
                                        padding: {left: 16, right: 16}
                                        spacing: 8

                                        translation_overlay_style_label = <Label> {
                                            width: 90, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_MEDIUM>{ font_size: 13.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "浮窗样式"
                                        }

                                        overlay_style_compact = <Button> {
                                            width: Fit, height: 28
                                            padding: {left: 12, right: 12}
                                            text: "紧凑"
                                            draw_bg: {
                                                instance active: 1.0
                                                instance dark_mode: 0.0
                                                instance border_radius: 6.0
                                                fn pixel(self) -> vec4 {
                                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                    let act = vec4(0.231, 0.435, 0.831, 1.0);
                                                    let inact = mix(vec4(0.0, 0.0, 0.0, 0.0), vec4(0.16, 0.20, 0.28, 0.0), self.dark_mode);
                                                    sdf.fill(mix(inact, act, self.active));
                                                    let inactive_border = mix(vec4(0.231, 0.435, 0.831, 0.35), vec4(0.42, 0.50, 0.63, 1.0), self.dark_mode);
                                                    sdf.stroke(mix(inactive_border, vec4(0.0,0.0,0.0,0.0), self.active), 1.0);
                                                    return sdf.result;
                                                }
                                            }
                                            draw_text: {
                                                instance active: 1.0
                                                instance dark_mode: 0.0
                                                text_style: <FONT_MEDIUM>{ font_size: 12.0 }
                                                fn get_color(self) -> vec4 {
                                                    let normal = mix(vec4(0.231, 0.435, 0.831, 1.0), vec4(0.68, 0.78, 0.98, 1.0), self.dark_mode);
                                                    return mix(normal, vec4(1.0,1.0,1.0,1.0), self.active);
                                                }
                                            }
                                        }

                                        overlay_style_full = <Button> {
                                            width: Fit, height: 28
                                            padding: {left: 12, right: 12}
                                            text: "全屏"
                                            draw_bg: {
                                                instance active: 0.0
                                                instance dark_mode: 0.0
                                                instance border_radius: 6.0
                                                fn pixel(self) -> vec4 {
                                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                                    let act = vec4(0.231, 0.435, 0.831, 1.0);
                                                    let inact = mix(vec4(0.0, 0.0, 0.0, 0.0), vec4(0.16, 0.20, 0.28, 0.0), self.dark_mode);
                                                    sdf.fill(mix(inact, act, self.active));
                                                    let inactive_border = mix(vec4(0.231, 0.435, 0.831, 0.35), vec4(0.42, 0.50, 0.63, 1.0), self.dark_mode);
                                                    sdf.stroke(mix(inactive_border, vec4(0.0,0.0,0.0,0.0), self.active), 1.0);
                                                    return sdf.result;
                                                }
                                            }
                                            draw_text: {
                                                instance active: 0.0
                                                instance dark_mode: 0.0
                                                text_style: <FONT_MEDIUM>{ font_size: 12.0 }
                                                fn get_color(self) -> vec4 {
                                                    let normal = mix(vec4(0.231, 0.435, 0.831, 1.0), vec4(0.68, 0.78, 0.98, 1.0), self.dark_mode);
                                                    return mix(normal, vec4(1.0,1.0,1.0,1.0), self.active);
                                                }
                                            }
                                        }
                                    }

                                    // 分隔线
                                    <View> {
                                        width: Fill, height: 1
                                        margin: {left: 16, right: 16}
                                        show_bg: true
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            fn pixel(self) -> vec4 {
                                                return mix((SLATE_100), (SLATE_700), self.dark_mode);
                                            }
                                        }
                                    }

                                    // 文字大小
                                    setting_row_font_size = <View> {
                                        width: Fill, height: 52
                                        flow: Right
                                        align: {y: 0.5}
                                        padding: {left: 16, right: 16}
                                        spacing: 12

                                        translation_font_size_label = <Label> {
                                            width: 90, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_MEDIUM>{ font_size: 13.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "文字大小"
                                        }

                                        font_size_dropdown = <SettingsDeviceDropDown> {
                                            width: Fill, height: 32
                                            labels: ["小", "正常", "大"]
                                            values: ["small", "normal", "large"]
                                        }
                                    }

                                    // 分隔线
                                    <View> {
                                        width: Fill, height: 1
                                        margin: {left: 16, right: 16}
                                        show_bg: true
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            fn pixel(self) -> vec4 {
                                                return mix((SLATE_100), (SLATE_700), self.dark_mode);
                                            }
                                        }
                                    }

                                    // 滚动位置
                                    setting_row_anchor_position = <View> {
                                        width: Fill, height: 52
                                        flow: Right
                                        align: {y: 0.5}
                                        padding: {left: 16, right: 16}
                                        spacing: 12

                                        translation_anchor_position_label = <Label> {
                                            width: 90, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_MEDIUM>{ font_size: 13.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "滚动位置"
                                        }

                                        anchor_position_dropdown = <SettingsDeviceDropDown> {
                                            width: Fill, height: 32
                                            labels: ["50%", "60%", "70%", "80%", "90%", "100%"]
                                            values: ["50", "60", "70", "80", "90", "100"]
                                        }
                                    }

                                    // 分隔线
                                    <View> {
                                        width: Fill, height: 1
                                        margin: {left: 16, right: 16}
                                        show_bg: true
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            fn pixel(self) -> vec4 {
                                                return mix((SLATE_100), (SLATE_700), self.dark_mode);
                                            }
                                        }
                                    }

                                    // 浮窗透明度
                                    setting_row_opacity = <View> {
                                        width: Fill, height: 52
                                        flow: Right
                                        align: {y: 0.5}
                                        padding: {left: 16, right: 16}
                                        spacing: 12

                                        translation_opacity_label = <Label> {
                                            width: 90, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_MEDIUM>{ font_size: 13.0 }
                                                fn get_color(self) -> vec4 {
                                                    return mix((MOXIN_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                }
                                            }
                                            text: "浮窗不透明度"
                                        }

                                        opacity_dropdown = <SettingsDeviceDropDown> {
                                            width: Fill, height: 32
                                            labels: ["100%", "90%", "85%", "75%", "65%", "50%", "35%"]
                                            values: ["1.0", "0.9", "0.85", "0.75", "0.65", "0.5", "0.35"]
                                        }
                                    }
                                    // 分隔线
                                    <View> {
                                        width: Fill, height: 1
                                        margin: {left: 16, right: 16}
                                        show_bg: true
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            fn pixel(self) -> vec4 {
                                                return mix((SLATE_100), (SLATE_700), self.dark_mode);
                                            }
                                        }
                                    }

                                } // End settings_card

                                // Spacer
                                <View> { width: Fill, height: Fill }

                                // 屏幕录制权限提示（macOS，仅在权限被拒绝时显示）
                                translation_permission_hint = <View> {
                                    width: Fill, height: Fit
                                    visible: false
                                    padding: {top: 0, bottom: 8}
                                    translation_permission_hint_label = <Label> {
                                        width: Fill, height: Fit
                                        draw_text: {
                                            text_style: <FONT_REGULAR>{ font_size: 12.0 }
                                            fn get_color(self) -> vec4 {
                                                return #F59E0B;
                                            }
                                            wrap: Word
                                        }
                                        text: "屏幕录制权限未授权。请前往系统设置 → 隐私与安全性 → 屏幕录制，启用 Moxin Voice，然后重启应用。"
                                    }
                                }

                                // 启动按钮
                                translation_start_btn = <Button> {
                                    width: Fill, height: 48
                                    text: "启动实时翻译"
                                    draw_bg: {
                                        instance border_radius: 10.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                            sdf.fill(vec4(0.231, 0.435, 0.831, 1.0));
                                            return sdf.result;
                                        }
                                    }
                                    draw_text: {
                                        text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                        fn get_color(self) -> vec4 { return vec4(1.0, 1.0, 1.0, 1.0); }
                                    }
                                }
                            } // End translation_settings_panel

                            // ── 运行面板（启动后）─────────────────────────────
                            translation_running_panel = <View> {
                                width: Fill, height: Fill
                                flow: Down
                                spacing: 12
                                visible: false

                                // 日志区域
                                translation_log_card = <RoundedView> {
                                    width: Fill, height: Fill
                                    flow: Down
                                    spacing: 0
                                    show_bg: true
                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        instance border_radius: 10.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                            sdf.fill(mix((WHITE), (SLATE_800), self.dark_mode));
                                            sdf.stroke(mix((SLATE_200), (SLATE_700), self.dark_mode), 1.0);
                                            return sdf.result;
                                        }
                                    }

                                    // 日志头
                                    <View> {
                                        width: Fill, height: 36
                                        flow: Right
                                        align: {y: 0.5}
                                        padding: {left: 14, right: 14}

                                        translation_log_title = <Label> {
                                            width: Fill, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                                                fn get_color(self) -> vec4 { return mix((MOXIN_TEXT_SECONDARY), (MOXIN_TEXT_SECONDARY_DARK), self.dark_mode); }
                                            }
                                            text: "运行日志"
                                        }
                                    }

                                    <View> {
                                        width: Fill, height: 1
                                        show_bg: true
                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            fn pixel(self) -> vec4 { return mix((SLATE_100), (SLATE_700), self.dark_mode); }
                                        }
                                    }

                                    translation_log_scroll = <ScrollYView> {
                                        width: Fill, height: Fill
                                        flow: Down
                                        padding: {left: 14, right: 14, top: 10, bottom: 10}
                                        spacing: 2

                                        translation_log_label = <Label> {
                                            width: Fill, height: Fit
                                            draw_text: {
                                                instance dark_mode: 0.0
                                                text_style: <FONT_REGULAR>{ font_size: 11.5 }
                                                wrap: Word
                                                fn get_color(self) -> vec4 { return mix(vec4(0.3,0.35,0.42,1.0), vec4(0.65,0.7,0.78,1.0), self.dark_mode); }
                                            }
                                            text: ""
                                        }
                                    }
                                } // End translation_log_card

                                // 显示浮窗按钮 — 在用户用红叉关闭浮窗后重新唤起
                                translation_show_overlay_btn = <Button> {
                                    width: Fill, height: 40
                                    text: "显示浮窗"
                                    draw_bg: {
                                        instance border_radius: 10.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                            sdf.fill(vec4(0.18, 0.42, 0.85, 1.0));
                                            return sdf.result;
                                        }
                                    }
                                    draw_text: {
                                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                        fn get_color(self) -> vec4 { return vec4(1.0, 1.0, 1.0, 1.0); }
                                    }
                                }

                                // 停止按钮
                                translation_stop_btn = <Button> {
                                    width: Fill, height: 44
                                    text: "停止翻译"
                                    draw_bg: {
                                        instance border_radius: 10.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                            sdf.fill(vec4(0.85, 0.25, 0.25, 1.0));
                                            return sdf.result;
                                        }
                                    }
                                    draw_text: {
                                        text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                        fn get_color(self) -> vec4 { return vec4(1.0, 1.0, 1.0, 1.0); }
                                    }
                                }
                            } // End translation_running_panel
                        } // End translation_body
                    } // End translation_page

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
                        instance border_radius: 10.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0., 1., self.rect_size.x, self.rect_size.y - 1., 11.0);
                            sdf.fill(vec4(0., 0., 0., 0.03));
                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y - 2., 10.0);
                            let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                            sdf.fill(bg);
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
                        padding: {left: 0.0, right: 0.0, top: 4.0, bottom: 0.0}
                        align: {x: 0.5, y: 0.5}
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
                    width: Fill, height: Fit
                    flow: Down
                    spacing: 2

                    voice_name_clip = <View> {
                        width: Fill, height: Fit
                        flow: Right

                        voice_name_scroller = <View> {
                            width: Fit, height: Fit
                            flow: Right
                            spacing: 24

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

                            current_voice_name_clone = <Label> {
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
                        }
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

        download_modal = <View> {
            width: Fill, height: Fill
            flow: Overlay
            align: {x: 0.5, y: 0.5}
            visible: false

            download_backdrop = <View> {
                width: Fill, height: Fill
                show_bg: true
                draw_bg: {
                    fn pixel(self) -> vec4 {
                        return vec4(0.0, 0.0, 0.0, 0.45);
                    }
                }
            }

            download_dialog = <RoundedView> {
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

                download_header = <View> {
                    width: Fill, height: Fit
                    padding: {left: 20, right: 20, top: 18, bottom: 12}
                    flow: Down
                    spacing: 6

                    download_title = <Label> {
                        width: Fill, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_BOLD>{ font_size: 17.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                            }
                        }
                        text: "下载音频"
                    }

                    download_subtitle = <Label> {
                        width: Fill, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: { font_size: 12.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                            }
                        }
                        text: "选择要导出的音频格式"
                    }
                }

                download_actions = <View> {
                    width: Fill, height: Fit
                    flow: Down
                    spacing: 8
                    padding: {left: 20, right: 20, top: 0, bottom: 14}

                    download_mp3_btn = <Button> {
                        width: Fill, height: 38
                        text: "MP3 文件"
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

                    download_wav_btn = <Button> {
                        width: Fill, height: 38
                        text: "WAV 文件"
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

                download_footer = <View> {
                    width: Fill, height: Fit
                    padding: {left: 20, right: 20, top: 0, bottom: 16}
                    flow: Right
                    align: {x: 1.0, y: 0.5}

                    download_cancel_btn = <Button> {
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
                        width: Fill, height: Fit
                        margin: {left: 0, right: 0, top: 0, bottom: 0}
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
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                // Background
                                sdf.rect(0., 0., self.rect_size.x, self.rect_size.y);
                                let base = mix((WHITE), (SLATE_800), self.dark_mode);
                                let hover_color = mix((SLATE_50), (SLATE_700), self.dark_mode);
                                let selected_color = mix((PRIMARY_50), (PRIMARY_900), self.dark_mode);
                                let color = mix(base, hover_color, self.hover);
                                let color = mix(color, selected_color, self.selected);
                                sdf.fill(color);
                                // iOS-style bottom divider (inset after avatar)
                                sdf.rect(78., self.rect_size.y - 1.0, self.rect_size.x - 78., 1.0);
                                let divider = mix(vec4(0.0, 0.0, 0.0, 0.14), vec4(1.0, 1.0, 1.0, 0.14), self.dark_mode);
                                sdf.fill(divider);
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
                                padding: {left: 0.0, right: 0.0, top: 4.0, bottom: 0.0}
                                align: {x: 0.5, y: 0.5}
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
                                width: Fill, height: Fit
                                padding: {top: 4, bottom: 2}
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

                voice_picker_empty_container = <View> {
                    width: Fill, height: Fit
                    visible: false

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
                flow: Overlay
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

                // Existing content
                dialog_content = <View> {
                    width: Fill, height: Fill
                    flow: Down
                    spacing: 0

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

                } // end dialog_content

                // Semi-transparent overlay shown while switching inference backend
                switching_overlay = <View> {
                    width: Fill, height: Fill
                    visible: false
                    flow: Down
                    align: {x: 0.5, y: 0.5}
                    spacing: 16

                    show_bg: true
                    draw_bg: {
                        instance border_radius: 16.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                            sdf.fill(vec4(0.0, 0.0, 0.0, 0.55));
                            return sdf.result;
                        }
                    }

                    switching_spinner = <View> {
                        width: 36, height: 36
                        show_bg: true
                        draw_bg: {
                            instance phase: 0.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                let cx = self.rect_size.x * 0.5;
                                let cy = self.rect_size.y * 0.5;
                                let radius = min(cx, cy) - 3.0;
                                let pi = 3.141592653589793;
                                sdf.circle(cx, cy, radius);
                                sdf.stroke(vec4(1.0, 1.0, 1.0, 0.2), 3.0);
                                let start = self.phase * 2.0 * pi;
                                let end = start + pi * 1.5;
                                sdf.arc_round_caps(cx, cy, radius, start, end, 3.0);
                                sdf.fill(vec4(0.39, 0.40, 0.95, 1.0));
                                return sdf.result;
                            }
                        }
                    }

                    switching_status = <Label> {
                        width: Fit, height: Fit
                        draw_text: {
                            text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                            fn get_color(self) -> vec4 { return (WHITE); }
                        }
                        text: "正在切换"
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
                width: 400, height: 560
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
                        text: "Settings"
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
                            text: "Language"
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
                            text: "Theme"
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
                            text: "About"
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
                            text: "Moxin Voice v0.1.0"
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
                            text: "Powered by OminiX MLX · Qwen3-TTS-MLX"
                        }

                        about_ominix = <Label> {
                            width: Fit, height: Fit
                            draw_text: {
                                instance dark_mode: 0.0
                                text_style: { font_size: 11.0 }
                                fn get_color(self) -> vec4 {
                                    return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                }
                            }
                            text: "github.com/OminiX-ai/OminiX-MLX"
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
                        text: "Moxin Voice"
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
    translation_dora: Option<DoraIntegration>,

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
    voice_name_marquee_offset: f64,
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
    dora_start_attempt_at: Option<std::time::Instant>,
    #[rust]
    dora_start_in_flight: bool,
    #[rust]
    dora_start_rx: Option<Receiver<DoraStartupEvent>>,
    #[rust]
    dora_pending_dataflow_path: Option<PathBuf>,
    /// TTS dataflow was stopped to make room for live translation.
    /// Auto-restart is suppressed while this is true.
    #[rust]
    tts_paused_for_translation: bool,

    #[rust]
    backend_switching: bool,
    #[rust]
    backend_switch_bridges_dropped: bool,

    #[rust]
    loading_dismissed: bool,

    #[rust]
    spinner_phase: f64,

    #[rust]
    runtime_init_state: RuntimeInitState,
    #[rust]
    runtime_init_rx: Option<Receiver<RuntimeInitEvent>>,
    #[rust]
    runtime_init_status_text: String,
    #[rust]
    runtime_init_detail_text: String,
    #[rust]
    qwen_download_in_progress: bool,
    #[rust]
    qwen_download_rx: Option<Receiver<QwenModelDownloadEvent>>,
    #[rust]
    qwen_model_status_text: String,

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
    #[rust]
    user_display_name: String,
    #[rust]
    user_avatar_letter: String,
    #[rust]
    user_profile_customized: bool,
    #[rust]
    user_settings_tab: u8,
    #[rust]
    app_preferences: AppPreferences,
    #[rust]
    runtime_status_dora: String,
    #[rust]
    runtime_status_asr: String,
    #[rust]
    runtime_status_tts: String,
    #[rust]
    runtime_status_model: String,
    #[rust]
    runtime_status_training_backend: String,
    #[rust]
    default_input_device_name: String,
    #[rust]
    default_output_device_name: String,
    #[rust]
    available_output_devices: Vec<String>,
    #[rust]
    available_input_devices: Vec<String>,
    #[rust]
    selected_output_device_idx: usize,
    #[rust]
    selected_input_device_idx: usize,

    // ── Translation page state ────────────────────────────────────────────────
    /// Whether the translation dataflow is currently running
    #[rust]
    translation_running: bool,
    /// Source language code, e.g. "zh"
    #[rust]
    translation_src_lang: String,
    /// Target language code, e.g. "en"
    #[rust]
    translation_tgt_lang: String,
    /// Log lines displayed in the running panel
    #[rust]
    translation_log_lines: Vec<String>,
    /// Timer for polling metrics (CPU/MEM) while running
    #[rust]
    translation_metrics_timer: Timer,
    /// Last translation entry mirrored into the in-app log (for de-dup without consuming dirty state)
    #[rust]
    translation_last_logged_fingerprint: Option<String>,
    /// Available CPAL audio input device names (populated lazily on first 更改 click)
    #[rust]
    translation_audio_devices: Vec<String>,
    /// Index into translation_audio_devices (0 = system default)
    #[rust]
    translation_device_idx: usize,
    /// Whether the overlay is in fullscreen mode
    #[rust]
    translation_overlay_fullscreen: bool,
    /// Overlay window background opacity (0.0..1.0)
    #[rust]
    translation_overlay_opacity: f64,
    /// Overlay text size preset: "small" | "normal" | "large"
    #[rust]
    translation_overlay_font_size_preset: String,
    /// Overlay anchor position preset percentage: "50" | "60" | ... | "100"
    #[rust]
    translation_overlay_anchor_position_preset: String,
    /// Audio source for translation: false = microphone, true = system audio
    #[rust]
    /// Whether we have already triggered the background screen-recording permission probe
    #[rust]
    translation_permission_probed: bool,
    /// One-shot timer used to check probe result and show the restart hint
    #[rust]
    translation_permission_timer: Timer,

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
    #[rust]
    download_modal_visible: bool,
    #[rust]
    pending_download_source: Option<DownloadSource>,
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
            self.audio_player = Some(TTSPlayer::new_with_output_device(
                self.app_preferences.preferred_output_device.as_deref(),
            ));
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
            self.voice_name_marquee_offset = 0.0;
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
            // selected_tts_model_id synced below, AFTER qwen validation
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
            self.download_modal_visible = false;
            self.pending_download_source = None;

            // Initialize TTS parameters
            self.tts_speed = 1.0;
            self.tts_pitch = 0.0;
            self.tts_volume = 100.0;
            self.tts_slider_dragging = None;
            self.controls_panel_tab = 1;
            self.apply_controls_panel_tab_visibility(cx);
            self.update_settings_tabs(cx);
            self.update_sidebar_nav_states(cx);

            // Initialize global settings state (restore persisted language)
            self.global_settings_visible = false;
            self.app_preferences = app_preferences::load_preferences();
            self.app_language = if self.app_preferences.app_language == "en" {
                "en".to_string()
            } else {
                "zh".to_string()
            };
            let _ = i18n::set_locale(&self.app_language);
            // Qwen3-only: always override to qwen3 regardless of stored preference.
            // PrimeSpeech fallback removed. See doc/REFACTOR_QWEN3_ONLY.md.
            self.app_preferences.inference_backend = "qwen3_tts_mlx".to_string();
            self.app_preferences.zero_shot_backend = "qwen3_tts_mlx".to_string();
            self.app_preferences.training_backend = "option_c".to_string(); // Qwen3 ICL mode
            // Sync selected model with validated inference_backend preference
            {
                let validated_backend = self.app_preferences.inference_backend.clone();
                self.selected_tts_model_id = if self.model_options.iter().any(|m| m.id == validated_backend) {
                    Some(validated_backend)
                } else {
                    self.model_options.first().map(|m| m.id.clone())
                };
            }
            self.user_profile_customized = true;
            self.user_settings_tab = 0;
            self.user_display_name = self.app_preferences.display_name.clone();
            self.user_avatar_letter = self.app_preferences.avatar_letter.clone();
            self.runtime_status_dora = "-".to_string();
            self.runtime_status_asr = "-".to_string();
            self.runtime_status_tts = "-".to_string();
            self.runtime_status_model = "-".to_string();
            self.runtime_status_training_backend = self.app_preferences.training_backend.clone();
            self.default_input_device_name =
                default_input_device_name().unwrap_or_else(|| "Unknown".to_string());
            self.default_output_device_name =
                default_output_device_name().unwrap_or_else(|| "Unknown".to_string());
            self.available_output_devices = list_output_devices();
            self.available_input_devices = list_input_devices();
            self.selected_output_device_idx = 0;
            self.selected_input_device_idx = 0;
            if let Some(preferred) = self.app_preferences.preferred_output_device.as_ref() {
                if let Some(idx) = self
                    .available_output_devices
                    .iter()
                    .position(|name| name == preferred)
                {
                    self.selected_output_device_idx = idx;
                }
            }
            if let Some(preferred) = self.app_preferences.preferred_input_device.as_ref() {
                if let Some(idx) = self
                    .available_input_devices
                    .iter()
                    .position(|name| name == preferred)
                {
                    self.selected_input_device_idx = idx;
                }
            }
            std::env::set_var(
                "MOXIN_TRAINING_BACKEND",
                self.app_preferences.training_backend.clone(),
            );
            std::env::set_var(
                "MOXIN_INFERENCE_BACKEND",
                self.app_preferences.inference_backend.clone(),
            );
            std::env::set_var(
                "MOXIN_ZERO_SHOT_BACKEND",
                self.app_preferences.zero_shot_backend.clone(),
            );

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
            self.sync_user_profile_ui(cx);
            self.apply_preferences_defaults(cx);
            self.apply_history_retention_policy(cx);
            self.update_runtime_status_ui(cx);
            self.update_system_paths_ui(cx);
            self.update_audio_devices_ui(cx);
            self.update_user_settings_tabs(cx);
            self.apply_dark_mode(cx);
            self.runtime_init_state = RuntimeInitState::Idle;
            self.runtime_init_rx = None;
            self.runtime_init_status_text = self.tr("初始化中...", "Initializing...").to_string();
            self.runtime_init_detail_text =
                self.tr("正在检查运行环境", "Checking runtime environment").to_string();
            self.qwen_download_in_progress = false;
            self.qwen_download_rx = None;
            self.qwen_model_status_text = self.tr("未就绪", "Not ready").to_string();
            self.dora_start_attempt_at = None;
            self.dora_start_in_flight = false;
            self.dora_start_rx = None;
            self.dora_pending_dataflow_path = None;

            // Translation page defaults
            self.translation_running = false;
            self.translation_src_lang = "zh".to_string();
            self.translation_tgt_lang = "en".to_string();
            self.translation_log_lines = Vec::new();
            self.translation_metrics_timer = Timer::default();
            self.translation_last_logged_fingerprint = None;
            self.translation_audio_devices = Vec::new();
            self.translation_device_idx = 0; // 0 = System Audio, 1 = System Default Mic
            self.translation_overlay_fullscreen = true;
            self.translation_overlay_opacity = 1.0;
            self.translation_overlay_font_size_preset = "normal".to_string();
            self.translation_overlay_anchor_position_preset = "70".to_string();
            self.translation_permission_probed = false;
            self.translation_permission_timer = Timer::default();
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

        if self.runtime_init_state == RuntimeInitState::Idle {
            self.start_runtime_initialization(cx);
        }

        if !self.dora_started && !self.tts_paused_for_translation {
            let can_start_dataflow = match self.runtime_init_state {
                RuntimeInitState::Idle | RuntimeInitState::Ready => true,
                RuntimeInitState::Running | RuntimeInitState::Failed => false,
            };
            if can_start_dataflow {
                self.dora_started = true;
                // Auto-start dataflow on app launch
                self.auto_start_dataflow(cx);
            }
        }

        if self.translation_dora.is_none() {
            self.translation_dora = Some(DoraIntegration::new());
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

        // Translation metrics polling (1s)
        if self.translation_metrics_timer.is_event(event).is_some() && self.translation_running {
            self.poll_translation_metrics(cx);
        }

        // Screen-recording permission probe result (fires 2 s after navigating to translation page)
        #[cfg(target_os = "macos")]
        if self.translation_permission_timer.is_event(event).is_some() {
            self.update_translation_permission_hint(cx);
        }

        // Poll for audio and logs
        if self.update_timer.is_event(event).is_some() {
            self.poll_runtime_initialization(cx);
            self.poll_qwen_model_download(cx);
            self.poll_dora_startup(cx);
            self.poll_dora_events(cx);
            self.poll_translation_dora_events(cx);
            self.maybe_retry_dataflow_start(cx);

            // Two-phase wait after backend switch: first bridges drop, then come back to 4
            if self.backend_switching {
                // Animate the in-dialog spinner
                self.spinner_phase += 0.03;
                if self.spinner_phase > 1.0 { self.spinner_phase -= 1.0; }
                self.view
                    .view(ids!(model_picker_modal.model_picker_dialog.switching_overlay.switching_spinner))
                    .apply_over(cx, live! { draw_bg: { phase: (self.spinner_phase) } });

                let is_running = self.dora.as_ref().map(|d| d.is_running()).unwrap_or(false);
                let bridge_count = self.dora.as_ref()
                    .map(|d| d.shared_dora_state().status.read().active_bridges.len())
                    .unwrap_or(0);
                if !self.backend_switch_bridges_dropped {
                    if !is_running || bridge_count == 0 {
                        self.backend_switch_bridges_dropped = true;
                        // Old dataflow is down — now safe to start the new one
                        self.auto_start_dataflow(cx);
                    }
                } else if bridge_count >= 4 {
                    self.backend_switching = false;
                    self.backend_switch_bridges_dropped = false;
                    self.view.view(ids!(model_picker_modal.model_picker_dialog.switching_overlay)).set_visible(cx, false);
                    self.model_picker_visible = false;
                    self.view.view(ids!(model_picker_modal)).set_visible(cx, false);
                    self.show_toast(cx, self.tr("推理后端已就绪", "Inference backend is ready"));
                    self.view.redraw(cx);
                }
            }

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
                            let effective_rate = self.effective_audio_sample_rate();
                            let duration_secs = if effective_rate > 0 {
                                sample_count as f32 / effective_rate as f32
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
                            self.audio_playing_time = 0.0;
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

            if self.current_page == AppPage::UserSettings {
                self.update_user_settings_page(cx);
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

                if self.runtime_init_state == RuntimeInitState::Running
                    || self.runtime_init_state == RuntimeInitState::Failed
                {
                    self.view
                        .label(ids!(loading_overlay.loading_content.loading_status))
                        .set_text(cx, &self.runtime_init_status_text);
                    self.view
                        .label(ids!(loading_overlay.loading_content.loading_detail))
                        .set_text(cx, &self.runtime_init_detail_text);
                } else {
                    // Initial startup: dismiss once dora is running
                    let is_running = self.dora.as_ref().map(|d| d.is_running()).unwrap_or(false);
                    if is_running {
                        self.view
                            .label(ids!(loading_overlay.loading_content.loading_status))
                            .set_text(cx, self.tr("已连接", "Connected"));
                        self.view
                            .label(ids!(loading_overlay.loading_content.loading_detail))
                            .set_text(cx, self.tr("TTS 引擎已就绪", "TTS engine ready"));
                        self.loading_dismissed = true;
                        self.view.view(ids!(loading_overlay)).set_visible(cx, false);
                        self.view.redraw(cx);
                        self.add_log(cx, "[INFO] [tts] Dataflow connected, UI ready");
                    } else {
                        self.view
                            .label(ids!(loading_overlay.loading_content.loading_status))
                            .set_text(cx, self.tr("连接中...", "Connecting..."));
                        self.view
                            .label(ids!(loading_overlay.loading_content.loading_detail))
                            .set_text(cx, self.tr("正在启动 TTS 数据流引擎", "Starting TTS dataflow engine"));
                    }
                }

                self.view.redraw(cx);
            }

            self.update_voice_name_marquee(cx);
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
                .text_input(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.search_input))
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

        // ── Translation page navigation ───────────────────────────────────────
        if self
            .view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_translation))
            .clicked(&actions)
        {
            self.switch_page(cx, AppPage::Translation);
            self.populate_translation_input_dropdown(cx);
            self.update_translation_lang_dropdowns(cx);
            self.update_translation_overlay_style_buttons(cx);
            self.update_translation_anchor_position_dropdown(cx);
            self.update_translation_opacity_dropdown(cx);
            // Probe screen-recording permission early so the OS dialog appears
            // before the user clicks Start (macOS requires a restart after granting).
            #[cfg(target_os = "macos")]
            if !self.translation_permission_probed {
                self.translation_permission_probed = true;
                moxin_dora_bridge::widgets::probe_permission_async();
                // Check probe result after 2 s and show/hide the restart hint.
                self.translation_permission_timer = cx.start_timeout(2.0);
            }
        }

        // ── Translation page: start / stop / settings buttons ────────────────
        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.translation_start_btn))
            .clicked(&actions)
        {
            self.start_translation_dataflow(cx);
        }
        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_running_panel.translation_stop_btn))
            .clicked(&actions)
        {
            self.stop_translation_dataflow(cx);
        }
        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_running_panel.translation_show_overlay_btn))
            .clicked(&actions)
        {
            // Force a false → true dirty edge so app.rs re-runs the show path
            // (makeKeyAndOrderFront:) even if the state was already `true`
            // when the user closed the window manually.
            if let Some(shared) = self.translation_shared_state() {
                shared.translation_window_visible.set(false);
                shared.translation_window_visible.set(true);
            }
        }

        // 更改 input source — dropdown selection
        // Index layout: 0 = System Audio, 1 = System Default Mic, 2..N = CPAL devices
        if let Some(idx) = self
            .view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_source.translation_source_dropdown))
            .changed(&actions)
        {
            self.translation_device_idx = idx;
            if idx == 0 {
                // System Audio via ScreenCaptureKit
                self.send_audio_source_to_bridge(true);
                if let Some(shared) = self.translation_shared_state() {
                    shared.translation_input_device.set(None);
                }
            } else {
                // Microphone — idx 1 = default, idx 2..N = CPAL device
                self.send_audio_source_to_bridge(false);
                let device_for_bridge = if idx == 1 {
                    None
                } else if idx - 1 <= self.translation_audio_devices.len() {
                    self.translation_audio_devices.get(idx - 2).cloned()
                } else {
                    None
                };
                if let Some(shared) = self.translation_shared_state() {
                    shared.translation_input_device.set(device_for_bridge);
                }
            }
        }

        // Overlay style: compact / fullscreen
        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_overlay.overlay_style_compact))
            .clicked(&actions)
        {
            self.translation_overlay_fullscreen = false;
            self.update_translation_overlay_style_buttons(cx);
            if let Some(shared) = self.translation_shared_state() {
                shared.translation_overlay_fullscreen.set(false);
            }
        }
        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_overlay.overlay_style_full))
            .clicked(&actions)
        {
            self.translation_overlay_fullscreen = true;
            self.update_translation_overlay_style_buttons(cx);
            if let Some(shared) = self.translation_shared_state() {
                shared.translation_overlay_fullscreen.set(true);
            }
        }

        // Overlay opacity dropdown
        {
            let opacity_values: [f64; 7] = [1.0, 0.9, 0.85, 0.75, 0.65, 0.5, 0.35];
            if let Some(idx) = self
                .view
                .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_opacity.opacity_dropdown))
                .changed(&actions)
            {
                let opacity = opacity_values.get(idx).copied().unwrap_or(0.85);
                self.translation_overlay_opacity = opacity;
                if let Some(shared) = self.translation_shared_state() {
                    shared.translation_overlay_opacity.set(opacity);
                }
            }
        }

        // Source language dropdown
        {
            let lang_codes = ["zh", "en", "ja", "fr"];
            if let Some(idx) = self
                .view
                .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_src_lang.src_lang_dropdown))
                .changed(&actions)
            {
                if let Some(code) = lang_codes.get(idx) {
                    self.translation_src_lang = code.to_string();
                    // Prevent same language for both source and target
                    if self.translation_src_lang == self.translation_tgt_lang {
                        // Pick the first different language
                        for &c in &lang_codes {
                            if c != *code {
                                self.translation_tgt_lang = c.to_string();
                                break;
                            }
                        }
                        self.update_translation_lang_dropdowns(cx);
                    }
                    self.sync_translation_overlay_lang_pair();
                }
            }
        }

        // Target language dropdown
        {
            let lang_codes = ["en", "zh", "ja", "fr"];
            if let Some(idx) = self
                .view
                .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_tgt_lang.tgt_lang_dropdown))
                .changed(&actions)
            {
                if let Some(code) = lang_codes.get(idx) {
                    self.translation_tgt_lang = code.to_string();
                    // Prevent same language for both source and target
                    if self.translation_tgt_lang == self.translation_src_lang {
                        let src_codes = ["zh", "en", "ja", "fr"];
                        for &c in &src_codes {
                            if c != *code {
                                self.translation_src_lang = c.to_string();
                                break;
                            }
                        }
                        self.update_translation_lang_dropdowns(cx);
                    }
                    self.sync_translation_overlay_lang_pair();
                }
            }
        }

        // Overlay font size preset dropdown
        {
            let presets = ["small", "normal", "large"];
            if let Some(idx) = self
                .view
                .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_font_size.font_size_dropdown))
                .changed(&actions)
            {
                if let Some(preset) = presets.get(idx) {
                    self.translation_overlay_font_size_preset = (*preset).to_string();
                    self.update_translation_font_size_dropdown(cx);
                    self.sync_translation_overlay_font_size();
                }
            }
        }

        // Overlay scroll anchor position dropdown
        {
            let presets = ["50", "60", "70", "80", "90", "100"];
            if let Some(idx) = self
                .view
                .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_anchor_position.anchor_position_dropdown))
                .changed(&actions)
            {
                if let Some(preset) = presets.get(idx) {
                    self.translation_overlay_anchor_position_preset = (*preset).to_string();
                    self.update_translation_anchor_position_dropdown(cx);
                    self.sync_translation_overlay_anchor_position();
                }
            }
        }

        // Handle sidebar footer click (navigate to User & Settings page)
        if self
            .view
            .view(ids!(app_layout.sidebar.sidebar_footer))
            .finger_up(&actions)
            .is_some()
            || self
                .view
                .button(ids!(app_layout.sidebar.sidebar_footer.global_settings_btn))
                .clicked(&actions)
        {
            self.switch_page(cx, AppPage::UserSettings);
            self.update_language_options(cx);
            self.update_theme_options(cx);
            self.apply_localization(cx);
            self.update_user_settings_page(cx);
            self.view.redraw(cx);
        }

        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_tab_bar.tab_profile_btn))
            .clicked(&actions)
        {
            self.user_settings_tab = 0;
            self.update_user_settings_tabs(cx);
        }
        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_tab_bar.tab_app_btn))
            .clicked(&actions)
        {
            self.user_settings_tab = 1;
            self.update_user_settings_tabs(cx);
        }
        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_tab_bar.tab_runtime_btn))
            .clicked(&actions)
        {
            self.user_settings_tab = 2;
            self.update_user_settings_tabs(cx);
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
            // Always sync with current inference_backend when opening picker
            let current_backend = self.app_preferences.inference_backend.clone();
            self.selected_tts_model_id = if self.model_options.iter().any(|m| m.id == current_backend) {
                Some(current_backend)
            } else {
                self.model_options.first().map(|m| m.id.clone())
            };
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

        if self
            .view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.profile_card.profile_actions.save_profile_btn
            ))
            .clicked(&actions)
        {
            let raw_name = self
                .view
                .text_input(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.profile_card.profile_body.profile_form.name_row.name_input))
                .text();
            let trimmed = raw_name.trim().to_string();
            if !trimmed.is_empty() {
                self.user_display_name = trimmed.clone();
                self.user_avatar_letter = trimmed
                    .chars()
                    .next()
                    .map(|c| c.to_uppercase().to_string())
                    .unwrap_or_default();
            }
            self.persist_app_preferences(cx);
            self.sync_user_profile_ui(cx);
            self.show_toast(cx, self.tr("用户名已保存", "Username saved"));
        }

        if self
            .view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_actions.save_defaults_btn
            ))
            .clicked(&actions)
        {
            let speed = self
                .view
                .text_input(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.speed_col.speed_input))
                .text()
                .parse::<f64>()
                .unwrap_or(self.tts_speed)
                .clamp(0.5, 2.0);
            let pitch = self
                .view
                .text_input(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.pitch_col.pitch_input))
                .text()
                .parse::<f64>()
                .unwrap_or(self.tts_pitch)
                .clamp(-12.0, 12.0);
            let volume = self
                .view
                .text_input(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.volume_col.volume_input))
                .text()
                .parse::<f64>()
                .unwrap_or(self.tts_volume)
                .clamp(0.0, 100.0);
            self.app_preferences.default_speed = speed;
            self.app_preferences.default_pitch = pitch;
            self.app_preferences.default_volume = volume;
            self.app_preferences.default_voice_id = self.selected_voice_id.clone();
            self.persist_app_preferences(cx);
            self.update_user_settings_page(cx);
            self.show_toast(cx, self.tr("默认参数已保存", "Default synthesis settings saved"));
        }

        if self
            .view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_actions.apply_defaults_now_btn
            ))
            .clicked(&actions)
        {
            self.tts_speed = self.app_preferences.default_speed.clamp(0.5, 2.0);
            self.tts_pitch = self.app_preferences.default_pitch.clamp(-12.0, 12.0);
            self.tts_volume = self.app_preferences.default_volume.clamp(0.0, 100.0);
            self.update_tts_param_controls(cx);
            self.show_toast(cx, self.tr("已应用默认参数", "Applied defaults to current controls"));
        }

        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.runtime_card.runtime_refresh_btn))
            .clicked(&actions)
        {
            self.update_user_settings_page(cx);
        }

        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.path_actions.open_model_dir_btn))
            .clicked(&actions)
        {
            match Self::open_path_in_finder(Self::models_dir().as_path()) {
                Ok(_) => {}
                Err(e) => self.add_log(cx, &format!("[WARN] [settings] {}", e)),
            }
        }
        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.path_actions.open_log_dir_btn))
            .clicked(&actions)
        {
            match Self::open_path_in_finder(Self::app_logs_dir().as_path()) {
                Ok(_) => {}
                Err(e) => self.add_log(cx, &format!("[WARN] [settings] {}", e)),
            }
        }
        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.path_actions.open_workspace_dir_btn))
            .clicked(&actions)
        {
            match Self::open_path_in_finder(Self::workspace_dir().as_path()) {
                Ok(_) => {}
                Err(e) => self.add_log(cx, &format!("[WARN] [settings] {}", e)),
            }
        }
        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.clear_cache_btn))
            .clicked(&actions)
        {
            self.clear_app_cache(cx);
            self.show_toast(cx, self.tr("缓存已清理", "Cache cleared"));
        }

        if let Some(idx) = self
            .view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.privacy_card.retention_pick_row.retention_dropdown))
            .changed(&actions)
        {
            self.app_preferences.history_retention_days = match idx {
                1 => 30,
                2 => 7,
                _ => -1,
            };
            self.apply_history_retention_policy(cx);
            self.persist_app_preferences(cx);
            self.update_user_settings_page(cx);
        }
        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.privacy_card.privacy_actions.clear_tts_history_btn))
            .clicked(&actions)
        {
            self.clear_tts_history(cx);
            self.show_toast(cx, self.tr("TTS 历史已清空", "TTS history cleared"));
        }
        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.privacy_card.privacy_actions.clear_training_artifacts_btn))
            .clicked(&actions)
        {
            self.clear_training_artifacts(cx);
            self.show_toast(cx, self.tr("训练中间产物已清理", "Training artifacts cleared"));
        }

        if self
            .view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.devices_header.refresh_devices_btn))
            .clicked(&actions)
        {
            self.available_output_devices = list_output_devices();
            self.available_input_devices = list_input_devices();
            if self.selected_output_device_idx >= self.available_output_devices.len() {
                self.selected_output_device_idx = 0;
            }
            if self.selected_input_device_idx >= self.available_input_devices.len() {
                self.selected_input_device_idx = 0;
            }
            self.update_audio_devices_ui(cx);
        }
        if let Some(idx) = self
            .view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.input_pick_row.input_device_dropdown))
            .changed(&actions)
        {
            if idx == 0 {
                self.app_preferences.preferred_input_device = None;
            } else if idx - 1 < self.available_input_devices.len() {
                self.selected_input_device_idx = idx - 1;
                self.app_preferences.preferred_input_device =
                    Some(self.available_input_devices[self.selected_input_device_idx].clone());
            }
            self.persist_app_preferences(cx);
            self.update_audio_devices_ui(cx);
            self.show_toast(cx, self.tr("输入设备已更新", "Input device updated"));
        }
        if let Some(idx) = self
            .view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.output_pick_row.output_device_dropdown))
            .changed(&actions)
        {
            if idx == 0 {
                self.app_preferences.preferred_output_device = None;
            } else if idx - 1 < self.available_output_devices.len() {
                self.selected_output_device_idx = idx - 1;
                self.app_preferences.preferred_output_device =
                    Some(self.available_output_devices[self.selected_output_device_idx].clone());
            }
            self.persist_app_preferences(cx);
            self.recreate_players_with_selected_output();
            self.update_audio_devices_ui(cx);
            self.show_toast(cx, self.tr("输出设备已更新", "Output device updated"));
        }

        // zero_shot_backend_dropdown and training_backend_dropdown handlers removed.
        // Qwen3-only mode: these dropdowns are hidden. See doc/REFACTOR_QWEN3_ONLY.md.
        if let Some(idx) = self
            .view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.debug_pick_row.debug_logs_dropdown))
            .changed(&actions)
        {
            self.app_preferences.debug_logs_enabled = idx == 1;
            std::env::set_var(
                "RUST_LOG",
                if self.app_preferences.debug_logs_enabled {
                    "debug"
                } else {
                    "info"
                },
            );
            self.persist_app_preferences(cx);
            self.update_user_settings_page(cx);
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
            || self
                .view
                .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.language_section.language_options.lang_en_option))
                .clicked(&actions)
        {
            self.app_language = "en".to_string();
            let _ = i18n::set_locale("en");
            self.persist_app_preferences(cx);
            self.update_language_options(cx);
            self.apply_localization(cx);
            self.load_voice_library(cx);
        }

        if self
            .view
            .button(ids!(global_settings_modal.settings_dialog.settings_content.language_section.language_options.lang_zh_option))
            .clicked(&actions)
            || self
                .view
                .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.language_section.language_options.lang_zh_option))
                .clicked(&actions)
        {
            self.app_language = "zh".to_string();
            let _ = i18n::set_locale("zh");
            self.persist_app_preferences(cx);
            self.update_language_options(cx);
            self.apply_localization(cx);
            self.load_voice_library(cx);
        }

        // Handle theme selection in global settings
        if self
            .view
            .button(ids!(global_settings_modal.settings_dialog.settings_content.theme_section.theme_options.theme_light_option))
            .clicked(&actions)
            || self
                .view
                .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.theme_section.theme_options.theme_light_option))
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
            || self
                .view
                .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.theme_section.theme_options.theme_dark_option))
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

        if self.download_modal_visible {
            if self
                .view
                .button(ids!(download_modal.download_dialog.download_footer.download_cancel_btn))
                .clicked(&actions)
            {
                self.close_download_modal(cx);
            }
            if self
                .view
                .view(ids!(download_modal.download_backdrop))
                .finger_up(&actions)
                .is_some()
            {
                self.close_download_modal(cx);
            }
            if self
                .view
                .button(ids!(download_modal.download_dialog.download_actions.download_mp3_btn))
                .clicked(&actions)
            {
                self.download_audio_to_format(cx, DownloadFormat::Mp3);
            }
            if self
                .view
                .button(ids!(download_modal.download_dialog.download_actions.download_wav_btn))
                .clicked(&actions)
            {
                self.download_audio_to_format(cx, DownloadFormat::Wav);
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
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_gender.filter_male_btn)).clicked(&actions) {
            let was_active = self.library_category_filter == VoiceFilter::Male;
            self.library_category_filter = if was_active { VoiceFilter::All } else { VoiceFilter::Male };
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_gender.filter_female_btn)).clicked(&actions) {
            let was_active = self.library_category_filter == VoiceFilter::Female;
            self.library_category_filter = if was_active { VoiceFilter::All } else { VoiceFilter::Female };
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_gender.age_adult_btn)).clicked(&actions) {
            const ADULT_BIT: u8 = 0b01;
            if self.library_age_filter & ADULT_BIT != 0 {
                self.library_age_filter &= !ADULT_BIT;
            } else {
                self.library_age_filter |= ADULT_BIT;
            }
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_gender.age_youth_btn)).clicked(&actions) {
            const YOUTH_BIT: u8 = 0b10;
            if self.library_age_filter & YOUTH_BIT != 0 {
                self.library_age_filter &= !YOUTH_BIT;
            } else {
                self.library_age_filter |= YOUTH_BIT;
            }
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_style.style_sweet_btn)).clicked(&actions) {
            const SWEET_BIT: u8 = 0b01;
            if self.library_style_filter & SWEET_BIT != 0 {
                self.library_style_filter &= !SWEET_BIT;
            } else {
                self.library_style_filter |= SWEET_BIT;
            }
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_style.style_magnetic_btn)).clicked(&actions) {
            const MAGNETIC_BIT: u8 = 0b10;
            if self.library_style_filter & MAGNETIC_BIT != 0 {
                self.library_style_filter &= !MAGNETIC_BIT;
            } else {
                self.library_style_filter |= MAGNETIC_BIT;
            }
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_trait.trait_prof_btn)).clicked(&actions) {
            const PROF_BIT: u8 = 0b01;
            if self.library_trait_filter & PROF_BIT != 0 {
                self.library_trait_filter &= !PROF_BIT;
            } else {
                self.library_trait_filter |= PROF_BIT;
            }
            self.update_category_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_trait.trait_character_btn)).clicked(&actions) {
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
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.language_filter.lang_all_btn)).clicked(&actions) {
            self.library_language_filter = LanguageFilter::All;
            self.update_language_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.language_filter.lang_zh_btn)).clicked(&actions) {
            self.library_language_filter = LanguageFilter::Chinese;
            self.update_language_filter_buttons(cx);
            self.update_library_display(cx);
        }
        if self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.language_filter.lang_en_btn)).clicked(&actions) {
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

        // advanced_mode_btn handler removed — Qwen3-only, Pro mode hidden.
        // See doc/REFACTOR_QWEN3_ONLY.md to restore.

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
            let can_preview = match voice.source {
                crate::voice_data::VoiceSource::Builtin
                | crate::voice_data::VoiceSource::BundledIcl => voice.preview_audio.is_some(),
                crate::voice_data::VoiceSource::Custom | crate::voice_data::VoiceSource::Trained => {
                    voice.reference_audio_path.is_some()
                }
            };
            
            // Check preview button click
            if can_preview {
                match event.hits(cx, preview_btn_area) {
                    Hit::FingerUp(fe) if fe.was_tap() => {
                        self.preview_voice(cx, voice.id.clone());
                    }
                    _ => {}
                }
            }
            
            // Check delete button click (only for custom voices)
            if voice.source != crate::voice_data::VoiceSource::Builtin && voice.source != crate::voice_data::VoiceSource::BundledIcl {
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
                        self.open_download_modal(cx, DownloadSource::History(entry_id.clone()));
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
                        // Only close dialog immediately if NOT switching backend
                        // (if switching, the timer will close the dialog when new dataflow is ready)
                        if !self.backend_switching {
                            self.model_picker_visible = false;
                            self.view.view(ids!(model_picker_modal)).set_visible(cx, false);
                        }
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
        if let Some(changed_text) = self
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
        {
            let mut effective_text = changed_text;
            if effective_text.chars().count() > TTS_INPUT_MAX_CHARS {
                let cutoff = effective_text
                    .char_indices()
                    .nth(TTS_INPUT_MAX_CHARS)
                    .map(|(idx, _)| idx)
                    .unwrap_or(effective_text.len());
                effective_text.truncate(cutoff);
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
                    .set_text(cx, &effective_text);
                self.show_toast(
                    cx,
                    self.tr(
                        "文本已自动截断到 1,000 字符",
                        "Text was automatically truncated to 1,000 characters",
                    ),
                );
            }
            self.update_char_count_from_text(cx, &effective_text);
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
            let selected_voice_is_trained = self
                .selected_voice_id
                .as_ref()
                .and_then(|id| self.library_voices.iter().find(|v| &v.id == id))
                .map(|voice| voice.source == crate::voice_data::VoiceSource::Trained)
                .unwrap_or(false);
            if Self::is_qwen_backend(&self.app_preferences.inference_backend) && selected_voice_is_trained {
                self.show_toast(
                    cx,
                    self.tr(
                        "Qwen 推理后端暂不支持训练音色，请切换推理后端或选择其他音色",
                        "Qwen inference backend does not support trained voices. Switch backend or voice",
                    ),
                );
                self.add_log(
                    cx,
                    "[WARN] [tts] Generate blocked: trained voice selected while inference backend is qwen",
                );
                self.set_generate_button_loading(cx, false);
                return;
            }

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
            self.open_download_modal(cx, DownloadSource::CurrentAudio);
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
                                crate::voice_data::VoiceSource::Builtin
                                | crate::voice_data::VoiceSource::BundledIcl => self.tr("内置", "Built-in"),
                                crate::voice_data::VoiceSource::Custom => self.tr("自定义", "Custom"),
                                crate::voice_data::VoiceSource::Trained => self.tr("训练", "Trained"),
                            };
                            let is_custom = source != crate::voice_data::VoiceSource::Builtin
                                && source != crate::voice_data::VoiceSource::BundledIcl;
                            let can_preview = match source {
                                crate::voice_data::VoiceSource::Builtin
                                | crate::voice_data::VoiceSource::BundledIcl => voice.preview_audio.is_some(),
                                crate::voice_data::VoiceSource::Custom
                                | crate::voice_data::VoiceSource::Trained => {
                                    voice.reference_audio_path.is_some()
                                }
                            };
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
                            // Show preview button only when preview audio exists.
                            card.button(ids!(actions.preview_btn)).set_visible(cx, can_preview);

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
                            let highlighted_voice_id = self.selected_voice_id.as_ref();
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
    fn ensure_bundle_bin_on_path() {
        let Some(exe_path) = std::env::current_exe().ok() else {
            return;
        };
        let Some(bin_dir) = exe_path.parent() else {
            return;
        };

        let bin_dir_str = bin_dir.to_string_lossy().to_string();
        if bin_dir_str.is_empty() {
            return;
        }

        let current_path = std::env::var("PATH").unwrap_or_default();
        if current_path.split(':').any(|p| p == bin_dir_str) {
            return;
        }

        let merged_path = if current_path.is_empty() {
            bin_dir_str
        } else {
            format!("{}:{}", bin_dir_str, current_path)
        };
        std::env::set_var("PATH", merged_path);
    }

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
            "primespeech_mlx" => "GPT-SoVITS v2 MLX 推理引擎。支持内置音色、零样本克隆和训练音色推理，适合高质量音色克隆场景。".to_string(),
            "qwen3_tts_mlx" => "Qwen3 TTS MLX 推理引擎。支持内置音色和零样本音色克隆，在 Apple Silicon 上运行轻量快速。".to_string(),
            _ => model.description.clone(),
        }
    }

    fn localized_model_badge(&self, model: &TtsModelOption) -> Option<String> {
        if model.id == "qwen3_tts_mlx" {
            if self.qwen_download_in_progress {
                return Some(self.tr("下载中", "Downloading").to_string());
            }
            if !Self::qwen_custom_ready() {
                return Some(self.tr("待下载", "Download needed").to_string());
            }
            return None;
        }
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
                "Zero-shot" => "零样本克隆".to_string(),
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

    fn normalized_profile_name(input: &str) -> String {
        let name = input.trim();
        if name.is_empty() {
            "User".to_string()
        } else {
            name.chars().take(24).collect::<String>()
        }
    }

    fn normalized_avatar_letter(input: &str, fallback_name: &str) -> String {
        if let Some(first) = input.trim().chars().next() {
            return first.to_uppercase().collect::<String>();
        }
        if let Some(first) = fallback_name.chars().next() {
            return first.to_uppercase().collect::<String>();
        }
        "U".to_string()
    }

    fn sync_user_profile_ui(&mut self, cx: &mut Cx) {
        self.view
            .label(ids!(app_layout.sidebar.sidebar_footer.user_details.user_name))
            .set_text(cx, &self.user_display_name);
        self.view
            .label(ids!(app_layout.sidebar.sidebar_footer.user_avatar.avatar_letter))
            .set_text(cx, &self.user_avatar_letter);
        self.view
            .text_input(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.profile_card.profile_body.profile_form.name_row.name_input))
            .set_text(cx, &self.user_display_name);
    }

    fn persist_app_preferences(&mut self, cx: &mut Cx) {
        self.app_preferences.app_language = self.app_language.clone();
        self.app_preferences.display_name = self.user_display_name.clone();
        self.app_preferences.avatar_letter = self.user_avatar_letter.clone();
        self.app_preferences.default_speed = self.tts_speed;
        self.app_preferences.default_pitch = self.tts_pitch;
        self.app_preferences.default_volume = self.tts_volume;
        self.app_preferences.default_voice_id = self.selected_voice_id.clone();
        self.app_preferences.training_backend = self.runtime_status_training_backend.clone();
        if let Err(e) = app_preferences::save_preferences(&self.app_preferences) {
            self.add_log(cx, &format!("[WARN] [prefs] Failed to save preferences: {}", e));
        }
    }

    fn app_logs_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join("Library")
            .join("Logs")
            .join("MoxinVoice")
    }

    fn models_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".OminiX")
            .join("models")
            .join("gpt-sovits-mlx")
    }

    fn workspace_dir() -> PathBuf {
        dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".dora")
            .join("primespeech")
    }

    fn qwen_root_dir() -> PathBuf {
        std::env::var("QWEN3_TTS_MODEL_ROOT")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                dirs::home_dir()
                    .unwrap_or_else(|| PathBuf::from("."))
                    .join(".OminiX")
                    .join("models")
                    .join("qwen3-tts-mlx")
            })
    }

    fn qwen_custom_model_dir() -> PathBuf {
        std::env::var("QWEN3_TTS_CUSTOMVOICE_MODEL_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| {
                Self::qwen_root_dir().join("Qwen3-TTS-12Hz-1.7B-CustomVoice-8bit")
            })
    }
    fn qwen_base_model_dir() -> PathBuf {
        std::env::var("QWEN3_TTS_BASE_MODEL_DIR")
            .map(PathBuf::from)
            .unwrap_or_else(|_| Self::qwen_root_dir().join("Qwen3-TTS-12Hz-1.7B-Base-8bit"))
    }
    fn qwen_model_dir_ready(model_dir: &Path) -> bool {
        model_dir.join("config.json").exists()
            && model_dir.join("generation_config.json").exists()
            && model_dir.join("vocab.json").exists()
            && model_dir.join("merges.txt").exists()
            && (model_dir.join("model.safetensors").exists()
                || model_dir.join("model.safetensors.index.json").exists())
            && model_dir.join("speech_tokenizer").join("config.json").exists()
            && model_dir.join("speech_tokenizer").join("model.safetensors").exists()
    }

    fn qwen_custom_ready() -> bool {
        Self::qwen_model_dir_ready(&Self::qwen_custom_model_dir())
    }

    fn qwen_base_ready() -> bool {
        Self::qwen_model_dir_ready(&Self::qwen_base_model_dir())
    }

    fn resolve_qwen_download_script_path() -> Option<PathBuf> {
        if let Ok(resources) = std::env::var("MOXIN_APP_RESOURCES") {
            let bundled = PathBuf::from(resources)
                .join("scripts")
                .join("download_qwen3_tts_models.py");
            if bundled.exists() {
                return Some(bundled);
            }
        }
        let cwd = std::env::current_dir().ok()?;
        let local = cwd.join("scripts").join("download_qwen3_tts_models.py");
        if local.exists() {
            return Some(local);
        }
        None
    }

    fn open_path_in_finder(path: &Path) -> Result<(), String> {
        if !path.exists() {
            fs::create_dir_all(path)
                .map_err(|e| format!("Failed to create path {:?}: {}", path, e))?;
        }
        #[cfg(target_os = "macos")]
        {
            Command::new("open")
                .arg(path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to open {:?}: {}", path, e))?;
        }
        #[cfg(not(target_os = "macos"))]
        {
            Command::new("xdg-open")
                .arg(path)
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .map_err(|e| format!("Failed to open {:?}: {}", path, e))?;
        }
        Ok(())
    }

    fn apply_preferences_defaults(&mut self, cx: &mut Cx) {
        if let Some(voice_id) = self.app_preferences.default_voice_id.clone() {
            if self.library_voices.iter().any(|v| v.id == voice_id) {
                self.selected_voice_id = Some(voice_id);
            }
        }
        self.tts_speed = self.app_preferences.default_speed.clamp(0.5, 2.0);
        self.tts_pitch = self.app_preferences.default_pitch.clamp(-12.0, 12.0);
        self.tts_volume = self.app_preferences.default_volume.clamp(0.0, 100.0);
        self.sync_selected_voice_ui(cx);
        self.update_tts_param_controls(cx);
        self.update_user_settings_page(cx);
    }

    fn apply_history_retention_policy(&mut self, cx: &mut Cx) {
        let retention = self.app_preferences.history_retention_days;
        if retention < 0 {
            return;
        }
        let now = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or(Duration::from_secs(0))
            .as_secs();
        let threshold = now.saturating_sub((retention as u64) * 24 * 60 * 60);
        let mut removed_audio_files = Vec::new();
        self.tts_history.retain(|entry| {
            if entry.created_at >= threshold {
                true
            } else {
                removed_audio_files.push(entry.audio_file.clone());
                false
            }
        });
        for file in removed_audio_files {
            let _ = tts_history::delete_audio_file(&file);
        }
        self.persist_tts_history(cx);
        self.update_history_display(cx);
    }

    fn refresh_runtime_status(&mut self) {
        let dora_running = self.dora.as_ref().map(|d| d.is_running()).unwrap_or(false);
        self.runtime_status_dora = if dora_running {
            "Running".to_string()
        } else {
            "Stopped".to_string()
        };
        let mut asr_connected = false;
        let mut tts_connected = false;
        if let Some(dora) = &self.dora {
            let status = dora.shared_dora_state().status.read();
            for bridge in &status.active_bridges {
                if bridge.contains("asr") || bridge.contains("audio-input") {
                    asr_connected = true;
                }
                if bridge.contains("tts") || bridge.contains("prompt-input") {
                    tts_connected = true;
                }
            }
        }
        self.runtime_status_asr = if asr_connected {
            "Connected".to_string()
        } else {
            "Disconnected".to_string()
        };
        self.runtime_status_tts = if tts_connected {
            "Connected".to_string()
        } else {
            "Disconnected".to_string()
        };
        self.runtime_status_model = self
            .selected_tts_model_id
            .clone()
            .unwrap_or_else(|| "unknown".to_string());
        self.runtime_status_training_backend = self.app_preferences.training_backend.clone();
    }

    fn update_runtime_status_ui(&mut self, cx: &mut Cx) {
        self.refresh_runtime_status();
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.runtime_card.dora_status))
            .set_text(cx, &format!("Dora: {}", self.runtime_status_dora));
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.runtime_card.asr_status))
            .set_text(cx, &format!("ASR: {}", self.runtime_status_asr));
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.runtime_card.tts_status))
            .set_text(cx, &format!("TTS: {}", self.runtime_status_tts));
    }

    fn update_system_paths_ui(&mut self, cx: &mut Cx) {
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.model_path_label))
            .set_text(cx, &format!("Models: {}", Self::models_dir().display()));
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.log_path_label))
            .set_text(cx, &format!("Logs: {}", Self::app_logs_dir().display()));
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.workspace_path_label))
            .set_text(cx, &format!("Workspace: {}", Self::workspace_dir().display()));
    }

    fn update_audio_devices_ui(&mut self, cx: &mut Cx) {
        self.default_input_device_name =
            default_input_device_name().unwrap_or_else(|| "Unknown".to_string());
        self.default_output_device_name =
            default_output_device_name().unwrap_or_else(|| "Unknown".to_string());
        let mut input_labels = vec!["系统默认".to_string()];
        input_labels.extend(self.available_input_devices.clone());
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.input_pick_row.input_device_dropdown))
            .set_labels(cx, input_labels);
        let input_selected_idx = self
            .app_preferences
            .preferred_input_device
            .as_ref()
            .and_then(|name| self.available_input_devices.iter().position(|n| n == name))
            .map(|idx| idx + 1)
            .unwrap_or(0);
        self.selected_input_device_idx = input_selected_idx.saturating_sub(1);
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.input_pick_row.input_device_dropdown))
            .set_selected_item(cx, input_selected_idx);

        let mut output_labels = vec!["系统默认".to_string()];
        output_labels.extend(self.available_output_devices.clone());
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.output_pick_row.output_device_dropdown))
            .set_labels(cx, output_labels);
        let output_selected_idx = self
            .app_preferences
            .preferred_output_device
            .as_ref()
            .and_then(|name| self.available_output_devices.iter().position(|n| n == name))
            .map(|idx| idx + 1)
            .unwrap_or(0);
        self.selected_output_device_idx = output_selected_idx.saturating_sub(1);
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.output_pick_row.output_device_dropdown))
            .set_selected_item(cx, output_selected_idx);
    }

    fn update_user_settings_page(&mut self, cx: &mut Cx) {
        self.update_runtime_status_ui(cx);
        self.update_system_paths_ui(cx);
        self.update_audio_devices_ui(cx);
        let en = self.is_english();
        let voice = self
            .selected_voice_id
            .clone()
            .unwrap_or_else(|| "Doubao".to_string());
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_voice_label))
            .set_text(cx, &format!("默认音色: {}", voice));
        self.view
            .text_input(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.speed_col.speed_input))
            .set_text(cx, &format!("{:.2}", self.app_preferences.default_speed));
        self.view
            .text_input(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.pitch_col.pitch_input))
            .set_text(cx, &format!("{:.1}", self.app_preferences.default_pitch));
        self.view
            .text_input(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.volume_col.volume_input))
            .set_text(cx, &format!("{:.0}", self.app_preferences.default_volume));
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.privacy_card.retention_pick_row.retention_dropdown))
            .set_labels(
                cx,
                if en {
                    vec!["Forever".to_string(), "30 days".to_string(), "7 days".to_string()]
                } else {
                    vec!["永久".to_string(), "30天".to_string(), "7天".to_string()]
                },
            );
        let retention_idx = match self.app_preferences.history_retention_days {
            30 => 1,
            7 => 2,
            _ => 0,
        };
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.privacy_card.retention_pick_row.retention_dropdown))
            .set_selected_item(cx, retention_idx);

        // Qwen3-only: hide zero-shot backend picker and training backend picker.
        // These rows are kept in live_design for easy restoration.
        // See doc/REFACTOR_QWEN3_ONLY.md.
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.zero_shot_backend_pick_row))
            .set_visible(cx, false);
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.backend_pick_row))
            .set_visible(cx, false);

        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.debug_pick_row.debug_logs_dropdown))
            .set_labels(
                cx,
                if en {
                    vec!["Off".to_string(), "On".to_string()]
                } else {
                    vec!["关闭".to_string(), "开启".to_string()]
                },
            );
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.debug_pick_row.debug_logs_dropdown))
            .set_selected_item(cx, if self.app_preferences.debug_logs_enabled { 1 } else { 0 });

        let custom_ready = Self::qwen_custom_ready();
        let base_ready = Self::qwen_base_ready();
        if !self.qwen_download_in_progress {
            self.qwen_model_status_text = if custom_ready && base_ready {
                self.tr("已就绪（推理+克隆）", "Ready (inference + clone)").to_string()
            } else if custom_ready {
                self.tr("部分就绪（仅推理）", "Partially ready (inference only)").to_string()
            } else {
                self.tr("未就绪", "Not ready").to_string()
            };
        }
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.qwen_status_row.qwen_status_value))
            .set_text(cx, &self.qwen_model_status_text);

        self.update_user_settings_tabs(cx);
    }

    fn clear_app_cache(&mut self, cx: &mut Cx) {
        let runtime_out = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".dora")
            .join("runtime")
            .join("out");
        let share_tmp = Self::workspace_dir().join("share");
        let temp_dir = Self::workspace_dir().join("tmp");
        for dir in [&runtime_out, &share_tmp, &temp_dir] {
            if dir.exists() {
                let _ = fs::remove_dir_all(dir);
            }
            let _ = fs::create_dir_all(dir);
        }
        self.add_log(cx, "[INFO] [settings] Cleared runtime cache directories");
    }

    fn clear_training_artifacts(&mut self, cx: &mut Cx) {
        let trained_dir = Self::workspace_dir().join("trained_models");
        if trained_dir.exists() {
            let _ = fs::remove_dir_all(&trained_dir);
        }
        let _ = fs::create_dir_all(&trained_dir);
        self.add_log(cx, "[INFO] [settings] Cleared trained model artifacts");
    }

    fn recreate_players_with_selected_output(&mut self) {
        self.audio_player = Some(TTSPlayer::new_with_output_device(
            self.app_preferences.preferred_output_device.as_deref(),
        ));
        self.preview_player = Some(TTSPlayer::new_with_output_device(
            self.app_preferences.preferred_output_device.as_deref(),
        ));
    }

    fn apply_localization(&mut self, cx: &mut Cx) {
        let en = self.is_english();

        self.view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_tts))
            .set_text(cx, self.tr("文本转语音", "Text to Speech"));
        self.view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_library))
            .set_text(cx, self.tr("音色库", "Voice Library"));
        self.view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_clone))
            .set_text(cx, self.tr("音色克隆", "Voice Clone"));
        self.view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_history))
            .set_text(cx, self.tr("历史", "History"));
        self.view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_translation))
            .set_text(cx, self.tr("实时翻译", "Live Translation"));
        self.view
            .button(ids!(app_layout.sidebar.sidebar_footer.global_settings_btn))
            .set_text(cx, self.tr("⚙", "⚙"));

        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.page_header.page_title
            ))
            .set_text(cx, self.tr("实时翻译", "Live Translation"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.page_header.translation_status_badge.translation_status_text
            ))
            .set_text(cx, self.tr("运行中", "Running"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_source.translation_source_label
            ))
            .set_text(cx, self.tr("输入源", "Input Source"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_src_lang.translation_src_lang_label
            ))
            .set_text(cx, self.tr("输入语言", "Source Language"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_tgt_lang.translation_tgt_lang_label
            ))
            .set_text(cx, self.tr("目标语言", "Target Language"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_overlay.translation_overlay_style_label
            ))
            .set_text(cx, self.tr("浮窗样式", "Overlay Style"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_font_size.translation_font_size_label
            ))
            .set_text(cx, self.tr("文字大小", "Text Size"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_anchor_position.translation_anchor_position_label
            ))
            .set_text(cx, self.tr("滚动位置", "Scroll Position"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_overlay.overlay_style_compact
            ))
            .set_text(cx, self.tr("紧凑", "Compact"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_overlay.overlay_style_full
            ))
            .set_text(cx, self.tr("全屏", "Fullscreen"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_opacity.translation_opacity_label
            ))
            .set_text(cx, self.tr("浮窗不透明度", "Overlay Opacity"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.translation_start_btn
            ))
            .set_text(cx, self.tr("启动实时翻译", "Start Live Translation"));
        #[cfg(target_os = "macos")]
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.translation_permission_hint.translation_permission_hint_label
            ))
            .set_text(cx, self.tr(
                "屏幕录制权限未授权。请前往系统设置 → 隐私与安全性 → 屏幕录制，启用 Moxin Voice，然后重启应用。",
                "Screen recording permission not granted. Go to System Settings → Privacy & Security → Screen Recording, enable Moxin Voice, then restart the app.",
            ));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_running_panel.translation_log_card.translation_log_title
            ))
            .set_text(cx, self.tr("运行日志", "Runtime Logs"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_running_panel.translation_show_overlay_btn
            ))
            .set_text(cx, self.tr("显示浮窗", "Show Overlay"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_running_panel.translation_stop_btn
            ))
            .set_text(cx, self.tr("停止翻译", "Stop Translation"));
        self.update_translation_settings_layout_for_locale(cx);
        self.update_translation_lang_dropdowns(cx);
        self.update_translation_overlay_style_buttons(cx);
        self.update_translation_font_size_dropdown(cx);
        self.update_translation_anchor_position_dropdown(cx);
        self.populate_translation_input_dropdown(cx);
        self.update_translation_opacity_dropdown(cx);
        self.sync_translation_overlay_locale();

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
                content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_gender.row_label
            ))
            .set_text(cx, self.tr("性别年龄", "Gender/Age"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_gender.filter_male_btn
            ))
            .set_text(cx, self.tr("男声", "Male"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_gender.filter_female_btn
            ))
            .set_text(cx, self.tr("女声", "Female"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_gender.age_adult_btn
            ))
            .set_text(cx, self.tr("成年", "Adult"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_gender.age_youth_btn
            ))
            .set_text(cx, self.tr("青年", "Youth"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_style.row_label
            ))
            .set_text(cx, self.tr("风格", "Style"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_style.style_sweet_btn
            ))
            .set_text(cx, self.tr("甜美", "Sweet"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_style.style_magnetic_btn
            ))
            .set_text(cx, self.tr("磁性", "Magnetic"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_trait.row_label
            ))
            .set_text(cx, self.tr("声音特质", "Traits"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_trait.trait_prof_btn
            ))
            .set_text(cx, self.tr("专业播音", "Pro Voice"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_trait.trait_character_btn
            ))
            .set_text(cx, self.tr("特色人物", "Character"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.language_filter.lang_all_btn
            ))
            .set_text(cx, self.tr("全部语言", "All"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.language_filter.lang_zh_btn
            ))
            .set_text(cx, self.tr("中文", "ZH"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.language_filter.lang_en_btn
            ))
            .set_text(cx, self.tr("英文", "EN"));
        self.view
            .text_input(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.search_input
            ))
            .apply_over(
                cx,
                live! {
                    empty_text: (if en { "Search voices..." } else { "搜索音色..." })
                },
            );
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.refresh_btn
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
        // Qwen3-only: Pro/Advanced mode removed. Hide advanced_mode_btn.
        // Restore by un-commenting and re-enabling CloneMode::Pro. See doc/REFACTOR_QWEN3_ONLY.md.
        self.view
            .view(ids!(
                content_wrapper.main_content.left_column.content_area.clone_page.clone_header.clone_title_section.mode_selector
            ))
            .set_visible(cx, false); // hide entire mode_selector (both tabs)
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.clone_page.clone_header.create_task_btn
            ))
            .set_text(cx, self.tr("创建任务", "Create Task"));
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
                content_wrapper.main_content.left_column.content_area.user_settings_page.user_settings_header.user_settings_title
            ))
            .set_text(cx, self.tr("用户设置", "User Settings"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_tab_bar.tab_profile_btn
            ))
            .set_text(cx, self.tr("通用", "General"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_tab_bar.tab_app_btn
            ))
            .set_text(cx, self.tr("语音", "Voice"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_tab_bar.tab_runtime_btn
            ))
            .set_text(cx, self.tr("系统", "System"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.app_settings_title
            ))
            .set_text(cx, self.tr("通用设置", "General Settings"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.language_section.language_title
            ))
            .set_text(cx, self.tr("语言", "Language"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.theme_section.theme_title
            ))
            .set_text(cx, self.tr("主题", "Theme"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.profile_card.profile_title
            ))
            .set_text(cx, self.tr("个人资料", "Profile"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.profile_card.profile_body.profile_form.name_row.name_label
            ))
            .set_text(cx, self.tr("用户名", "Username"));
        self.view
            .text_input(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.profile_card.profile_body.profile_form.name_row.name_input
            ))
            .set_empty_text(cx, self.tr("输入用户名", "Enter username").to_string());
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.profile_card.profile_actions.save_profile_btn
            ))
            .set_text(cx, self.tr("保存", "Save"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_title
            ))
            .set_text(cx, self.tr("推理默认参数", "Default Inference Params"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.speed_col.speed_label
            ))
            .set_text(cx, self.tr("语速", "Speed"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.pitch_col.pitch_label
            ))
            .set_text(cx, self.tr("音高", "Pitch"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.volume_col.volume_label
            ))
            .set_text(cx, self.tr("音量", "Volume"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_actions.save_defaults_btn
            ))
            .set_text(cx, self.tr("保存默认", "Save Defaults"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_actions.apply_defaults_now_btn
            ))
            .set_text(cx, self.tr("应用到当前", "Apply to Current"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.runtime_card.runtime_title
            ))
            .set_text(cx, self.tr("运行状态", "Runtime Status"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.runtime_card.runtime_refresh_btn
            ))
            .set_text(cx, self.tr("刷新状态", "Refresh Status"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.paths_title
            ))
            .set_text(cx, self.tr("本地路径与资源", "Local Paths & Resources"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.path_actions.open_model_dir_btn
            ))
            .set_text(cx, self.tr("打开模型目录", "Open Models"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.path_actions.open_log_dir_btn
            ))
            .set_text(cx, self.tr("打开日志目录", "Open Logs"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.path_actions.open_workspace_dir_btn
            ))
            .set_text(cx, self.tr("打开工作目录", "Open Workspace"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.clear_cache_btn
            ))
            .set_text(cx, self.tr("清理缓存", "Clear Cache"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.privacy_card.privacy_title
            ))
            .set_text(cx, self.tr("隐私与数据", "Privacy & Data"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.privacy_card.retention_pick_row.retention_pick_label
            ))
            .set_text(cx, self.tr("历史保留", "History retention"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.privacy_card.privacy_actions.clear_tts_history_btn
            ))
            .set_text(cx, self.tr("清空 TTS 历史", "Clear TTS History"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.privacy_card.privacy_actions.clear_training_artifacts_btn
            ))
            .set_text(cx, self.tr("清理训练中间产物", "Clear Training Artifacts"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.devices_title
            ))
            .set_text(cx, self.tr("音频设备", "Audio Devices"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.input_pick_row.input_pick_label
            ))
            .set_text(cx, self.tr("输入", "Input"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.output_pick_row.output_pick_label
            ))
            .set_text(cx, self.tr("输出", "Output"));
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.devices_header.refresh_devices_btn
            ))
            .set_text(cx, self.tr("刷新设备", "Refresh Devices"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.experiments_title
            ))
            .set_text(cx, self.tr("实验功能", "Experimental"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.zero_shot_backend_pick_row.zero_shot_backend_pick_label
            ))
            .set_text(cx, self.tr("Zero-shot 后端", "Zero-shot backend"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.backend_pick_row.backend_pick_label
            ))
            .set_text(cx, self.tr("训练后端", "Training backend"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.debug_pick_row.debug_pick_label
            ))
            .set_text(cx, self.tr("Debug 日志", "Debug logs"));
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.qwen_status_row.qwen_status_label
            ))
            .set_text(cx, self.tr("Qwen 模型", "Qwen models"));
        self.view
            .drop_down(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.privacy_card.retention_pick_row.retention_dropdown
            ))
            .set_labels(
                cx,
                if en {
                    vec!["Forever".to_string(), "30 days".to_string(), "7 days".to_string()]
                } else {
                    vec!["永久".to_string(), "30天".to_string(), "7天".to_string()]
                },
            );
        self.view
            .drop_down(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.zero_shot_backend_pick_row.zero_shot_backend_dropdown
            ))
            .set_labels(
                cx,
                if en {
                    vec![
                        "PrimeSpeech MLX".to_string(),
                        "Qwen3 TTS MLX".to_string(),
                    ]
                } else {
                    vec![
                        "PrimeSpeech MLX".to_string(),
                        "Qwen3 TTS MLX".to_string(),
                    ]
                },
            );
        self.view
            .drop_down(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.backend_pick_row.training_backend_dropdown
            ))
            .set_labels(
                cx,
                if en {
                    vec![
                        "Compatibility (Python)".to_string(),
                        "MLX Experimental (Rust)".to_string(),
                        "Qwen3 Mode".to_string(),
                    ]
                } else {
                    vec![
                        "兼容模式（Python）".to_string(),
                        "MLX 实验模式（Rust）".to_string(),
                        "Qwen3 模式".to_string(),
                    ]
                },
            );
        self.view
            .drop_down(ids!(
                content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.debug_pick_row.debug_logs_dropdown
            ))
            .set_labels(
                cx,
                if en {
                    vec!["Off".to_string(), "On".to_string()]
                } else {
                    vec!["关闭".to_string(), "开启".to_string()]
                },
            );

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
        self.update_audio_player_action_layout_for_locale(cx);

        self.view
            .label(ids!(download_modal.download_dialog.download_header.download_title))
            .set_text(cx, self.tr("下载音频", "Download audio"));
        self.view
            .label(ids!(download_modal.download_dialog.download_header.download_subtitle))
            .set_text(
                cx,
                self.tr(
                    "选择要导出的音频格式",
                    "Choose the audio format to export",
                ),
            );
        self.view
            .button(ids!(download_modal.download_dialog.download_actions.download_mp3_btn))
            .set_text(cx, self.tr("MP3 文件", "MP3 file"));
        self.view
            .button(ids!(download_modal.download_dialog.download_actions.download_wav_btn))
            .set_text(cx, self.tr("WAV 文件", "WAV file"));
        self.view
            .button(ids!(download_modal.download_dialog.download_footer.download_cancel_btn))
            .set_text(cx, self.tr("取消", "Cancel"));

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
            .set_text(cx, self.tr("设置", "Settings"));
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.language_section.language_title))
            .set_text(cx, self.tr("语言", "Language"));
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.theme_section.theme_title))
            .set_text(cx, self.tr("主题", "Theme"));
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.about_section.about_title))
            .set_text(cx, self.tr("关于", "About"));
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.about_section.about_engine))
            .set_text(cx, self.tr("基于 OminiX MLX · Qwen3-TTS-MLX", "Powered by OminiX MLX · Qwen3-TTS-MLX"));
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.about_section.about_ominix))
            .set_text(cx, "github.com/OminiX-ai/OminiX-MLX");
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
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.language_section.language_options.lang_en_option))
            .set_text(cx, "English");
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.language_section.language_options.lang_zh_option))
            .set_text(cx, if en { "Chinese" } else { "中文" });
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.theme_section.theme_options.theme_light_option))
            .set_text(cx, self.tr("☀️ 浅色", "☀️ Light"));
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.theme_section.theme_options.theme_dark_option))
            .set_text(cx, self.tr("🌙 深色", "🌙 Dark"));

        if !self.user_profile_customized {
            self.user_display_name = self.tr("用户", "User").to_string();
        }
        self.sync_user_profile_ui(cx);
        self.update_user_settings_page(cx);

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

        // Hide audio player bar when in History mode
        let show_player = self.has_generated_audio && !history_mode
            && self.current_page == AppPage::TextToSpeech;
        self.view
            .view(ids!(content_wrapper.audio_player_bar))
            .set_visible(cx, show_player);

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
        let user_settings_active = if self.current_page == AppPage::UserSettings {
            1.0
        } else {
            0.0
        };
        let translation_active = if self.current_page == AppPage::Translation { 1.0 } else { 0.0 };

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
        self.view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_translation))
            .apply_over(
                cx,
                live! {
                    draw_bg: { active: (translation_active) }
                    draw_text: { active: (translation_active) }
                },
            );
        self.view
            .view(ids!(app_layout.sidebar.sidebar_footer))
            .apply_over(cx, live! { draw_bg: { active: (user_settings_active) } });
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
        let show_user_settings = page == AppPage::UserSettings;
        let show_translation = page == AppPage::Translation;

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
        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .user_settings_page
            ))
            .set_visible(cx, show_user_settings);
        self.view
            .view(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
                    .translation_page
            ))
            .set_visible(cx, show_translation);

        // Show audio player bar only on TTS page (not History tab) after first successful generation
        let history_mode = show_tts && self.controls_panel_tab == 2;
        self.view
            .view(ids!(content_wrapper.audio_player_bar))
            .set_visible(cx, show_tts && self.has_generated_audio && !history_mode);

        if show_library {
            self.update_library_display(cx);
        }
        if show_user_settings {
            self.update_user_settings_page(cx);
            self.update_user_settings_tabs(cx);
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
        self.update_char_count_from_text(cx, &text);
    }

    fn update_char_count_from_text(&mut self, cx: &mut Cx, text: &str) {
        let count = text.chars().count();
        let label = if self.is_english() {
            format!("{} / 1,000 characters", count)
        } else {
            format!("{} / 1,000 字符", count)
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
        let selected_voice_is_trained = self
            .selected_voice_id
            .as_ref()
            .and_then(|id| self.library_voices.iter().find(|v| &v.id == id))
            .map(|voice| voice.source == crate::voice_data::VoiceSource::Trained)
            .unwrap_or(false);
        let trained_voice_supported = !(
            Self::is_qwen_backend(&self.app_preferences.inference_backend) && selected_voice_is_trained
        );

        // Update button text
        let button_text = if loading {
            self.tr("生成中...", "Generating...")
        } else if !trained_voice_supported {
            self.tr("Qwen 暂不支持训练音色", "Qwen does not support trained voices")
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
            .set_enabled(cx, !loading && trained_voice_supported);

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
        let history_mode =
            self.current_page == AppPage::TextToSpeech && self.controls_panel_tab == 2;
        let show_player =
            self.current_page == AppPage::TextToSpeech && self.has_generated_audio && !history_mode;
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
                    .voice_name_clip
                    .voice_name_scroller
                    .current_voice_name
            ))
            .set_text(cx, voice_name);
        self.view
            .label(ids!(
                content_wrapper
                    .audio_player_bar
                    .voice_info
                    .voice_name_container
                    .voice_name_clip
                    .voice_name_scroller
                    .current_voice_name_clone
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
        self.reset_voice_name_marquee(cx);
    }

    fn reset_voice_name_marquee(&mut self, cx: &mut Cx) {
        self.voice_name_marquee_offset = 0.0;
        self.view
            .view(ids!(
                content_wrapper
                    .audio_player_bar
                    .voice_info
                    .voice_name_container
                    .voice_name_clip
                    .voice_name_scroller
            ))
            .apply_over(cx, live! { margin: { left: 0.0 } });
        self.view
            .label(ids!(
                content_wrapper
                    .audio_player_bar
                    .voice_info
                    .voice_name_container
                    .voice_name_clip
                    .voice_name_scroller
                    .current_voice_name_clone
            ))
            .set_visible(cx, false);
    }

    fn update_voice_name_marquee(&mut self, cx: &mut Cx) {
        let clip = self.view.view(ids!(
            content_wrapper
                .audio_player_bar
                .voice_info
                .voice_name_container
                .voice_name_clip
        ));
        let label = self.view.label(ids!(
            content_wrapper
                .audio_player_bar
                .voice_info
                .voice_name_container
                .voice_name_clip
                .voice_name_scroller
                .current_voice_name
        ));
        let clone = self.view.label(ids!(
            content_wrapper
                .audio_player_bar
                .voice_info
                .voice_name_container
                .voice_name_clip
                .voice_name_scroller
                .current_voice_name_clone
        ));
        let scroller = self.view.view(ids!(
            content_wrapper
                .audio_player_bar
                .voice_info
                .voice_name_container
                .voice_name_clip
                .voice_name_scroller
        ));

        let clip_area = clip.area();
        let label_area = label.area();
        if clip_area.is_empty() || label_area.is_empty() {
            return;
        }

        let clip_width = clip_area.rect(cx).size.x.max(0.0);
        let label_width = label_area.rect(cx).size.x.max(0.0);
        if clip_width <= 1.0 || label_width <= 1.0 {
            return;
        }

        if label_width <= clip_width - 2.0 {
            if self.voice_name_marquee_offset != 0.0 {
                self.voice_name_marquee_offset = 0.0;
                scroller.apply_over(cx, live! { margin: { left: 0.0 } });
            }
            clone.set_visible(cx, false);
            return;
        }

        clone.set_visible(cx, true);

        let dt = 0.1;
        let speed = 15.0;
        let gap = 24.0;
        let max_offset = label_width + gap;
        self.voice_name_marquee_offset += speed * dt;
        if self.voice_name_marquee_offset >= max_offset {
            self.voice_name_marquee_offset -= max_offset;
        }

        scroller.apply_over(
            cx,
            live! { margin: { left: (-self.voice_name_marquee_offset) } },
        );
        self.view.redraw(cx);
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
        let effective_rate = self.effective_audio_sample_rate();
        let has_playable_audio = self.has_generated_audio && audio_len > 0 && effective_rate > 0;
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
            let duration_secs = audio_len as f32 / effective_rate as f32;
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
        let effective_rate = self.effective_audio_sample_rate();
        if samples.is_empty() || effective_rate == 0 {
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
        if let Err(e) = Self::write_wav_file_with_sample_rate(&audio_path, &samples, effective_rate)
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

        let duration_secs = samples.len() as f32 / effective_rate as f32;
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
            sample_rate: effective_rate,
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
        let effective_rate = self.effective_audio_sample_rate();
        if audio_len == 0 || effective_rate == 0 {
            return;
        }

        let total_duration = audio_len as f32 / effective_rate as f32;
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
        } else if voice.source == VoiceSource::BundledIcl {
            // BundledIcl voice: use bundled ref audio as preview
            let ref_filename = voice.reference_audio_path.as_deref().unwrap_or("ref.wav");
            match self.resolve_bundled_icl_ref_path(&voice.id, ref_filename) {
                Some(path) => path,
                None => {
                    self.add_log(cx, &format!("[WARN] [tts] Bundled ref audio not found for: {}", voice_id));
                    return;
                }
            }
        } else {
            // Built-in voice: resolve preview audio path by backend
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

            if self.app_preferences.inference_backend == "qwen3_tts_mlx" {
                // Qwen3 preview audio: bundled in repo/app
                self.resolve_qwen_preview_path(&preview_file)
            } else {
                // PrimeSpeech preview audio: reference WAVs in the models dir
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                home.join(".dora")
                    .join("models")
                    .join("primespeech")
                    .join("moyoyo")
                    .join("ref_audios")
                    .join(&preview_file)
            }
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
                    self.preview_player = Some(TTSPlayer::new_with_output_device(
                        self.app_preferences.preferred_output_device.as_deref(),
                    ));
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

    /// Resolve the path for a Qwen3-TTS built-in voice preview WAV file.
    /// Search order:
    ///   1. App bundle: $MOXIN_APP_RESOURCES/qwen3-previews/<file>
    ///   2. Dev repo:   <exe>/../../node-hub/dora-qwen3-tts-mlx/previews/<file>
    ///   3. Fallback:   ~/.OminiX/models/qwen3-tts-mlx/previews/<file>
    fn resolve_qwen_preview_path(&self, filename: &str) -> PathBuf {
        // 1. Bundle location (set by the launcher script for packaged apps)
        if let Ok(res) = std::env::var("MOXIN_APP_RESOURCES") {
            let p = PathBuf::from(&res).join("qwen3-previews").join(filename);
            if p.exists() {
                return p;
            }
        }
        // 2. Dev: repo root relative to the compiled binary
        //    target/{debug,release}/moxin-voice-shell → ../../node-hub/dora-qwen3-tts-mlx/previews
        if let Ok(exe) = std::env::current_exe() {
            if let Some(target_dir) = exe.parent().and_then(|d| d.parent()) {
                let p = target_dir
                    .join("node-hub")
                    .join("dora-qwen3-tts-mlx")
                    .join("previews")
                    .join(filename);
                if p.exists() {
                    return p;
                }
            }
        }
        // 3. Fallback: user-local generated previews
        let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
        home.join(".OminiX")
            .join("models")
            .join("qwen3-tts-mlx")
            .join("previews")
            .join(filename)
    }

    /// Resolve the absolute path of a bundled ICL voice's reference audio.
    /// voice_id: e.g. "baiyang", filename: e.g. "ref.wav"
    fn resolve_bundled_icl_ref_path(&self, voice_id: &str, filename: &str) -> Option<PathBuf> {
        // 1. App bundle
        if let Ok(res) = std::env::var("MOXIN_APP_RESOURCES") {
            let p = PathBuf::from(&res).join("qwen3-voices").join(voice_id).join(filename);
            if p.exists() {
                return Some(p);
            }
        }
        // 2. Dev: current working directory (project root when using cargo run)
        if let Ok(cwd) = std::env::current_dir() {
            let p = cwd
                .join("node-hub")
                .join("dora-qwen3-tts-mlx")
                .join("voices")
                .join(voice_id)
                .join(filename);
            if p.exists() {
                return Some(p);
            }
        }
        // 3. Dev: exe path → target/{debug,release}/../../../ (project root)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(project_dir) = exe.parent().and_then(|d| d.parent()).and_then(|d| d.parent()) {
                let p = project_dir
                    .join("node-hub")
                    .join("dora-qwen3-tts-mlx")
                    .join("voices")
                    .join(voice_id)
                    .join(filename);
                if p.exists() {
                    return Some(p);
                }
            }
        }
        None
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

    fn start_qwen_model_download(
        &mut self,
        cx: &mut Cx,
        target_backend: &str,
        need_custom: bool,
        need_base: bool,
    ) {
        if self.qwen_download_in_progress {
            self.show_toast(
                cx,
                self.tr("Qwen 模型下载中，请稍候", "Qwen model download already running"),
            );
            return;
        }
        if !need_custom && !need_base {
            return;
        }

        let Some(script_path) = Self::resolve_qwen_download_script_path() else {
            self.show_toast(
                cx,
                self.tr(
                    "未找到 Qwen 下载脚本，无法启动下载",
                    "Qwen download script not found",
                ),
            );
            return;
        };

        let qwen_root = Self::qwen_root_dir();
        let log_dir = PathBuf::from(
            std::env::var("HOME").unwrap_or_else(|_| ".".to_string()) + "/Library/Logs/MoxinVoice",
        );
        let _ = std::fs::create_dir_all(&log_dir);
        let qwen_log = log_dir.join("qwen_model_download.log");

        self.qwen_download_in_progress = true;
        self.qwen_model_status_text = self.tr("下载中...", "Downloading...").to_string();
        self.update_user_settings_page(cx);
        self.add_log(
            cx,
            &format!(
                "[INFO] [qwen] Starting background model download (backend={}, custom={}, base={})",
                target_backend, need_custom, need_base
            ),
        );

        let (tx, rx) = mpsc::channel::<QwenModelDownloadEvent>();
        self.qwen_download_rx = Some(rx);

        thread::spawn(move || {
            let _ = tx.send(QwenModelDownloadEvent::Stage(
                "Preparing Qwen model download".to_string(),
            ));

            let conda_bin = std::env::var("MOXIN_CONDA_BIN")
                .ok()
                .filter(|p| Path::new(p).exists())
                .unwrap_or_else(|| {
                    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                    format!("{}/.moxinvoice/conda/bin/conda", home)
                });
            let conda_env_prefix = std::env::var("MOXIN_CONDA_ENV_PREFIX")
                .ok()
                .unwrap_or_else(|| {
                    let home = std::env::var("HOME").unwrap_or_else(|_| ".".to_string());
                    let env = std::env::var("MOXIN_CONDA_ENV").unwrap_or_else(|_| "moxin-studio".to_string());
                    format!("{}/.moxinvoice/conda/envs/{}", home, env)
                });

            let log_file = match OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&qwen_log)
            {
                Ok(f) => f,
                Err(err) => {
                    let _ = tx.send(QwenModelDownloadEvent::DoneErr(format!(
                        "open qwen download log failed: {}",
                        err
                    )));
                    return;
                }
            };
            let log_file_err = match log_file.try_clone() {
                Ok(f) => f,
                Err(err) => {
                    let _ = tx.send(QwenModelDownloadEvent::DoneErr(format!(
                        "clone qwen download log fd failed: {}",
                        err
                    )));
                    return;
                }
            };

            let mut cmd = if Path::new(&conda_bin).exists() && Path::new(&conda_env_prefix).exists() {
                let mut c = Command::new(&conda_bin);
                c.arg("run")
                    .arg("-p")
                    .arg(&conda_env_prefix)
                    .arg("python")
                    .arg(&script_path);
                c
            } else {
                let mut c = Command::new("python3");
                c.arg(&script_path);
                c
            };

            cmd.arg("--root").arg(&qwen_root);
            if need_custom {
                cmd.arg("--need-custom");
            }
            if need_base {
                cmd.arg("--need-base");
            }
            cmd.stdout(Stdio::from(log_file));
            cmd.stderr(Stdio::from(log_file_err));

            let status = cmd.status();
            match status {
                Ok(s) if s.success() => {
                    let _ = tx.send(QwenModelDownloadEvent::DoneOk);
                }
                Ok(s) => {
                    let _ = tx.send(QwenModelDownloadEvent::DoneErr(format!(
                        "qwen model download exited with status {} (log: {})",
                        s,
                        qwen_log.display()
                    )));
                }
                Err(err) => {
                    let _ = tx.send(QwenModelDownloadEvent::DoneErr(format!(
                        "failed to launch qwen model download: {}",
                        err
                    )));
                }
            }
        });
    }

    fn poll_qwen_model_download(&mut self, cx: &mut Cx) {
        let mut latest_event: Option<QwenModelDownloadEvent> = None;
        if let Some(rx) = &self.qwen_download_rx {
            while let Ok(ev) = rx.try_recv() {
                latest_event = Some(ev);
            }
        }
        let Some(event) = latest_event else {
            return;
        };

        match event {
            QwenModelDownloadEvent::Stage(detail) => {
                self.qwen_model_status_text = self.tr("下载中...", "Downloading...").to_string();
                self.add_log(cx, &format!("[INFO] [qwen] {}", detail));
                self.update_user_settings_page(cx);
            }
            QwenModelDownloadEvent::DoneOk => {
                self.qwen_download_in_progress = false;
                self.qwen_download_rx = None;
                let custom_ready = Self::qwen_custom_ready();
                let base_ready = Self::qwen_base_ready();
                self.qwen_model_status_text = if custom_ready && base_ready {
                    self.tr("已就绪（推理+克隆）", "Ready (inference + clone)").to_string()
                } else if custom_ready {
                    self.tr("部分就绪（仅推理）", "Partially ready (inference only)").to_string()
                } else {
                    self.tr("未就绪", "Not ready").to_string()
                };
                self.update_user_settings_page(cx);
                self.show_toast(
                    cx,
                    self.tr(
                        "Qwen 模型下载完成，请在设置中重新切换后端",
                        "Qwen models downloaded. Please switch backend again in Settings",
                    ),
                );
                self.add_log(cx, "[INFO] [qwen] Qwen model download completed");
            }
            QwenModelDownloadEvent::DoneErr(err) => {
                self.qwen_download_in_progress = false;
                self.qwen_download_rx = None;
                self.qwen_model_status_text = self.tr("下载失败", "Download failed").to_string();
                self.update_user_settings_page(cx);
                self.show_toast(
                    cx,
                    self.tr(
                        "Qwen 模型下载失败，请查看日志",
                        "Qwen model download failed. Check logs",
                    ),
                );
                self.add_log(cx, &format!("[ERROR] [qwen] {}", err));
            }
        }
    }

    fn start_runtime_initialization(&mut self, cx: &mut Cx) {
        Self::ensure_bundle_bin_on_path();

        let app_resources = match std::env::var("MOXIN_APP_RESOURCES") {
            Ok(v) => PathBuf::from(v),
            Err(_) => {
                // Dev mode default: skip bootstrap/preflight unless explicitly enabled.
                let enable_dev_bootstrap =
                    std::env::var("MOXIN_ENABLE_DEV_BOOTSTRAP").ok().as_deref() == Some("1");
                let cwd = std::env::current_dir().unwrap_or_else(|_| PathBuf::from("."));
                let direct = cwd.join("scripts").join("macos_preflight.sh");
                if enable_dev_bootstrap && direct.exists() {
                    cwd
                } else {
                    self.runtime_init_state = RuntimeInitState::Ready;
                    self.runtime_init_status_text = self.tr("连接中...", "Connecting...").to_string();
                    self.runtime_init_detail_text =
                        self.tr("正在启动 TTS 数据流引擎", "Starting TTS dataflow engine").to_string();
                    return;
                }
            }
        };

        let pre = app_resources.join("scripts/macos_preflight.sh");
        let boot = app_resources.join("scripts/macos_bootstrap.sh");
        if !pre.exists() || !boot.exists() {
            self.runtime_init_state = RuntimeInitState::Ready;
            self.runtime_init_status_text = self.tr("连接中...", "Connecting...").to_string();
            self.runtime_init_detail_text =
                self.tr("正在启动 TTS 数据流引擎", "Starting TTS dataflow engine").to_string();
            return;
        }

        let log_dir = PathBuf::from(
            std::env::var("HOME").unwrap_or_else(|_| ".".to_string()) + "/Library/Logs/MoxinVoice",
        );
        let _ = std::fs::create_dir_all(&log_dir);
        let bootstrap_log = log_dir.join("bootstrap.log");
        let bootstrap_state = log_dir.join("bootstrap_state.txt");
        let _ = std::fs::remove_file(&bootstrap_state);

        self.runtime_init_state = RuntimeInitState::Running;
        self.runtime_init_status_text =
            self.tr("初始化中...", "Initializing...").to_string();
        self.runtime_init_detail_text =
            self.tr("正在检查运行环境", "Checking runtime environment").to_string();

        self.add_log(cx, "[INFO] [startup] Checking runtime dependencies...");

        let (tx, rx) = mpsc::channel::<RuntimeInitEvent>();
        self.runtime_init_rx = Some(rx);
        let inference_backend = self.app_preferences.inference_backend.clone();
        let zero_shot_backend = self.app_preferences.zero_shot_backend.clone();
        let app_resources_env = app_resources.clone();

        thread::spawn(move || {
            let _ = tx.send(RuntimeInitEvent::Stage {
                status: "Initializing runtime".to_string(),
                detail: "Checking environment".to_string(),
            });

            let pre_ok = Command::new(&pre)
                .env("MOXIN_APP_RESOURCES", &app_resources_env)
                .env("MOXIN_INFERENCE_BACKEND", &inference_backend)
                .env("MOXIN_ZERO_SHOT_BACKEND", &zero_shot_backend)
                .arg("--quick")
                .status()
                .map(|s| s.success())
                .unwrap_or(false);
            if pre_ok {
                let _ = tx.send(RuntimeInitEvent::DoneOk);
                return;
            }

            let _ = tx.send(RuntimeInitEvent::Stage {
                status: "Initializing runtime (0/10)".to_string(),
                detail: "Installing dependencies and models (first launch may take several minutes)"
                    .to_string(),
            });

            let log_file = match OpenOptions::new()
                .create(true)
                .truncate(true)
                .write(true)
                .open(&bootstrap_log)
            {
                Ok(f) => f,
                Err(err) => {
                    let _ = tx.send(RuntimeInitEvent::DoneErr(format!(
                        "Cannot open bootstrap log: {}",
                        err
                    )));
                    return;
                }
            };
            let log_file_err = match log_file.try_clone() {
                Ok(f) => f,
                Err(err) => {
                    let _ = tx.send(RuntimeInitEvent::DoneErr(format!(
                        "Cannot clone bootstrap log fd: {}",
                        err
                    )));
                    return;
                }
            };

            let mut child = match Command::new(&boot)
                .env("MOXIN_APP_RESOURCES", &app_resources_env)
                .env("MOXIN_INFERENCE_BACKEND", &inference_backend)
                .env("MOXIN_ZERO_SHOT_BACKEND", &zero_shot_backend)
                .env("MOXIN_BOOTSTRAP_STATE_PATH", &bootstrap_state)
                .stdout(Stdio::from(log_file))
                .stderr(Stdio::from(log_file_err))
                .spawn()
            {
                Ok(c) => c,
                Err(err) => {
                    let _ = tx.send(RuntimeInitEvent::DoneErr(format!(
                        "Failed to launch bootstrap: {}",
                        err
                    )));
                    return;
                }
            };

            let mut last_state = String::new();
            let boot_ok = loop {
                if let Ok(state_content) = std::fs::read_to_string(&bootstrap_state) {
                    let state = state_content.trim().to_string();
                    if !state.is_empty() && state != last_state {
                        last_state = state.clone();
                        if let Some((progress, rest)) = state.split_once('|') {
                            let mut parts = rest.splitn(2, '|');
                            let title = parts.next().unwrap_or("").trim();
                            let detail = parts.next().unwrap_or("").trim();
                            let _ = tx.send(RuntimeInitEvent::Stage {
                                status: format!("Initializing runtime ({})", progress.trim()),
                                detail: if title.is_empty() && detail.is_empty() {
                                    "Working...".to_string()
                                } else if detail.is_empty() {
                                    title.to_string()
                                } else {
                                    format!("{} - {}", title, detail)
                                },
                            });
                        }
                    }
                }

                match child.try_wait() {
                    Ok(Some(status)) => break status.success(),
                    Ok(None) => {
                        std::thread::sleep(std::time::Duration::from_millis(300));
                    }
                    Err(_) => break false,
                }
            };

            if !boot_ok {
                let _ = tx.send(RuntimeInitEvent::DoneErr(format!(
                    "Initialization failed. Check: {}",
                    bootstrap_log.display()
                )));
                return;
            }

            let _ = tx.send(RuntimeInitEvent::Stage {
                status: "Runtime ready".to_string(),
                detail: "Preparing TTS engine".to_string(),
            });
            let _ = tx.send(RuntimeInitEvent::DoneOk);
        });
    }

    fn poll_runtime_initialization(&mut self, cx: &mut Cx) {
        let mut latest_event: Option<RuntimeInitEvent> = None;
        if let Some(rx) = &self.runtime_init_rx {
            while let Ok(ev) = rx.try_recv() {
                latest_event = Some(ev);
            }
        }

        if let Some(ev) = latest_event {
            match ev {
                RuntimeInitEvent::Stage { status, detail } => {
                    self.runtime_init_status_text = status;
                    self.runtime_init_detail_text = detail;
                }
                RuntimeInitEvent::DoneOk => {
                    self.runtime_init_state = RuntimeInitState::Ready;
                    self.runtime_init_status_text = self.tr("连接中...", "Connecting...").to_string();
                    self.runtime_init_detail_text =
                        self.tr("正在启动 TTS 数据流引擎", "Starting TTS dataflow engine").to_string();
                    self.runtime_init_rx = None;
                    self.add_log(cx, "[INFO] [startup] Runtime initialization completed");
                }
                RuntimeInitEvent::DoneErr(message) => {
                    self.runtime_init_state = RuntimeInitState::Failed;
                    self.runtime_init_status_text =
                        self.tr("初始化失败", "Initialization failed").to_string();
                    self.runtime_init_detail_text = message.clone();
                    self.runtime_init_rx = None;
                    self.add_log(cx, &format!("[ERROR] [startup] {}", message));
                }
            }
        }
    }

    fn auto_start_dataflow(&mut self, cx: &mut Cx) {
        Self::ensure_bundle_bin_on_path();

        let should_start = self.dora.as_ref().map(|d| !d.is_running()).unwrap_or(false);
        if !should_start || self.dora_start_in_flight {
            return;
        }

        let dataflow_path = match self.materialize_runtime_dataflow(cx) {
            Ok(path) => path,
            Err(err) => {
                self.add_log(
                    cx,
                    &format!("[ERROR] [tts] Failed to prepare runtime dataflow: {}", err),
                );
                self.dora_start_in_flight = false;
                return;
            }
        };
        if !dataflow_path.exists() {
            self.add_log(
                cx,
                &format!(
                    "[ERROR] [tts] Dataflow file not found: {}",
                    dataflow_path.display()
                ),
            );
            self.dora_start_in_flight = false;
            return;
        }
        self.dora_start_in_flight = true;
        self.dora_start_attempt_at = Some(std::time::Instant::now());
        self.dora_pending_dataflow_path = Some(dataflow_path);

        self.add_log(cx, "[INFO] [tts] Ensuring Dora runtime is up...");
        let (tx, rx) = mpsc::channel::<DoraStartupEvent>();
        self.dora_start_rx = Some(rx);

        thread::spawn(move || {
            let _ = tx.send(DoraStartupEvent::Stage(
                "Checking Dora runtime".to_string(),
            ));

            let mut dora_ready = Command::new("dora")
                .args(["system", "status"])
                .status()
                .map(|status| status.success())
                .unwrap_or(false);

            if !dora_ready {
                let _ = tx.send(DoraStartupEvent::Stage(
                    "Starting Dora runtime".to_string(),
                ));

                match Command::new("dora").args(["up"]).status() {
                    Ok(status) if status.success() => {}
                    Ok(status) => {
                        let _ = tx.send(DoraStartupEvent::Failed(format!(
                            "`dora up` exited with status {}",
                            status
                        )));
                        return;
                    }
                    Err(err) => {
                        let _ = tx.send(DoraStartupEvent::Failed(format!(
                            "Failed to run `dora up`: {}",
                            err
                        )));
                        return;
                    }
                }

                let _ = tx.send(DoraStartupEvent::Stage(
                    "Waiting for Dora runtime readiness".to_string(),
                ));

                for _ in 0..12 {
                    dora_ready = Command::new("dora")
                        .args(["system", "status"])
                        .status()
                        .map(|status| status.success())
                        .unwrap_or(false);
                    if dora_ready {
                        break;
                    }
                    thread::sleep(Duration::from_millis(250));
                }
            }

            if dora_ready {
                let _ = tx.send(DoraStartupEvent::Ready);
            } else {
                let _ = tx.send(DoraStartupEvent::Failed(
                    "Dora runtime not ready after startup".to_string(),
                ));
            }
        });
    }

    fn poll_dora_startup(&mut self, cx: &mut Cx) {
        let mut latest_event: Option<DoraStartupEvent> = None;
        if let Some(rx) = &self.dora_start_rx {
            while let Ok(event) = rx.try_recv() {
                latest_event = Some(event);
            }
        }

        let Some(event) = latest_event else {
            return;
        };

        match event {
            DoraStartupEvent::Stage(detail) => {
                self.add_log(cx, &format!("[INFO] [tts] {}", detail));
            }
            DoraStartupEvent::Ready => {
                self.dora_start_rx = None;
                self.add_log(cx, "[INFO] [tts] Dora runtime ready");

                let Some(dataflow_path) = self.dora_pending_dataflow_path.take() else {
                    self.dora_start_in_flight = false;
                    self.dora_started = false;
                    self.add_log(cx, "[ERROR] [tts] Missing pending dataflow path");
                    return;
                };

                self.add_log(cx, "[INFO] [tts] Auto-starting TTS dataflow...");
                if let Some(dora) = &mut self.dora {
                    if dora.start_dataflow(dataflow_path) {
                        self.add_log(
                            cx,
                            "[INFO] [tts] Dataflow start command submitted, connecting...",
                        );
                    } else {
                        self.dora_start_in_flight = false;
                        self.dora_started = false;
                        self.add_log(cx, "[ERROR] [tts] Failed to submit Dora start command");
                    }
                } else {
                    self.dora_start_in_flight = false;
                    self.dora_started = false;
                    self.add_log(cx, "[ERROR] [tts] Dora integration not initialized");
                }
            }
            DoraStartupEvent::Failed(message) => {
                self.dora_start_rx = None;
                self.dora_pending_dataflow_path = None;
                self.dora_start_in_flight = false;
                self.dora_started = false;
                self.add_log(cx, &format!("[ERROR] [tts] {}", message));
                self.show_toast(
                    cx,
                    self.tr(
                        "Dora 启动失败，请查看日志",
                        "Dora startup failed. Please check logs",
                    ),
                );
            }
        }
    }

    fn maybe_retry_dataflow_start(&mut self, cx: &mut Cx) {
        if !self.dora_started {
            return;
        }
        if self.dora_start_in_flight {
            return;
        }
        if matches!(
            self.runtime_init_state,
            RuntimeInitState::Running | RuntimeInitState::Failed
        ) {
            return;
        }

        let is_running = self.dora.as_ref().map(|d| d.is_running()).unwrap_or(false);
        if is_running {
            return;
        }

        let should_retry = self
            .dora_start_attempt_at
            .map(|t| t.elapsed() >= Duration::from_secs(4))
            .unwrap_or(true);
        if should_retry {
            self.add_log(
                cx,
                "[WARN] [tts] Dora dataflow not running yet, retrying startup...",
            );
            self.dora_started = false;
        }
    }

    fn poll_dora_events(&mut self, cx: &mut Cx) {
        let events = self
            .dora
            .as_ref()
            .map(|d| d.poll_events())
            .unwrap_or_default();
        for event in events {
            match event {
                crate::dora_integration::DoraEvent::DataflowStarted { dataflow_id } => {
                    self.dora_start_in_flight = false;
                    self.add_log(cx, &format!("[INFO] [tts] Dora dataflow started: {}", dataflow_id));
                }
                crate::dora_integration::DoraEvent::DataflowStopped => {
                    self.dora_start_in_flight = false;
                    self.add_log(cx, "[WARN] [tts] Dora dataflow stopped");
                    self.dora_started = false;
                }
                crate::dora_integration::DoraEvent::Error { message } => {
                    self.dora_start_in_flight = false;
                    self.add_log(cx, &format!("[ERROR] [tts] Dora error: {}", message));
                    self.dora_started = false;
                    self.show_toast(
                        cx,
                        self.tr(
                            "Dora 启动失败，正在重试，请查看日志",
                            "Dora start failed; retrying. Please check logs",
                        ),
                    );
                }
                crate::dora_integration::DoraEvent::AsrTranscription { .. } => {}
            }
        }
    }

    fn poll_translation_dora_events(&mut self, cx: &mut Cx) {
        let events = self
            .translation_dora
            .as_ref()
            .map(|d| d.poll_events())
            .unwrap_or_default();

        for event in events {
            match event {
                crate::dora_integration::DoraEvent::DataflowStarted { dataflow_id } => {
                    self.add_translation_log(
                        cx,
                        &format!(
                            "[INFO] {}: {}",
                            self.tr("翻译数据流已启动", "Translation dataflow started"),
                            dataflow_id
                        ),
                    );
                }
                crate::dora_integration::DoraEvent::DataflowStopped => {
                    self.add_translation_log(
                        cx,
                        &format!(
                            "[WARN] {}",
                            self.tr("翻译数据流已停止", "Translation dataflow stopped")
                        ),
                    );
                    self.translation_running = false;
                    self.show_translation_running_panel(cx, false);
                    cx.stop_timer(self.translation_metrics_timer);
                }
                crate::dora_integration::DoraEvent::Error { message } => {
                    self.add_translation_log(
                        cx,
                        &format!(
                            "[ERROR] {}: {}",
                            self.tr("翻译数据流错误", "Translation dataflow error"),
                            message
                        ),
                    );
                    self.translation_running = false;
                    self.show_translation_running_panel(cx, false);
                    cx.stop_timer(self.translation_metrics_timer);
                    if let Some(shared) = self.translation_shared_state() {
                        shared.translation_window_visible.set(false);
                    }
                }
                crate::dora_integration::DoraEvent::AsrTranscription { .. } => {}
            }
        }
    }

    fn is_qwen_backend(backend: &str) -> bool {
        backend == "qwen3_tts_mlx"
    }

    fn normalize_inference_backend(_raw: &str) -> &'static str {
        // Qwen3-only: always qwen3. See doc/REFACTOR_QWEN3_ONLY.md.
        "qwen3_tts_mlx"
    }

    fn normalize_zero_shot_backend(_raw: &str) -> &'static str {
        // Qwen3-only: always qwen3. See doc/REFACTOR_QWEN3_ONLY.md.
        "qwen3_tts_mlx"
    }

    fn absolutize_dataflow_paths(template_path: &Path, content: &str) -> String {
        let base_dir = template_path
            .parent()
            .unwrap_or_else(|| Path::new("."));
        let mut output = String::with_capacity(content.len() + 256);
        let has_trailing_newline = content.ends_with('\n');

        for line in content.lines() {
            let trimmed = line.trim_start();
            let indent = &line[..line.len().saturating_sub(trimmed.len())];

            if let Some(raw_value) = trimmed.strip_prefix("path:") {
                let raw_value = raw_value.trim();

                let (path_value, quoted) = if raw_value.len() >= 2
                    && ((raw_value.starts_with('"') && raw_value.ends_with('"'))
                        || (raw_value.starts_with('\'') && raw_value.ends_with('\'')))
                {
                    (&raw_value[1..raw_value.len() - 1], true)
                } else {
                    (raw_value, false)
                };

                let should_resolve = path_value != "dynamic"
                    && !path_value.starts_with('/')
                    && path_value.contains('/');

                if should_resolve {
                    let candidate = base_dir.join(path_value);
                    if candidate.exists() {
                        let resolved = candidate.canonicalize().unwrap_or(candidate);
                        if quoted {
                            output.push_str(&format!("{indent}path: \"{}\"\n", resolved.display()));
                        } else {
                            output.push_str(&format!("{indent}path: {}\n", resolved.display()));
                        }
                        continue;
                    }
                }
            }

            output.push_str(line);
            output.push('\n');
        }

        if !has_trailing_newline && output.ends_with('\n') {
            output.pop();
        }
        output
    }

    fn resolve_dataflow_template_path(&self) -> PathBuf {
        let env_path = std::env::var("MOXIN_DATAFLOW_PATH")
            .ok()
            .map(PathBuf::from)
            .filter(|p| p.exists());
        if let Some(path) = env_path {
            return path;
        }

        let app_resources = std::env::var("MOXIN_APP_RESOURCES")
            .ok()
            .map(PathBuf::from)
            .filter(|p| p.exists());
        if let Some(resources) = app_resources {
            let bundle_dataflow = resources.join("dataflow").join("tts.yml");
            if bundle_dataflow.exists() {
                return bundle_dataflow;
            }
        }

        PathBuf::from("apps/moxin-voice/dataflow/tts.yml")
    }

    fn materialize_runtime_dataflow(&mut self, cx: &mut Cx) -> Result<PathBuf, String> {
        let template_path = self.resolve_dataflow_template_path();
        let template = fs::read_to_string(&template_path).map_err(|e| {
            format!(
                "read template failed ({}): {}",
                template_path.display(),
                e
            )
        })?;

        let inference_backend =
            Self::normalize_inference_backend(&self.app_preferences.inference_backend).to_string();
        let zero_shot_backend =
            Self::normalize_zero_shot_backend(&self.app_preferences.zero_shot_backend).to_string();
        std::env::set_var("MOXIN_INFERENCE_BACKEND", &inference_backend);
        std::env::set_var("MOXIN_ZERO_SHOT_BACKEND", &zero_shot_backend);
        let asr_bin = Self::resolve_dora_binary("dora-qwen3-asr")
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                self.add_log(
                    cx,
                    "[WARN] [tts] dora-qwen3-asr binary not found — run: cargo build --release -p dora-qwen3-asr",
                );
                String::new()
            });
        let qwen_tts_bin = Self::resolve_dora_binary("qwen-tts-node")
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                self.add_log(
                    cx,
                    "[WARN] [tts] qwen-tts-node binary not found — run: cargo build --release -p dora-qwen3-tts-mlx",
                );
                String::new()
            });

        let rendered = template
            .replace("__MOXIN_INFERENCE_BACKEND__", &inference_backend)
            .replace("__MOXIN_ZERO_SHOT_BACKEND__", &zero_shot_backend)
            .replace("__ASR_BIN_PATH__", &asr_bin)
            .replace("__QWEN_TTS_BIN_PATH__", &qwen_tts_bin);
        let rendered = Self::absolutize_dataflow_paths(&template_path, &rendered);

        let runtime_dir = dirs::home_dir()
            .unwrap_or_else(|| PathBuf::from("."))
            .join(".dora")
            .join("runtime")
            .join("dataflow");
        fs::create_dir_all(&runtime_dir)
            .map_err(|e| format!("create runtime dataflow dir failed: {}", e))?;
        let runtime_path = runtime_dir.join("tts.runtime.yml");
        fs::write(&runtime_path, rendered)
            .map_err(|e| format!("write runtime dataflow failed: {}", e))?;

        self.add_log(
            cx,
            &format!(
                "[INFO] [tts] Runtime dataflow prepared: {} (inference={}, zero_shot={})",
                runtime_path.display(),
                inference_backend,
                zero_shot_backend
            ),
        );
        Ok(runtime_path)
    }

    fn resolve_translation_dataflow_template_path(&self) -> Option<PathBuf> {
        let env_path = std::env::var("MOXIN_TRANSLATION_DATAFLOW_PATH")
            .ok()
            .map(PathBuf::from)
            .filter(|p| p.exists());
        if env_path.is_some() {
            return env_path;
        }

        let app_resources = std::env::var("MOXIN_APP_RESOURCES")
            .ok()
            .map(PathBuf::from)
            .filter(|p| p.exists());
        if let Some(resources) = app_resources {
            let bundle_dataflow = resources.join("dataflow").join("translation_qwen35.yml");
            if bundle_dataflow.exists() {
                return Some(bundle_dataflow);
            }
        }

        [
            PathBuf::from("apps/moxin-voice/dataflow/translation_qwen35.yml"),
            dirs::home_dir()
                .unwrap_or_default()
                .join(".OminiX/dataflows/translation_qwen35.yml"),
        ]
        .into_iter()
        .find(|p| p.exists())
    }

    fn stop_dora(&mut self, cx: &mut Cx) {
        if self.dora.is_none() {
            return;
        }

        self.add_log(cx, "[INFO] [tts] Stopping TTS dataflow...");

        if let Some(dora) = &mut self.dora {
            dora.stop_dataflow();
        }
        self.dora_started = false;
        self.dora_start_in_flight = false;
        self.dora_start_attempt_at = None;
        self.dora_start_rx = None;
        self.dora_pending_dataflow_path = None;

        self.add_log(cx, "[INFO] [tts] Dataflow stopped");
    }

    // ── Translation helpers ───────────────────────────────────────────────────

    /// Toggle the translation overlay on/off.
    ///
    /// When activating:
    ///   1. Materialises `translation_qwen35.yml` with src/tgt lang env placeholders replaced
    ///   2. Starts a separate Dora dataflow for the translation pipeline
    ///   3. Sets `SharedDoraState.translation_window_visible = true` so `app.rs` shows the window
    ///
    /// When deactivating: stops the translation dataflow and hides the window.
    /// Start the translation dataflow and switch the translation page to running view.
    fn start_translation_dataflow(&mut self, cx: &mut Cx) {
        Self::ensure_bundle_bin_on_path();

        // Pause TTS dataflow to free the ASR process before starting translation.
        if self.dora_started {
            self.tts_paused_for_translation = true;
            self.stop_dora(cx);
            self.add_translation_log(cx, &format!("[INFO] {}", self.tr("已暂停 TTS 数据流", "TTS dataflow paused")));
        }

        // Reset in-page translation log view each run to avoid stale scroll/content
        // bleeding through the semi-transparent overlay.
        self.translation_log_lines.clear();
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_running_panel.translation_log_card.translation_log_scroll.translation_log_label))
            .set_text(cx, "");
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_running_panel.translation_log_card.translation_log_scroll))
            .set_scroll_pos(cx, dvec2(0.0, 0.0));

        self.add_translation_log(
            cx,
            &format!(
                "[INFO] {}...",
                self.tr("正在启动翻译数据流", "Starting translation dataflow")
            ),
        );

        let template_path = match self.resolve_translation_dataflow_template_path() {
            Some(p) => p,
            None => {
                self.add_translation_log(
                    cx,
                    &format!(
                        "[ERROR] {}",
                        self.tr(
                            "未找到 translation_qwen35.yml",
                            "translation_qwen35.yml not found"
                        )
                    ),
                );
                return;
            }
        };

        // Read template and substitute language placeholders
        let template_content = match std::fs::read_to_string(&template_path) {
            Ok(s) => s,
            Err(e) => {
                self.add_translation_log(
                    cx,
                    &format!("[ERROR] {}: {}", self.tr("读取模板失败", "Failed to read template"), e),
                );
                return;
            }
        };

        let src_upper = self.translation_src_lang.to_uppercase();
        let tgt_upper = self.translation_tgt_lang.to_uppercase();

        // Endpoint detection policy (language-aware):
        // - start/end frame hysteresis
        // - minimum segment duration gate
        // - start/end RMS hysteresis
        let (
            speech_start_frames,
            speech_end_frames,
            speech_end_ms,
            question_end_silence_ms,
            min_segment_ms,
            start_rms_threshold,
            end_rms_threshold,
        ) = match self.translation_src_lang.as_str() {
            // Chinese: lower endpoint/min-segment latency while preserving stability.
            "zh" => (5_i32, 10_i32, 420_i32, 1200_i32, 420_i32, 0.018_f32, 0.010_f32),
            "en" => (4_i32, 10_i32, 300_i32, 900_i32, 300_i32, 0.015_f32, 0.009_f32),
            "fr" => (4_i32, 10_i32, 320_i32, 1000_i32, 320_i32, 0.016_f32, 0.009_f32),
            _ => (4_i32, 10_i32, 320_i32, 1000_i32, 320_i32, 0.016_f32, 0.009_f32),
        };
        let max_segment_ms = 8000_i32;

        // System audio (ScreenCaptureKit) has a much cleaner signal than a microphone:
        // no breath noise, no room echo, no handling noise.  Use lower RMS thresholds
        // so quiet speech / video audio isn't gated out by the VAD.
        let is_system_audio = self.translation_device_idx == 0;
        let (start_rms_threshold, end_rms_threshold) = if is_system_audio {
            (start_rms_threshold * 0.3, end_rms_threshold * 0.3)
        } else {
            (start_rms_threshold, end_rms_threshold)
        };

        // Resolve absolute binary paths for dev (target/release or target/debug)
        // and DMG distribution (binary sibling to current_exe).
        let asr_path = Self::resolve_dora_binary("dora-qwen3-asr")
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                self.add_translation_log(cx, "[WARN] dora-qwen3-asr binary not found — run: cargo build --release -p dora-qwen3-asr");
                String::new()
            });
        let translator_path = Self::resolve_dora_binary("dora-qwen35-translator")
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| {
                self.add_translation_log(cx, "[WARN] dora-qwen35-translator binary not found — run: cargo build --release -p dora-qwen35-translator");
                String::new()
            });

        let rendered = template_content
            .replace("__TRANSLATION_SRC_LANG__", &self.translation_src_lang)
            .replace("__TRANSLATION_TGT_LANG__", &self.translation_tgt_lang)
            .replace("__ASR_BIN_PATH__", &asr_path)
            .replace("__TRANSLATOR_BIN_PATH__", &translator_path)
            .replace("__SPEECH_START_FRAMES__", &speech_start_frames.to_string())
            .replace("__SPEECH_END_FRAMES__", &speech_end_frames.to_string())
            .replace("__SPEECH_END_MS__", &speech_end_ms.to_string())
            .replace(
                "__QUESTION_END_SILENCE_MS__",
                &question_end_silence_ms.to_string(),
            )
            .replace("__MIN_SEGMENT_MS__", &min_segment_ms.to_string())
            .replace("__MAX_SEGMENT_MS__", &max_segment_ms.to_string())
            .replace(
                "__START_RMS_THRESHOLD__",
                &format!("{:.4}", start_rms_threshold),
            )
            .replace("__END_RMS_THRESHOLD__", &format!("{:.4}", end_rms_threshold));
        let rendered = Self::absolutize_dataflow_paths(&template_path, &rendered);

        // Write rendered dataflow to a temp file
        let tmp_path = std::env::temp_dir().join("moxin_translation_dataflow.yml");
        if let Err(e) = std::fs::write(&tmp_path, &rendered) {
            self.add_translation_log(
                cx,
                &format!("[ERROR] {}: {}", self.tr("写入临时文件失败", "Failed to write temp file"), e),
            );
            return;
        }

        if self.translation_dora.is_none() {
            self.translation_dora = Some(DoraIntegration::new());
        }

        let started = self
            .translation_dora
            .as_ref()
            .map(|d| d.start_dataflow(tmp_path))
            .unwrap_or(false);
        if !started {
            self.add_translation_log(
                cx,
                &format!(
                    "[ERROR] {}",
                    self.tr(
                        "启动失败：未能提交翻译数据流启动命令",
                        "Failed to start: could not submit translation dataflow command"
                    )
                ),
            );
            return;
        }
        self.add_translation_log(
            cx,
            &format!(
                "[INFO] {} ({} → {})",
                self.tr("数据流启动命令已提交", "Dataflow start command submitted"),
                src_upper,
                tgt_upper
            ),
        );
        self.add_translation_log(
            cx,
            &format!(
                "[INFO] {}: start={}f, end={}ms ({}f), min_segment={}ms, question_end={}ms, max_segment={}ms, rms(start/end)={:.4}/{:.4}",
                self.tr("VAD 策略", "VAD policy"),
                speech_start_frames,
                speech_end_ms,
                speech_end_frames,
                min_segment_ms,
                question_end_silence_ms,
                max_segment_ms,
                start_rms_threshold,
                end_rms_threshold
            ),
        );

        // Show the translation overlay window via SharedDoraState
        if let Some(shared) = self.translation_shared_state() {
            shared.translation_locale_en.set(self.is_english());
            shared.translation_lang_pair.set((
                self.translation_src_lang.clone(),
                self.translation_tgt_lang.clone(),
            ));
            shared
                .translation_font_size_preset
                .set(self.translation_overlay_font_size_preset.clone());
            shared
                .translation_anchor_position_preset
                .set(self.translation_overlay_anchor_position_preset.clone());
            // Force a visibility dirty edge even if state was previously true
            // (e.g. user manually closed the OS window while state remained true).
            shared.translation_window_visible.set(false);
            shared.translation_window_visible.set(true);
            shared
                .translation_overlay_fullscreen
                .set(self.translation_overlay_fullscreen);
            shared
                .translation_overlay_opacity
                .set(self.translation_overlay_opacity);
        }
        // Sync audio source from current dropdown selection
        self.send_audio_source_to_bridge(self.translation_device_idx == 0);

        // Switch to running view
        self.translation_running = true;
        self.show_translation_running_panel(cx, true);

        // Start metrics polling timer (1s interval)
        self.translation_metrics_timer = cx.start_interval(1.0);
    }

    /// Stop the translation dataflow and return to settings view.
    fn stop_translation_dataflow(&mut self, cx: &mut Cx) {
        self.add_translation_log(
            cx,
            &format!("[INFO] {}...", self.tr("正在停止翻译", "Stopping translation")),
        );

        // Hide the overlay window
        if let Some(shared) = self.translation_shared_state() {
            shared.translation_window_visible.set(false);
            shared.translation.set(None);
        }

        if let Some(dora) = &self.translation_dora {
            let _ = dora.stop_dataflow();
        }

        self.translation_running = false;
        self.show_translation_running_panel(cx, false);
        self.translation_last_logged_fingerprint = None;

        // Stop metrics timer
        cx.stop_timer(self.translation_metrics_timer);

        // Resume TTS dataflow if it was paused for translation.
        if self.tts_paused_for_translation {
            self.tts_paused_for_translation = false;
            // dora_started is already false after stop_dora; clearing the pause flag
            // lets the event-loop auto-start block restart TTS on the next tick.
            self.add_log(cx, "[INFO] [tts] Resuming TTS dataflow after translation stop...");
        }
    }

    /// Poll translation updates and mirror completed entries to the log.
    fn poll_translation_metrics(&mut self, cx: &mut Cx) {
        // Read translation status from shared state
        if let Some(shared) = self.translation_shared_state() {
            let translation_snapshot = shared.translation.read();
            if let Some(update) = translation_snapshot {
                // Mirror newly arrived translation to the log WITHOUT consuming dirty flag.
                let fingerprint = format!(
                    "h{}|p={}",
                    update.history.len(), update.pending_source_text.len()
                );
                let should_log = self
                    .translation_last_logged_fingerprint
                    .as_ref()
                    .map(|last| last != &fingerprint)
                    .unwrap_or(true);

                if should_log {
                    if let Some(last) = update.history.last() {
                        let msg = format!(
                            "[{}] {} → {}",
                            self.tr("翻译完成", "Translated"),
                            last.source_text,
                            last.translation
                        );
                        self.add_translation_log(cx, &msg);
                    }
                    self.translation_last_logged_fingerprint = Some(fingerprint);
                }
            }
        }
    }

    /// Toggle which panel is visible in the translation page body.
    fn show_translation_running_panel(&mut self, cx: &mut Cx, running: bool) {
        // Status badge in page header
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.translation_page.page_header.translation_status_badge))
            .set_visible(cx, running);

        // Settings panel ↔ running panel
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel))
            .set_visible(cx, !running);
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_running_panel))
            .set_visible(cx, running);

        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.translation_page))
            .redraw(cx);
    }

    /// Append a line to the translation page log and refresh the label.
    fn add_translation_log(&mut self, cx: &mut Cx, line: &str) {
        let ts = {
            let now = std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap_or_default()
                .as_secs();
            let hh = (now / 3600) % 24;
            let mm = (now / 60) % 60;
            let ss = now % 60;
            format!("{:02}:{:02}:{:02}", hh, mm, ss)
        };
        self.translation_log_lines.push(format!("[{}] {}", ts, line));
        // Keep at most 200 lines
        if self.translation_log_lines.len() > 200 {
            self.translation_log_lines.drain(..self.translation_log_lines.len() - 200);
        }
        let text = self.translation_log_lines.join("\n");
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_running_panel.translation_log_card.translation_log_scroll.translation_log_label))
            .set_text(cx, &text);
    }

    /// Update the language dropdown selections after a programmatic change.
    fn update_translation_lang_dropdowns(&mut self, cx: &mut Cx) {
        let src_codes = ["zh", "en", "ja", "fr"];
        let tgt_codes = ["en", "zh", "ja", "fr"];

        let src_idx = src_codes.iter().position(|c| *c == self.translation_src_lang).unwrap_or(0);
        let tgt_idx = tgt_codes.iter().position(|c| *c == self.translation_tgt_lang).unwrap_or(0);

        let (src_labels, tgt_labels) = if self.is_english() {
            (
                vec![
                    "Chinese".to_string(),
                    "English".to_string(),
                    "Japanese".to_string(),
                    "French".to_string(),
                ],
                vec![
                    "English".to_string(),
                    "Chinese".to_string(),
                    "Japanese".to_string(),
                    "French".to_string(),
                ],
            )
        } else {
            (
                vec![
                    "中文".to_string(),
                    "英语".to_string(),
                    "日语".to_string(),
                    "法语".to_string(),
                ],
                vec![
                    "英语".to_string(),
                    "中文".to_string(),
                    "日语".to_string(),
                    "法语".to_string(),
                ],
            )
        };

        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_src_lang.src_lang_dropdown))
            .set_labels(cx, src_labels);
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_tgt_lang.tgt_lang_dropdown))
            .set_labels(cx, tgt_labels);
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_src_lang.src_lang_dropdown))
            .set_selected_item(cx, src_idx);
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_tgt_lang.tgt_lang_dropdown))
            .set_selected_item(cx, tgt_idx);
    }

    /// Update the active state of the 紧凑/全屏 overlay style buttons.
    fn update_translation_overlay_style_buttons(&mut self, cx: &mut Cx) {
        let full = if self.translation_overlay_fullscreen { 1.0_f64 } else { 0.0 };
        let compact = 1.0 - full;
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_overlay.overlay_style_compact))
            .apply_over(cx, live! { draw_bg: { active: (compact) } draw_text: { active: (compact) } });
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_overlay.overlay_style_full))
            .apply_over(cx, live! { draw_bg: { active: (full) } draw_text: { active: (full) } });
    }

    fn send_audio_source_to_bridge(&self, use_system_audio: bool) {
        use moxin_dora_bridge::widgets::AudioSource;
        let source = if use_system_audio { AudioSource::SystemAudio } else { AudioSource::Microphone };
        // Write to shared state — the AEC bridge polls this and switches source live.
        if let Some(shared) = self.translation_shared_state() {
            shared.translation_audio_source.set(source);
        }
    }

    /// Keep translation settings labels in one line across locales.
    /// English needs a wider left label column, so we shrink right controls accordingly.
    fn update_translation_settings_layout_for_locale(&mut self, cx: &mut Cx) {
        if self.is_english() {
            self.view
                .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_source.translation_source_label))
                .apply_over(cx, live! { width: 160.0 });
            self.view
                .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_src_lang.translation_src_lang_label))
                .apply_over(cx, live! { width: 160.0 });
            self.view
                .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_tgt_lang.translation_tgt_lang_label))
                .apply_over(cx, live! { width: 160.0 });
            self.view
                .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_overlay.translation_overlay_style_label))
                .apply_over(cx, live! { width: 160.0 });
            self.view
                .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_font_size.translation_font_size_label))
                .apply_over(cx, live! { width: 160.0 });
            self.view
                .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_anchor_position.translation_anchor_position_label))
                .apply_over(cx, live! { width: 160.0 });
            self.view
                .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_opacity.translation_opacity_label))
                .apply_over(cx, live! { width: 160.0 });
            return;
        }

        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_source.translation_source_label))
            .apply_over(cx, live! { width: 90.0 });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_src_lang.translation_src_lang_label))
            .apply_over(cx, live! { width: 90.0 });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_tgt_lang.translation_tgt_lang_label))
            .apply_over(cx, live! { width: 90.0 });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_overlay.translation_overlay_style_label))
            .apply_over(cx, live! { width: 90.0 });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_font_size.translation_font_size_label))
            .apply_over(cx, live! { width: 90.0 });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_anchor_position.translation_anchor_position_label))
            .apply_over(cx, live! { width: 90.0 });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_opacity.translation_opacity_label))
            .apply_over(cx, live! { width: 90.0 });
    }

    fn update_audio_player_action_layout_for_locale(&mut self, cx: &mut Cx) {
        if self.is_english() {
            self.view
                .view(ids!(content_wrapper.audio_player_bar.download_section))
                .apply_over(cx, live! { width: 250.0 });
            self.view
                .button(ids!(content_wrapper.audio_player_bar.download_section.download_btn))
                .apply_over(cx, live! { padding: { left: 20.0, right: 20.0 } });
            self.view
                .button(ids!(content_wrapper.audio_player_bar.download_section.share_btn))
                .apply_over(cx, live! { padding: { left: 20.0, right: 20.0 } });
            return;
        }
        self.view
            .view(ids!(content_wrapper.audio_player_bar.download_section))
            .apply_over(cx, live! { width: 220.0 });
        self.view
            .button(ids!(content_wrapper.audio_player_bar.download_section.download_btn))
            .apply_over(cx, live! { padding: { left: 24.0, right: 24.0 } });
        self.view
            .button(ids!(content_wrapper.audio_player_bar.download_section.share_btn))
            .apply_over(cx, live! { padding: { left: 24.0, right: 24.0 } });
    }

    /// Sync the opacity dropdown selection with the current opacity value.
    fn update_translation_opacity_dropdown(&mut self, cx: &mut Cx) {
        let opacity_values: [f64; 7] = [1.0, 0.9, 0.85, 0.75, 0.65, 0.5, 0.35];
        let idx = opacity_values
            .iter()
            .position(|v| (*v - self.translation_overlay_opacity).abs() < 0.01)
            .unwrap_or(0); // default to 100%
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_opacity.opacity_dropdown))
            .set_selected_item(cx, idx);
    }

    fn update_translation_font_size_dropdown(&mut self, cx: &mut Cx) {
        let labels = if self.is_english() {
            vec![
                "Small".to_string(),
                "Normal".to_string(),
                "Large".to_string(),
            ]
        } else {
            vec!["小".to_string(), "正常".to_string(), "大".to_string()]
        };
        let idx = match self.translation_overlay_font_size_preset.as_str() {
            "small" => 0,
            "large" => 2,
            _ => 1,
        };
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_font_size.font_size_dropdown))
            .set_labels(cx, labels);
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_font_size.font_size_dropdown))
            .set_selected_item(cx, idx);
    }

    fn update_translation_anchor_position_dropdown(&mut self, cx: &mut Cx) {
        let labels = vec![
            "50%".to_string(),
            "60%".to_string(),
            "70%".to_string(),
            "80%".to_string(),
            "90%".to_string(),
            "100%".to_string(),
        ];
        let idx = match self.translation_overlay_anchor_position_preset.as_str() {
            "50" => 0,
            "60" => 1,
            "70" => 2,
            "80" => 3,
            "90" => 4,
            "100" => 5,
            _ => 0,
        };
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_anchor_position.anchor_position_dropdown))
            .set_labels(cx, labels);
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_anchor_position.anchor_position_dropdown))
            .set_selected_item(cx, idx);
    }

    /// Show or hide the screen-recording permission restart hint based on probe result.
    #[cfg(target_os = "macos")]
    fn update_translation_permission_hint(&mut self, cx: &mut Cx) {
        let denied = moxin_dora_bridge::widgets::permission_granted() == Some(false);
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.translation_permission_hint))
            .set_visible(cx, denied);
    }

    /// Populate the translation input device dropdown with available CPAL devices.
    fn populate_translation_input_dropdown(&mut self, cx: &mut Cx) {
        use cpal::traits::{DeviceTrait, HostTrait};

        let host = cpal::default_host();
        let mut names: Vec<String> = Vec::new();
        if let Ok(devs) = host.input_devices() {
            for d in devs {
                if let Ok(name) = d.name() {
                    if !names.contains(&name) {
                        names.push(name);
                    }
                }
            }
        }
        self.translation_audio_devices = names;

        // Index 0 = System Audio, 1 = System Default Mic, 2..N = CPAL devices
        let mut labels = vec![
            self.tr("系统音频", "System Audio").to_string(),
            self.tr("系统默认麦克风", "System Default Microphone").to_string(),
        ];
        labels.extend(self.translation_audio_devices.clone());

        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_source.translation_source_dropdown))
            .set_labels(cx, labels);
        let total = self.translation_audio_devices.len() + 2;
        let selected_idx = self.translation_device_idx.min(total.saturating_sub(1));
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_source.translation_source_dropdown))
            .set_selected_item(cx, selected_idx);
    }

    fn translation_shared_state(&self) -> Option<Arc<moxin_dora_bridge::SharedDoraState>> {
        self.translation_dora
            .as_ref()
            .map(|dora| dora.shared_dora_state().clone())
    }

    fn sync_translation_overlay_locale(&self) {
        if let Some(shared) = self.translation_shared_state() {
            shared.translation_locale_en.set(self.is_english());
        }
    }

    fn sync_translation_overlay_lang_pair(&self) {
        if let Some(shared) = self.translation_shared_state() {
            shared.translation_lang_pair.set((
                self.translation_src_lang.clone(),
                self.translation_tgt_lang.clone(),
            ));
        }
    }

    fn sync_translation_overlay_font_size(&self) {
        if let Some(shared) = self.translation_shared_state() {
            shared
                .translation_font_size_preset
                .set(self.translation_overlay_font_size_preset.clone());
        }
    }

    fn sync_translation_overlay_anchor_position(&self) {
        if let Some(shared) = self.translation_shared_state() {
            shared
                .translation_anchor_position_preset
                .set(self.translation_overlay_anchor_position_preset.clone());
        }
    }

    /// Resolve the absolute path of a Dora node binary.
    /// Search order (ensures correct path in both dev and DMG app bundle):
    ///   1. Same directory as the current executable (app bundle / installed)
    ///   2. target/release/ relative to CWD  (dev, release build)
    ///   3. target/debug/  relative to CWD  (dev, debug build)
    fn resolve_dora_binary(name: &str) -> Option<std::path::PathBuf> {
        // 1. App bundle: binary lives beside the main executable
        if let Ok(exe) = std::env::current_exe() {
            if let Some(dir) = exe.parent() {
                let candidate = dir.join(name);
                if candidate.exists() {
                    return Some(candidate);
                }
            }
        }
        // 2. dev release build
        let release = std::path::PathBuf::from("target/release").join(name);
        if release.exists() {
            return release.canonicalize().ok().or(Some(release));
        }
        // 3. dev debug build
        let debug = std::path::PathBuf::from("target/debug").join(name);
        if debug.exists() {
            return debug.canonicalize().ok().or(Some(debug));
        }
        None
    }

    fn generate_speech(&mut self, cx: &mut Cx) {
        // Qwen backend currently does not support VOICE:TRAINED prompt format.
        let selected_voice_is_trained = self
            .selected_voice_id
            .as_ref()
            .and_then(|id| self.library_voices.iter().find(|v| &v.id == id))
            .map(|voice| voice.source == crate::voice_data::VoiceSource::Trained)
            .unwrap_or(false);
        if Self::is_qwen_backend(&self.app_preferences.inference_backend) && selected_voice_is_trained {
            self.add_log(
                cx,
                "[WARN] [tts] Qwen backend does not support trained voices (VOICE:TRAINED)",
            );
            self.show_toast(
                cx,
                self.tr(
                    "Qwen 推理后端暂不支持训练音色，请切换推理后端或选择其他音色",
                    "Qwen inference backend does not support trained voices. Switch backend or voice",
                ),
            );
            self.set_generate_button_loading(cx, false);
            return;
        }

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

        if text.chars().count() > TTS_INPUT_MAX_CHARS {
            self.show_toast(
                cx,
                self.tr(
                    "文本超过 1,000 字符限制，请缩短后重试",
                    "Text exceeds 1,000 character limit, please shorten and try again",
                ),
            );
            self.set_generate_button_loading(cx, false);
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
            } else if voice.source == crate::voice_data::VoiceSource::BundledIcl {
                // Bundled ref voice - force x-vector clone path by sending empty prompt_text.
                if let Some(ref_filename) = &voice.reference_audio_path {
                    let ref_audio_path = self
                        .resolve_bundled_icl_ref_path(&voice.id, ref_filename)
                        .map(|p| p.to_string_lossy().to_string())
                        .unwrap_or_default();

                    if ref_audio_path.is_empty() {
                        self.add_log(cx, &format!(
                            "[WARN] [tts] BundledIcl voice '{}' ref audio not found, using default",
                            voice.id
                        ));
                        format!("VOICE:vivian|{}", text)
                    } else {
                        self.add_log(cx, &format!(
                            "[INFO] [tts] BundledIcl voice '{}' ref audio (x-vector mode): {}",
                            voice.id, ref_audio_path
                        ));
                        format!(
                            "VOICE:CUSTOM|{}|{}|{}|{}",
                            ref_audio_path, "", voice.language, text
                        )
                    }
                } else {
                    format!("VOICE:vivian|{}", text)
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
            let effective_rate = self.effective_audio_sample_rate();
            let total_duration = playback_samples.len() as f64 / effective_rate as f64;
            let is_resuming = self.audio_playing_time > 0.1
                && self.audio_playing_time < (total_duration - 0.1);

            if let Some(player) = &self.audio_player {
                // Always stop and clear buffer first to avoid audio overlap
                player.stop();

                if is_resuming {
                    // Resume from paused position - write remaining audio from current position
                    let current_sample_index = (self.audio_playing_time * effective_rate as f64) as usize;
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

    fn open_download_modal(&mut self, cx: &mut Cx, source: DownloadSource) {
        match &source {
            DownloadSource::CurrentAudio => {
                if self.tts_status == TTSStatus::Generating {
                    self.add_log(cx, "[INFO] [download] Download is disabled while generating new audio");
                    self.show_toast(
                        cx,
                        self.tr("生成中，暂不可下载", "Download is disabled while generating"),
                    );
                    return;
                }
                if self.effective_audio_samples().is_empty() {
                    self.add_log(cx, "[WARN] [download] No audio available to download");
                    self.show_toast(cx, self.tr("暂无可下载音频", "No audio available to download"));
                    return;
                }
            }
            DownloadSource::History(entry_id) => {
                let Some(entry) = self.tts_history.iter().find(|h| h.id == *entry_id) else {
                    self.add_log(
                        cx,
                        &format!("[WARN] [download] History entry not found: {}", entry_id),
                    );
                    return;
                };
                let src = tts_history::history_audio_path(&entry.audio_file);
                if !src.exists() {
                    self.show_toast(
                        cx,
                        self.tr("历史音频文件不存在", "History audio file not found"),
                    );
                    return;
                }
            }
        }

        self.pending_download_source = Some(source);
        self.download_modal_visible = true;
        self.view.view(ids!(download_modal)).set_visible(cx, true);
        self.view.redraw(cx);
    }

    fn close_download_modal(&mut self, cx: &mut Cx) {
        self.download_modal_visible = false;
        self.pending_download_source = None;
        self.view.view(ids!(download_modal)).set_visible(cx, false);
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

    fn download_audio_to_format(&mut self, cx: &mut Cx, format: DownloadFormat) {
        let source = match self.pending_download_source.clone() {
            Some(source) => source,
            None => {
                self.add_log(cx, "[WARN] [download] No pending download source");
                return;
            }
        };

        let saved_path = match source {
            DownloadSource::CurrentAudio => self.export_current_audio(cx, format),
            DownloadSource::History(entry_id) => self.export_history_audio(cx, &entry_id, format),
        };

        let Some(saved_path) = saved_path else {
            return;
        };

        self.add_log(
            cx,
            &format!(
                "[INFO] [download] Saved {} audio to {}",
                Self::download_format_key(format),
                saved_path.display()
            ),
        );
        self.show_toast(
            cx,
            &format!(
                "{} {}",
                self.tr("下载成功：", "Downloaded:"),
                saved_path
                    .file_name()
                    .and_then(|name| name.to_str())
                    .unwrap_or_else(|| Self::download_format_file_label(format))
            ),
        );
        self.close_download_modal(cx);
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

    fn download_format_extension(format: DownloadFormat) -> &'static str {
        match format {
            DownloadFormat::Mp3 => "mp3",
            DownloadFormat::Wav => "wav",
        }
    }

    fn download_format_key(format: DownloadFormat) -> &'static str {
        match format {
            DownloadFormat::Mp3 => "mp3",
            DownloadFormat::Wav => "wav",
        }
    }

    fn download_format_file_label(format: DownloadFormat) -> &'static str {
        match format {
            DownloadFormat::Mp3 => "MP3",
            DownloadFormat::Wav => "WAV",
        }
    }

    fn export_current_audio(&mut self, cx: &mut Cx, format: DownloadFormat) -> Option<PathBuf> {
        let samples = self.effective_audio_samples().to_vec();
        let effective_rate = self.effective_audio_sample_rate();
        if samples.is_empty() {
            self.add_log(cx, "[WARN] [download] No current audio available");
            self.show_toast(cx, self.tr("暂无可下载音频", "No audio available to download"));
            return None;
        }
        if effective_rate == 0 {
            self.add_log(cx, "[ERROR] [download] Current audio sample rate is invalid");
            self.show_toast(
                cx,
                self.tr("音频采样率无效，无法下载", "Invalid sample rate, download failed"),
            );
            return None;
        }

        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_secs())
            .unwrap_or(0);
        let safe_voice = Self::sanitize_file_component(&self.current_voice_name);
        let filename = format!(
            "tts_output_{}_{}.{}",
            safe_voice,
            timestamp,
            Self::download_format_extension(format)
        );
        let target = Self::export_path_for_filename(&filename);

        let export_result = match format {
            DownloadFormat::Wav => self
                .write_wav_file(&target, &samples)
                .map_err(|e| e.to_string()),
            DownloadFormat::Mp3 => {
                Self::write_mp3_file_from_samples(&target, &samples, effective_rate)
            }
        };

        match export_result {
            Ok(()) => Some(target),
            Err(error) => {
                self.add_log(
                    cx,
                    &format!("[ERROR] [download] Failed to export current audio: {}", error),
                );
                self.show_toast(cx, self.tr("下载失败", "Download failed"));
                None
            }
        }
    }

    fn export_history_audio(
        &mut self,
        cx: &mut Cx,
        entry_id: &str,
        format: DownloadFormat,
    ) -> Option<PathBuf> {
        let entry = match self.tts_history.iter().find(|h| h.id == entry_id).cloned() {
            Some(v) => v,
            None => {
                self.add_log(
                    cx,
                    &format!("[WARN] [download] History entry not found: {}", entry_id),
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

        let safe_voice = Self::sanitize_file_component(&entry.voice_name);
        let filename = format!(
            "tts_history_{}_{}.{}",
            entry.created_at,
            safe_voice,
            Self::download_format_extension(format)
        );
        let target = Self::export_path_for_filename(&filename);

        let export_result = match format {
            DownloadFormat::Wav => std::fs::copy(&src, &target).map(|_| ()).map_err(|e| e.to_string()),
            DownloadFormat::Mp3 => Self::convert_wav_file_to_mp3(&src, &target),
        };

        match export_result {
            Ok(()) => Some(target),
            Err(error) => {
                self.add_log(
                    cx,
                    &format!(
                        "[ERROR] [download] Failed to export history audio {}: {}",
                        entry.id, error
                    ),
                );
                self.show_toast(cx, self.tr("下载失败", "Download failed"));
                None
            }
        }
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

    fn write_wav_file(&self, path: &PathBuf, samples: &[f32]) -> std::io::Result<()> {
        Self::write_wav_file_with_sample_rate(path, samples, self.effective_audio_sample_rate())
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

    fn write_mp3_file_from_samples(
        target: &PathBuf,
        samples: &[f32],
        sample_rate: u32,
    ) -> Result<(), String> {
        if sample_rate == 0 {
            return Err("invalid sample rate".to_string());
        }

        let temp_wav = std::env::temp_dir().join(format!(
            "moxin_tts_export_{}_{}.wav",
            std::process::id(),
            SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map(|d| d.as_nanos())
                .unwrap_or(0)
        ));

        Self::write_wav_file_with_sample_rate(&temp_wav, samples, sample_rate)
            .map_err(|e| format!("failed to create temp wav: {}", e))?;

        let conversion = Self::convert_wav_file_to_mp3(&temp_wav, target);
        let _ = std::fs::remove_file(&temp_wav);
        conversion
    }

    fn convert_wav_file_to_mp3(source: &PathBuf, target: &PathBuf) -> Result<(), String> {
        let ffmpeg = Self::resolve_ffmpeg_binary()
            .ok_or_else(|| "ffmpeg not found in PATH or common install locations".to_string())?;

        if target.exists() {
            std::fs::remove_file(target)
                .map_err(|e| format!("failed to replace existing target: {}", e))?;
        }

        let output = Command::new(&ffmpeg)
            .arg("-hide_banner")
            .arg("-loglevel")
            .arg("error")
            .arg("-y")
            .arg("-i")
            .arg(source)
            .arg("-vn")
            .arg("-codec:a")
            .arg("libmp3lame")
            .arg("-q:a")
            .arg("2")
            .arg(target)
            .output()
            .map_err(|e| format!("failed to launch ffmpeg ({}): {}", ffmpeg.display(), e))?;

        if output.status.success() {
            Ok(())
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            let stdout = String::from_utf8_lossy(&output.stdout).trim().to_string();
            let detail = if !stderr.is_empty() {
                stderr
            } else if !stdout.is_empty() {
                stdout
            } else {
                format!("exit status {}", output.status)
            };
            Err(format!("ffmpeg failed: {}", detail))
        }
    }

    fn resolve_ffmpeg_binary() -> Option<PathBuf> {
        for name in ["ffmpeg", "ffmpeg.exe"] {
            if let Some(path) = Self::find_binary_in_path(name) {
                return Some(path);
            }
        }

        for candidate in [
            "/opt/homebrew/bin/ffmpeg",
            "/usr/local/bin/ffmpeg",
            "/opt/local/bin/ffmpeg",
        ] {
            let path = PathBuf::from(candidate);
            if path.is_file() {
                return Some(path);
            }
        }

        None
    }

    fn find_binary_in_path(name: &str) -> Option<PathBuf> {
        let path_var = std::env::var_os("PATH")?;
        for dir in std::env::split_paths(&path_var) {
            let candidate = dir.join(name);
            if candidate.is_file() {
                return Some(candidate);
            }
        }
        None
    }

    // ============ Voice Library Methods ============

    /// Load voice library from disk
    fn load_voice_library(&mut self, cx: &mut Cx) {
        self.library_loading = true;
        self.add_log(cx, "[INFO] [library] Loading voice library...");

        // Load backend-specific builtin voices (locale-aware for Qwen3)
        let mut voices = crate::voice_data::get_builtin_voices_for_backend(
            &self.app_preferences.inference_backend,
            &self.app_language,
        );
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

        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_gender.filter_male_btn))
            .apply_over(cx, live! { draw_bg: { active: (male_active) } draw_text: { active: (male_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_gender.filter_female_btn))
            .apply_over(cx, live! { draw_bg: { active: (female_active) } draw_text: { active: (female_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_gender.age_adult_btn))
            .apply_over(cx, live! { draw_bg: { active: (adult_active) } draw_text: { active: (adult_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_gender.age_youth_btn))
            .apply_over(cx, live! { draw_bg: { active: (youth_active) } draw_text: { active: (youth_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_style.style_sweet_btn))
            .apply_over(cx, live! { draw_bg: { active: (sweet_active) } draw_text: { active: (sweet_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_style.style_magnetic_btn))
            .apply_over(cx, live! { draw_bg: { active: (magnetic_active) } draw_text: { active: (magnetic_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_trait.trait_prof_btn))
            .apply_over(cx, live! { draw_bg: { active: (prof_active) } draw_text: { active: (prof_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.category_filter.row_trait.trait_character_btn))
            .apply_over(cx, live! { draw_bg: { active: (character_active) } draw_text: { active: (character_active) } });
    }

    /// Update language filter button states
    fn update_language_filter_buttons(&mut self, cx: &mut Cx) {
        let all_active = if self.library_language_filter == LanguageFilter::All { 1.0 } else { 0.0 };
        let zh_active = if self.library_language_filter == LanguageFilter::Chinese { 1.0 } else { 0.0 };
        let en_active = if self.library_language_filter == LanguageFilter::English { 1.0 } else { 0.0 };

        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.language_filter.lang_all_btn))
            .apply_over(cx, live! { draw_bg: { active: (all_active) } draw_text: { active: (all_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.language_filter.lang_zh_btn))
            .apply_over(cx, live! { draw_bg: { active: (zh_active) } draw_text: { active: (zh_active) } });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.library_page.library_header.title_and_tags.language_filter.lang_en_btn))
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

    fn update_user_settings_tabs(&mut self, cx: &mut Cx) {
        let general_active = if self.user_settings_tab == 0 { 1.0 } else { 0.0 };
        let voice_active = if self.user_settings_tab == 1 { 1.0 } else { 0.0 };
        let system_active = if self.user_settings_tab == 2 { 1.0 } else { 0.0 };
        let dark_mode = self.dark_mode;

        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_tab_bar.tab_profile_btn))
            .apply_over(
                cx,
                live! { draw_bg: { active: (general_active), dark_mode: (dark_mode) } draw_text: { active: (general_active), dark_mode: (dark_mode) } },
            );
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_tab_bar.tab_app_btn))
            .apply_over(
                cx,
                live! { draw_bg: { active: (voice_active), dark_mode: (dark_mode) } draw_text: { active: (voice_active), dark_mode: (dark_mode) } },
            );
        self.view
            .button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_tab_bar.tab_runtime_btn))
            .apply_over(
                cx,
                live! { draw_bg: { active: (system_active), dark_mode: (dark_mode) } draw_text: { active: (system_active), dark_mode: (dark_mode) } },
            );

        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel))
            .set_visible(cx, self.user_settings_tab == 0);
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel))
            .set_visible(cx, self.user_settings_tab == 1);
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel))
            .set_visible(cx, self.user_settings_tab == 2);
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
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.language_section.language_options.lang_en_option))
            .apply_over(cx, live! {
                draw_bg: { active: (en_active), dark_mode: (dark_mode) }
                draw_text: { active: (en_active), dark_mode: (dark_mode) }
            });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.language_section.language_options.lang_zh_option))
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
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.theme_section.theme_options.theme_light_option))
            .apply_over(cx, live! {
                draw_bg: { active: (light_active), dark_mode: (dark_mode) }
                draw_text: { active: (light_active), dark_mode: (dark_mode) }
            });
        self.view.button(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.theme_section.theme_options.theme_dark_option))
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

    fn effective_audio_sample_rate(&self) -> u32 {
        if self.stored_audio_samples.is_empty() {
            self.stored_audio_sample_rate
        } else if self.processed_audio_samples.is_empty() {
            self.stored_audio_sample_rate
        } else {
            32000
        }
    }

    fn resample_linear(samples: &[f32], in_rate: u32, out_rate: u32) -> Vec<f32> {
        if samples.is_empty() || in_rate == 0 || out_rate == 0 || in_rate == out_rate {
            return samples.to_vec();
        }

        let ratio = out_rate as f32 / in_rate as f32;
        let new_len = (samples.len() as f32 * ratio).round().max(1.0) as usize;
        let mut result = Vec::with_capacity(new_len);

        for i in 0..new_len {
            let src_idx = i as f32 / ratio;
            let idx = src_idx as usize;
            let frac = src_idx - idx as f32;
            let s1 = samples.get(idx).copied().unwrap_or(0.0);
            let s2 = samples.get(idx + 1).copied().unwrap_or(s1);
            result.push(s1 + (s2 - s1) * frac);
        }

        result
    }

    fn rebuild_processed_audio_samples(&mut self) {
        if self.stored_audio_samples.is_empty() {
            self.processed_audio_samples.clear();
            return;
        }

        // The local player expects 32k source audio. Qwen MLX returns 24k,
        // so normalize playback/export samples here to avoid pitch/time distortion.
        if self.stored_audio_sample_rate > 0 && self.stored_audio_sample_rate != 32000 {
            self.processed_audio_samples = Self::resample_linear(
                &self.stored_audio_samples,
                self.stored_audio_sample_rate,
                32000,
            );
        } else {
            self.processed_audio_samples = self.stored_audio_samples.clone();
        }
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

        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.page_header.page_title
            ))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .view(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.page_header.translation_status_badge
            ))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .view(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card
            ))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_source.translation_source_label
            ))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_src_lang.translation_src_lang_label
            ))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_tgt_lang.translation_tgt_lang_label
            ))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_overlay.translation_overlay_style_label
            ))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_font_size.translation_font_size_label
            ))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_anchor_position.translation_anchor_position_label
            ))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_opacity.translation_opacity_label
            ))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_overlay.overlay_style_compact
            ))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } });
        self.view
            .button(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_overlay.overlay_style_full
            ))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } });
        self.view
            .drop_down(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_source.translation_source_dropdown
            ))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } popup_menu: { draw_bg: { dark_mode: (dark_mode) } menu_item: { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } } } });
        self.view
            .drop_down(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_src_lang.src_lang_dropdown
            ))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } popup_menu: { draw_bg: { dark_mode: (dark_mode) } menu_item: { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } } } });
        self.view
            .drop_down(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_tgt_lang.tgt_lang_dropdown
            ))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } popup_menu: { draw_bg: { dark_mode: (dark_mode) } menu_item: { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } } } });
        self.view
            .drop_down(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_font_size.font_size_dropdown
            ))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } popup_menu: { draw_bg: { dark_mode: (dark_mode) } menu_item: { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } } } });
        self.view
            .drop_down(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_anchor_position.anchor_position_dropdown
            ))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } popup_menu: { draw_bg: { dark_mode: (dark_mode) } menu_item: { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } } } });
        self.view
            .drop_down(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_settings_panel.settings_card.setting_row_opacity.opacity_dropdown
            ))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } popup_menu: { draw_bg: { dark_mode: (dark_mode) } menu_item: { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } } } });
        self.view
            .view(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_running_panel.translation_log_card
            ))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(
                content_wrapper.main_content.left_column.content_area.translation_page.translation_body.translation_running_panel.translation_log_card.translation_log_title
            ))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });

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
        self.update_user_settings_tabs(cx);
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
            .view(ids!(download_modal.download_dialog))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(download_modal.download_dialog.download_header.download_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(download_modal.download_dialog.download_header.download_subtitle))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .button(ids!(download_modal.download_dialog.download_actions.download_mp3_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .button(ids!(download_modal.download_dialog.download_actions.download_wav_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
        self.view
            .button(ids!(download_modal.download_dialog.download_footer.download_cancel_btn))
            .apply_over(
                cx,
                live! {
                    draw_bg: { dark_mode: (dark_mode) }
                    draw_text: { dark_mode: (dark_mode) }
                },
            );
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
        self.view
            .label(ids!(global_settings_modal.settings_dialog.settings_content.about_section.about_ominix))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });

        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.user_settings_header.user_settings_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.app_settings_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.language_section.language_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.profile_panel.app_settings_card.theme_section.theme_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.speed_col.speed_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.pitch_col.pitch_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.volume_col.volume_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .text_input(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.speed_col.speed_input))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } });
        self.view
            .text_input(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.pitch_col.pitch_input))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } });
        self.view
            .text_input(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.defaults_card.defaults_row.volume_col.volume_input))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } });
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.runtime_card))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.runtime_card.runtime_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.runtime_card.dora_status))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.runtime_card.asr_status))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.runtime_card.tts_status))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.paths_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.model_path_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.log_path_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.paths_card.workspace_path_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.privacy_card))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.privacy_card.privacy_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.privacy_card.retention_pick_row.retention_pick_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.devices_header.devices_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.input_pick_row.input_pick_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.output_pick_row.output_pick_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.experiments_title))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.zero_shot_backend_pick_row.zero_shot_backend_pick_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.backend_pick_row.backend_pick_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.debug_pick_row.debug_pick_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.qwen_status_row.qwen_status_label))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.qwen_status_row.qwen_status_value))
            .apply_over(cx, live! { draw_text: { dark_mode: (dark_mode) } });
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.input_pick_row.input_device_dropdown))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } popup_menu: { width: 520.0 draw_bg: { dark_mode: (dark_mode) } menu_item: { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } } } });
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.app_panel.devices_card.output_pick_row.output_device_dropdown))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } popup_menu: { width: 520.0 draw_bg: { dark_mode: (dark_mode) } menu_item: { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } } } });
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.privacy_card.retention_pick_row.retention_dropdown))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } popup_menu: { width: 520.0 draw_bg: { dark_mode: (dark_mode) } menu_item: { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } } } });
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.zero_shot_backend_pick_row.zero_shot_backend_dropdown))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } popup_menu: { width: 520.0 draw_bg: { dark_mode: (dark_mode) } menu_item: { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } } } });
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.backend_pick_row.training_backend_dropdown))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } popup_menu: { width: 520.0 draw_bg: { dark_mode: (dark_mode) } menu_item: { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } } } });
        self.view
            .drop_down(ids!(content_wrapper.main_content.left_column.content_area.user_settings_page.settings_scroll.settings_scroll_content.runtime_panel.experiments_card.debug_pick_row.debug_logs_dropdown))
            .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } popup_menu: { width: 520.0 draw_bg: { dark_mode: (dark_mode) } menu_item: { draw_bg: { dark_mode: (dark_mode) } draw_text: { dark_mode: (dark_mode) } } } });
        self.view.redraw(cx);
    }

    fn get_voice_picker_voices(&self) -> Vec<Voice> {
        use crate::voice_data::VoiceSource;

        self.library_voices
            .iter()
            .filter(|v| match self.voice_picker_tab {
                0 => true,
                1 => v.source != VoiceSource::Builtin && v.source != VoiceSource::BundledIcl,
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

        let active_voice_id = self.selected_voice_id.as_ref();
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
            .portal_list(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_picker_list))
            .set_visible(cx, !is_empty);
        self.view
            .label(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_picker_empty))
            .set_text(cx, empty_text);
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_picker_empty_container))
            .set_visible(cx, is_empty);

        self.view.redraw(cx);
    }

    fn sync_selected_model_ui(&mut self, cx: &mut Cx) {
        // Qwen3-only: hide model picker row — no backend choice available.
        self.view
            .view(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.input_section.bottom_bar.model_row))
            .set_visible(cx, false);

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
        // Check if Qwen3 model is ready before switching
        if model_id == "qwen3_tts_mlx" && !Self::qwen_custom_ready() {
            self.start_qwen_model_download(
                cx,
                "qwen3_tts_mlx",
                true,
                self.app_preferences.zero_shot_backend == "qwen3_tts_mlx",
            );
            self.show_toast(
                cx,
                self.tr(
                    "Qwen 推理模型未就绪，已开始后台下载，完成后可切换",
                    "Qwen inference model not ready. Background download started. You can switch after download completes.",
                ),
            );
            return;
        }

        if let Some(model) = self.model_options.iter().find(|m| m.id == model_id).cloned() {
            self.selected_tts_model_id = Some(model.id.clone());
            // Update the inference_backend preference to match selected model
            self.app_preferences.inference_backend = model.id.clone();
            std::env::set_var("MOXIN_INFERENCE_BACKEND", &model.id);
            self.persist_app_preferences(cx);
            self.load_voice_library(cx);
            self.update_user_settings_page(cx);
            self.set_generate_button_loading(cx, self.tts_status == TTSStatus::Generating);
            self.stop_dora(cx);
            // Do NOT call auto_start_dataflow here — stop_dora is async, so is_running() is still
            // true immediately after. The timer will call auto_start_dataflow once the old bridges
            // have dropped (phase 1 complete), then wait for 4 bridges to come up (phase 2).
            // Show in-dialog switching overlay and wait for new dataflow to be ready
            self.backend_switching = true;
            self.backend_switch_bridges_dropped = false;
            self.view.view(ids!(model_picker_modal.model_picker_dialog.switching_overlay)).set_visible(cx, true);
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
        self.set_generate_button_loading(cx, self.tts_status == TTSStatus::Generating);
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
            .map(|v| v.source != crate::voice_data::VoiceSource::Builtin && v.source != crate::voice_data::VoiceSource::BundledIcl)
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
        if self.voice_picker_active_voice_id.as_deref() == Some(&voice_id) {
            self.voice_picker_active_voice_id = None;
        }

        self.update_library_display(cx);
        self.reset_voice_lists_after_delete(cx);
        self.sync_selected_voice_ui(cx);
        self.update_voice_picker_controls(cx);
        self.show_toast(cx, self.tr("音色已删除", "Voice deleted successfully"));
    }

    fn reset_voice_lists_after_delete(&mut self, _cx: &mut Cx) {
        self.view
            .portal_list(ids!(content_wrapper.main_content.left_column.content_area.library_page.voice_list))
            .set_first_id(0);
        self.view
            .portal_list(ids!(content_wrapper.main_content.left_column.content_area.tts_page.cards_container.controls_panel.settings_panel.inline_voice_picker.voice_picker_list))
            .set_first_id(0);
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
        } else if voice.source == VoiceSource::BundledIcl {
            // Use bundled ref audio as preview
            let ref_filename = voice.reference_audio_path.as_deref().unwrap_or("ref.wav");
            match self.resolve_bundled_icl_ref_path(&voice.id, ref_filename) {
                Some(path) => path,
                None => {
                    self.add_log(cx, &format!("[WARN] [library] Bundled ref audio not found for: {}", voice_id));
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
            if self.app_preferences.inference_backend == "qwen3_tts_mlx" {
                // Qwen3 preview audio: bundled in repo/app
                self.resolve_qwen_preview_path(&preview_file)
            } else {
                let home = dirs::home_dir().unwrap_or_else(|| PathBuf::from("."));
                home.join(".dora")
                    .join("models")
                    .join("primespeech")
                    .join("moyoyo")
                    .join("ref_audios")
                    .join(&preview_file)
            }
        };

        if !audio_path.exists() {
            self.add_log(cx, &format!("[WARN] [library] Preview audio file not found: {:?}", audio_path));
            return;
        }

        // Load and play WAV file
        match self.load_wav_file(&audio_path) {
            Ok(samples) => {
                if self.preview_player.is_none() {
                    self.preview_player = Some(TTSPlayer::new_with_output_device(
                        self.app_preferences.preferred_output_device.as_deref(),
                    ));
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
    pub fn translation_shared_dora_state(
        &self,
    ) -> Option<Arc<moxin_dora_bridge::SharedDoraState>> {
        self.borrow()
            .and_then(|inner| inner.translation_shared_state())
    }

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

    pub fn set_translation_overlay_font_size_preset(&self, cx: &mut Cx, preset: &str) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.translation_overlay_font_size_preset = match preset {
                "small" | "large" | "normal" => preset.to_string(),
                _ => "normal".to_string(),
            };
            inner.update_translation_font_size_dropdown(cx);
            inner.sync_translation_overlay_font_size();
        }
    }
}
