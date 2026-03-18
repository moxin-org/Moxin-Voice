//! Voice Clone Modal - UI for creating custom voices via zero-shot cloning
//!
//! Supports two modes:
//! 1. Express Mode (Zero-shot): Select existing audio file + manually enter prompt text OR record voice via microphone + auto-transcribe with ASR
//! 2. Pro Mode (Few-shot Training): Record 3-10 minutes of audio and train custom GPT-SoVITS models

use crate::audio_player::TTSPlayer;
use crate::task_persistence;
use crate::voice_data::{CloningStatus, Voice};
use crate::voice_persistence;
use makepad_widgets::*;
use parking_lot::Mutex;
use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::Instant;

/// Clone mode - Express (zero-shot ICL) or Pro (few-shot training).
///
/// Pro mode is currently disabled in the UI (Qwen3-only refactor).
/// The `Pro` variant and related code are kept for restoration.
/// See doc/REFACTOR_QWEN3_ONLY.md.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CloneMode {
    Express,  // Zero-shot ICL cloning (up to 30s reference audio)
    Pro,      // Few-shot training (3-10 minute audio) — hidden, see above
}

impl Default for CloneMode {
    fn default() -> Self {
        Self::Express
    }
}

/// Recording state
#[derive(Clone, Debug, PartialEq, Default)]
pub enum RecordingStatus {
    #[default]
    Idle,
    Recording,
    Transcribing,
    Completed,
    Error(String),
}

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    use moxin_widgets::theme::*;

    // Modal overlay background
    ModalOverlay = <View> {
        width: Fill, height: Fill
        show_bg: true
        draw_bg: {
            fn pixel(self) -> vec4 {
                return vec4(0.0, 0.0, 0.0, 0.5);
            }
        }
    }

    // Text input field with label
    LabeledInput = <View> {
        width: Fill, height: Fit
        flow: Down
        spacing: 6

        label = <Label> {
            width: Fill, height: Fit
            draw_text: {
                instance dark_mode: 0.0
                text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                fn get_color(self) -> vec4 {
                    return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                }
            }
        }

        input = <TextInput> {
            width: Fill, height: 40
            padding: {left: 12, right: 12, top: 8, bottom: 8}
            empty_text: ""

            draw_bg: {
                instance dark_mode: 0.0
                instance focus: 0.0
                border_radius: 6.0
                fn pixel(self) -> vec4 {
                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                    let bg = mix((WHITE), (SLATE_700), self.dark_mode);
                    sdf.fill(bg);
                    let border_normal = mix((SLATE_200), (SLATE_600), self.dark_mode);
                    let border_focused = (PRIMARY_500);
                    let border = mix(border_normal, border_focused, self.focus);
                    sdf.stroke(border, mix(1.0, 2.0, self.focus));
                    return sdf.result;
                }
            }

            draw_text: {
                instance dark_mode: 0.0
                text_style: { font_size: 13.0 }
                fn get_color(self) -> vec4 {
                    return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                }
            }

            draw_cursor: {
                instance focus: 0.0
                uniform border_radius: 0.5
                fn pixel(self) -> vec4 {
                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                    sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, self.border_radius);
                    sdf.fill((PRIMARY_500));
                    return sdf.result;
                }
            }

            draw_selection: {
                instance focus: 0.0
                fn pixel(self) -> vec4 {
                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                    sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 1.0);
                    sdf.fill(mix(vec4(0.23, 0.51, 0.97, 0.2), vec4(0.23, 0.51, 0.97, 0.35), self.focus));
                    return sdf.result;
                }
            }
        }
    }

    // File selector row with record option
    FileSelector = <View> {
        width: Fill, height: Fit
        flow: Down
        spacing: 6

        label = <Label> {
            width: Fill, height: Fit
            draw_text: {
                instance dark_mode: 0.0
                text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                fn get_color(self) -> vec4 {
                    return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                }
            }
            text: "Reference Audio (3-10 seconds)"
        }

        file_row = <View> {
            width: Fill, height: 60
            flow: Right
            spacing: 8
            align: {y: 0.5}
            padding: {left: 8, right: 8}
            show_bg: true

            draw_bg: {
                instance is_drag_over: 0.0
                instance border_radius: 6.0
                fn pixel(self) -> vec4 {
                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                    // Background: subtle blue tint when dragging over
                    sdf.fill(vec4(0.39, 0.40, 0.95, self.is_drag_over * 0.07));
                    // Border: subtle gray normally, bright blue when dragging
                    let border_color = mix(
                        vec4(0.75, 0.77, 0.82, 0.7),
                        vec4(0.39, 0.40, 0.95, 1.0),
                        self.is_drag_over
                    );
                    sdf.stroke(border_color, mix(1.0, 2.0, self.is_drag_over));
                    return sdf.result;
                }
            }

            // Record button (microphone)
            record_btn = <Button> {
                width: 36, height: 36

                draw_bg: {
                    instance dark_mode: 0.0
                    instance hover: 0.0
                    instance recording: 0.0

                    fn pixel(self) -> vec4 {
                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                        sdf.circle(18.0, 18.0, 17.0);

                        // Background
                        let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                        let hover_color = mix((RED_100), (RED_900), self.dark_mode);
                        let recording_color = mix((RED_500), (RED_400), self.dark_mode);
                        let color = mix(base, hover_color, self.hover * (1.0 - self.recording));
                        let color = mix(color, recording_color, self.recording);
                        sdf.fill(color);

                        // Microphone icon or stop square
                        if self.recording > 0.5 {
                            // Stop icon (square)
                            sdf.rect(13.0, 13.0, 10.0, 10.0);
                            sdf.fill((WHITE));
                        } else {
                            // Microphone icon (simplified)
                            let icon_color = mix((SLATE_500), (SLATE_400), self.dark_mode);
                            let icon_color = mix(icon_color, (RED_500), self.hover);
                            // Mic body
                            sdf.box(15.0, 10.0, 6.0, 10.0, 3.0);
                            sdf.fill(icon_color);
                            // Mic stand arc
                            sdf.move_to(12.0, 18.0);
                            sdf.line_to(12.0, 20.0);
                            sdf.line_to(18.0, 24.0);
                            sdf.line_to(24.0, 20.0);
                            sdf.line_to(24.0, 18.0);
                            sdf.stroke(icon_color, 1.5);
                            // Mic stand
                            sdf.move_to(18.0, 24.0);
                            sdf.line_to(18.0, 27.0);
                            sdf.stroke(icon_color, 1.5);
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

            select_btn = <Button> {
                width: Fit, height: 36
                padding: {left: 16, right: 16}
                text: "Select File..."

                draw_bg: {
                    instance dark_mode: 0.0
                    instance hover: 0.0
                    border_radius: 6.0
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
                    text_style: { font_size: 12.0 }
                    fn get_color(self) -> vec4 {
                        return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                    }
                }
            }

            file_name = <Label> {
                width: Fill, height: Fit
                draw_text: {
                    instance dark_mode: 0.0
                    text_style: { font_size: 12.0 }
                    fn get_color(self) -> vec4 {
                        return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                    }
                }
                text: "No file selected · drag audio here"
            }

            // Preview button
            preview_btn = <Button> {
                width: 36, height: 36
                visible: false

                draw_bg: {
                    instance dark_mode: 0.0
                    instance hover: 0.0
                    instance playing: 0.0

                    fn pixel(self) -> vec4 {
                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                        sdf.circle(18.0, 18.0, 17.0);
                        let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                        let hover_color = mix((PRIMARY_100), (PRIMARY_700), self.dark_mode);
                        let color = mix(base, hover_color, self.hover);
                        sdf.fill(color);

                        // Draw play triangle
                        if self.playing > 0.5 {
                            // Stop icon (square)
                            sdf.rect(13.0, 13.0, 10.0, 10.0);
                            let icon_color = mix((PRIMARY_600), (PRIMARY_300), self.dark_mode);
                            sdf.fill(icon_color);
                        } else {
                            // Play icon (triangle)
                            sdf.move_to(14.0, 11.0);
                            sdf.line_to(25.0, 18.0);
                            sdf.line_to(14.0, 25.0);
                            sdf.close_path();
                            let icon_color = mix((SLATE_500), (SLATE_400), self.dark_mode);
                            sdf.fill(mix(icon_color, (PRIMARY_500), self.hover));
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

            // Clear recording button — shown after recording/upload, hidden initially
            clear_recording_btn = <Button> {
                width: 36, height: 36
                visible: false

                draw_bg: {
                    instance dark_mode: 0.0
                    instance hover: 0.0

                    fn pixel(self) -> vec4 {
                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                        sdf.circle(18.0, 18.0, 17.0);
                        let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                        let hover_color = mix((RED_100), (RED_900), self.dark_mode);
                        sdf.fill(mix(base, hover_color, self.hover));
                        let icon_color = mix((SLATE_500), (SLATE_400), self.dark_mode);
                        let icon_color = mix(icon_color, (RED_400), self.hover);
                        sdf.move_to(12.0, 12.0); sdf.line_to(24.0, 24.0);
                        sdf.stroke(icon_color, 2.0);
                        sdf.move_to(24.0, 12.0); sdf.line_to(12.0, 24.0);
                        sdf.stroke(icon_color, 2.0);
                        return sdf.result;
                    }
                }

                draw_text: {
                    text_style: { font_size: 0.0 }
                    fn get_color(self) -> vec4 { return vec4(0.0, 0.0, 0.0, 0.0); }
                }
            }
        }

        audio_info = <Label> {
            width: Fill, height: Fit
            margin: { top: 4 }
            draw_text: {
                instance dark_mode: 0.0
                text_style: { font_size: 11.0 }
                fn get_color(self) -> vec4 {
                    return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                }
            }
            text: ""
        }
    }

    // Language dropdown
    LanguageSelector = <View> {
        width: Fill, height: Fit
        flow: Down
        spacing: 6

        label = <Label> {
            width: Fill, height: Fit
            draw_text: {
                instance dark_mode: 0.0
                text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                fn get_color(self) -> vec4 {
                    return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                }
            }
            text: "Language"
        }

        lang_row = <View> {
            width: Fill, height: Fit
            flow: Right
            spacing: 12

            zh_btn = <Button> {
                width: Fit, height: 36
                padding: {left: 20, right: 20}
                text: "Chinese"

                draw_bg: {
                    instance dark_mode: 0.0
                    instance hover: 0.0
                    instance selected: 1.0
                    border_radius: 6.0
                    fn pixel(self) -> vec4 {
                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                        let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                        let selected_color = mix((PRIMARY_100), (PRIMARY_800), self.dark_mode);
                        let hover_color = mix((SLATE_200), (SLATE_600), self.dark_mode);
                        let color = mix(base, selected_color, self.selected);
                        let color = mix(color, hover_color, self.hover * (1.0 - self.selected));
                        sdf.fill(color);
                        let border = mix(mix((SLATE_300), (SLATE_500), self.dark_mode), (PRIMARY_500), self.selected);
                        sdf.stroke(border, 1.0);
                        return sdf.result;
                    }
                }

                draw_text: {
                    instance dark_mode: 0.0
                    instance selected: 1.0
                    text_style: { font_size: 12.0 }
                    fn get_color(self) -> vec4 {
                        let base = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                        let selected = mix((PRIMARY_700), (PRIMARY_200), self.dark_mode);
                        return mix(base, selected, self.selected);
                    }
                }
            }

            en_btn = <Button> {
                width: Fit, height: 36
                padding: {left: 20, right: 20}
                text: "English"

                draw_bg: {
                    instance dark_mode: 0.0
                    instance hover: 0.0
                    instance selected: 0.0
                    border_radius: 6.0
                    fn pixel(self) -> vec4 {
                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                        let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                        let selected_color = mix((PRIMARY_100), (PRIMARY_800), self.dark_mode);
                        let hover_color = mix((SLATE_200), (SLATE_600), self.dark_mode);
                        let color = mix(base, selected_color, self.selected);
                        let color = mix(color, hover_color, self.hover * (1.0 - self.selected));
                        sdf.fill(color);
                        let border = mix(mix((SLATE_300), (SLATE_500), self.dark_mode), (PRIMARY_500), self.selected);
                        sdf.stroke(border, 1.0);
                        return sdf.result;
                    }
                }

                draw_text: {
                    instance dark_mode: 0.0
                    instance selected: 0.0
                    text_style: { font_size: 12.0 }
                    fn get_color(self) -> vec4 {
                        let base = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                        let selected = mix((PRIMARY_700), (PRIMARY_200), self.dark_mode);
                        return mix(base, selected, self.selected);
                    }
                }
            }
        }
    }

    // Progress log area (compact)
    ProgressLog = <View> {
        width: Fill, height: 0
        flow: Down
        spacing: 4
        visible: false

        label = <Label> {
            width: Fill, height: Fit
            draw_text: {
                instance dark_mode: 0.0
                text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                fn get_color(self) -> vec4 {
                    return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                }
            }
            text: "Progress"
        }

        log_scroll = <ScrollYView> {
            width: Fill, height: Fill
            show_bg: true
            draw_bg: {
                instance dark_mode: 0.0
                fn pixel(self) -> vec4 {
                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 4.0);
                    let bg = mix((SLATE_50), (SLATE_800), self.dark_mode);
                    sdf.fill(bg);
                    sdf.stroke(mix((SLATE_200), (SLATE_600), self.dark_mode), 1.0);
                    return sdf.result;
                }
            }

            log_content = <Label> {
                width: Fill, height: Fit
                padding: {left: 10, right: 10, top: 8, bottom: 8}
                draw_text: {
                    instance dark_mode: 0.0
                    text_style: { font_size: 11.0, line_spacing: 1.5 }
                    fn get_color(self) -> vec4 {
                        return mix((SLATE_600), (SLATE_300), self.dark_mode);
                    }
                }
                text: "Ready to clone voice..."
            }
        }
    }

    // Action button
    ActionButton = <Button> {
        width: Fit, height: 40
        padding: {left: 24, right: 24}

        draw_bg: {
            instance dark_mode: 0.0
            instance hover: 0.0
            instance pressed: 0.0
            instance primary: 0.0
            border_radius: 6.0

            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);

                // Primary button style
                let primary_base = mix((PRIMARY_500), (PRIMARY_400), self.dark_mode);
                let primary_hover = mix((PRIMARY_600), (PRIMARY_300), self.dark_mode);
                let primary_pressed = mix((PRIMARY_700), (PRIMARY_500), self.dark_mode);

                // Secondary button style
                let secondary_base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                let secondary_hover = mix((SLATE_200), (SLATE_600), self.dark_mode);
                let secondary_pressed = mix((SLATE_300), (SLATE_500), self.dark_mode);

                let base = mix(secondary_base, primary_base, self.primary);
                let hover_color = mix(secondary_hover, primary_hover, self.primary);
                let pressed_color = mix(secondary_pressed, primary_pressed, self.primary);

                let color = mix(base, hover_color, self.hover);
                let color = mix(color, pressed_color, self.pressed);

                sdf.fill(color);

                // Border for secondary
                if self.primary < 0.5 {
                    sdf.stroke(mix((SLATE_300), (SLATE_500), self.dark_mode), 1.0);
                }

                return sdf.result;
            }
        }

        draw_text: {
            instance dark_mode: 0.0
            instance primary: 0.0
            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
            fn get_color(self) -> vec4 {
                let secondary = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                return mix(secondary, (WHITE), self.primary);
            }
        }
    }

    // Mode tab button
    ModeTabButton = <Button> {
        width: Fit, height: 44
        padding: {left: 24, right: 24}

        draw_bg: {
            instance dark_mode: 0.0
            instance hover: 0.0
            instance active: 0.0

            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);

                // Background
                let base = mix((SLATE_50), (SLATE_700), self.dark_mode);
                let active_bg = mix((WHITE), (SLATE_800), self.dark_mode);
                let bg = mix(base, active_bg, self.active);
                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 0.0);
                sdf.fill(bg);

                // Bottom border (active indicator)
                if self.active > 0.5 {
                    let border_color = mix((PRIMARY_500), (PRIMARY_400), self.dark_mode);
                    sdf.move_to(0., self.rect_size.y);
                    sdf.line_to(self.rect_size.x, self.rect_size.y);
                    sdf.stroke(border_color, 3.0);
                }

                return sdf.result;
            }
        }

        draw_text: {
            instance dark_mode: 0.0
            instance active: 0.0
            text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
            fn get_color(self) -> vec4 {
                let inactive = mix((SLATE_600), (SLATE_400), self.dark_mode);
                let active_color = mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                return mix(inactive, active_color, self.active);
            }
        }
    }

    // Main modal dialog
    pub VoiceCloneModal = {{VoiceCloneModal}} {
        width: Fill, height: Fill
        flow: Overlay
        visible: false

        // Overlay background
        overlay = <ModalOverlay> {}

        // Modal container (scrollable when window is small)
        modal_container = <ScrollYView> {
            width: Fill, height: Fill
            align: {x: 0.5, y: 0.0}
            padding: {top: 40, bottom: 40}
            scroll_bars: <ScrollBars> {
                show_scroll_x: false
                show_scroll_y: true
            }

            // Centering wrapper
            modal_wrapper = <View> {
                width: Fill, height: Fit
                align: {x: 0.5, y: 0.0}

            // Modal content
            modal_content = <RoundedView> {
                width: 520, height: Fit
                flow: Down
                padding: 0

                draw_bg: {
                    instance dark_mode: 0.0
                    border_radius: 12.0
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
                    flow: Right
                    align: {y: 0.5}

                    show_bg: true
                    draw_bg: {
                        instance dark_mode: 0.0
                        fn pixel(self) -> vec4 {
                            return mix((SLATE_50), (SLATE_700), self.dark_mode);
                        }
                    }

                    title = <Label> {
                        width: Fill, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: <FONT_BOLD>{ font_size: 16.0 }
                            fn get_color(self) -> vec4 {
                                return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                            }
                        }
                        text: "Clone Voice"
                    }

                    close_btn = <Button> {
                        width: 32, height: 32

                        draw_bg: {
                            instance dark_mode: 0.0
                            instance hover: 0.0
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                sdf.circle(16.0, 16.0, 15.0);
                                let base = mix((SLATE_100), (SLATE_600), self.dark_mode);
                                let hover_color = mix((SLATE_200), (SLATE_500), self.dark_mode);
                                sdf.fill(mix(base, hover_color, self.hover));

                                // X icon
                                let x_color = mix((SLATE_500), (SLATE_300), self.dark_mode);
                                sdf.move_to(11.0, 11.0);
                                sdf.line_to(21.0, 21.0);
                                sdf.stroke(x_color, 1.5);
                                sdf.move_to(21.0, 11.0);
                                sdf.line_to(11.0, 21.0);
                                sdf.stroke(x_color, 1.5);

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
                }

                // Body
                body = <View> {
                    width: Fill, height: Fit
                    flow: Down
                    spacing: 0

                    // EXPRESS MODE CONTENT (existing zero-shot cloning)
                    express_mode_content = <View> {
                        width: Fill, height: Fit
                        flow: Down
                        padding: {left: 24, right: 24, top: 16, bottom: 16}
                        spacing: 16
                        visible: true

                        // File selector
                        file_selector = <FileSelector> {}

                        // Reference text input with transcription loading overlay
                        prompt_text_container = <View> {
                        width: Fill, height: Fit
                        flow: Overlay

                        prompt_text_input = <LabeledInput> {
                            label = { text: "Reference Text (what the audio says)" }
                            input = {
                                height: 60
                                empty_text: "Enter the exact text spoken in the reference audio..."
                            }
                        }

                        // Transcription loading overlay
                        transcription_loading_overlay = <View> {
                            width: Fill, height: Fill
                            flow: Overlay
                            align: {x: 0.5, y: 0.5}
                            visible: false

                            // Semi-transparent backdrop
                            loading_backdrop = <View> {
                                width: Fill, height: Fill
                                show_bg: true
                                draw_bg: {
                                    instance dark_mode: 0.0
                                    fn pixel(self) -> vec4 {
                                        let bg = mix(vec4(1.0, 1.0, 1.0, 0.92), vec4(0.15, 0.15, 0.15, 0.92), self.dark_mode);
                                        return bg;
                                    }
                                }
                            }

                            // Loading content
                            loading_content = <View> {
                                width: Fit, height: Fit
                                flow: Right
                                spacing: 12
                                align: {x: 0.5, y: 0.5}
                                padding: {left: 16, right: 16, top: 8, bottom: 8}

                                show_bg: true
                                draw_bg: {
                                    instance dark_mode: 0.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 8.0);
                                        let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                        sdf.fill(bg);
                                        sdf.stroke(mix((PRIMARY_200), (PRIMARY_600), self.dark_mode), 1.5);
                                        return sdf.result;
                                    }
                                }

                                // Spinner (3 rotating dots)
                                loading_spinner = <View> {
                                    width: 24, height: 24
                                    show_bg: true
                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        instance rotation: 0.0

                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            let center = vec2(self.rect_size.x * 0.5, self.rect_size.y * 0.5);
                                            let radius = 8.0;
                                            let dot_radius = 2.5;

                                            let base_color = mix((PRIMARY_500), (PRIMARY_400), self.dark_mode);
                                            let mut result = vec4(0.0, 0.0, 0.0, 0.0);

                                            // Draw 3 dots in a circle
                                            for i in 0..3 {
                                                let angle = (float(i) / 3.0) * 6.28318530718 + self.rotation * 6.28318530718;
                                                let dot_x = center.x + cos(angle) * radius;
                                                let dot_y = center.y + sin(angle) * radius;

                                                let opacity = (float(i) / 3.0) * 0.6 + 0.4;

                                                sdf.circle(dot_x, dot_y, dot_radius);
                                                let dot_color = vec4(base_color.rgb, base_color.a * opacity);
                                                result = mix(result, dot_color, sdf.fill_keep(dot_color).a);
                                            }

                                            return result;
                                        }
                                    }
                                }

                                loading_label = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((PRIMARY_600), (PRIMARY_300), self.dark_mode);
                                        }
                                    }
                                    text: "Transcribing..."
                                }
                            }
                        }
                    }

                    // Voice name input
                    voice_name_input = <LabeledInput> {
                        label = { text: "Voice Name" }
                        input = {
                            empty_text: "Enter a name for this voice..."
                        }
                    }

                    // Language selector
                    language_selector = <LanguageSelector> {}

                        // Progress log
                        progress_log = <ProgressLog> {}
                    } // end express_mode_content

                    // PRO MODE CONTENT (few-shot training)
                    pro_mode_content = <View> {
                        width: Fill, height: Fit
                        flow: Down
                        padding: {left: 24, right: 24, top: 16, bottom: 16}
                        spacing: 16
                        visible: false

                        // Training recording section
                        training_recording_section = <View> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 12

                            label = <Label> {
                                width: Fill, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 12.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "Training Audio"
                            }

                            instruction = <Label> {
                                width: Fill, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: { font_size: 11.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "Record or upload audio (WAV, 10s-10min). Longer recordings (3-10 min) produce better results."
                            }

                            record_row = <View> {
                                width: Fill, height: Fit
                                flow: Right
                                spacing: 12
                                align: {y: 0.5}
                                padding: {left: 8, right: 8, top: 8, bottom: 8}
                                show_bg: true

                                draw_bg: {
                                    instance is_drag_over: 0.0
                                    instance border_radius: 6.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        sdf.fill(vec4(0.39, 0.40, 0.95, self.is_drag_over * 0.07));
                                        let border_color = mix(
                                            vec4(0.75, 0.77, 0.82, 0.7),
                                            vec4(0.39, 0.40, 0.95, 1.0),
                                            self.is_drag_over
                                        );
                                        sdf.stroke(border_color, mix(1.0, 2.0, self.is_drag_over));
                                        return sdf.result;
                                    }
                                }

                                record_btn = <Button> {
                                    width: 48, height: 48

                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        instance recording: 0.0
                                        instance pulse: 0.0

                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.circle(24.0, 24.0, 22.0);

                                            if self.recording > 0.5 {
                                                // Recording state: pulsing red
                                                let pulse_intensity = 0.8 + 0.2 * sin(self.pulse * 6.28);
                                                let color = mix((RED_500), (RED_600), pulse_intensity);
                                                sdf.fill(color);

                                                // Stop square icon
                                                sdf.rect(18.0, 18.0, 12.0, 12.0);
                                                sdf.fill((WHITE));
                                            } else {
                                                // Idle state: gray with red mic
                                                let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                                sdf.fill(bg);

                                                // Microphone icon (simplified)
                                                sdf.box(20.0, 14.0, 8.0, 12.0, 4.0);
                                                sdf.fill((RED_500));
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

                                recording_info = <View> {
                                    width: Fill, height: Fit
                                    flow: Down
                                    spacing: 4

                                    duration_label = <Label> {
                                        width: Fill, height: Fit
                                        draw_text: {
                                            instance dark_mode: 0.0
                                            text_style: <FONT_SEMIBOLD>{ font_size: 14.0 }
                                            fn get_color(self) -> vec4 {
                                                return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                            }
                                        }
                                        text: "Click to start recording"
                                    }

                                    progress_label = <Label> {
                                        width: Fill, height: Fit
                                        draw_text: {
                                            instance dark_mode: 0.0
                                            text_style: { font_size: 11.0 }
                                            fn get_color(self) -> vec4 {
                                                return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                            }
                                        }
                                        text: "Target: 5-10 minutes"
                                    }

                                    // Clear recording button — shown after recording/upload
                                    clear_recording_btn = <Button> {
                                        width: Fit, height: 22
                                        padding: {left: 6, right: 6}
                                        visible: false

                                        draw_bg: {
                                            instance dark_mode: 0.0
                                            instance hover: 0.0
                                            fn pixel(self) -> vec4 {
                                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 4.0);
                                                let base = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                                let hover_color = mix((RED_50), (RED_900), self.dark_mode);
                                                sdf.fill(mix(base, hover_color, self.hover));
                                                let border = mix((SLATE_300), (SLATE_500), self.dark_mode);
                                                sdf.stroke(border, 1.0);
                                                return sdf.result;
                                            }
                                        }

                                        draw_text: {
                                            instance dark_mode: 0.0
                                            instance hover: 0.0
                                            text_style: { font_size: 10.0 }
                                            fn get_color(self) -> vec4 {
                                                let base = mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                                                let hover_color = mix((RED_600), (RED_300), self.dark_mode);
                                                return mix(base, hover_color, self.hover);
                                            }
                                        }

                                        text: "× Clear"
                                    }
                                }

                                or_label = <Label> {
                                    width: Fit, height: Fit
                                    margin: {left: 8, right: 8}
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: { font_size: 11.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_TERTIARY), (TEXT_TERTIARY_DARK), self.dark_mode);
                                        }
                                    }
                                    text: "or"
                                }

                                upload_training_btn = <Button> {
                                    width: Fit, height: 36
                                    padding: {left: 16, right: 16, top: 8, bottom: 8}

                                    draw_bg: {
                                        instance dark_mode: 0.0
                                        fn pixel(self) -> vec4 {
                                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                            sdf.box(1., 1., self.rect_size.x - 2., self.rect_size.y - 2., 6.0);
                                            let bg = mix((SLATE_100), (SLATE_700), self.dark_mode);
                                            let border = mix((SLATE_300), (SLATE_500), self.dark_mode);
                                            sdf.fill(bg);
                                            sdf.stroke(border, 1.0);
                                            return sdf.result;
                                        }
                                    }

                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: <FONT_SEMIBOLD>{ font_size: 10.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                        }
                                    }

                                    text: "Upload Audio File"
                                }
                            }

                            uploaded_file_info = <View> {
                                width: Fill, height: Fit
                                visible: false

                                label = <Label> {
                                    width: Fill, height: Fit
                                    draw_text: {
                                        instance dark_mode: 0.0
                                        text_style: { font_size: 11.0 }
                                        fn get_color(self) -> vec4 {
                                            return mix((GREEN_600), (GREEN_400), self.dark_mode);
                                        }
                                    }
                                    text: ""
                                }
                            }

                            // Duration progress bar
                            duration_bar = <View> {
                                width: Fill, height: 6
                                visible: false
                                show_bg: true

                                draw_bg: {
                                    instance dark_mode: 0.0
                                    instance progress: 0.0

                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);

                                        // Background bar
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 3.0);
                                        let bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                        sdf.fill(bg);

                                        // Progress bar
                                        let progress_width = self.rect_size.x * self.progress;
                                        sdf.box(0., 0., progress_width, self.rect_size.y, 3.0);

                                        // Color gradient: green -> yellow -> red
                                        let color = mix((GREEN_500), (YELLOW_500), smoothstep(0.3, 0.7, self.progress));
                                        let color = mix(color, (RED_500), smoothstep(0.7, 1.0, self.progress));
                                        sdf.fill(color);

                                        return sdf.result;
                                    }
                                }
                            }
                        }

                        // Voice name + language (reuse from express mode)
                        voice_name_input = <LabeledInput> {
                            label = { text: "Voice Name" }
                            input = { empty_text: "Enter a name for this trained voice..." }
                        }

                        language_selector = <LanguageSelector> {}

                        // GPU warning
                        gpu_warning = <View> {
                            width: Fill, height: Fit
                            padding: 12
                            visible: false
                            show_bg: true
                            draw_bg: {
                                instance dark_mode: 0.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 6.0);
                                    // Warning colors: amber/orange for light/dark mode
                                    let bg = mix(#fef3c7, #78350f, self.dark_mode);
                                    sdf.fill(bg);
                                    let border = mix(#fcd34d, #b45309, self.dark_mode);
                                    sdf.stroke(border, 1.0);
                                    return sdf.result;
                                }
                            }

                            message = <Label> {
                                width: Fill, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: { font_size: 11.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix(#78350f, #fef3c7, self.dark_mode);
                                    }
                                }
                                text: "⚠️ No GPU detected. Training will be VERY slow (8-24 hours). Consider using a machine with CUDA GPU."
                            }
                        }

                        // Training progress section
                        training_progress_section = <View> {
                            width: Fill, height: Fit
                            flow: Down
                            spacing: 12
                            visible: false

                            stage_label = <Label> {
                                width: Fill, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "Training Status: Preparing..."
                            }

                            progress_bar = <View> {
                                width: Fill, height: 8
                                show_bg: true
                                draw_bg: {
                                    instance dark_mode: 0.0
                                    instance progress: 0.0

                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);

                                        // Background
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 4.0);
                                        let bg = mix((SLATE_200), (SLATE_600), self.dark_mode);
                                        sdf.fill(bg);

                                        // Progress
                                        let progress_width = self.rect_size.x * self.progress;
                                        sdf.box(0., 0., progress_width, self.rect_size.y, 4.0);
                                        sdf.fill((PRIMARY_500));

                                        return sdf.result;
                                    }
                                }
                            }

                        }
                    } // end pro_mode_content
                } // end body

                // Footer with action buttons
                footer = <View> {
                    width: Fill, height: Fit
                    padding: {left: 24, right: 24, top: 16, bottom: 20}
                    flow: Right
                    align: {y: 0.5}
                    spacing: 12

                    show_bg: true
                    draw_bg: {
                        instance dark_mode: 0.0
                        fn pixel(self) -> vec4 {
                            return mix((SLATE_50), (SLATE_700), self.dark_mode);
                        }
                    }

                    // Error message (left side)
                    error_message = <Label> {
                        width: Fill, height: Fit
                        draw_text: {
                            instance dark_mode: 0.0
                            text_style: { font_size: 13.0 }
                            fn get_color(self) -> vec4 {
                                return mix((RED_600), (RED_400), self.dark_mode);
                            }
                        }
                        text: ""
                    }

                    // Express mode buttons
                    express_actions = <View> {
                        width: Fit, height: Fit
                        flow: Right
                        spacing: 12
                        visible: true

                        cancel_btn = <ActionButton> {
                            text: "Cancel"
                            draw_bg: { primary: 0.0 }
                            draw_text: { primary: 0.0 }
                        }

                        save_btn = <ActionButton> {
                            text: "Save Voice"
                            draw_bg: { primary: 1.0 }
                            draw_text: { primary: 1.0 }
                        }
                    }

                    // Pro mode buttons
                    pro_actions = <View> {
                        width: Fit, height: Fit
                        flow: Right
                        spacing: 12
                        visible: false

                        cancel_training_btn = <ActionButton> {
                            text: "Cancel Training"
                            draw_bg: { primary: 0.0 }
                            draw_text: { primary: 0.0 }
                            visible: false
                        }

                        start_training_btn = <ActionButton> {
                            text: "创建克隆任务"
                            draw_bg: { primary: 1.0 }
                            draw_text: { primary: 1.0 }
                        }
                    }
                }
            } // end modal_content
            } // end modal_wrapper
        } // end modal_container

        // ASR loading overlay (shown when ASR bridge is not ready)
        // IMPORTANT: Must be after modal_container to appear on top in Overlay flow
        asr_loading_overlay = <View> {
            width: Fill, height: Fill
            flow: Overlay
            align: {x: 0.5, y: 0.5}
            visible: false

            // Semi-transparent backdrop
            loading_backdrop = <View> {
                width: Fill, height: Fill
                show_bg: true
                draw_bg: {
                    fn pixel(self) -> vec4 {
                        return vec4(0.0, 0.0, 0.0, 0.6);
                    }
                }
            }

            // Loading content
            loading_content = <RoundedView> {
                width: 300, height: Fit
                flow: Down
                padding: {left: 32, right: 32, top: 28, bottom: 28}
                spacing: 20
                align: {x: 0.5, y: 0.5}

                draw_bg: {
                    instance dark_mode: 0.0
                    border_radius: 12.0
                    fn pixel(self) -> vec4 {
                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                        let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                        sdf.fill(bg);
                        return sdf.result;
                    }
                }

                // Loading spinner (8 rotating dots)
                loading_spinner = <View> {
                    width: Fill, height: 60
                    align: {x: 0.5, y: 0.5}

                    show_bg: true
                    draw_bg: {
                        instance dark_mode: 0.0
                        instance rotation: 0.0

                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            let center = vec2(self.rect_size.x * 0.5, self.rect_size.y * 0.5);
                            let radius = 20.0;
                            let dot_radius = 3.0;
                            let num_dots = 8.0;

                            let base_color = mix((PRIMARY_500), (PRIMARY_400), self.dark_mode);
                            let mut result = vec4(0.0, 0.0, 0.0, 0.0);

                            // Draw 8 dots in a circle
                            for i in 0..8 {
                                let angle = (float(i) / num_dots) * 6.28318530718 + self.rotation * 6.28318530718;
                                let dot_x = center.x + cos(angle) * radius;
                                let dot_y = center.y + sin(angle) * radius;

                                // Calculate opacity based on position (creates rotation effect)
                                let opacity = (float(i) / num_dots) * 0.8 + 0.2;

                                sdf.circle(dot_x, dot_y, dot_radius);
                                let dot_color = vec4(base_color.rgb, base_color.a * opacity);
                                result = mix(result, dot_color, sdf.fill_keep(dot_color).a);
                            }

                            return result;
                        }
                    }
                }

                // Loading message
                loading_message = <Label> {
                    width: Fill, height: Fit
                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: <FONT_SEMIBOLD>{ font_size: 15.0 }
                        fn get_color(self) -> vec4 {
                            return mix((TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                        }
                    }
                    text: "Waiting for ASR Bridge..."
                }

                // Sub message
                loading_submessage = <Label> {
                    width: Fill, height: Fit
                    draw_text: {
                        instance dark_mode: 0.0
                        text_style: { font_size: 13.0 }
                        fn get_color(self) -> vec4 {
                            return mix((TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
                        }
                    }
                    text: "Voice recording will be available once ASR is ready."
                }
            }
        }
    }
}

/// Actions emitted by VoiceCloneModal
#[derive(Clone, Debug, DefaultNone)]
pub enum VoiceCloneModalAction {
    None,
    Closed,
    VoiceCreated(Voice),
    TaskCreated(crate::task_persistence::CloneTask),
    SendAudioToAsr {
        samples: Vec<f32>,
        sample_rate: u32,
        language: String,
        audio_path: std::path::PathBuf,
    },
}

#[derive(Live, Widget)]
pub struct VoiceCloneModal {
    #[deref]
    view: View,

    #[rust]
    dark_mode: f64,

    #[rust]
    selected_file: Option<PathBuf>,

    #[rust]
    is_drag_over: bool,

    #[rust]
    audio_info: Option<voice_persistence::AudioInfo>,

    #[rust]
    selected_language: String,

    #[rust]
    cloning_status: CloningStatus,

    #[rust]
    log_messages: Vec<String>,

    #[rust]
    preview_player: Option<TTSPlayer>,

    #[rust]
    preview_playing: bool,

    // Recording state
    #[rust]
    recording_status: RecordingStatus,

    #[rust]
    recording_buffer: Arc<Mutex<Vec<f32>>>,

    #[rust]
    is_recording: Arc<AtomicBool>,

    #[rust]
    recording_start_time: Option<std::time::Instant>,

    #[rust]
    recorded_audio_path: Option<PathBuf>,

    #[rust]
    recording_sample_rate: Arc<Mutex<u32>>,

    #[rust]
    processing_complete: Arc<AtomicBool>,

    #[rust]
    temp_audio_file: Arc<Mutex<Option<PathBuf>>>,

    // ASR state
    #[rust]
    pending_asr_audio: Option<(Vec<f32>, u32, PathBuf)>, // (samples, sample_rate, audio_path)

    #[rust]
    asr_sent: bool,

    #[rust]
    asr_sent_time: Option<std::time::Instant>,

    #[rust]
    shared_dora_state: Option<std::sync::Arc<moxin_dora_bridge::SharedDoraState>>,

    // ASR bridge readiness
    #[rust]
    asr_bridge_ready: bool,

    #[rust]
    loading_animation_start_time: Option<std::time::Instant>,

    // Transcription loading animation
    #[rust]
    transcription_animation_start_time: Option<std::time::Instant>,

    // Pro mode training fields
    #[rust]
    clone_mode: CloneMode,

    #[rust]
    current_task_id: Option<String>,

    #[rust]
    recording_for_training: bool,

    #[rust]
    training_audio_file: Option<PathBuf>,

    #[rust]
    training_audio_samples: Vec<f32>,

    #[rust]
    max_training_duration: f32,

    #[rust]
    training_recording_start: Option<Instant>,

    #[rust]
    gpu_check_done: bool,
    #[rust]
    has_gpu: bool,
}

impl LiveHook for VoiceCloneModal {
    fn after_new_from_doc(&mut self, _cx: &mut Cx) {
        self.clone_mode = CloneMode::Express;
        self.max_training_duration = 600.0; // 10 minutes
        self.recording_for_training = false;
    }
}

impl Widget for VoiceCloneModal {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);

        // Handle system file drag-and-drop
        match event {
            Event::Drag(de) => {
                // CRITICAL: Tell macOS we accept this drag.
                // Without setting response to Copy, macOS returns NSDragOperation::None
                // and will never fire performDragOperation (Event::Drop).
                *de.response.lock().unwrap() = DragResponse::Copy;

                if !self.is_drag_over {
                    self.is_drag_over = true;
                    self.set_drop_zone_highlight(cx, 1.0);
                    self.view.redraw(cx);
                }
            }
            Event::DragEnd => {
                if self.is_drag_over {
                    self.is_drag_over = false;
                    self.set_drop_zone_highlight(cx, 0.0);
                    self.view.redraw(cx);
                }
            }
            Event::Drop(de) => {
                self.is_drag_over = false;
                // Extract data from de before any mutable self borrow
                let audio_exts = ["wav", "mp3", "flac", "ogg", "m4a"];
                let dropped_path: Option<std::path::PathBuf> = de.items.iter()
                    .find(|item| {
                        if let DragItem::FilePath { path, .. } = item {
                            let lower = path.to_lowercase();
                            audio_exts.iter().any(|ext| lower.ends_with(ext))
                        } else {
                            false
                        }
                    })
                    .and_then(|item| {
                        if let DragItem::FilePath { path, .. } = item {
                            // macOS drag-and-drop passes paths as URL-encoded strings
                            // (e.g. Chinese filenames become %E8%AF%AD...).
                            // Decode percent-encoding before creating the PathBuf.
                            Some(std::path::PathBuf::from(percent_decode(path)))
                        } else {
                            None
                        }
                    });
                let had_items = !de.items.is_empty();
                // Reset visual feedback
                self.set_drop_zone_highlight(cx, 0.0);
                // Process the dropped file
                if let Some(path) = dropped_path {
                    eprintln!("[DragDrop] Audio file dropped: {}", path.display());
                    match self.clone_mode {
                        CloneMode::Express => self.handle_file_selected(cx, path),
                        CloneMode::Pro => self.handle_training_file_selected(cx, path),
                    }
                } else if had_items {
                    eprintln!("[DragDrop] Dropped items contain no supported audio file (wav/mp3/flac/ogg/m4a)");
                }
            }
            _ => {}
        }

        // Initialize defaults
        if self.selected_language.is_empty() {
            self.selected_language = "zh".to_string();
        }

        // Check ASR bridge readiness (only if not already ready and overlay is visible)
        if !self.asr_bridge_ready {
            if let Some(ref shared_state) = self.shared_dora_state {
                let status = shared_state.status.read();
                // Only check for ASR listener - audio-input is only needed for live recording
                let has_asr = status
                    .active_bridges
                    .iter()
                    .any(|b| b.contains("asr-listener") || b.contains("asr"));
                let has_audio_input = status
                    .active_bridges
                    .iter()
                    .any(|b| b.contains("audio-input"));

                if has_asr {
                    // ASR is now ready - hide loading overlay
                    self.asr_bridge_ready = true;
                    self.loading_animation_start_time = None; // Reset animation timer
                    self.view
                        .view(ids!(asr_loading_overlay))
                        .set_visible(cx, false);

                    if has_audio_input {
                        self.add_log(
                            cx,
                            "[INFO] ASR bridge is ready (recording + transcription available)!",
                        );
                    } else {
                        self.add_log(cx, "[INFO] ASR bridge is ready (transcription available, live recording disabled)!");
                    }
                    self.view.redraw(cx);
                } else {
                    // Still waiting - update loading animation based on time
                    if self.loading_animation_start_time.is_none() {
                        self.loading_animation_start_time = Some(std::time::Instant::now());
                    }

                    // Calculate rotation based on elapsed time (1 full rotation per 2 seconds)
                    let elapsed = self.loading_animation_start_time.unwrap().elapsed().as_secs_f64();
                    let rotation = (elapsed * 0.5) % 1.0; // 0.5 rotations per second

                    self.view
                        .view(ids!(asr_loading_overlay.loading_content.loading_spinner))
                        .apply_over(
                            cx,
                            live! {
                                draw_bg: { rotation: (rotation) }
                            },
                        );

                    // Keep redrawing to animate
                    self.view.redraw(cx);
                }
            } else {
                // DEBUG: shared_dora_state is None
                if self.loading_animation_start_time.is_none() {
                    self.loading_animation_start_time = Some(std::time::Instant::now());
                    self.add_log(cx, "[DEBUG] shared_dora_state is None in handle_event");
                }

                // Still animate even without state, based on time
                let elapsed = self.loading_animation_start_time.unwrap().elapsed().as_secs_f64();
                let rotation = (elapsed * 0.5) % 1.0; // 0.5 rotations per second

                self.view
                    .view(ids!(asr_loading_overlay.loading_content.loading_spinner))
                    .apply_over(
                        cx,
                        live! {
                            draw_bg: { rotation: (rotation) }
                        },
                    );

                self.view.redraw(cx);
            }
        }

        // Check preview playback completion
        if self.preview_playing {
            if let Some(player) = &self.preview_player {
                if player.check_playback_finished() {
                    self.preview_playing = false;
                    self.update_preview_button(cx, false);
                    self.add_log(cx, "[INFO] Preview playback finished");
                }
            }
        }

        // Check if recording processing is complete
        if self.processing_complete.load(Ordering::Relaxed) {
            self.processing_complete.store(false, Ordering::Relaxed);

            // Load the recorded audio file
            let path = { self.temp_audio_file.lock().take() };

            if let Some(path) = path {
                self.add_log(cx, "[INFO] Loading recorded audio...");
                // Validate the file first
                self.handle_file_selected(cx, path.clone());
                // Then start ASR transcription
                self.transcribe_audio(cx, &path);
            }
        }

        // Handle ASR: request parent to send audio if pending
        if !self.asr_sent {
            if let Some((samples, sample_rate, audio_path)) = self.pending_asr_audio.clone() {
                self.add_log(
                    cx,
                    &format!(
                        "[INFO] Requesting parent to send {} samples to ASR...",
                        samples.len()
                    ),
                );

                // Send action to parent screen to send audio to ASR
                cx.widget_action(
                    self.widget_uid(),
                    &scope.path,
                    VoiceCloneModalAction::SendAudioToAsr {
                        samples,
                        sample_rate,
                        language: self.selected_language.clone(),
                        audio_path,
                    },
                );

                self.asr_sent = true;
                self.asr_sent_time = Some(std::time::Instant::now());

                // Delete any stale ASR result file
                let result_path = dirs::home_dir()
                    .unwrap_or_default()
                    .join(".dora")
                    .join("asr_result.json");
                let _ = std::fs::remove_file(&result_path);

                self.add_log(
                    cx,
                    "[INFO] Audio send request sent, waiting for transcription...",
                );
            }
        }

        // Poll for ASR transcription result
        if self.asr_sent && self.recording_status == RecordingStatus::Transcribing {
            // Try primary path: shared_dora_state (Dora event routing)
            let transcription_result = self
                .shared_dora_state
                .as_ref()
                .and_then(|shared| shared.asr_transcription.read_if_dirty());

            // Try fallback path: ASR result file (for macOS where Dora event routing may fail)
            let (language, text) = if let Some(Some((lang, txt))) = transcription_result {
                (lang, txt)
            } else if let Some(sent_time) = self.asr_sent_time {
                // Only check file after 2 seconds (give ASR time to process)
                if sent_time.elapsed() > std::time::Duration::from_secs(2) {
                    let result_path = dirs::home_dir()
                        .unwrap_or_default()
                        .join(".dora")
                        .join("asr_result.json");
                    if let Ok(content) = std::fs::read_to_string(&result_path) {
                        if let Ok(json) = serde_json::from_str::<serde_json::Value>(&content) {
                            let lang = json.get("language").and_then(|v| v.as_str()).unwrap_or("auto").to_string();
                            let txt = json.get("text").and_then(|v| v.as_str()).unwrap_or("").to_string();
                            if !txt.is_empty() {
                                // Delete the file to avoid re-reading stale results
                                let _ = std::fs::remove_file(&result_path);
                                (lang, txt)
                            } else {
                                ("".into(), "".into())
                            }
                        } else {
                            ("".into(), "".into())
                        }
                    } else {
                        ("".into(), "".into())
                    }
                } else {
                    ("".into(), "".into())
                }
            } else {
                ("".into(), "".into())
            };

            if !text.is_empty() {
                self.add_log(
                    cx,
                    &format!("[INFO] Transcription received ({}): {}", language, text),
                );

                // Auto-fill prompt text field (Express flow behavior).
                self.view
                    .text_input(ids!(
                        modal_container
                            .modal_wrapper
                            .modal_content
                            .body
                            .prompt_text_input
                            .input
                    ))
                    .set_text(cx, &text);

                self.recording_status = RecordingStatus::Completed;
                self.pending_asr_audio = None;
                self.asr_sent = false;
                self.asr_sent_time = None;

                // Reset animation timer and hide transcription loading overlay
                self.transcription_animation_start_time = None;
                if self.clone_mode == CloneMode::Express {
                    self.view
                        .view(ids!(
                            modal_container
                                .modal_wrapper
                                .modal_content
                                .body
                                .prompt_text_container
                                .transcription_loading_overlay
                        ))
                        .set_visible(cx, false);
                }

                // Clear the ASR result
                if let Some(ref shared) = self.shared_dora_state {
                    shared.asr_transcription.set(None);
                }
            }
        }

        // Keep redrawing while processing to check for completion and animate spinner
        if self.recording_status == RecordingStatus::Transcribing {
            // Update transcription loading animation based on time
            if self.transcription_animation_start_time.is_none() {
                self.transcription_animation_start_time = Some(std::time::Instant::now());
            }

            // Calculate rotation based on elapsed time (1 full rotation per 2 seconds)
            let elapsed = self.transcription_animation_start_time.unwrap().elapsed().as_secs_f64();
            let rotation = (elapsed * 0.5) % 1.0; // 0.5 rotations per second

            if self.clone_mode == CloneMode::Express {
                self.view
                    .view(ids!(
                        modal_container
                            .modal_wrapper
                            .modal_content
                            .body
                            .prompt_text_container
                            .transcription_loading_overlay
                            .loading_content
                            .loading_spinner
                    ))
                    .apply_over(
                        cx,
                        live! {
                            draw_bg: { rotation: (rotation) }
                        },
                    );
            }

            self.view.redraw(cx);
        }

        // Handle overlay click to close (MUST be BEFORE Event::Actions early return)
        let overlay = self.view.view(ids!(overlay));
        match event.hits(cx, overlay.area()) {
            Hit::FingerUp(fe) if fe.was_tap() => {
                // Check if click is outside modal content
                let modal_content = self
                    .view
                    .view(ids!(modal_container.modal_wrapper.modal_content));
                if !modal_content.area().rect(cx).contains(fe.abs) {
                    self.close(cx, scope);
                    return;
                }
            }
            _ => {}
        }

        // Handle close button (MUST be BEFORE Event::Actions early return)
        let close_btn = self.view.button(ids!(
            modal_container.modal_wrapper.modal_content.header.close_btn
        ));
        match event.hits(cx, close_btn.area()) {
            Hit::FingerUp(fe) if fe.was_tap() => {
                self.close(cx, scope);
                return;
            }
            _ => {}
        }

        // Handle cancel button
        let cancel_btn = self.view.button(ids!(
            modal_container
                .modal_wrapper
                .modal_content
                .footer
                .cancel_btn
        ));
        match event.hits(cx, cancel_btn.area()) {
            Hit::FingerUp(fe) if fe.was_tap() => {
                self.close(cx, scope);
                return;
            }
            _ => {}
        }

        // Handle record button
        let record_btn = self.view.button(ids!(
            modal_container
                .modal_wrapper
                .modal_content
                .body
                .file_selector
                .file_row
                .record_btn
        ));
        match event.hits(cx, record_btn.area()) {
            Hit::FingerUp(fe) if fe.was_tap() => {
                self.toggle_recording(cx);
            }
            _ => {}
        }

        // Handle file select button
        let select_btn = self.view.button(ids!(
            modal_container
                .modal_wrapper
                .modal_content
                .body
                .file_selector
                .file_row
                .select_btn
        ));
        match event.hits(cx, select_btn.area()) {
            Hit::FingerUp(fe) if fe.was_tap() => {
                self.open_file_dialog(cx);
            }
            _ => {}
        }

        // Handle preview button
        let preview_btn = self.view.button(ids!(
            modal_container
                .modal_wrapper
                .modal_content
                .body
                .file_selector
                .file_row
                .preview_btn
        ));
        match event.hits(cx, preview_btn.area()) {
            Hit::FingerUp(fe) if fe.was_tap() => {
                self.toggle_preview(cx);
            }
            _ => {}
        }

        // Handle express clear recording button
        let express_clear_btn = self.view.button(ids!(
            modal_container.modal_wrapper.modal_content.body.file_selector.file_row.clear_recording_btn
        ));
        match event.hits(cx, express_clear_btn.area()) {
            Hit::FingerUp(fe) if fe.was_tap() => {
                self.clear_express_recording(cx);
            }
            _ => {}
        }

        // Handle language buttons
        let zh_btn = self.view.button(ids!(
            modal_container
                .modal_wrapper
                .modal_content
                .body
                .language_selector
                .lang_row
                .zh_btn
        ));
        match event.hits(cx, zh_btn.area()) {
            Hit::FingerUp(fe) if fe.was_tap() => {
                self.selected_language = "zh".to_string();
                self.update_language_buttons(cx);
            }
            _ => {}
        }

        let en_btn = self.view.button(ids!(
            modal_container
                .modal_wrapper
                .modal_content
                .body
                .language_selector
                .lang_row
                .en_btn
        ));
        match event.hits(cx, en_btn.area()) {
            Hit::FingerUp(fe) if fe.was_tap() => {
                self.selected_language = "en".to_string();
                self.update_language_buttons(cx);
            }
            _ => {}
        }

        // Handle save button (Express mode)
        let save_btn = self.view.button(ids!(
            modal_container.modal_wrapper.modal_content.footer.express_actions.save_btn
        ));
        match event.hits(cx, save_btn.area()) {
            Hit::FingerUp(fe) if fe.was_tap() => {
                self.save_voice(cx, scope);
            }
            _ => {}
        }

        // Pro mode: Training recording button
        if self.clone_mode == CloneMode::Pro {
            let record_btn = self.view.button(ids!(
                modal_container.modal_wrapper.modal_content.body.pro_mode_content
                .training_recording_section.record_row.record_btn
            ));
            match event.hits(cx, record_btn.area()) {
                Hit::FingerUp(fe) if fe.was_tap() => {
                    self.toggle_training_recording(cx);
                }
                _ => {}
            }

            // Upload training audio button
            let upload_training_btn = self.view.button(ids!(
                modal_container.modal_wrapper.modal_content.body.pro_mode_content
                .training_recording_section.record_row.upload_training_btn
            ));
            match event.hits(cx, upload_training_btn.area()) {
                Hit::FingerUp(fe) if fe.was_tap() => {
                    self.open_training_file_dialog(cx);
                }
                _ => {}
            }

            // Start training button (now creates a Pending task)
            let start_training_btn = self.view.button(ids!(
                modal_container.modal_wrapper.modal_content.footer.pro_actions.start_training_btn
            ));
            match event.hits(cx, start_training_btn.area()) {
                Hit::FingerUp(fe) if fe.was_tap() => {
                    self.start_training(cx, scope);
                }
                _ => {}
            }

            // Pro mode clear recording button
            let pro_clear_btn = self.view.button(ids!(
                modal_container.modal_wrapper.modal_content.body.pro_mode_content
                .training_recording_section.record_row.recording_info.clear_recording_btn
            ));
            match event.hits(cx, pro_clear_btn.area()) {
                Hit::FingerUp(fe) if fe.was_tap() => {
                    self.clear_pro_recording(cx);
                }
                _ => {}
            }
        }

        // Extract actions - keep for any remaining action-based handling
        let _actions = match event {
            Event::Actions(actions) => actions.as_slice(),
            _ => return,
        };
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}

impl VoiceCloneModal {
    /// Update the drag-over visual highlight on the drop zone for the current mode.
    fn set_drop_zone_highlight(&mut self, cx: &mut Cx, value: f64) {
        match self.clone_mode {
            CloneMode::Express => {
                self.view
                    .view(ids!(modal_container.modal_wrapper.modal_content.body.file_selector.file_row))
                    .apply_over(cx, live! { draw_bg: { is_drag_over: (value) } });
            }
            CloneMode::Pro => {
                self.view
                    .view(ids!(modal_container.modal_wrapper.modal_content.body.pro_mode_content.training_recording_section.record_row))
                    .apply_over(cx, live! { draw_bg: { is_drag_over: (value) } });
            }
        }
    }

    /// Clean up old temporary recording files from previous sessions
    ///
    /// Removes temp files older than 1 hour to prevent disk buildup.
    /// Should be called on app startup or modal initialization.
    pub fn cleanup_old_temp_files() {
        use std::fs;
        use std::time::{Duration, SystemTime};

        let temp_dir = std::env::temp_dir();

        // Try to read temp directory
        let entries = match fs::read_dir(&temp_dir) {
            Ok(entries) => entries,
            Err(e) => {
                eprintln!("[VoiceClone] Failed to read temp dir for cleanup: {}", e);
                return;
            }
        };

        let threshold = SystemTime::now() - Duration::from_secs(3600); // 1 hour

        for entry in entries.flatten() {
            let path = entry.path();

            // Only process our temp files
            if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                if name.starts_with("voice_clone_recording_") && name.ends_with(".wav") {
                    // Check file age
                    if let Ok(metadata) = fs::metadata(&path) {
                        if let Ok(modified) = metadata.modified() {
                            if modified < threshold {
                                // File is older than threshold, remove it
                                if let Err(e) = fs::remove_file(&path) {
                                    eprintln!("[VoiceClone] Failed to remove old temp file {:?}: {}", path, e);
                                } else {
                                    eprintln!("[VoiceClone] Cleaned up old temp file: {:?}", path);
                                }
                            }
                        }
                    }
                }
            }
        }
    }

    /// Returns true if Qwen3 is the active zero-shot (express) backend.
    /// Reads MOXIN_INFERENCE_BACKEND which is set by the main backend switcher.
    fn is_qwen3_zero_shot() -> bool {
        std::env::var("MOXIN_INFERENCE_BACKEND")
            .ok()
            .map(|v| v.to_ascii_lowercase().contains("qwen3"))
            .unwrap_or(false)
    }

    /// Returns "option_b" if Qwen3/MLX is the active pro-mode training backend, "option_a" otherwise.
    /// Reads MOXIN_TRAINING_BACKEND which is set by the Experiments settings dropdown.
    fn get_training_backend() -> String {
        std::env::var("MOXIN_TRAINING_BACKEND")
            .ok()
            .map(|v| v.to_ascii_lowercase())
            .map(|v| {
                if matches!(v.as_str(), "option_b" | "rust" | "mlx" | "b") {
                    "option_b".to_string()
                } else {
                    "option_a".to_string()
                }
            })
            .unwrap_or_else(|| "option_a".to_string())
    }

    fn add_log(&mut self, cx: &mut Cx, message: &str) {
        self.log_messages.push(message.to_string());
        let log_text = self.log_messages.join("\n");
        self.view
            .label(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .progress_log
                    .log_scroll
                    .log_content
            ))
            .set_text(cx, &log_text);
        self.view.redraw(cx);
    }

    fn clear_log(&mut self, cx: &mut Cx) {
        self.log_messages.clear();
        self.view
            .label(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .progress_log
                    .log_scroll
                    .log_content
            ))
            .set_text(cx, "Ready to clone voice...");
        self.view.redraw(cx);
    }

    fn show_error(&mut self, cx: &mut Cx, message: &str) {
        self.view
            .label(ids!(modal_container.modal_wrapper.modal_content.footer.error_message))
            .set_text(cx, message);
        self.view.redraw(cx);
    }

    fn clear_error(&mut self, cx: &mut Cx) {
        self.view
            .label(ids!(modal_container.modal_wrapper.modal_content.footer.error_message))
            .set_text(cx, "");
        self.view.redraw(cx);
    }

    fn open_file_dialog(&mut self, cx: &mut Cx) {
        // Use rfd for native file dialog
        let dialog = rfd::FileDialog::new()
            .add_filter("Audio Files", &["wav", "mp3", "flac", "ogg"])
            .add_filter("WAV Files", &["wav"])
            .set_title("Select Reference Audio");

        if let Some(path) = dialog.pick_file() {
            self.handle_file_selected(cx, path);
        }
    }

    fn handle_file_selected(&mut self, cx: &mut Cx, path: PathBuf) {
        // Update file name label
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown");
        self.view
            .label(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .file_selector
                    .file_row
                    .file_name
            ))
            .set_text(cx, file_name);

        // Validate audio file
        self.add_log(cx, "[INFO] Validating audio file...");

        match voice_persistence::validate_audio_file(&path) {
            Ok(info) => {
                self.add_log(
                    cx,
                    &format!(
                        "[INFO] Audio OK: {:.1}s, {}Hz, {} channels",
                        info.duration_secs, info.sample_rate, info.channels
                    ),
                );

                for warning in &info.warnings {
                    self.add_log(cx, &format!("[WARN] {}", warning));
                }

                // Update audio info label
                let info_text = format!(
                    "Duration: {:.1}s | Sample rate: {}Hz | Channels: {}",
                    info.duration_secs, info.sample_rate, info.channels
                );
                self.view
                    .label(ids!(
                        modal_container
                            .modal_wrapper
                            .modal_content
                            .body
                            .file_selector
                            .audio_info
                    ))
                    .set_text(cx, &info_text);

                self.audio_info = Some(info);
                self.selected_file = Some(path.clone());
                self.clear_error(cx);

                // Show preview button
                self.view
                    .button(ids!(
                        modal_container
                            .modal_wrapper
                            .modal_content
                            .body
                            .file_selector
                            .file_row
                            .preview_btn
                    ))
                    .set_visible(cx, true);

                // Show clear button so user can discard and re-record/re-select
                self.view
                    .button(ids!(
                        modal_container
                            .modal_wrapper
                            .modal_content
                            .body
                            .file_selector
                            .file_row
                            .clear_recording_btn
                    ))
                    .set_visible(cx, true);

                // Trigger ASR transcription for uploaded file (same as recording flow)
                // Skip if already transcribing (avoids re-trigger when called from ASR result handler)
                if self.recording_status != RecordingStatus::Transcribing {
                    self.transcribe_audio(cx, &path);
                }
            }
            Err(e) => {
                self.selected_file = None;
                self.audio_info = None;

                // Reset file name label
                self.view
                    .label(ids!(
                        modal_container
                            .modal_wrapper
                            .modal_content
                            .body
                            .file_selector
                            .file_row
                            .file_name
                    ))
                    .set_text(cx, "No file selected · drag audio here");

                // Clear audio info label
                self.view
                    .label(ids!(
                        modal_container
                            .modal_wrapper
                            .modal_content
                            .body
                            .file_selector
                            .audio_info
                    ))
                    .set_text(cx, "");

                // Hide preview and clear buttons
                self.view
                    .button(ids!(
                        modal_container
                            .modal_wrapper
                            .modal_content
                            .body
                            .file_selector
                            .file_row
                            .preview_btn
                    ))
                    .set_visible(cx, false);
                self.view
                    .button(ids!(
                        modal_container
                            .modal_wrapper
                            .modal_content
                            .body
                            .file_selector
                            .file_row
                            .clear_recording_btn
                    ))
                    .set_visible(cx, false);

                // Show error to user
                self.show_error(cx, &e);
            }
        }

        self.view.redraw(cx);
    }

    fn toggle_preview(&mut self, cx: &mut Cx) {
        if self.preview_playing {
            // Stop preview
            if let Some(player) = &self.preview_player {
                player.stop();
            }
            self.preview_playing = false;
            self.update_preview_button(cx, false);
            return;
        }

        // Play preview
        if let Some(path) = &self.selected_file {
            // Initialize player if needed
            if self.preview_player.is_none() {
                self.preview_player = Some(TTSPlayer::new());
            }

            // Load and play audio
            match self.load_wav_for_preview(path) {
                Ok(samples) => {
                    if let Some(player) = &self.preview_player {
                        player.write_audio(&samples);
                    }
                    self.preview_playing = true;
                    self.update_preview_button(cx, true);
                    self.add_log(cx, "[INFO] Playing preview...");
                }
                Err(e) => {
                    self.add_log(cx, &format!("[ERROR] Failed to play: {}", e));
                }
            }
        }
    }

    fn load_wav_for_preview(&self, path: &PathBuf) -> Result<Vec<f32>, String> {
        use hound::WavReader;

        let reader = WavReader::open(path).map_err(|e| format!("Failed to open WAV: {}", e))?;

        let spec = reader.spec();
        let sample_rate = spec.sample_rate;
        let channels = spec.channels as usize;

        // Read samples
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

        // Convert to mono
        let mono_samples: Vec<f32> = if channels > 1 {
            samples
                .chunks(channels)
                .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                .collect()
        } else {
            samples
        };

        // Resample to 32000 Hz if needed
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

    fn update_preview_button(&mut self, cx: &mut Cx, playing: bool) {
        let playing_val = if playing { 1.0 } else { 0.0 };
        self.view
            .button(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .file_selector
                    .file_row
                    .preview_btn
            ))
            .apply_over(
                cx,
                live! {
                    draw_bg: { playing: (playing_val) }
                },
            );
        self.view.redraw(cx);
    }

    fn update_language_buttons(&mut self, cx: &mut Cx) {
        let zh_selected = if self.selected_language == "zh" {
            1.0
        } else {
            0.0
        };
        let en_selected = if self.selected_language == "en" {
            1.0
        } else {
            0.0
        };

        self.view
            .button(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .language_selector
                    .lang_row
                    .zh_btn
            ))
            .apply_over(
                cx,
                live! {
                    draw_bg: { selected: (zh_selected) }
                    draw_text: { selected: (zh_selected) }
                },
            );

        self.view
            .button(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .language_selector
                    .lang_row
                    .en_btn
            ))
            .apply_over(
                cx,
                live! {
                    draw_bg: { selected: (en_selected) }
                    draw_text: { selected: (en_selected) }
                },
            );

        self.view.redraw(cx);
    }

    fn save_voice(&mut self, cx: &mut Cx, scope: &mut Scope) {
        // Clear previous error message
        self.clear_error(cx);

        // Validate inputs
        let voice_name = self
            .view
            .text_input(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .voice_name_input
                    .input
            ))
            .text();

        if voice_name.trim().is_empty() {
            self.show_error(cx, "Please enter a voice name");
            return;
        }

        let prompt_text = self
            .view
            .text_input(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .prompt_text_input
                    .input
            ))
            .text();

        if prompt_text.trim().is_empty() {
            self.show_error(cx, "Please enter the reference text");
            return;
        }

        let source_path = match &self.selected_file {
            Some(p) => p.clone(),
            None => {
                self.show_error(cx, "Please select a reference audio file");
                return;
            }
        };

        // Validate audio duration — limits differ by backend:
        // option_a (GPT-SoVITS zero-shot): 3-10 seconds
        // option_b (Qwen3 zero-shot): 3-30 seconds
        let express_max_secs: f32 = if Self::is_qwen3_zero_shot() { 30.0 } else { 10.0 };
        if let Some(ref info) = self.audio_info {
            if info.duration_secs < 3.0 {
                self.show_error(
                    cx,
                    &format!(
                        "Audio too short ({:.1}s). Required: 3-{:.0} seconds",
                        info.duration_secs, express_max_secs
                    ),
                );
                return;
            }
            if info.duration_secs > express_max_secs {
                self.show_error(
                    cx,
                    &format!(
                        "Audio too long ({:.1}s). Required: 3-{:.0} seconds",
                        info.duration_secs, express_max_secs
                    ),
                );
                return;
            }
        } else {
            self.show_error(cx, "Audio file not validated. Please re-select the file");
            return;
        }

        self.cloning_status = CloningStatus::ValidatingAudio;
        self.add_log(cx, "[INFO] Starting voice creation...");

        // Generate unique voice ID
        let voice_id = voice_persistence::generate_voice_id(&voice_name);
        self.add_log(cx, &format!("[INFO] Voice ID: {}", voice_id));

        // Copy audio file
        self.cloning_status = CloningStatus::CopyingFiles;
        self.add_log(cx, "[INFO] Copying reference audio...");

        let relative_path = match voice_persistence::copy_reference_audio(&voice_id, &source_path) {
            Ok(path) => path,
            Err(e) => {
                self.add_log(cx, &format!("[ERROR] {}", e));
                self.cloning_status = CloningStatus::Error(e);
                return;
            }
        };

        self.add_log(cx, "[INFO] Audio file copied successfully");

        // Create voice object
        let voice = Voice::new_custom(
            voice_id.clone(),
            voice_name.trim().to_string(),
            self.selected_language.clone(),
            relative_path,
            prompt_text.trim().to_string(),
        );

        // Save to config
        self.cloning_status = CloningStatus::SavingConfig;
        self.add_log(cx, "[INFO] Saving voice configuration...");

        match voice_persistence::add_custom_voice(voice.clone()) {
            Ok(_) => {
                self.add_log(cx, "");
                self.add_log(cx, "✓ Voice created successfully!");
                self.add_log(cx, "You can now close this dialog.");
                self.cloning_status = CloningStatus::Completed;

                // Update save button to show completion
                self.view
                    .button(ids!(
                        modal_container.modal_wrapper.modal_content.footer.save_btn
                    ))
                    .set_text(cx, "✓ Done");

                // Emit action
                cx.widget_action(
                    self.widget_uid(),
                    &scope.path,
                    VoiceCloneModalAction::VoiceCreated(voice),
                );

                // Close the modal after successful save
                self.close(cx, scope);
            }
            Err(e) => {
                self.add_log(cx, &format!("[ERROR] Failed to save: {}", e));
                self.cloning_status = CloningStatus::Error(e);
            }
        }
    }

    fn close(&mut self, cx: &mut Cx, scope: &mut Scope) {
        // Stop any recording
        if self.is_recording.load(Ordering::Relaxed) {
            self.is_recording.store(false, Ordering::Relaxed);
        }
        self.recording_status = RecordingStatus::Idle;

        // Stop any preview playing
        if let Some(player) = &self.preview_player {
            player.stop();
        }
        self.preview_playing = false;

        // Reset state
        self.selected_file = None;
        self.audio_info = None;
        self.cloning_status = CloningStatus::Idle;
        self.recorded_audio_path = None;
        self.clear_log(cx);
        self.clear_error(cx);

        // Reset form fields
        self.view
            .text_input(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .voice_name_input
                    .input
            ))
            .set_text(cx, "");
        self.view
            .text_input(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .prompt_text_input
                    .input
            ))
            .set_text(cx, "");
        self.view
            .label(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .file_selector
                    .file_row
                    .file_name
            ))
            .set_text(cx, "No file selected · drag audio here");
        self.view
            .label(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .file_selector
                    .audio_info
            ))
            .set_text(cx, "");
        self.view
            .button(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .file_selector
                    .file_row
                    .preview_btn
            ))
            .set_visible(cx, false);
        self.view
            .button(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .file_selector
                    .file_row
                    .clear_recording_btn
            ))
            .set_visible(cx, false);

        // Reset record button
        self.update_record_button(cx, false);

        // Hide modal
        self.view.set_visible(cx, false);

        // Emit closed action
        cx.widget_action(
            self.widget_uid(),
            &scope.path,
            VoiceCloneModalAction::Closed,
        );
    }

    fn toggle_recording(&mut self, cx: &mut Cx) {
        if self.is_recording.load(Ordering::Relaxed) {
            self.stop_recording(cx);
        } else {
            self.start_recording(cx);
        }
    }

    /// Clear the Express mode recording / selected file, resetting to idle state.
    fn clear_express_recording(&mut self, cx: &mut Cx) {
        // Stop any preview playing
        if let Some(player) = &self.preview_player {
            player.stop();
        }
        self.preview_playing = false;
        self.update_preview_button(cx, false);

        // Clear state
        self.selected_file = None;
        self.audio_info = None;
        self.recording_status = RecordingStatus::Idle;
        self.recorded_audio_path = None;

        // Reset file name label
        self.view.label(ids!(
            modal_container.modal_wrapper.modal_content.body.file_selector.file_row.file_name
        )).set_text(cx, "No file selected · drag audio here");

        // Clear audio info label
        self.view.label(ids!(
            modal_container.modal_wrapper.modal_content.body.file_selector.audio_info
        )).set_text(cx, "");

        // Hide preview and clear buttons
        self.view.button(ids!(
            modal_container.modal_wrapper.modal_content.body.file_selector.file_row.preview_btn
        )).set_visible(cx, false);
        self.view.button(ids!(
            modal_container.modal_wrapper.modal_content.body.file_selector.file_row.clear_recording_btn
        )).set_visible(cx, false);

        self.update_record_button(cx, false);
        self.clear_error(cx);
        self.view.redraw(cx);
    }

    /// Clear the Pro mode recording / uploaded file, resetting to idle state.
    fn clear_pro_recording(&mut self, cx: &mut Cx) {
        self.training_audio_file = None;
        self.training_audio_samples.clear();

        // Reset duration and progress labels
        self.view.label(ids!(
            modal_container.modal_wrapper.modal_content.body.pro_mode_content
            .training_recording_section.record_row.recording_info.duration_label
        )).set_text(cx, "Click to start recording");

        self.view.label(ids!(
            modal_container.modal_wrapper.modal_content.body.pro_mode_content
            .training_recording_section.record_row.recording_info.progress_label
        )).set_text(cx, "Target: 5-10 minutes");

        // Hide the clear button
        self.view.button(ids!(
            modal_container.modal_wrapper.modal_content.body.pro_mode_content
            .training_recording_section.record_row.recording_info.clear_recording_btn
        )).set_visible(cx, false);

        // Hide uploaded file info
        self.view.view(ids!(
            modal_container.modal_wrapper.modal_content.body.pro_mode_content
            .training_recording_section.uploaded_file_info
        )).set_visible(cx, false);

        // Disable the start training button
        self.view.button(ids!(
            modal_container.modal_wrapper.modal_content.footer.pro_actions.start_training_btn
        )).set_enabled(cx, false);

        self.view.redraw(cx);
    }

    fn start_recording(&mut self, cx: &mut Cx) {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        self.add_log(cx, "[INFO] Starting microphone recording...");
        let max_s = if Self::is_qwen3_zero_shot() { 30 } else { 10 };
        self.add_log(cx, &format!("[INFO] Speak clearly for 3-{} seconds", max_s));

        // Initialize buffer and sample rate
        self.recording_buffer = Arc::new(Mutex::new(Vec::new()));
        self.is_recording = Arc::new(AtomicBool::new(true));
        self.recording_sample_rate = Arc::new(Mutex::new(16000)); // Default, will be updated
        self.recording_start_time = Some(std::time::Instant::now());
        self.recording_status = RecordingStatus::Recording;

        // Update UI
        self.update_record_button(cx, true);

        // Start recording in background thread
        let buffer = Arc::clone(&self.recording_buffer);
        let is_recording = Arc::clone(&self.is_recording);
        let sample_rate_store = Arc::clone(&self.recording_sample_rate);

        std::thread::spawn(move || {
            let host = cpal::default_host();

            let device = match host.default_input_device() {
                Some(d) => d,
                None => {
                    eprintln!("[VoiceClone] No input device found");
                    is_recording.store(false, Ordering::Relaxed);
                    return;
                }
            };

            eprintln!("[VoiceClone] Using input device: {:?}", device.name());

            // Get device's default/supported config instead of forcing 16kHz
            let supported_config = match device.default_input_config() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[VoiceClone] Failed to get default input config: {}", e);
                    is_recording.store(false, Ordering::Relaxed);
                    return;
                }
            };

            let sample_rate = supported_config.sample_rate().0;
            let channels = supported_config.channels() as usize;
            eprintln!(
                "[VoiceClone] Using config: {}Hz, {} channels",
                sample_rate, channels
            );

            // Store the actual sample rate for later resampling
            *sample_rate_store.lock() = sample_rate;

            let config: cpal::StreamConfig = supported_config.into();

            let buffer_clone = Arc::clone(&buffer);
            let is_recording_clone = Arc::clone(&is_recording);

            // We'll store raw samples and resample later
            let stream = match device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if is_recording_clone.load(Ordering::Relaxed) {
                        // Convert to mono if stereo
                        if channels > 1 {
                            let mono: Vec<f32> = data
                                .chunks(channels)
                                .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                                .collect();
                            buffer_clone.lock().extend_from_slice(&mono);
                        } else {
                            buffer_clone.lock().extend_from_slice(data);
                        }
                    }
                },
                |err| eprintln!("[VoiceClone] Recording error: {}", err),
                None,
            ) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[VoiceClone] Failed to build input stream: {}", e);
                    is_recording.store(false, Ordering::Relaxed);
                    return;
                }
            };

            if let Err(e) = stream.play() {
                eprintln!("[VoiceClone] Failed to start stream: {}", e);
                is_recording.store(false, Ordering::Relaxed);
                return;
            }

            eprintln!("[VoiceClone] Recording started at {}Hz", sample_rate);

            // Keep stream alive while recording (max 12 seconds)
            let max_duration = std::time::Duration::from_secs(12);
            let start = std::time::Instant::now();

            while is_recording.load(Ordering::Relaxed) && start.elapsed() < max_duration {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            // Auto-stop after max duration
            is_recording.store(false, Ordering::Relaxed);
            eprintln!("[VoiceClone] Recording stopped ({}Hz mono)", sample_rate);
        });

        self.view.redraw(cx);
    }

    fn stop_recording(&mut self, cx: &mut Cx) {
        self.is_recording.store(false, Ordering::Relaxed);
        self.update_record_button(cx, false);

        // Calculate duration
        let duration = self
            .recording_start_time
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);

        self.add_log(cx, &format!("[INFO] Recording stopped ({:.1}s)", duration));

        // Validate duration
        if duration < 3.0 {
            self.add_log(
                cx,
                "[ERROR] Recording too short. Please record at least 3 seconds.",
            );
            self.recording_status = RecordingStatus::Error("Recording too short".to_string());
            self.view.redraw(cx);
            return;
        }

        let express_max_secs: f32 = if Self::is_qwen3_zero_shot() { 30.0 } else { 10.0 };
        if duration > express_max_secs {
            self.add_log(cx, &format!("[WARN] Recording over {:.0}s will be trimmed to {:.0}s", express_max_secs, express_max_secs));
        }

        self.recording_status = RecordingStatus::Transcribing;
        self.add_log(cx, "[INFO] Processing recorded audio...");
        self.view.redraw(cx);

        // Process in background thread to avoid blocking UI
        let buffer = Arc::clone(&self.recording_buffer);
        let sample_rate_store = Arc::clone(&self.recording_sample_rate);
        let processing_complete = Arc::clone(&self.processing_complete);
        let temp_file_store = Arc::clone(&self.temp_audio_file);
        let express_max_secs_bg: u32 = if Self::is_qwen3_zero_shot() { 30 } else { 10 };

        std::thread::spawn(move || {
            // Give the recording thread a moment to finalize
            std::thread::sleep(std::time::Duration::from_millis(300));

            // Get samples and sample rate
            let samples: Vec<f32> = {
                let buf = buffer.lock();
                buf.clone()
            };

            let source_sample_rate = *sample_rate_store.lock();

            if samples.is_empty() {
                eprintln!("[VoiceClone] No audio recorded");
                return;
            }

            let duration = samples.len() as f32 / source_sample_rate as f32;
            eprintln!(
                "[VoiceClone] Recorded {} samples at {}Hz ({:.1}s)",
                samples.len(),
                source_sample_rate,
                duration
            );

            // Validate duration in processing thread (defensive check)
            // This prevents race conditions or edge cases where short recordings slip through
            const MIN_DURATION: f32 = 3.0;
            if duration < MIN_DURATION {
                eprintln!(
                    "[VoiceClone] ERROR: Recording too short ({:.1}s < {}s), aborting processing",
                    duration, MIN_DURATION
                );
                return;
            }

            // Resample to 16kHz if needed
            let target_sample_rate: u32 = 16000;
            let resampled: Vec<f32> = if source_sample_rate != target_sample_rate {
                eprintln!(
                    "[VoiceClone] Resampling {}Hz -> {}Hz",
                    source_sample_rate, target_sample_rate
                );
                Self::resample(&samples, source_sample_rate, target_sample_rate)
            } else {
                samples
            };

            // Trim to backend-specific max (Qwen3: 30s, GPT-SoVITS: 10s)
            let max_samples = (express_max_secs_bg * target_sample_rate) as usize;
            let trimmed_samples: Vec<f32> = if resampled.len() > max_samples {
                resampled[..max_samples].to_vec()
            } else {
                resampled
            };

            let final_duration = trimmed_samples.len() as f32 / target_sample_rate as f32;
            eprintln!(
                "[VoiceClone] Final audio: {} samples ({:.1}s)",
                trimmed_samples.len(),
                final_duration
            );

            // Save to temp file with unique name to prevent conflicts
            // Uses PID + nanosecond timestamp to ensure uniqueness even if PID is reused
            let temp_dir = std::env::temp_dir();
            let unique_suffix = format!(
                "{}_{}",
                std::process::id(),
                std::time::SystemTime::now()
                    .duration_since(std::time::UNIX_EPOCH)
                    .map(|d| d.as_nanos())
                    .unwrap_or(0)
            );
            let temp_file = temp_dir.join(format!("voice_clone_recording_{}.wav", unique_suffix));

            if let Err(e) = Self::save_wav_static(&temp_file, &trimmed_samples, target_sample_rate)
            {
                eprintln!("[VoiceClone] Failed to save WAV: {}", e);
                return;
            }

            eprintln!("[VoiceClone] Audio saved to: {:?}", temp_file);

            // Store the temp file path and signal completion
            *temp_file_store.lock() = Some(temp_file.clone());
            processing_complete.store(true, Ordering::Relaxed);

            eprintln!("[VoiceClone] Processing complete. Please enter text manually.");
        });
    }

    /// Simple linear interpolation resampling
    /// High-quality audio resampling using sinc interpolation with anti-aliasing
    ///
    /// Uses the rubato library which implements proper anti-aliasing filters
    /// to prevent artifacts when upsampling or downsampling.
    fn resample(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
        use rubato::{
            Resampler, SincFixedIn, SincInterpolationParameters, SincInterpolationType,
            WindowFunction,
        };

        if source_rate == target_rate {
            return samples.to_vec();
        }

        // Calculate resampling ratio
        let resample_ratio = target_rate as f64 / source_rate as f64;

        // Create high-quality sinc resampler
        // Parameters chosen for good quality/performance balance for voice:
        // - sinc_len: 256 (higher = better quality but slower)
        // - f_cutoff: 0.95 (cutoff frequency relative to Nyquist)
        // - oversampling_factor: 256 (interpolation quality)
        // - window: BlackmanHarris2 (good sidelobe suppression)
        let params = SincInterpolationParameters {
            sinc_len: 256,
            f_cutoff: 0.95,
            oversampling_factor: 256,
            interpolation: SincInterpolationType::Linear,
            window: WindowFunction::BlackmanHarris2,
        };

        let mut resampler = SincFixedIn::<f32>::new(
            resample_ratio,
            2.0,      // max_resample_ratio_relative (allows ±2x variation)
            params,
            samples.len(),
            1,        // mono (1 channel)
        )
        .expect("Failed to create resampler");

        // Rubato expects input as Vec<Vec<f32>> for multi-channel
        // We have mono, so wrap in a single-channel vec
        let input_frames = vec![samples.to_vec()];

        // Process resampling
        match resampler.process(&input_frames, None) {
            Ok(output_frames) => {
                // Extract the mono channel
                output_frames[0].clone()
            }
            Err(e) => {
                eprintln!("[VoiceClone] Resampling error: {}, falling back to linear interpolation", e);
                // Fallback to simple linear interpolation on error
                Self::resample_linear_fallback(samples, source_rate, target_rate)
            }
        }
    }

    /// Fallback linear interpolation resampler (used if rubato fails)
    fn resample_linear_fallback(samples: &[f32], source_rate: u32, target_rate: u32) -> Vec<f32> {
        let ratio = target_rate as f64 / source_rate as f64;
        let new_len = (samples.len() as f64 * ratio) as usize;
        let mut result = Vec::with_capacity(new_len);

        for i in 0..new_len {
            let src_idx = i as f64 / ratio;
            let idx = src_idx as usize;
            let frac = (src_idx - idx as f64) as f32;

            let s1 = samples.get(idx).copied().unwrap_or(0.0);
            let s2 = samples.get(idx + 1).copied().unwrap_or(s1);
            result.push(s1 + (s2 - s1) * frac);
        }

        result
    }

    /// Static version of save_wav for use in background threads
    fn save_wav_static(path: &PathBuf, samples: &[f32], sample_rate: u32) -> Result<(), String> {
        use hound::{SampleFormat, WavSpec, WavWriter};

        let spec = WavSpec {
            channels: 1,
            sample_rate,
            bits_per_sample: 16,
            sample_format: SampleFormat::Int,
        };

        let mut writer =
            WavWriter::create(path, spec).map_err(|e| format!("Failed to create WAV: {}", e))?;

        for &sample in samples {
            let sample_i16 = (sample * 32767.0).clamp(-32768.0, 32767.0) as i16;
            writer
                .write_sample(sample_i16)
                .map_err(|e| format!("Failed to write sample: {}", e))?;
        }

        writer
            .finalize()
            .map_err(|e| format!("Failed to finalize WAV: {}", e))?;

        Ok(())
    }

    fn transcribe_audio(&mut self, cx: &mut Cx, audio_path: &PathBuf) {
        self.add_log(cx, "[INFO] Preparing ASR transcription via Dora...");

        // Load audio file
        match hound::WavReader::open(audio_path) {
            Ok(mut reader) => {
                let spec = reader.spec();
                let source_sample_rate = spec.sample_rate;
                let channels = spec.channels as usize;

                // Read all samples as f32
                let raw_samples: Vec<f32> = if spec.sample_format == hound::SampleFormat::Float {
                    reader.samples::<f32>().filter_map(|s| s.ok()).collect()
                } else {
                    reader
                        .samples::<i16>()
                        .filter_map(|s| s.ok())
                        .map(|s| s as f32 / 32768.0)
                        .collect()
                };

                // Convert to mono if stereo
                let mono_samples: Vec<f32> = if channels > 1 {
                    raw_samples
                        .chunks(channels)
                        .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                        .collect()
                } else {
                    raw_samples
                };

                // Resample to 16kHz for ASR
                let target_rate: u32 = 16000;
                let samples = if source_sample_rate != target_rate {
                    Self::resample(&mono_samples, source_sample_rate, target_rate)
                } else {
                    mono_samples
                };

                self.add_log(
                    cx,
                    &format!(
                        "[INFO] Audio prepared for ASR: {} samples ({}ch {}Hz -> 1ch 16kHz)",
                        samples.len(), channels, source_sample_rate
                    ),
                );

                // Store for sending in handle_event
                self.pending_asr_audio = Some((samples, target_rate, audio_path.clone()));
                self.asr_sent = false;
                self.recording_status = RecordingStatus::Transcribing;

                // Show transcription loading overlay (Express mode) and reset animation timer
                self.transcription_animation_start_time = None;
                if self.clone_mode == CloneMode::Express {
                    self.view
                        .view(ids!(
                            modal_container
                                .modal_wrapper
                                .modal_content
                                .body
                                .prompt_text_container
                                .transcription_loading_overlay
                        ))
                        .set_visible(cx, true);
                }
            }
            Err(e) => {
                self.add_log(cx, &format!("[ERROR] Failed to read audio for ASR: {}", e));
                self.add_log(cx, "[INFO] Please enter the reference text manually.");
                self.recording_status = RecordingStatus::Idle;
            }
        }

        self.view.redraw(cx);
    }

    fn update_record_button(&mut self, cx: &mut Cx, recording: bool) {
        let recording_val = if recording { 1.0 } else { 0.0 };
        self.view
            .button(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .file_selector
                    .file_row
                    .record_btn
            ))
            .apply_over(
                cx,
                live! {
                    draw_bg: { recording: (recording_val) }
                },
            );
        self.view.redraw(cx);
    }

    // ========== Pro Mode Training Methods ==========

    /// Switch between Express (ICL zero-shot) and Pro (few-shot training) modes.
    ///
    /// **Qwen3-only refactor note**: Pro mode is hidden from the UI and its
    /// mode_selector button handler is removed in screen.rs. This function and
    /// all CloneMode::Pro branches are kept for easy restoration.
    /// See doc/REFACTOR_QWEN3_ONLY.md.
    pub fn switch_to_mode(&mut self, cx: &mut Cx, mode: CloneMode) {
        self.clone_mode = mode;

        match mode {
            CloneMode::Express => {
                // Show/hide content
                self.view.view(ids!(modal_container.modal_wrapper.modal_content.body.express_mode_content))
                    .set_visible(cx, true);
                self.view.view(ids!(modal_container.modal_wrapper.modal_content.body.pro_mode_content))
                    .set_visible(cx, false);
                self.view.view(ids!(modal_container.modal_wrapper.modal_content.footer.express_actions))
                    .set_visible(cx, true);
                self.view.view(ids!(modal_container.modal_wrapper.modal_content.footer.pro_actions))
                    .set_visible(cx, false);
            }

            CloneMode::Pro => {
                self.view.view(ids!(modal_container.modal_wrapper.modal_content.body.express_mode_content))
                    .set_visible(cx, false);
                self.view.view(ids!(modal_container.modal_wrapper.modal_content.body.pro_mode_content))
                    .set_visible(cx, true);
                self.view.view(ids!(modal_container.modal_wrapper.modal_content.footer.express_actions))
                    .set_visible(cx, false);
                self.view.view(ids!(modal_container.modal_wrapper.modal_content.footer.pro_actions))
                    .set_visible(cx, true);

                // Check GPU availability
                self.check_gpu_availability(cx);
            }
        }

        self.view.redraw(cx);
    }

    fn toggle_training_recording(&mut self, cx: &mut Cx) {
        if self.recording_for_training {
            self.stop_training_recording(cx);
        } else {
            self.start_training_recording(cx);
        }
    }

    fn start_training_recording(&mut self, cx: &mut Cx) {
        use cpal::traits::{DeviceTrait, HostTrait, StreamTrait};

        self.add_training_log(cx, "[INFO] Starting long recording (target: 5-10 minutes)");
        self.add_training_log(cx, "[INFO] Speak clearly with varied sentences for best training results");

        self.recording_for_training = true;
        self.training_audio_samples.clear();
        self.training_recording_start = Some(Instant::now());

        // Update UI
        self.view.label(ids!(
            modal_container.modal_wrapper.modal_content.body.pro_mode_content
            .training_recording_section.record_row.recording_info.duration_label
        )).set_text(cx, "Recording... 0:00 / 10:00");

        self.view.view(ids!(
            modal_container.modal_wrapper.modal_content.body.pro_mode_content
            .training_recording_section.duration_bar
        )).set_visible(cx, true);

        self.view.button(ids!(
            modal_container.modal_wrapper.modal_content.body.pro_mode_content
            .training_recording_section.record_row.record_btn
        )).apply_over(cx, live! { draw_bg: { recording: 1.0 } });

        // Start CPAL recording (reuse existing audio capture logic from Express mode)
        // Initialize buffer and atomic flags
        self.recording_buffer = Arc::new(Mutex::new(Vec::new()));
        self.is_recording = Arc::new(AtomicBool::new(true));
        self.recording_sample_rate = Arc::new(Mutex::new(48000));

        let buffer = Arc::clone(&self.recording_buffer);
        let is_recording = Arc::clone(&self.is_recording);
        let sample_rate_store = Arc::clone(&self.recording_sample_rate);

        std::thread::spawn(move || {
            let host = cpal::default_host();
            let device = match host.default_input_device() {
                Some(d) => d,
                None => {
                    eprintln!("[TrainingRec] No input device found");
                    return;
                }
            };

            let supported_config = match device.default_input_config() {
                Ok(c) => c,
                Err(e) => {
                    eprintln!("[TrainingRec] Failed to get config: {}", e);
                    return;
                }
            };

            let sample_rate = supported_config.sample_rate().0;
            let channels = supported_config.channels() as usize;
            *sample_rate_store.lock() = sample_rate;

            let config: cpal::StreamConfig = supported_config.into();
            let buffer_clone = Arc::clone(&buffer);
            let is_recording_clone = Arc::clone(&is_recording);

            let stream = match device.build_input_stream(
                &config,
                move |data: &[f32], _: &cpal::InputCallbackInfo| {
                    if is_recording_clone.load(Ordering::Relaxed) {
                        // Convert to mono
                        if channels > 1 {
                            let mono: Vec<f32> = data
                                .chunks(channels)
                                .map(|chunk| chunk.iter().sum::<f32>() / channels as f32)
                                .collect();
                            buffer_clone.lock().extend_from_slice(&mono);
                        } else {
                            buffer_clone.lock().extend_from_slice(data);
                        }
                    }
                },
                |err| eprintln!("[TrainingRec] Error: {}", err),
                None,
            ) {
                Ok(s) => s,
                Err(e) => {
                    eprintln!("[TrainingRec] Failed to build stream: {}", e);
                    return;
                }
            };

            if let Err(e) = stream.play() {
                eprintln!("[TrainingRec] Failed to start: {}", e);
                return;
            }

            eprintln!("[TrainingRec] Recording started ({}Hz, {} channels)", sample_rate, channels);

            // Keep alive (max 12 minutes)
            let max_duration = std::time::Duration::from_secs(12 * 60);
            let start = std::time::Instant::now();

            while is_recording.load(Ordering::Relaxed) && start.elapsed() < max_duration {
                std::thread::sleep(std::time::Duration::from_millis(100));
            }

            is_recording.store(false, Ordering::Relaxed);
            eprintln!("[TrainingRec] Recording stopped");
        });

        self.view.redraw(cx);
    }

    fn stop_training_recording(&mut self, cx: &mut Cx) {
        self.is_recording.store(false, Ordering::Relaxed);
        self.recording_for_training = false;

        // Calculate duration
        let duration = self.training_recording_start
            .map(|t| t.elapsed().as_secs_f32())
            .unwrap_or(0.0);

        self.add_training_log(cx, &format!("[INFO] Recording stopped ({:.1}s)", duration));

        // Reset record button UI and duration label regardless of validation outcome
        self.view.button(ids!(
            modal_container.modal_wrapper.modal_content.body.pro_mode_content
            .training_recording_section.record_row.record_btn
        )).apply_over(cx, live! { draw_bg: { recording: 0.0 } });

        self.view.label(ids!(
            modal_container.modal_wrapper.modal_content.body.pro_mode_content
            .training_recording_section.record_row.recording_info.duration_label
        )).set_text(cx, &format!("Recorded: {:.1}s", duration));

        self.view.redraw(cx);

        // Validate duration (minimum 10 seconds)
        if duration < 10.0 {
            self.add_training_log(cx, &format!(
                    "[ERROR] Recording too short: {:.1}s (minimum: 10s)",
                    duration
            ));
            self.training_audio_file = None;
            return;
        }

        if duration > 600.0 {
            self.add_training_log(cx, &format!(
                    "[WARNING] Recording too long: {:.1}s, trimming to 600s (10 minutes)",
                    duration
            ));
        }

        // Get recorded samples
        let samples: Vec<f32> = {
            let buffer = self.recording_buffer.lock();
            buffer.clone()
        };

        let source_sample_rate = *self.recording_sample_rate.lock();

        // Resample to 32kHz (required by GPT-SoVITS)
        let target_sample_rate = 32000;
        let resampled = if source_sample_rate != target_sample_rate {
            Self::resample(&samples, source_sample_rate, target_sample_rate)
        } else {
            samples
        };

        // Trim to 600s (10 minutes) to match UI description
        let max_training_samples = 600 * target_sample_rate as usize;
        let resampled = if resampled.len() > max_training_samples {
            resampled[..max_training_samples].to_vec()
        } else {
            resampled
        };

        // Store samples
        self.training_audio_samples = resampled;

        // Save to temp file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!(
            "training_audio_{}.wav",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ));

        match Self::save_wav_static(&temp_file, &self.training_audio_samples, target_sample_rate) {
            Ok(_) => {
                self.training_audio_file = Some(temp_file.clone());
                self.add_training_log(cx, &format!(
                        "[SUCCESS] Recording saved ({:.1}s, {:.1} MB)",
                        duration,
                        (self.training_audio_samples.len() * 4) as f64 / 1_000_000.0
                ));

                // Enable start training button
                self.view
                    .button(ids!(
                        modal_container
                            .modal_wrapper
                            .modal_content
                            .footer
                            .pro_actions
                            .start_training_btn
                    ))
                    .set_enabled(cx, true);

                // Show clear button so user can discard and re-record
                self.view.button(ids!(
                    modal_container.modal_wrapper.modal_content.body.pro_mode_content
                    .training_recording_section.record_row.recording_info.clear_recording_btn
                )).set_visible(cx, true);

                // Auto-transcribe training recording via dora-asr.
                self.transcribe_audio(cx, &temp_file);
            }
            Err(e) => {
                self.add_training_log(cx, &format!("[ERROR] Failed to save audio: {}", e));
            }
        }

    }

    fn open_training_file_dialog(&mut self, cx: &mut Cx) {
        let dialog = rfd::FileDialog::new()
            .add_filter("Audio Files", &["wav", "mp3", "flac", "ogg"])
            .add_filter("WAV Files", &["wav"])
            .set_title("Select Training Audio (10s-10min)");

        if let Some(path) = dialog.pick_file() {
            self.handle_training_file_selected(cx, path);
        }
    }

    fn handle_training_file_selected(&mut self, cx: &mut Cx, path: PathBuf) {
        let file_name = path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or("Unknown")
            .to_string();

        self.add_training_log(cx, &format!("[INFO] Loading audio file: {}", file_name));

        // Read WAV file using hound
        let reader = match hound::WavReader::open(&path) {
            Ok(r) => r,
            Err(e) => {
                let msg = format!("Failed to read audio file: {}", e);
                self.add_training_log(cx, &format!("[ERROR] {}", msg));
                self.show_error(cx, &msg);
                return;
            }
        };

        let spec = reader.spec();
        let source_sample_rate = spec.sample_rate;
        let channels = spec.channels as u32;

        // Read all samples and convert to mono f32
        let samples: Vec<f32> = match spec.sample_format {
            hound::SampleFormat::Int => {
                let bit_depth = spec.bits_per_sample;
                let max_val = (1u32 << (bit_depth - 1)) as f32;
                reader
                    .into_samples::<i32>()
                    .filter_map(|s| s.ok())
                    .collect::<Vec<i32>>()
                    .chunks(channels as usize)
                    .map(|chunk| {
                        let sum: f32 = chunk.iter().map(|&s| s as f32 / max_val).sum();
                        sum / channels as f32
                    })
                    .collect()
            }
            hound::SampleFormat::Float => {
                reader
                .into_samples::<f32>()
                .filter_map(|s| s.ok())
                .collect::<Vec<f32>>()
                .chunks(channels as usize)
                .map(|chunk| {
                    let sum: f32 = chunk.iter().sum();
                    sum / channels as f32
                })
                    .collect()
            }
        };

        // Calculate duration
        let duration = samples.len() as f32 / source_sample_rate as f32;

        self.add_training_log(cx, &format!(
                "[INFO] Audio loaded: {:.1}s, {}Hz, {} ch",
                duration, source_sample_rate, channels
        ));

        // Validate duration (minimum 10 seconds)
        if duration < 10.0 {
            let msg = format!(
                "Audio too short: {:.1}s (minimum: 10s)",
                duration
            );
            self.add_training_log(cx, &format!("[ERROR] {}", msg));
            self.show_error(cx, &msg);
            return;
        }

        if duration > 600.0 {
            self.add_training_log(cx, &format!(
                    "[WARNING] Audio too long: {:.1}s, trimming to 600s (10 minutes)",
                    duration
            ));
        }

        // Resample to 32kHz (required by GPT-SoVITS)
        let target_sample_rate = 32000;
        let resampled = if source_sample_rate != target_sample_rate {
            Self::resample(&samples, source_sample_rate, target_sample_rate)
        } else {
            samples
        };

        // Trim to 600s (10 minutes) to match UI description
        let max_training_samples = 600 * target_sample_rate as usize;
        let resampled = if resampled.len() > max_training_samples {
            resampled[..max_training_samples].to_vec()
        } else {
            resampled
        };

        // Store samples
        self.training_audio_samples = resampled;

        // Save to temp file
        let temp_dir = std::env::temp_dir();
        let temp_file = temp_dir.join(format!(
            "training_audio_{}.wav",
            std::time::SystemTime::now()
                .duration_since(std::time::UNIX_EPOCH)
                .unwrap()
                .as_secs()
        ));

        match Self::save_wav_static(&temp_file, &self.training_audio_samples, target_sample_rate) {
            Ok(_) => {
                self.training_audio_file = Some(temp_file.clone());
                self.add_training_log(cx, &format!(
                        "[SUCCESS] Audio file ready ({:.1}s, {:.1} MB)",
                        duration,
                        (self.training_audio_samples.len() * 4) as f64 / 1_000_000.0
                ));

                // Update duration label
                self.view.label(ids!(
                    modal_container.modal_wrapper.modal_content.body.pro_mode_content
                    .training_recording_section.record_row.recording_info.duration_label
                )).set_text(cx, &format!("Uploaded: {:.1}s", duration));

                // Show uploaded file info
                self.view.label(ids!(
                    modal_container.modal_wrapper.modal_content.body.pro_mode_content
                    .training_recording_section.uploaded_file_info.label
                )).set_text(cx, &format!("File: {}", file_name));
                self.view.view(ids!(
                    modal_container.modal_wrapper.modal_content.body.pro_mode_content
                    .training_recording_section.uploaded_file_info
                )).set_visible(cx, true);

                // Enable start training button
                self.view
                    .button(ids!(
                        modal_container
                            .modal_wrapper
                            .modal_content
                            .footer
                            .pro_actions
                            .start_training_btn
                    ))
                    .set_enabled(cx, true);

                // Show clear button so user can discard and re-upload/re-record
                self.view.button(ids!(
                    modal_container.modal_wrapper.modal_content.body.pro_mode_content
                    .training_recording_section.record_row.recording_info.clear_recording_btn
                )).set_visible(cx, true);

                // Auto-transcribe uploaded training audio via dora-asr.
                self.transcribe_audio(cx, &temp_file);
            }
            Err(e) => {
                self.add_training_log(cx, &format!("[ERROR] Failed to save audio: {}", e));
                self.show_error(cx, &format!("Failed to save audio: {}", e));
            }
        }

        self.view.redraw(cx);
    }

    fn start_training(&mut self, cx: &mut Cx, scope: &mut Scope) {
        // Validate inputs
        let voice_name = self.view.text_input(ids!(
            modal_container.modal_wrapper.modal_content.body.pro_mode_content.voice_name_input.input
        )).text();

        if voice_name.is_empty() {
            self.show_error(cx, "Voice name is required");
            return;
        }

        let Some(audio_file) = &self.training_audio_file.clone() else {
            self.show_error(cx, "No training audio available. Please record or upload at least 10 seconds of audio first.");
            return;
        };

        let reference_text = self
            .view
            .text_input(ids!(
                modal_container
                    .modal_wrapper
                    .modal_content
                    .body
                    .express_mode_content
                    .prompt_text_container
                    .prompt_text_input
                    .input
            ))
            .text()
            .trim()
            .to_string();

        let training_backend = std::env::var("MOXIN_TRAINING_BACKEND")
            .ok()
            .map(|v| v.to_ascii_lowercase())
            .filter(|v| {
                matches!(
                    v.as_str(),
                    "option_a" | "option_b" | "python" | "rust" | "mlx" | "legacy" | "a" | "b"
                )
            })
            .map(|v| {
                if matches!(v.as_str(), "option_b" | "rust" | "mlx" | "b") {
                    "option_b".to_string()
                } else {
                    "option_a".to_string()
                }
            })
            .unwrap_or_else(|| "option_a".to_string());

        self.add_training_log(
            cx,
            &format!("[INFO] Training backend: {}", training_backend),
        );

        if training_backend == "option_b" && reference_text.is_empty() {
            self.show_error(
                cx,
                "Option B (Rust/MLX) requires reference text. Please wait for ASR transcription or enter text in the reference field.",
            );
            return;
        }

        let voice_id = voice_persistence::generate_voice_id(&voice_name);

        // Build the current timestamp string using civil_from_days (Howard Hinnant)
        let created_at = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map(|d| {
                let secs = d.as_secs();
                let sec = secs % 60;
                let min = (secs / 60) % 60;
                let hour = (secs / 3600) % 24;
                let total_days = (secs / 86400) as i64;
                let z = total_days + 719468;
                let era = if z >= 0 { z } else { z - 146096 } / 146097;
                let doe = z - era * 146097;
                let yoe = (doe - doe / 1460 + doe / 36524 - doe / 146096) / 365;
                let y = yoe + era * 400;
                let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
                let mp = (5 * doy + 2) / 153;
                let day = (doy - (153 * mp + 2) / 5 + 1) as u32;
                let month = if mp < 10 { (mp + 3) as u32 } else { (mp - 9) as u32 };
                let year = if month <= 2 { y + 1 } else { y };
                format!("{}-{:02}-{:02} {:02}:{:02}:{:02}", year, month, day, hour, min, sec)
            })
            .unwrap_or_else(|_| "unknown".to_string());

        // Create a Pending CloneTask
        let task = crate::task_persistence::CloneTask {
            id: voice_id.clone(),
            name: voice_name.clone(),
            status: crate::task_persistence::CloneTaskStatus::Pending,
            progress: 0.0,
            created_at,
            audio_path: Some(audio_file.to_string_lossy().to_string()),
            reference_text: if reference_text.is_empty() {
                None
            } else {
                Some(reference_text)
            },
            started_at: None,
            completed_at: None,
            message: Some("Waiting in queue...".to_string()),
            training_backend: Some(training_backend),
            current_step: None,
            sub_step: None,
            sub_total: None,
        };

        // Persist to disk
        if let Err(e) = task_persistence::add_task(task.clone()) {
            self.show_error(cx, &format!("Failed to save task: {}", e));
            return;
        }

        self.current_task_id = Some(voice_id);

        // Emit TaskCreated action so parent can open task detail page
        cx.widget_action(
            self.widget_uid(),
            &scope.path,
            VoiceCloneModalAction::TaskCreated(task),
        );

        // Close modal
        self.close(cx, scope);
    }

    fn check_gpu_availability(&mut self, cx: &mut Cx) {
        if !self.gpu_check_done {
            // First time: run the check and cache the result
            let has_gpu = std::process::Command::new("python")
                .arg("-c")
                .arg("import torch; print(torch.cuda.is_available())")
                .output()
                .map(|out| String::from_utf8_lossy(&out.stdout).trim() == "True")
                .unwrap_or(false);

            self.has_gpu = has_gpu;
            self.gpu_check_done = true;
        }

        if !self.has_gpu {
            // Show warning
            self.view.view(ids!(
                modal_container.modal_wrapper.modal_content.body.pro_mode_content.gpu_warning
            )).set_visible(cx, true);
        } else {
            self.view.view(ids!(
                modal_container.modal_wrapper.modal_content.body.pro_mode_content.gpu_warning
            )).set_visible(cx, false);
        }
    }

    fn add_training_log(&mut self, _cx: &mut Cx, message: &str) {
        eprintln!("[Training] {}", message);
    }
}

impl VoiceCloneModalRef {
    /// Show the modal
    pub fn show(&self, cx: &mut Cx) {
        // Clean up old temp files from previous sessions (async, non-blocking)
        std::thread::spawn(|| {
            VoiceCloneModal::cleanup_old_temp_files();
        });

        if let Some(mut inner) = self.borrow_mut() {
            inner.view.set_visible(cx, true);
            inner.clear_log(cx);

            // Check if ASR bridge is ready
            let is_asr_ready = if let Some(ref shared_state) = inner.shared_dora_state {
                let status = shared_state.status.read();
                // Only check for ASR listener - audio-input is only needed for live recording
                let has_asr = status
                    .active_bridges
                    .iter()
                    .any(|b| b.contains("asr-listener") || b.contains("asr"));
                let has_audio_input = status
                    .active_bridges
                    .iter()
                    .any(|b| b.contains("audio-input"));

                inner.add_log(
                    cx,
                    &format!(
                        "[DEBUG] ASR bridge check: has_asr={}, has_audio_input={}, bridges={:?}",
                        has_asr, has_audio_input, status.active_bridges
                    ),
                );

                if has_asr {
                    if has_audio_input {
                        inner.add_log(
                            cx,
                            "[INFO] ASR is ready (recording + transcription available)",
                        );
                    } else {
                        inner.add_log(cx, "[INFO] ASR is ready (transcription available, live recording disabled)");
                    }
                    true
                } else {
                    inner.add_log(cx, "[INFO] Waiting for ASR bridge to initialize...");
                    false
                }
            } else {
                inner.add_log(cx, "[DEBUG] shared_dora_state is None");
                false
            };

            // Update ASR readiness state and show/hide loading overlay
            inner.asr_bridge_ready = is_asr_ready;
            inner.loading_animation_start_time = None; // Reset animation timer
            inner.transcription_animation_start_time = None; // Reset animation timer

            // Show/hide loading overlay based on ASR readiness
            inner.add_log(
                cx,
                &format!("[DEBUG] Setting loading overlay visible: {}", !is_asr_ready),
            );
            inner
                .view
                .view(ids!(asr_loading_overlay))
                .set_visible(cx, !is_asr_ready);

            // Update loading overlay dark mode
            if !is_asr_ready {
                let dark_mode = inner.dark_mode;
                inner
                    .view
                    .view(ids!(asr_loading_overlay.loading_content))
                    .apply_over(
                        cx,
                        live! {
                            draw_bg: { dark_mode: (dark_mode) }
                        },
                    );
                inner
                    .view
                    .label(ids!(asr_loading_overlay.loading_content.loading_message))
                    .apply_over(
                        cx,
                        live! {
                            draw_text: { dark_mode: (dark_mode) }
                        },
                    );
                inner
                    .view
                    .label(ids!(asr_loading_overlay.loading_content.loading_submessage))
                    .apply_over(
                        cx,
                        live! {
                            draw_text: { dark_mode: (dark_mode) }
                        },
                    );
                inner
                    .view
                    .view(ids!(asr_loading_overlay.loading_content.loading_spinner))
                    .apply_over(
                        cx,
                        live! {
                            draw_bg: { dark_mode: (dark_mode) }
                        },
                    );
            }

            inner.update_language_buttons(cx);
            inner.view.redraw(cx);
        }
    }

    /// Hide the modal
    pub fn hide(&self, cx: &mut Cx) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.view.set_visible(cx, false);
        }
    }

    /// Update dark mode
    pub fn update_dark_mode(&self, cx: &mut Cx, dark_mode: f64) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.dark_mode = dark_mode;

            // Apply to modal content
            inner
                .view
                .view(ids!(modal_container.modal_wrapper.modal_content))
                .apply_over(
                    cx,
                    live! {
                        draw_bg: { dark_mode: (dark_mode) }
                    },
                );

            // Apply to header
            inner
                .view
                .view(ids!(modal_container.modal_wrapper.modal_content.header))
                .apply_over(
                    cx,
                    live! {
                        draw_bg: { dark_mode: (dark_mode) }
                    },
                );

            // Apply to footer
            inner
                .view
                .view(ids!(modal_container.modal_wrapper.modal_content.footer))
                .apply_over(
                    cx,
                    live! {
                        draw_bg: { dark_mode: (dark_mode) }
                    },
                );

            // Apply to transcription loading overlay
            inner
                .view
                .view(ids!(
                    modal_container
                        .modal_wrapper
                        .modal_content
                        .body
                        .prompt_text_container
                        .transcription_loading_overlay
                        .loading_backdrop
                ))
                .apply_over(
                    cx,
                    live! {
                        draw_bg: { dark_mode: (dark_mode) }
                    },
                );
            inner
                .view
                .view(ids!(
                    modal_container
                        .modal_wrapper
                        .modal_content
                        .body
                        .prompt_text_container
                        .transcription_loading_overlay
                        .loading_content
                ))
                .apply_over(
                    cx,
                    live! {
                        draw_bg: { dark_mode: (dark_mode) }
                    },
                );
            inner
                .view
                .view(ids!(
                    modal_container
                        .modal_wrapper
                        .modal_content
                        .body
                        .prompt_text_container
                        .transcription_loading_overlay
                        .loading_content
                        .loading_spinner
                ))
                .apply_over(
                    cx,
                    live! {
                        draw_bg: { dark_mode: (dark_mode) }
                    },
                );
            inner
                .view
                .label(ids!(
                    modal_container
                        .modal_wrapper
                        .modal_content
                        .body
                        .prompt_text_container
                        .transcription_loading_overlay
                        .loading_content
                        .loading_label
                ))
                .apply_over(
                    cx,
                    live! {
                        draw_text: { dark_mode: (dark_mode) }
                    },
                );

            // Apply to error message
            inner
                .view
                .label(ids!(modal_container.modal_wrapper.modal_content.footer.error_message))
                .apply_over(
                    cx,
                    live! {
                        draw_text: { dark_mode: (dark_mode) }
                    },
                );

            inner.view.redraw(cx);
        }
    }

    /// Set shared Dora state for ASR integration
    pub fn set_shared_dora_state(&self, state: std::sync::Arc<moxin_dora_bridge::SharedDoraState>) {
        if let Some(mut inner) = self.borrow_mut() {
            inner.shared_dora_state = Some(state);
        }
    }

    /// Show the modal in a specific mode (Express or Pro)
    pub fn show_with_mode(&self, cx: &mut Cx, mode: CloneMode) {
        self.show(cx);
        if let Some(mut inner) = self.borrow_mut() {
            inner.switch_to_mode(cx, mode);
        }
    }
}

/// Decode a percent-encoded URL path to a plain file system path.
///
/// macOS drag-and-drop returns file paths via `NSURL absoluteString`, which
/// percent-encodes non-ASCII characters (e.g. Chinese filenames become %E8%AF%AD...).
/// This decodes them back to the real UTF-8 path.
fn percent_decode(s: &str) -> String {
    let bytes = s.as_bytes();
    let mut out: Vec<u8> = Vec::with_capacity(bytes.len());
    let mut i = 0;
    while i < bytes.len() {
        if bytes[i] == b'%' && i + 2 < bytes.len() {
            let hi = (bytes[i + 1] as char).to_digit(16);
            let lo = (bytes[i + 2] as char).to_digit(16);
            if let (Some(h), Some(l)) = (hi, lo) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(bytes[i]);
        i += 1;
    }
    String::from_utf8(out).unwrap_or_else(|_| s.to_string())
}
