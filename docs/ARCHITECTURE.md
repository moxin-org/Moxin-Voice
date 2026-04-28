# Moxin Studio Architecture Guide

This document describes the modular architecture of Moxin Studio, a desktop application built with the Makepad UI framework. Apps are self-contained widgets that plug into a lightweight shell.

## Project Overview

**Moxin Studio** is an AI-powered desktop application for voice chat and model management, built with Rust and Makepad.

- **Version**: 0.1.0
- **Edition**: Rust 2021
- **License**: Apache-2.0
- **Repository**: https://github.com/moxin-org/moxin-studio
- **UI Framework**: Makepad (GPU-accelerated, immediate mode)

## Directory Structure

```
studio/
├── Cargo.toml              # Workspace configuration
├── ARCHITECTURE.md         # This file (English)
├── 架构指南.md              # Architecture guide (Chinese)
├── moxin-widgets/           # Shared reusable widgets (library)
│   ├── src/
│   │   ├── lib.rs          # Module exports and live_design registration
│   │   ├── theme.rs        # Fonts, colors (light/dark), base styles
│   │   ├── app_trait.rs    # MoxinApp trait, AppInfo, AppRegistry
│   │   ├── participant_panel.rs  # Speaker status with waveform
│   │   ├── waveform_view.rs     # FFT-style audio visualization
│   │   ├── log_panel.rs    # Markdown log display
│   │   ├── led_gauge.rs    # Buffer/level gauge
│   │   └── audio_player.rs # Audio playback engine
│   └── resources/
│       └── fonts/          # Manrope font files
├── moxin-studio-shell/      # Main shell application (binary)
│   ├── src/
│   │   ├── main.rs         # Entry point
│   │   ├── lib.rs          # SharedState definition
│   │   ├── app.rs          # Main App widget (~1,120 lines)
│   │   └── widgets/
│   │       ├── mod.rs
│   │       ├── sidebar.rs  # Navigation sidebar (~550 lines)
│   │       ├── log_panel.rs
│   │       └── participant_panel.rs
│   └── resources/
│       ├── fonts/          # Manrope font files
│       ├── icons/          # SVG icons
│       └── moxin-logo.png   # Application logo
└── apps/
    ├── moxin-fm/            # Moxin FM app (library)
    │   ├── src/
    │   │   ├── lib.rs
    │   │   ├── screen.rs   # Main screen (~1,360 lines)
    │   │   ├── moxin_hero.rs # Status bar (~660 lines)
    │   │   └── audio.rs    # Audio device management
    │   └── resources/
    └── moxin-settings/      # Settings app (library)
        ├── src/
        │   ├── lib.rs
        │   ├── screen.rs   # Settings screen (~415 lines)
        │   ├── providers_panel.rs  # Provider list (~320 lines)
        │   ├── provider_view.rs    # Provider config (~640 lines)
        │   ├── add_provider_modal.rs  # Add provider dialog
        │   └── data/
        │       ├── mod.rs
        │       ├── providers.rs    # Provider data types
        │       └── preferences.rs  # User preferences
        └── resources/
            └── icons/      # Provider icons
```

## Crate Dependencies

```
moxin-studio-shell (binary)
├── makepad-widgets
├── moxin-widgets
├── moxin-fm (optional, default enabled)
├── moxin-settings (optional, default enabled)
├── cpal (audio)
├── tokio (async runtime)
├── parking_lot (synchronization)
├── serde, serde_json (serialization)
├── dirs (user directories)
├── sysinfo (system metrics)
└── log, ctrlc

moxin-fm (library)
├── makepad-widgets
├── moxin-widgets
├── cpal
├── parking_lot
├── sysinfo
└── log

moxin-settings (library)
├── makepad-widgets
├── moxin-widgets
├── serde, serde_json
├── dirs
├── parking_lot
└── log

moxin-widgets (library)
├── makepad-widgets
├── cpal
├── parking_lot
├── crossbeam-channel
└── log
```

## Architecture Principles

### Plugin System: MoxinApp Trait

Apps implement the `MoxinApp` trait for standardized registration:

```rust
// moxin-widgets/src/app_trait.rs
pub trait MoxinApp {
    fn info() -> AppInfo where Self: Sized;  // Metadata
    fn live_design(cx: &mut Cx);             // Widget registration
}

pub struct AppInfo {
    pub name: &'static str,        // Display name
    pub id: &'static str,          // Unique ID
    pub description: &'static str, // Description
}

pub struct AppRegistry {
    apps: Vec<AppInfo>,  // Runtime app metadata
}
```

**Usage in Apps:**

```rust
impl MoxinApp for MoxinFMApp {
    fn info() -> AppInfo {
        AppInfo { name: "Moxin FM", id: "moxin-fm", description: "..." }
    }
    fn live_design(cx: &mut Cx) { screen::live_design(cx); }
}
```

**Usage in Shell:**

```rust
impl LiveRegister for App {
    fn live_register(cx: &mut Cx) {
        <MoxinFMApp as MoxinApp>::live_design(cx);
    }
}
```

> **Note**: Widget types still require compile-time imports due to Makepad's `live_design!` macro.
> The trait provides standardized metadata and registration, not runtime loading.

### Core Principle: Black-Box Apps

Apps are self-contained widgets. The shell knows nothing about their internal structure.

**Shell responsibilities:**

- Window chrome (title bar, buttons)
- Navigation (sidebar, tab bar)
- App switching (visibility toggling)
- Widget registration

**Shell must NOT:**

- Know about app-internal widgets
- Handle app-specific events
- Store app-specific state

**App responsibilities:**

- All internal UI layout
- All internal events
- All internal state
- Own resource files

### Minimal Coupling (4 Points Only)

#### 1. Import Statement

```rust
// moxin-studio-shell/src/app.rs
use moxin_fm::screen::MoxinFMScreen;
use moxin_settings::screen::SettingsScreen;
```

#### 2. Widget Registration (Order Matters!)

```rust
impl LiveRegister for App {
    fn live_register(cx: &mut Cx) {
        makepad_widgets::live_design(cx);
        moxin_widgets::live_design(cx);           // Shared first
        moxin_studio_shell::widgets::sidebar::live_design(cx);
        moxin_studio_shell::widgets::log_panel::live_design(cx);
        moxin_studio_shell::widgets::participant_panel::live_design(cx);
        moxin_fm::live_design(cx);                // Then apps
        moxin_settings::live_design(cx);
    }
}
```

#### 3. Widget Instantiation

```rust
live_design! {
    content = <View> {
        flow: Overlay
        fm_page = <MoxinFMScreen> {
            width: Fill, height: Fill
            visible: true
        }
        settings_page = <SettingsScreen> {
            width: Fill, height: Fill
            visible: false
        }
    }
}
```

#### 4. Visibility Toggling

```rust
// Navigation via apply_over
self.ui.view(ids!(content.fm_page)).apply_over(cx, live!{ visible: true });
self.ui.view(ids!(content.settings_page)).apply_over(cx, live!{ visible: false });
self.ui.redraw(cx);
```

## Widget Hierarchy

```
Window (1400x900)
├── Dashboard (base layer)
│   ├── Header
│   │   ├── Hamburger Button (21x21)
│   │   ├── Logo (40x40)
│   │   ├── Title "Moxin Studio"
│   │   └── User Profile Container
│   └── Content Area
│       └── Main Content (Overlay)
│           ├── fm_page (MoxinFMScreen)
│           │   ├── MoxinHero (status bar)
│           │   │   ├── Action Section (Start/Stop)
│           │   │   ├── Connection Section
│           │   │   ├── Buffer Section
│           │   │   ├── CPU Section
│           │   │   └── Memory Section
│           │   ├── Participant Container
│           │   │   ├── Student 1 Panel
│           │   │   ├── Student 2 Panel
│           │   │   └── Tutor Panel
│           │   ├── Chat Container
│           │   └── Audio Control Panel
│           ├── app_page (placeholder)
│           └── settings_page (SettingsScreen)
│               ├── ProvidersPanel (left)
│               ├── VerticalDivider
│               ├── ProviderView (right)
│               └── AddProviderModal (overlay)
├── Tab Overlay (modal layer)
│   ├── Tab Bar
│   └── Tab Content
├── Sidebar Menu Overlay (slide animation)
│   └── Sidebar
│       ├── Moxin FM Tab
│       ├── App List (1-20)
│       │   ├── Apps 1-4 (always visible)
│       │   ├── Pinned App (for Show More selection)
│       │   ├── Show More Button
│       │   └── More Apps Section (5-20, collapsible)
│       └── Settings Tab
├── User Menu Overlay
└── Sidebar Trigger Overlay (28x28)
```

## State Management

### Shell State (app.rs)

```rust
pub struct App {
    #[live] ui: WidgetRef,

    // Menu states
    #[rust] user_menu_open: bool,
    #[rust] sidebar_menu_open: bool,

    // Tab system
    #[rust] open_tabs: Vec<TabId>,       // TabId::Profile, TabId::Settings
    #[rust] active_tab: Option<TabId>,

    // Dark mode theming
    #[rust] dark_mode: bool,             // Current theme state
    #[rust] dark_mode_anim: f64,         // Animation progress (0.0-1.0)
    #[rust] dark_mode_animating: bool,   // Animation in progress

    // Responsive layout
    #[rust] last_window_size: DVec2,

    // Sidebar animation
    #[rust] sidebar_animating: bool,
    #[rust] sidebar_animation_start: f64,
    #[rust] sidebar_slide_in: bool,

    // App registry
    #[rust] app_registry: AppRegistry,   // Registered apps metadata
}

// Type-safe tab identifiers (replaces magic strings)
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TabId {
    Profile,
    Settings,
}
```

### State Management Pattern: Shell Coordinator

> **Note**: Traditional centralized state (Redux/Zustand) is NOT feasible in Makepad.
> See `STATE_MANAGEMENT_ANALYSIS.md` for detailed analysis.

**Recommended pattern**: Shell owns shared state, propagates via WidgetRef methods:

```rust
impl App {
    fn notify_dark_mode_change(&mut self, cx: &mut Cx, dark_mode: f64) {
        // Propagate to all apps via their Ref methods
        self.ui.mo_fa_fmscreen(ids!(fm_page)).update_dark_mode(cx, dark_mode);
        self.ui.settings_screen(ids!(settings_page)).update_dark_mode(cx, dark_mode);
    }
}
```

| What Works        | What Doesn't     |
| ----------------- | ---------------- |
| Shell owns state  | Redux Store<T>   |
| WidgetRef methods | Arc<Mutex<T>>    |
| Event propagation | Context/Provider |
| File persistence  | Zustand hooks    |

### Sidebar State (sidebar.rs)

```rust
pub struct Sidebar {
    #[deref] view: View,
    #[rust] more_apps_visible: bool,
    #[rust] selection: Option<SidebarSelection>,
    #[rust] pinned_app_name: Option<String>,
}

pub enum SidebarSelection {
    MoxinFM,
    App(usize),  // 1-20
    Settings,
}
```

### Settings State (screen.rs)

```rust
pub struct SettingsScreen {
    #[deref] view: View,
    #[rust] preferences: Option<Preferences>,
    #[rust] selected_provider_id: Option<ProviderId>,
}
```

### Shared State (lib.rs)

```rust
pub struct SharedState {
    pub buffer_fill: f64,
    pub is_connected: bool,
    pub cpu_usage: f32,
    pub memory_usage: f32,
}

pub type SharedStateRef = Arc<Mutex<SharedState>>;
```

## Animation System

### Sidebar Slide Animation

```rust
// Animation parameters
const ANIMATION_DURATION: f64 = 0.2;  // 200ms
const SIDEBAR_WIDTH: f64 = 180.0;

// Ease-out cubic easing
let eased = 1.0 - (1.0 - progress).powi(3);

// Position calculation
let x = if slide_in {
    -SIDEBAR_WIDTH * (1.0 - eased)  // -180 -> 0
} else {
    -SIDEBAR_WIDTH * eased           // 0 -> -180
};

// Apply via abs_pos
self.ui.view(ids!(sidebar_menu_overlay)).apply_over(cx, live!{
    abs_pos: (dvec2(x, 52.0))
});
```

## Theme System

### Fonts (Multi-language Support)

```rust
// All fonts support: Latin, Chinese (LXGWWenKai), Emoji (NotoColorEmoji)
FONT_REGULAR    // Normal text
FONT_MEDIUM     // Slightly bolder
FONT_SEMIBOLD   // Section headers
FONT_BOLD       // Titles
```

### Color Palette

#### Light Mode (Default)

```rust
DARK_BG = #f5f7fa        // Page background
PANEL_BG = #ffffff       // Card/panel background
ACCENT_BLUE = #3b82f6    // Primary action
ACCENT_GREEN = #10b981   // Success/active
TEXT_PRIMARY = #1f2937   // Main text
TEXT_SECONDARY = #6b7280 // Muted text
BORDER = #e5e7eb         // Border color
HOVER_BG = #f1f5f9       // Hover background
```

#### Dark Mode

```rust
DARK_BG_DARK = #0f172a       // Page background (dark)
PANEL_BG_DARK = #1f293b      // Card/panel background (dark)
ACCENT_BLUE_DARK = #60a5fa   // Primary action (brighter)
TEXT_PRIMARY_DARK = #f1f5f9  // Main text (dark)
TEXT_SECONDARY_DARK = #94a3b8 // Muted text (dark)
BORDER_DARK = #334155        // Border color (dark)
HOVER_BG_DARK = #334155      // Hover background (dark)
```

### Dark Mode Implementation

Widgets use `instance dark_mode` with shader `mix()`:

```rust
draw_bg: {
    instance dark_mode: 0.0  // 0.0=light, 1.0=dark
    fn get_color(self) -> vec4 {
        return mix((PANEL_BG), (PANEL_BG_DARK), self.dark_mode);
    }
}
```

**Important**: Theme constants work in `live_design!{}` but NOT in shader `fn pixel()`.
Use `vec4()` literals for colors inside shader functions.

### Runtime Color Updates

**Hex colors do NOT work in `apply_over()`!** Use `vec4()`:

```rust
// ❌ Fails
self.view.apply_over(cx, live!{ draw_bg: { color: #1f293b } });

// ✅ Works
self.view.apply_over(cx, live!{ draw_bg: { color: (vec4(0.12, 0.16, 0.23, 1.0)) } });
```

## Data Models

### Provider Configuration

```rust
pub enum ProviderType {
    OpenAi,
    DeepSeek,
    AlibabaCloud,
    Custom,
}

pub enum ProviderConnectionStatus {
    Disconnected,
    Connecting,
    Connected,
    Error(String),
}

pub struct Provider {
    pub id: ProviderId,
    pub name: String,
    pub url: String,
    pub api_key: Option<String>,
    pub provider_type: ProviderType,
    pub enabled: bool,
    pub models: Vec<String>,
    pub is_custom: bool,
    pub connection_status: ProviderConnectionStatus,
}
```

### Audio Device Info

```rust
pub struct AudioDeviceInfo {
    pub name: String,
    pub is_default: bool,
}
```

## Creating a New App

### Step 1: Create Crate Structure

```
apps/my-app/
├── Cargo.toml
├── src/
│   ├── lib.rs
│   └── screen.rs
└── resources/
    └── icons/
```

### Step 2: Define Cargo.toml

```toml
[package]
name = "my-app"
version.workspace = true
edition.workspace = true

[lib]
path = "src/lib.rs"

[dependencies]
makepad-widgets.workspace = true
moxin-widgets = { path = "../../moxin-widgets" }
```

### Step 3: Create lib.rs

```rust
mod screen;
pub use screen::*;

use makepad_widgets::*;

pub fn live_design(cx: &mut Cx) {
    screen::live_design(cx);
}
```

### Step 4: Create screen.rs

```rust
use makepad_widgets::*;

live_design! {
    use link::theme::*;
    use link::shaders::*;
    use link::widgets::*;
    use moxin_widgets::theme::*;

    pub MyAppScreen = {{MyAppScreen}} {
        width: Fill, height: Fill
        flow: Down
        show_bg: true
        draw_bg: { color: (DARK_BG) }

        // Your UI here
    }
}

#[derive(Live, LiveHook, Widget)]
pub struct MyAppScreen {
    #[deref]
    view: View,

    #[rust]
    my_state: bool,
}

impl Widget for MyAppScreen {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        self.view.handle_event(cx, event, scope);
        // Handle events
    }

    fn draw_walk(&mut self, cx: &mut Cx2d, scope: &mut Scope, walk: Walk) -> DrawStep {
        self.view.draw_walk(cx, scope, walk)
    }
}
```

### Step 5: Register in Shell

Add to `moxin-studio-shell/Cargo.toml`:

```toml
[features]
default = ["moxin-fm", "moxin-settings", "my-app"]
my-app = ["dep:my-app"]

[dependencies]
my-app = { path = "../apps/my-app", optional = true }
```

Add to `moxin-studio-shell/src/app.rs`:

```rust
use my_app::screen::MyAppScreen;

impl LiveRegister for App {
    fn live_register(cx: &mut Cx) {
        // ... existing ...
        my_app::live_design(cx);
    }
}
```

Add to live_design:

```rust
content = <View> {
    flow: Overlay
    fm_page = <MoxinFMScreen> { ... }
    my_app_page = <MyAppScreen> {
        width: Fill, height: Fill
        visible: false
    }
    settings_page = <SettingsScreen> { ... }
}
```

### Step 6: Add Navigation

Add sidebar button in `widgets/sidebar.rs`:

```rust
my_app_tab = <SidebarMenuButton> {
    text: "My App"
    draw_icon: {
        svg_file: dep("crate://self/resources/icons/my-app.svg")
    }
}
```

Add click handler in app.rs:

```rust
if self.ui.button(ids!(sidebar_menu_overlay.sidebar_content.my_app_tab)).clicked(&actions) {
    self.sidebar_menu_open = false;
    self.start_sidebar_slide_out(cx);
    // Toggle visibility...
}
```

## Event Handling Patterns

### Hover Events (Important!)

Hover events (`FingerHoverIn`/`FingerHoverOut`) must be handled **before** the early return for `Event::Actions`:

```rust
fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
    self.view.handle_event(cx, event, scope);

    // Handle hover BEFORE extracting actions
    for item in &items {
        match event.hits(cx, item.area()) {
            Hit::FingerHoverIn(_) => { /* hover effect */ }
            Hit::FingerHoverOut(_) => { /* reset */ }
            _ => {}
        }
    }

    // THEN extract actions
    let actions = match event {
        Event::Actions(actions) => actions.as_slice(),
        _ => return,  // Early return AFTER hover handling
    };

    // Handle clicks
}
```

### Button Clicks

```rust
if self.view.button(ids!(my_button)).clicked(actions) {
    // Handle click
}
```

### View Finger Events

```rust
if self.view.view(ids!(my_view)).finger_up(actions).is_some() {
    // Handle finger up on view
}
```

## Best Practices

1. **Keep apps self-contained**: All UI, state, and events within the app
2. **Use shared widgets for consistency**: Theme, common patterns
3. **Minimize shell coupling**: Only the 4 required points
4. **Register in order**: Dependencies before dependents
5. **Use `apply_over` for visibility**: More reliable than `set_visible()`
6. **Handle hover before early return**: Check event.hits() before extracting actions
7. **Restore selection state**: When sidebar reopens, call `restore_selection_state()`

## Troubleshooting

### Widget Not Rendering

- Check `live_design(cx)` is called in correct order
- Verify import paths in live_design macro
- Ensure `visible: true` is set

### Hover Not Working

- Ensure hover handling is before the `Event::Actions` early return
- Use `Hit::FingerHoverIn` / `Hit::FingerHoverOut`, not `MouseHoverIn`

### Events Not Firing

- Ensure `handle_event` calls `self.view.handle_event(...)`
- Verify button IDs match between live_design and code

### Animation Not Smooth

- Call `self.ui.redraw(cx)` at end of animation update
- Check `sidebar_animating` flag is being checked on every event

## Statistics

- **Total Crates**: 5 (1 binary, 4 libraries)
- **Total Lines**: ~6,500 lines of Rust
- **Apps**: 2 (moxin-fm, moxin-settings)
- **Shared Widgets**: 7 reusable components (fully documented)
- **Theme Colors**: 60+ (light/dark variants)
- **Default Window**: 1400x900 pixels
- **Sidebar Width**: 180 pixels

## Related Documents

| Document                       | Description                             |
| ------------------------------ | --------------------------------------- |
| `APP_DEVELOPMENT_GUIDE.md`     | Step-by-step guide for creating apps    |
| `STATE_MANAGEMENT_ANALYSIS.md` | Why Redux/Zustand don't work in Makepad |
| `CHECKLIST.md`                 | Master refactoring checklist (P0-P3)    |
| `moxin-widgets/src/*.rs`       | Widget rustdoc with usage examples      |

---

_Last Updated: 2026-01-04_
_Refactoring Complete: P0, P1, P2, P3_
