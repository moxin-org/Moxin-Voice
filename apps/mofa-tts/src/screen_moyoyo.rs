//! TTS Screen - MoYoYo.tts style interface with sidebar layout
//! This is a variant of the TTS screen with a sidebar navigation similar to MoYoYo.tts

use crate::audio_player::TTSPlayer;
use crate::dora_integration::DoraIntegration;
use crate::log_bridge;
use crate::training_executor::TrainingExecutor;
use crate::voice_clone_modal::{CloneMode, VoiceCloneModalAction, VoiceCloneModalWidgetExt};
use crate::voice_data::{TTSStatus, Voice};
use crate::voice_selector::{VoiceSelectorAction, VoiceSelectorWidgetExt};
use crate::task_persistence;
use hound::WavReader;
use makepad_widgets::*;
use std::path::PathBuf;

/// Current page in the application
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum AppPage {
    #[default]
    TextToSpeech,
    VoiceLibrary,
    VoiceClone,
    TaskDetail,
}

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;

    use mofa_widgets::theme::*;
    use crate::voice_selector::VoiceSelector;
    use crate::voice_clone_modal::VoiceCloneModal;

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

    // MoYoYo.tts Navigation item button for sidebar
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
                let active_color = (MOYOYO_PRIMARY);
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

    // TTS Screen - MoYoYo.tts style layout with sidebar
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

        // MoYoYo.tts style app layout with sidebar
        app_layout = <View> {
            width: Fill, height: Fill
            flow: Right
            spacing: 0
            padding: 0
            align: {x: 0.0, y: 0.0}

            // ============ MoYoYo.tts Sidebar ============
            sidebar = <View> {
                width: 220, height: Fill
                flow: Down
                spacing: 0
                
                show_bg: true
                draw_bg: {
                    fn pixel(self) -> vec4 {
                        return (MOYOYO_BG_SIDEBAR);
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
                                sdf.fill((MOYOYO_PRIMARY));
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
                        // MoYoYo.tts style: light gray background
                        return mix((MOYOYO_BG_PRIMARY), (MOYOYO_BG_PRIMARY_DARK), self.dark_mode);
                    }
                }

            // Main content area (fills remaining space) - MoYoYo.tts simplified layout
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

                // Main content area - MoYoYo.tts style unified layout
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

                    // Page title - MoYoYo.tts style
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
                                    return mix((MOYOYO_TEXT_PRIMARY), (MOYOYO_TEXT_PRIMARY_DARK), self.dark_mode);
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

                    // Text input section (fills space) - MoYoYo.tts card style
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
                                // MoYoYo.tts style: white card background
                                let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                sdf.fill(bg);
                                return sdf.result;
                            }
                        }

                        // Header - hidden for MoYoYo.tts style (title is in page_header now)
                        header = <View> {
                            width: Fill, height: 0
                            visible: false
                            
                            title = <Label> {
                                text: ""
                            }
                        }

                        // Text input container - MoYoYo.tts clean style
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
                                        return mix((MOYOYO_TEXT_PRIMARY), (MOYOYO_TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }

                                draw_cursor: {
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0.0, 0.0, self.rect_size.x, self.rect_size.y, 0.5);
                                        sdf.fill((MOYOYO_PRIMARY));
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

                        // Bottom bar with character count and generate button - MoYoYo.tts style
                        bottom_bar = <View> {
                            width: Fill, height: Fit
                            flow: Right
                            align: {x: 0.0, y: 0.5}
                            padding: {left: 20, right: 20, top: 16, bottom: 20}
                            spacing: 16
                            
                            show_bg: true
                            draw_bg: {
                                instance dark_mode: 0.0
                                fn pixel(self) -> vec4 {
                                    // Top border
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.rect(0.0, 0.0, self.rect_size.x, 1.0);
                                    let border = mix((MOYOYO_BORDER_LIGHT), (SLATE_700), self.dark_mode);
                                    sdf.fill(border);
                                    return sdf.result;
                                }
                            }

                            // Character count
                            char_count = <Label> {
                                width: Fit, height: Fit
                                align: {y: 0.5}
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: { font_size: 13.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((MOYOYO_TEXT_MUTED), (MOYOYO_TEXT_MUTED_DARK), self.dark_mode);
                                    }
                                }
                                text: "0 / 5,000 字符"
                            }

                            <View> { width: Fill, height: 1 }

                            // Generate button with spinner - MoYoYo.tts style
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
                                            let base = (MOYOYO_PRIMARY);
                                            let hover_color = (MOYOYO_PRIMARY_LIGHT);
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

                    // Voice selector panel (fixed width) - MoYoYo.tts card style
                    controls_panel = <View> {
                        width: 300, height: Fill
                        flow: Down

                        // Voice selector (fills available space)
                        voice_section = <RoundedView> {
                            width: Fill, height: Fill
                            flow: Down
                            show_bg: true
                            draw_bg: {
                                instance dark_mode: 0.0
                                instance border_radius: 16.0
                                fn pixel(self) -> vec4 {
                                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                    sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                    // MoYoYo.tts style: white card background
                                    let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                    sdf.fill(bg);
                                    return sdf.result;
                                }
                            }

                            voice_selector = <VoiceSelector> {
                                height: Fill
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
                        padding: {left: 24, right: 24, top: 24, bottom: 0}
                        visible: false  // Hidden by default

                        // Page header
                        library_header = <View> {
                            width: Fill, height: Fit
                            flow: Right
                            align: {y: 0.5}
                            spacing: 16

                            library_title = <Label> {
                                width: Fit, height: Fit
                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: <FONT_SEMIBOLD>{ font_size: 24.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((MOYOYO_TEXT_PRIMARY), (MOYOYO_TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                                text: "音色库"
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
                                    instance border_radius: 8.0
                                    fn pixel(self) -> vec4 {
                                        let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                        sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                                        let bg = mix((WHITE), (SLATE_800), self.dark_mode);
                                        sdf.fill(bg);
                                        let border = mix((MOYOYO_BORDER_LIGHT), (SLATE_700), self.dark_mode);
                                        sdf.stroke(border, 1.0);
                                        return sdf.result;
                                    }
                                }

                                draw_text: {
                                    instance dark_mode: 0.0
                                    text_style: { font_size: 14.0 }
                                    fn get_color(self) -> vec4 {
                                        return mix((MOYOYO_TEXT_PRIMARY), (MOYOYO_TEXT_PRIMARY_DARK), self.dark_mode);
                                    }
                                }
                            }

                            // Refresh button
                            refresh_btn = <Button> {
                                width: Fit, height: 40
                                padding: {left: 16, right: 16}
                                text: "🔄 刷新"

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
                                        return mix((MOYOYO_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
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
                                        return mix((MOYOYO_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
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
                                                let border = mix((MOYOYO_BORDER_LIGHT), (SLATE_700), self.dark_mode);
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
                                                    let bg = mix((MOYOYO_PRIMARY), (PRIMARY_400), self.dark_mode);
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
                                                        return mix((MOYOYO_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
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
                                                            return mix((MOYOYO_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
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
                                                            return mix((MOYOYO_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
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
                                                        return mix((MOYOYO_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                    }
                                                }
                                            }

                                            delete_btn = <Button> {
                                                width: Fit, height: 32
                                                padding: {left: 12, right: 12}
                                                text: "Delete"
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
                                                        let normal = mix((MOYOYO_TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
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
                                            return mix((MOYOYO_TEXT_PRIMARY), (MOYOYO_TEXT_PRIMARY_DARK), self.dark_mode);
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
                                                let active = (MOYOYO_PRIMARY);
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
                                                let active = (MOYOYO_PRIMARY);
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
                                        let base = (MOYOYO_PRIMARY);
                                        let hover_color = (MOYOYO_PRIMARY_LIGHT);
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
                                                return mix((MOYOYO_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
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
                                                let border = mix((MOYOYO_BORDER_LIGHT), (SLATE_700), self.dark_mode);
                                                sdf.stroke(border, 1.0);
                                                return sdf.result;
                                            }
                                        }

                                        top_row = <View> {
                                            width: Fill, height: Fit
                                            flow: Right
                                            align: {y: 0.5}
                                            spacing: 12

                                            task_name = <Label> {
                                                width: Fill, height: Fit
                                                draw_text: {
                                                    instance dark_mode: 0.0
                                                    text_style: <FONT_SEMIBOLD>{ font_size: 15.0 }
                                                    fn get_color(self) -> vec4 {
                                                        return mix((MOYOYO_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                    }
                                                }
                                                text: "Task Name"
                                            }

                                            status_badge = <View> {
                                                width: Fit, height: Fit
                                                padding: {left: 8, right: 8, top: 4, bottom: 4}
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
                                                        text_style: <FONT_SEMIBOLD>{ font_size: 11.0 }
                                                        fn get_color(self) -> vec4 {
                                                            return (WHITE);
                                                        }
                                                    }
                                                    text: "Completed"
                                                }
                                            }
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
                                                        return mix((MOYOYO_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
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
                                                        sdf.fill((MOYOYO_PRIMARY));
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
                                                            return mix((MOYOYO_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
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
                                                    text: "View"

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
                                                            return mix((MOYOYO_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
                                                        }
                                                    }
                                                }

                                                cancel_btn = <Button> {
                                                    width: Fit, height: 32
                                                    padding: {left: 12, right: 12}
                                                    text: "Cancel"
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
                                                            let normal = mix((MOYOYO_TEXT_SECONDARY), (TEXT_SECONDARY_DARK), self.dark_mode);
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
                                        return (MOYOYO_TEXT_PRIMARY);
                                    }
                                }
                            }

                            detail_task_name = <Label> {
                                width: Fit, height: Fit
                                draw_text: {
                                    text_style: <FONT_SEMIBOLD>{ font_size: 20.0 }
                                    fn get_color(self) -> vec4 {
                                        return (MOYOYO_TEXT_PRIMARY);
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
                                            return (MOYOYO_TEXT_SECONDARY);
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
                                visible: false

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
                                    fn get_color(self) -> vec4 { return (MOYOYO_TEXT_PRIMARY); }
                                }
                                text: "任务信息"
                            }

                            detail_times_row = <View> {
                                flow: Right
                                spacing: 40
                                width: Fill, height: Fit

                                detail_created_section = <View> {
                                    flow: Down, spacing: 4, width: Fit, height: Fit
                                    <Label> {
                                        width: Fit, height: Fit
                                        draw_text: {
                                            text_style: { font_size: 11.0 }
                                            fn get_color(self) -> vec4 { return (MOYOYO_TEXT_MUTED); }
                                        }
                                        text: "创建时间"
                                    }
                                    detail_created_at = <Label> {
                                        width: Fit, height: Fit
                                        draw_text: {
                                            text_style: { font_size: 13.0 }
                                            fn get_color(self) -> vec4 { return (MOYOYO_TEXT_PRIMARY); }
                                        }
                                        text: "-"
                                    }
                                }

                                detail_started_section = <View> {
                                    flow: Down, spacing: 4, width: Fit, height: Fit
                                    <Label> {
                                        width: Fit, height: Fit
                                        draw_text: {
                                            text_style: { font_size: 11.0 }
                                            fn get_color(self) -> vec4 { return (MOYOYO_TEXT_MUTED); }
                                        }
                                        text: "开始时间"
                                    }
                                    detail_started_at = <Label> {
                                        width: Fit, height: Fit
                                        draw_text: {
                                            text_style: { font_size: 13.0 }
                                            fn get_color(self) -> vec4 { return (MOYOYO_TEXT_PRIMARY); }
                                        }
                                        text: "-"
                                    }
                                }

                                detail_completed_section = <View> {
                                    flow: Down, spacing: 4, width: Fit, height: Fit
                                    <Label> {
                                        width: Fit, height: Fit
                                        draw_text: {
                                            text_style: { font_size: 11.0 }
                                            fn get_color(self) -> vec4 { return (MOYOYO_TEXT_MUTED); }
                                        }
                                        text: "完成时间"
                                    }
                                    detail_completed_at = <Label> {
                                        width: Fit, height: Fit
                                        draw_text: {
                                            text_style: { font_size: 13.0 }
                                            fn get_color(self) -> vec4 { return (MOYOYO_TEXT_PRIMARY); }
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
                                    fn get_color(self) -> vec4 { return (MOYOYO_TEXT_PRIMARY); }
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
                                            sdf.fill((MOYOYO_PRIMARY));
                                            return sdf.result;
                                        }
                                    }
                                }

                                detail_progress_text = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: {
                                        text_style: <FONT_SEMIBOLD>{ font_size: 13.0 }
                                        fn get_color(self) -> vec4 { return (MOYOYO_PRIMARY); }
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
                                <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOYOYO_TEXT_PRIMARY); } }
                                    text: "音频切片"
                                }
                                stage_1_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOYOYO_PRIMARY); } }
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
                                <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOYOYO_TEXT_PRIMARY); } }
                                    text: "语音识别"
                                }
                                stage_2_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOYOYO_PRIMARY); } }
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
                                <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOYOYO_TEXT_PRIMARY); } }
                                    text: "文本特征"
                                }
                                stage_3_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOYOYO_PRIMARY); } }
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
                                <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOYOYO_TEXT_PRIMARY); } }
                                    text: "HuBERT特征"
                                }
                                stage_4_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOYOYO_PRIMARY); } }
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
                                <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOYOYO_TEXT_PRIMARY); } }
                                    text: "语义Token"
                                }
                                stage_5_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOYOYO_PRIMARY); } }
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
                                <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOYOYO_TEXT_PRIMARY); } }
                                    text: "SoVITS训练"
                                }
                                stage_6_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOYOYO_PRIMARY); } }
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
                                <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOYOYO_TEXT_PRIMARY); } }
                                    text: "GPT训练"
                                }
                                stage_7_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOYOYO_PRIMARY); } }
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
                                <Label> {
                                    width: Fill, height: Fit
                                    draw_text: { text_style: { font_size: 13.0 } fn get_color(self) -> vec4 { return (MOYOYO_TEXT_PRIMARY); } }
                                    text: "推理测试"
                                }
                                stage_8_pct = <Label> {
                                    width: Fit, height: Fit
                                    draw_text: { text_style: { font_size: 12.0 } fn get_color(self) -> vec4 { return (MOYOYO_PRIMARY); } }
                                    text: ""
                                }
                            }

                            detail_message_label = <Label> {
                                width: Fill, height: Fit
                                draw_text: {
                                    text_style: { font_size: 12.0 }
                                    fn get_color(self) -> vec4 { return (MOYOYO_TEXT_SECONDARY); }
                                }
                                text: ""
                            }
                        }
                    } // End task_detail_page

                } // End content_area
            } // End left_column

            // Splitter handle for resizing - hidden in MoYoYo.tts style
            splitter = <Splitter> {
                visible: false
                width: 0
            }

            // Right Panel: System Log - hidden in MoYoYo.tts style
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
                            return mix((MOYOYO_BG_PRIMARY), (SLATE_800), self.dark_mode);
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
                                return mix((MOYOYO_TEXT_MUTED), (SLATE_400), self.dark_mode);
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

                // Log content panel with border - MoYoYo.tts card style
                log_content_column = <RoundedView> {
                    width: Fill, height: Fill
                    draw_bg: {
                        instance dark_mode: 0.0
                        instance border_radius: 12.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                            // MoYoYo.tts style: white card
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
                                let border = mix((MOYOYO_BORDER_LIGHT), (SLATE_700), self.dark_mode);
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
                                        return mix((MOYOYO_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
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
                                        return mix((MOYOYO_TEXT_SECONDARY), (SLATE_300), self.dark_mode);
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

            // Bottom audio player bar - MoYoYo.tts style
            audio_player_bar = <View> {
                width: Fill, height: 90
                flow: Right
                align: {x: 0.5, y: 0.5}
                padding: {left: 24, right: 24, top: 8, bottom: 8}
                spacing: 0

                show_bg: true
            draw_bg: {
                instance dark_mode: 0.0
                fn pixel(self) -> vec4 {
                    let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                    // Top border
                    sdf.rect(0.0, 0.0, self.rect_size.x, 1.0);
                    let border = mix((MOYOYO_BORDER_LIGHT), (SLATE_700), self.dark_mode);
                    sdf.fill(border);
                    // Background - MoYoYo.tts style white
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

                // Voice avatar - MoYoYo.tts style
                voice_avatar = <RoundedView> {
                    width: 48, height: 48
                    align: {x: 0.5, y: 0.5}
                    draw_bg: {
                        instance border_radius: 10.0
                        fn pixel(self) -> vec4 {
                            let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                            sdf.box(0., 0., self.rect_size.x, self.rect_size.y, self.border_radius);
                            sdf.fill((MOYOYO_PRIMARY));
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
                                return mix((MOYOYO_TEXT_PRIMARY), (TEXT_PRIMARY_DARK), self.dark_mode);
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
                                return mix((MOYOYO_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
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

                    // Play/Pause button - MoYoYo.tts style
                    play_btn = <PlayButton> {
                        text: ""
                        draw_bg: {
                            fn pixel(self) -> vec4 {
                                let sdf = Sdf2d::viewport(self.pos * self.rect_size);
                                let center = self.rect_size * 0.5;
                                sdf.circle(center.x, center.y, 17.0);
                                sdf.fill((MOYOYO_PRIMARY));
                                // Draw play icon
                                sdf.move_to(14.0, 11.0);
                                sdf.line_to(26.0, 18.0);
                                sdf.line_to(14.0, 25.0);
                                sdf.close_path();
                                sdf.fill((WHITE));
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
                                return mix((MOYOYO_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
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
                                return mix((MOYOYO_TEXT_MUTED), (TEXT_TERTIARY_DARK), self.dark_mode);
                            }
                        }
                        text: "00:00"
                    }
                }
            }

            // Right: Download button (fixed width for balance) - MoYoYo.tts style
            download_section = <View> {
                width: 140, height: Fill
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

                            // MoYoYo.tts style: primary color outline button
                            let base = vec4(0.39, 0.40, 0.95, 0.08);
                            let hover_color = vec4(0.39, 0.40, 0.95, 0.15);
                            let pressed_color = vec4(0.39, 0.40, 0.95, 0.25);
                            let border = (MOYOYO_PRIMARY);

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
                            return (MOYOYO_PRIMARY);
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

    // Current voice name for display
    #[rust]
    current_voice_name: String,

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
}

// Import CloneTask and CloneTaskStatus from task_persistence
use crate::task_persistence::{CloneTask, CloneTaskStatus};

impl Widget for TTSScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // IMPORTANT: Use capture_actions to consume child widget events and prevent
        // them from propagating to sibling screens (like mofa-debate)
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
            // Initialize voice name
            self.current_voice_name = "Doubao".to_string();
            // Initialize current page
            self.current_page = AppPage::TextToSpeech;
            
            // Initialize voice library state
            self.library_voices = Vec::new();
            self.library_search_query = String::new();
            self.library_loading = false;
            self.library_card_areas = Vec::new();
            
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
            
            // Add initial log entries
            self.log_entries
                .push("[INFO] [tts] MoFA TTS initialized".to_string());
            self.log_entries
                .push("[INFO] [tts] Default voice: Doubao (GPT-SoVITS)".to_string());
            self.log_entries
                .push("[INFO] [tts] Click 'Start' to connect to MoFA bridge".to_string());
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

        if !self.dora_started {
            self.dora_started = true;
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
                        // Transition to Ready state - user must click Play
                        if self.tts_status == TTSStatus::Generating {
                            let sample_count = self.stored_audio_samples.len();
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
                                .controls_panel
                                .voice_section
                                .voice_selector
                        ));
                        voice_selector.set_preview_playing(cx, None);
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

                            // Add to in-memory library list and refresh Library page
                            self.library_voices.push(new_voice.clone());
                            self.update_library_display(cx);

                            // Add to voice selector
                            let voice_selector = self.view.voice_selector(ids!(
                                content_wrapper
                                    .main_content
                                    .left_column
                                    .content_area
                                    .controls_panel
                                    .voice_section
                                    .voice_selector
                            ));
                            voice_selector.add_custom_voice(cx, new_voice);

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
                        .set_text(cx, "Connected");
                    self.view.label(ids!(loading_overlay.loading_content.loading_detail))
                        .set_text(cx, "TTS engine ready");
                    // Dismiss loading overlay
                    self.loading_dismissed = true;
                    self.view.view(ids!(loading_overlay)).set_visible(cx, false);
                    self.view.redraw(cx);
                    self.add_log(cx, "[INFO] [tts] Dataflow connected, UI ready");
                } else {
                    self.view.label(ids!(loading_overlay.loading_content.loading_status))
                        .set_text(cx, "Connecting...");
                    self.view.label(ids!(loading_overlay.loading_content.loading_detail))
                        .set_text(cx, "Starting TTS dataflow engine");
                }

                self.view.redraw(cx);
            }
        }

        // MofaHero actions not used in MoYoYo UI (dataflow auto-starts)

        // Handle navigation button clicks
        if self
            .view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_tts))
            .clicked(&actions)
        {
            self.switch_page(cx, AppPage::TextToSpeech);
        }

        if self
            .view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_library))
            .clicked(&actions)
        {
            self.switch_page(cx, AppPage::VoiceLibrary);
        }

        if self
            .view
            .button(ids!(app_layout.sidebar.sidebar_nav.nav_clone))
            .clicked(&actions)
        {
            self.switch_page(cx, AppPage::VoiceClone);
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
                    // Update voice name in player bar
                    self.current_voice_name = voice_id.clone();
                    self.view
                        .label(ids!(
                            content_wrapper
                                .audio_player_bar
                                .voice_info
                                .voice_name_container
                                .current_voice_name
                        ))
                        .set_text(cx, &voice_id);
                    // Update avatar initial
                    let initial = voice_id.chars().next().unwrap_or('?').to_string();
                    self.view
                        .label(ids!(
                            content_wrapper
                                .audio_player_bar
                                .voice_info
                                .voice_avatar
                                .avatar_initial
                        ))
                        .set_text(cx, &initial);
                    self.add_log(cx, &format!("[INFO] [tts] Voice selected: {}", voice_id));
                }
                VoiceSelectorAction::PreviewRequested(voice_id) => {
                    self.handle_preview_request(cx, &voice_id);
                }
                VoiceSelectorAction::RequestStartDora => {
                    // Dataflow auto-starts in MoYoYo UI; show status if not ready yet
                    let is_running = self.dora.as_ref().map(|d| d.is_running()).unwrap_or(false);
                    if !is_running {
                        self.show_toast(cx, "Dataflow is still connecting, please wait...");
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
                    // Delete custom voice
                    let voice_selector = self.view.voice_selector(ids!(
                        content_wrapper
                            .main_content
                            .left_column
                            .content_area
                            .controls_panel
                            .voice_section
                            .voice_selector
                    ));
                    match voice_selector.delete_custom_voice(cx, &voice_id) {
                        Ok(_) => {
                            self.add_log(cx, &format!("[INFO] [tts] Deleted voice: {}", voice_id));
                        }
                        Err(e) => {
                            self.add_log(
                                cx,
                                &format!("[ERROR] [tts] Failed to delete voice: {}", e),
                            );
                        }
                    }
                }
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
                    // Add the new voice to the selector
                    let voice_selector = self.view.voice_selector(ids!(
                        content_wrapper
                            .main_content
                            .left_column
                            .content_area
                            .controls_panel
                            .voice_section
                            .voice_selector
                    ));
                    voice_selector.add_custom_voice(cx, voice.clone());
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

                    // Delete the voice
                    let voice_selector = self.view.voice_selector(ids!(
                        content_wrapper
                            .main_content
                            .left_column
                            .content_area
                            .controls_panel
                            .voice_section
                            .voice_selector
                    ));
                    match voice_selector.delete_custom_voice(cx, &voice_id) {
                        Ok(_) => {
                            self.add_log(
                                cx,
                                &format!("[INFO] [tts] Voice '{}' deleted successfully", voice_id),
                            );
                        }
                        Err(e) => {
                            self.add_log(
                                cx,
                                &format!("[ERROR] [tts] Failed to delete voice: {}", e),
                            );
                        }
                    }
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
                main_content
                    .left_column
                    .content_area
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
                        "Please click 'Start MoFA' button first to initialize the dataflow",
                    );
                } else {
                    // Check if bridges are ready (expected: 4 bridges)
                    let bridge_count = dora.shared_dora_state().status.read().active_bridges.len();
                    if bridge_count < 4 {
                        // Starting but not ready
                        self.show_toast(
                            cx,
                            &format!("Dataflow is starting ({}/4 bridges connected), please wait...", bridge_count),
                        );
                        self.add_log(cx, &format!("[WARN] [tts] Bridges not ready yet: {}/4", bridge_count));
                    } else {
                        // Ready to generate
                        self.generate_speech(cx);
                    }
                }
            } else {
                self.show_toast(cx, "Dora integration not initialized");
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

        // Get UIDs of our two PortalLists using full paths to avoid name collisions
        let voice_list_uid = self.view.portal_list(ids!(
            content_wrapper.main_content.left_column.content_area.library_page.voice_list
        )).widget_uid();
        let task_list_uid = self.view.portal_list(ids!(
            content_wrapper.main_content.left_column.content_area.clone_page.task_portal_list
        )).widget_uid();

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
                            let language = voice.language.clone();
                            let source = voice.source.clone();
                            let type_text = match source {
                                crate::voice_data::VoiceSource::Builtin => "Built-in",
                                crate::voice_data::VoiceSource::Custom => "Custom",
                                crate::voice_data::VoiceSource::Trained => "Trained",
                            };
                            let is_custom = source != crate::voice_data::VoiceSource::Builtin;
                            let dark_mode = self.dark_mode;

                            let card = list.item(cx, item_id, live_id!(VoiceCard));

                            // Set voice data
                            card.label(ids!(avatar.avatar_initial)).set_text(cx, &initial);
                            card.label(ids!(voice_info.voice_name)).set_text(cx, &name);
                            card.label(ids!(voice_info.voice_meta.voice_language)).set_text(cx, &language);
                            card.label(ids!(voice_info.voice_meta.voice_type)).set_text(cx, type_text);

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
                                CloneTaskStatus::Completed => ("Completed", vec4(0.16, 0.65, 0.37, 1.0)),
                                CloneTaskStatus::Processing => ("Processing", vec4(0.39, 0.40, 0.95, 1.0)),
                                CloneTaskStatus::Pending => ("Pending", vec4(0.6, 0.6, 0.65, 1.0)),
                                CloneTaskStatus::Failed => ("Failed", vec4(0.8, 0.2, 0.2, 1.0)),
                                CloneTaskStatus::Cancelled => ("Cancelled", vec4(0.5, 0.5, 0.5, 1.0)),
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
                                let progress_text = format!("Progress: {:.0}%", task_progress * 100.0);
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
                                card.button(ids!(bottom_row.actions.cancel_btn)).set_text(cx, "停止");
                            } else {
                                card.button(ids!(bottom_row.actions.cancel_btn)).set_text(cx, "取消");
                            }
                            card.button(ids!(bottom_row.actions.delete_btn)).set_visible(cx, can_delete);

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

    /// Switch to a different page and update UI accordingly
    fn switch_page(&mut self, cx: &mut Cx, page: AppPage) {
        if self.current_page == page {
            return; // Already on this page
        }

        self.current_page = page;
        self.add_log(cx, &format!("[INFO] [ui] Switching to {:?} page", page));

        // Update navigation button states
        let (tts_active, library_active, clone_active) = match page {
            AppPage::TextToSpeech => (1.0, 0.0, 0.0),
            AppPage::VoiceLibrary => (0.0, 1.0, 0.0),
            AppPage::VoiceClone | AppPage::TaskDetail => (0.0, 0.0, 1.0),
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

        // Show audio player bar only on TTS page
        self.view
            .view(ids!(content_wrapper.audio_player_bar))
            .set_visible(cx, show_tts);

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
                main_content
                    .left_column
                    .content_area
                    .input_section
                    .input_container
                    .text_input
            ))
            .text();
        let count = text.chars().count();
        let label = format!("{} / 5,000 characters", count);
        self.view
            .label(ids!(
                main_content
                    .left_column
                    .content_area
                    .input_section
                    .bottom_bar
                    .char_count
            ))
            .set_text(cx, &label);
    }

    fn set_generate_button_loading(&mut self, cx: &mut Cx, loading: bool) {
        // Update button text
        let button_text = if loading {
            "Generating..."
        } else {
            "Generate Speech"
        };
        self.view
            .button(ids!(
                content_wrapper
                    .main_content
                    .left_column
                    .content_area
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
                        .input_section
                        .bottom_bar
                        .generate_section
                        .generate_spinner
                ))
                .animator_play(cx, ids!(spin.off));
        }

        self.view.redraw(cx);
    }

    fn update_player_bar(&mut self, cx: &mut Cx) {
        // Update status label
        let status_text = match &self.tts_status {
            TTSStatus::Idle => "Ready",
            TTSStatus::Generating => "Generating...",
            TTSStatus::Playing => "Playing",
            TTSStatus::Ready => "Audio Ready",
            TTSStatus::Error(msg) => msg.as_str(),
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
        if !self.stored_audio_samples.is_empty() && self.stored_audio_sample_rate > 0 {
            let duration_secs =
                self.stored_audio_samples.len() as f32 / self.stored_audio_sample_rate as f32;
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
        }

        self.view.redraw(cx);
    }

    fn update_playback_progress(&mut self, cx: &mut Cx) {
        // Calculate total duration and current position
        if self.stored_audio_samples.is_empty() || self.stored_audio_sample_rate == 0 {
            return;
        }

        let total_duration =
            self.stored_audio_samples.len() as f32 / self.stored_audio_sample_rate as f32;
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
                .controls_panel
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
            "*No log entries*".to_string()
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

        let dataflow_path = PathBuf::from("apps/mofa-tts/dataflow/tts.yml");
        if !dataflow_path.exists() {
            self.add_log(cx, "[ERROR] [tts] Dataflow file not found: apps/mofa-tts/dataflow/tts.yml");
            return;
        }

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
                "[WARN] [tts] Bridge not connected. Please start MoFA first.",
            );
            return;
        }

        let text = self
            .view
            .text_input(ids!(
                main_content
                    .left_column
                    .content_area
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

        let voice_selector = self.view.voice_selector(ids!(
            content_wrapper
                .main_content
                .left_column
                .content_area
                .controls_panel
                .voice_section
                .voice_selector
        ));

        let voice_id = voice_selector
            .selected_voice_id()
            .unwrap_or_else(|| "Luo Xiang".to_string());

        // Get full voice info to check if it's a custom voice
        let voice_info = voice_selector.get_voice(&voice_id);

        self.add_log(cx, &format!("[INFO] [tts] Using voice: {}", voice_id));
        self.add_log(cx, "========== VOICE DEBUG START ==========");

        // Debug: log voice source
        if let Some(ref v) = voice_info {
            self.add_log(cx, &format!("[DEBUG] [tts] Voice source: {:?}", v.source));
            self.add_log(cx, &format!("[DEBUG] [tts] Has GPT weights: {}", v.gpt_weights.is_some()));
            self.add_log(cx, &format!("[DEBUG] [tts] Has SoVITS weights: {}", v.sovits_weights.is_some()));
        } else {
            self.add_log(cx, "[DEBUG] [tts] Voice info is None - voice not found in selector");
        }
        self.add_log(cx, "========== VOICE DEBUG END ==========");

        // Clear previous audio
        self.stored_audio_samples.clear();
        self.stored_audio_sample_rate = 32000;

        self.tts_status = TTSStatus::Generating;
        self.set_generate_button_loading(cx, true);
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

        // Send prompt to dora
        let send_result = self
            .dora
            .as_ref()
            .map(|d| d.send_prompt(&prompt))
            .unwrap_or(false);

        if send_result {
            self.add_log(cx, "[INFO] [tts] Prompt sent to TTS engine");
        } else {
            self.add_log(cx, "[ERROR] [tts] Failed to send prompt to Dora");
            self.tts_status = TTSStatus::Error("Failed to send prompt".to_string());
            self.set_generate_button_loading(cx, false);
            self.update_player_bar(cx);
        }

        if let Some(player) = &self.audio_player {
            player.stop();
        }
    }

    fn toggle_playback(&mut self, cx: &mut Cx) {
        if self.tts_status == TTSStatus::Playing {
            // Pause
            if let Some(player) = &self.audio_player {
                player.pause();
            }
            self.tts_status = TTSStatus::Ready;
            self.add_log(cx, &format!("[INFO] [tts] Playback paused at {:.1}s", self.audio_playing_time));
        } else if !self.stored_audio_samples.is_empty() {
            // Check if we're resuming from a paused state or starting fresh
            let total_duration = self.stored_audio_samples.len() as f64 / self.stored_audio_sample_rate as f64;
            let is_resuming = self.audio_playing_time > 0.1
                && self.audio_playing_time < (total_duration - 0.1);

            if let Some(player) = &self.audio_player {
                // Always stop and clear buffer first to avoid audio overlap
                player.stop();

                if is_resuming {
                    // Resume from paused position - write remaining audio from current position
                    let current_sample_index = (self.audio_playing_time * self.stored_audio_sample_rate as f64) as usize;
                    if current_sample_index < self.stored_audio_samples.len() {
                        let remaining_samples = &self.stored_audio_samples[current_sample_index..];
                        player.write_audio(remaining_samples);
                        self.add_log(cx, &format!("[INFO] [tts] Resuming playback from {:.1}s", self.audio_playing_time));
                    }
                } else {
                    // Start from beginning
                    player.write_audio(&self.stored_audio_samples);
                    self.audio_playing_time = 0.0;
                    self.update_playback_progress(cx);
                    self.add_log(cx, "[INFO] [tts] Playing audio...");
                }
            }
            self.tts_status = TTSStatus::Playing;
        } else {
            self.add_log(cx, "[WARN] [tts] No audio to play");
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

    fn download_audio(&mut self, cx: &mut Cx) {
        if self.stored_audio_samples.is_empty() {
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
        match self.write_wav_file(&download_path) {
            Ok(_) => {
                self.add_log(
                    cx,
                    &format!("[INFO] [tts] Audio saved to: {}", download_path.display()),
                );
                // Show success toast
                self.show_toast(cx, "Downloaded successfully!");
            }
            Err(e) => {
                self.add_log(cx, &format!("[ERROR] [tts] Failed to save audio: {}", e));
            }
        }
    }

    fn write_wav_file(&self, path: &PathBuf) -> std::io::Result<()> {
        use std::io::Write;

        let sample_rate = self.stored_audio_sample_rate;
        let num_channels: u16 = 1;
        let bits_per_sample: u16 = 16;
        let byte_rate = sample_rate * (num_channels as u32) * (bits_per_sample as u32) / 8;
        let block_align: u16 = num_channels * bits_per_sample / 8;
        let data_size = (self.stored_audio_samples.len() * 2) as u32;
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
        for &sample in &self.stored_audio_samples {
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
    }

    /// Refresh voice library
    fn refresh_voice_library(&mut self, cx: &mut Cx) {
        self.load_voice_library(cx);
        self.show_toast(cx, "Voice library refreshed");
    }

    /// Filter voices based on search query
    fn get_filtered_voices(&self) -> Vec<Voice> {
        if self.library_search_query.is_empty() {
            self.library_voices.clone()
        } else {
            let query = self.library_search_query.to_lowercase();
            self.library_voices
                .iter()
                .filter(|v| {
                    v.name.to_lowercase().contains(&query)
                        || v.language.to_lowercase().contains(&query)
                })
                .cloned()
                .collect()
        }
    }

    /// Update library display
    fn update_library_display(&mut self, cx: &mut Cx) {
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
                "暂无音色，点击「刷新」加载"
            } else {
                "未找到匹配的音色"
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

        self.update_library_display(cx);
        self.show_toast(cx, "Voice deleted successfully");
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
                }
                self.preview_playing_voice_id = Some(voice_id.clone());
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
        
        // Find and update task status
        if let Some(task) = self.clone_tasks.iter_mut().find(|t| t.id == task_id) {
            task.status = CloneTaskStatus::Cancelled;
            task.message = Some("Task cancelled by user".to_string());
            
            // Save to disk
            if let Err(e) = task_persistence::save_clone_tasks(&self.clone_tasks) {
                self.add_log(cx, &format!("[ERROR] [clone] Failed to save tasks: {}", e));
            } else {
                self.add_log(cx, "[INFO] [clone] Task status saved to disk");
            }
        }
        
        // TODO: Cancel actual training process
        
        self.update_clone_display(cx);
        self.show_toast(cx, "Task cancelled");
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
            inner
                .view
                .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });

            // Apply dark mode to voice selector
            inner
                .view
                .voice_selector(ids!(
                    content_wrapper
                        .main_content
                        .left_column
                        .content_area
                        .controls_panel
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

            // Apply dark mode to audio player bar
            inner
                .view
                .view(ids!(content_wrapper.audio_player_bar))
                .apply_over(cx, live! { draw_bg: { dark_mode: (dark_mode) } });

            // Apply dark mode to voice clone modal
            inner
                .view
                .voice_clone_modal(ids!(voice_clone_modal))
                .update_dark_mode(cx, dark_mode);

            inner.view.redraw(cx);
        }
    }
}
