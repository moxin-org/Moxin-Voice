//! Card Component
//!
//! A reusable card component with rounded corners and background color.
//! Based on Moxin.tts card design.
//!
//! Usage
//!
//! ```rust,ignore
//! use moxin_widgets::card::*;
//!
//! live_design! {
//!     use moxin_widgets::card::*;
//!
//!     MyCard = <Card> {
//!         padding: {left: 16, right: 16, top: 16, bottom: 16}
//!
//!         <Label> { text: "Card Content" }
//!     }
//! }
//! ```

use makepad_widgets::*;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;
    use crate::theme::*;

    // Base Card component with Moxin.tts styling
    pub Card = <View> {
        width: Fill, height: Fit
        flow: Down
        padding: {left: 16, right: 16, top: 16, bottom: 16}

        show_bg: true
        draw_bg: {
            instance dark_mode: 0.0

            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);

                // Background color (light/dark mode)
                let bg = mix((MOXIN_BG_SECONDARY), (MOXIN_BG_SECONDARY_DARK), self.dark_mode);

                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 12.0);
                sdf.fill(bg);

                return sdf.result;
            }
        }
    }

    // Card with hover effect
    pub CardHoverable = <View> {
        width: Fill, height: Fit
        flow: Down
        padding: {left: 16, right: 16, top: 16, bottom: 16}
        cursor: Hand

        show_bg: true
        draw_bg: {
            instance dark_mode: 0.0
            instance hover: 0.0

            fn pixel(self) -> vec4 {
                let sdf = Sdf2d::viewport(self.pos * self.rect_size);

                // Base colors
                let bg_light = vec4(0.961, 0.969, 0.980, 1.0);  // f5f7fa
                let bg_dark = vec4(0.145, 0.145, 0.145, 1.0);   // 252525
                let bg = mix(bg_light, bg_dark, self.dark_mode);

                // Hover colors (slightly lighter/darker)
                let hover_light = vec4(0.945, 0.953, 0.965, 1.0);  // Slightly darker
                let hover_dark = vec4(0.165, 0.165, 0.165, 1.0);    // Slightly lighter
                let hover_bg = mix(hover_light, hover_dark, self.dark_mode);

                // Mix base and hover
                let final_bg = mix(bg, hover_bg, self.hover);

                sdf.box(0., 0., self.rect_size.x, self.rect_size.y, 12.0);
                sdf.fill(final_bg);

                return sdf.result;
            }
        }

        animator: {
            hover = {
                default: off
                off = {
                    from: {all: Forward {duration: 0.15}}
                    apply: { draw_bg: {hover: 0.0} }
                }
                on = {
                    from: {all: Forward {duration: 0.15}}
                    apply: { draw_bg: {hover: 1.0} }
                }
            }
        }
    }

    // Compact card with less padding
    pub CardCompact = <Card> {
        padding: {left: 12, right: 12, top: 12, bottom: 12}
    }

    // Large card with more padding
    pub CardLarge = <Card> {
        padding: {left: 20, right: 20, top: 20, bottom: 20}
    }
}
