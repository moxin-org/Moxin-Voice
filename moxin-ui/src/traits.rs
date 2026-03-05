//! Base Widget Traits for Moxin UI
//!
//! This module defines common traits that moxin-ui widgets implement
//! for consistent behavior across the component library.
//!
//! ## Traits Overview
//!
//! | Trait | Purpose |
//! |-------|---------|
//! | `MoxinWidget` | Base trait for all moxin-ui widgets |
//! | `Themeable` | Widgets that support dark mode theming |
//! | `DoraConnected` | Widgets that receive data from Dora bridges |
//! | `Maximizable` | Widgets that can expand to fill available space |
//! | `Clearable` | Widgets that can reset their state |
//!
//! ## Usage
//!
//! Widgets implement these traits to gain consistent behavior:
//!
//! ```rust,ignore
//! use moxin_ui::traits::{MoxinWidget, Themeable, Clearable};
//!
//! impl MoxinWidget for ChatPanel {
//!     fn widget_id(&self) -> &str { "chat_panel" }
//!     fn widget_title(&self) -> &str { "Chat" }
//! }
//!
//! impl Themeable for ChatPanelRef {
//!     fn apply_dark_mode(&self, cx: &mut Cx, dark_mode: f64) {
//!         if let Some(mut inner) = self.borrow_mut() {
//!             inner.view.apply_over(cx, live!{
//!                 draw_bg: { dark_mode: (dark_mode) }
//!             });
//!         }
//!     }
//! }
//!
//! impl Clearable for ChatPanelRef {
//!     fn clear(&self, cx: &mut Cx) {
//!         if let Some(mut inner) = self.borrow_mut() {
//!             inner.messages.clear();
//!             inner.view.redraw(cx);
//!         }
//!     }
//! }
//! ```

use makepad_widgets::Cx;

/// Base trait for all moxin-ui widgets.
///
/// Provides widget identification for registry and debugging.
pub trait MoxinWidget {
    /// Unique identifier for this widget type
    fn widget_id(&self) -> &str;

    /// Human-readable title for this widget
    fn widget_title(&self) -> &str;

    /// Optional description of the widget's purpose
    fn widget_description(&self) -> Option<&str> {
        None
    }
}

/// Trait for widgets that support dark mode theming.
///
/// Widgets implementing this trait can have their appearance
/// updated when the global dark mode setting changes.
///
/// # Implementation Notes
///
/// - The `dark_mode` value ranges from 0.0 (light) to 1.0 (dark)
/// - Intermediate values are used during animated transitions
/// - Use `apply_over` with `live!{}` to update shader instance variables
///
/// # Example
///
/// ```rust,ignore
/// impl Themeable for MyWidgetRef {
///     fn apply_dark_mode(&self, cx: &mut Cx, dark_mode: f64) {
///         if let Some(mut inner) = self.borrow_mut() {
///             inner.view.apply_over(cx, live!{
///                 draw_bg: { dark_mode: (dark_mode) }
///             });
///             inner.view.label(id!(title)).apply_over(cx, live!{
///                 draw_text: { dark_mode: (dark_mode) }
///             });
///         }
///     }
/// }
/// ```
pub trait Themeable {
    /// Apply dark mode value to the widget.
    ///
    /// # Arguments
    /// * `cx` - Makepad context for applying UI updates
    /// * `dark_mode` - Animation value (0.0 = light, 1.0 = dark)
    fn apply_dark_mode(&self, cx: &mut Cx, dark_mode: f64);
}

/// Trait for widgets that receive data from Dora bridges.
///
/// These widgets are connected to the Dora dataflow system and
/// need to update their state when new data arrives.
///
/// # Usage
///
/// The shell/app polls shared state and calls these methods:
///
/// ```rust,ignore
/// // In UI timer handler
/// if let Some(messages) = state.chat.read_if_dirty() {
///     chat_panel.on_data_update(cx, &messages);
/// }
/// ```
pub trait DoraConnected {
    /// Called when new data is available from Dora bridge.
    ///
    /// # Arguments
    /// * `cx` - Makepad context
    fn on_dora_connected(&self, cx: &mut Cx);

    /// Called when Dora connection is lost.
    ///
    /// # Arguments
    /// * `cx` - Makepad context
    fn on_dora_disconnected(&self, cx: &mut Cx);

    /// Check if the widget requires Dora connection to function.
    fn requires_dora(&self) -> bool {
        true
    }
}

/// Trait for widgets that can expand to fill available space.
///
/// Maximizable widgets have a toggle between normal and maximized states,
/// typically shown in panel headers.
pub trait Maximizable {
    /// Check if the widget is currently maximized.
    fn is_maximized(&self) -> bool;

    /// Toggle between maximized and normal state.
    ///
    /// # Arguments
    /// * `cx` - Makepad context for triggering layout updates
    fn toggle_maximize(&self, cx: &mut Cx);

    /// Set maximized state explicitly.
    ///
    /// # Arguments
    /// * `cx` - Makepad context
    /// * `maximized` - New maximized state
    fn set_maximized(&self, cx: &mut Cx, maximized: bool);
}

/// Trait for widgets that can reset their state.
///
/// Used when starting a new session or clearing data.
pub trait Clearable {
    /// Clear all widget state and redraw.
    ///
    /// # Arguments
    /// * `cx` - Makepad context for triggering redraw
    fn clear(&self, cx: &mut Cx);
}

/// Trait for widgets with timer-based animations.
///
/// Implements lifecycle management for animations that should
/// stop when the widget is hidden to conserve resources.
pub trait Animated {
    /// Start animations (called when widget becomes visible).
    ///
    /// # Arguments
    /// * `cx` - Makepad context
    fn start_animations(&self, cx: &mut Cx);

    /// Stop animations (called when widget becomes hidden).
    ///
    /// # Arguments
    /// * `cx` - Makepad context
    fn stop_animations(&self, cx: &mut Cx);

    /// Check if animations are currently running.
    fn is_animating(&self) -> bool;
}

/// Trait for widgets that can be focused.
///
/// Used for keyboard navigation and input focus management.
pub trait Focusable {
    /// Check if the widget currently has focus.
    fn has_focus(&self) -> bool;

    /// Request focus for this widget.
    ///
    /// # Arguments
    /// * `cx` - Makepad context
    fn request_focus(&self, cx: &mut Cx);

    /// Release focus from this widget.
    ///
    /// # Arguments
    /// * `cx` - Makepad context
    fn release_focus(&self, cx: &mut Cx);
}

#[cfg(test)]
mod tests {
    // Trait tests would require mock implementations
    // which depend on Makepad types not available in pure unit tests

    #[test]
    fn test_traits_defined() {
        // Placeholder to ensure module compiles
        assert!(true);
    }
}
