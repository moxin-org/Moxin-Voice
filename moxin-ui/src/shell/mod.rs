//! Reusable Shell Components for Moxin Applications
//!
//! This module contains shell layout components:
//!
//! ## Components
//!
//! - [`MoxinShell`] - Main application shell layout
//! - [`ShellHeader`] - Application header with title and controls
//! - [`ShellSidebar`] - Collapsible sidebar container
//! - [`StatusBar`] - Connection status and notifications
//!
//! ## Architecture
//!
//! The shell provides a consistent layout structure that apps can customize:
//!
//! ```text
//! ┌─────────────────────────────────────────────┐
//! │  ShellHeader                                │
//! ├─────────┬───────────────────────┬───────────┤
//! │         │                       │           │
//! │ Sidebar │     Center Content    │  Sidebar  │
//! │ (Left)  │     (App-specific)    │  (Right)  │
//! │         │                       │           │
//! ├─────────┴───────────────────────┴───────────┤
//! │  StatusBar                                  │
//! └─────────────────────────────────────────────┘
//! ```
//!
//! ## Usage
//!
//! ```rust,ignore
//! use moxin_ui::shell::*;
//!
//! live_design! {
//!     use moxin_ui::shell::layout::*;
//!     use moxin_ui::shell::header::*;
//!     use moxin_ui::shell::sidebar::*;
//!     use moxin_ui::shell::status_bar::*;
//!
//!     MyApp = <MoxinShell> {
//!         header_slot: <ShellHeader> { title: "My App" }
//!         content_slot: <MyAppContent> {}
//!         status_bar_slot: <StatusBar> {}
//!     }
//! }
//! ```

pub mod layout;
pub mod header;
pub mod sidebar;
pub mod status_bar;

// Re-export main types
pub use layout::{MoxinShell, MoxinShellRef, MoxinShellWidgetExt, MoxinShellAction};
pub use header::{ShellHeader, ShellHeaderRef, ShellHeaderWidgetExt, ShellHeaderAction};
pub use sidebar::{ShellSidebar, ShellSidebarRef, ShellSidebarWidgetExt, ShellSidebarAction, SidebarItem};
pub use status_bar::{StatusBar, StatusBarRef, StatusBarWidgetExt, StatusBarAction, ConnectionStatus};

use makepad_widgets::Cx;

/// Register all shell live designs with Makepad.
///
/// Call this from moxin_ui::live_design().
///
/// NOTE: Currently disabled due to Makepad live_design parsing issues.
/// Shell components are defined but not registered until the parsing issue is resolved.
pub fn live_design(_cx: &mut Cx) {
    // TODO: Investigate why Makepad's live parser fails with "Unexpected token #"
    // when parsing these components.
    //
    // For now, apps should define their own shell layouts inline
    // or use the Rust APIs directly.
    //
    // layout::live_design(cx);
    // header::live_design(cx);
    // sidebar::live_design(cx);
    // status_bar::live_design(cx);
}
