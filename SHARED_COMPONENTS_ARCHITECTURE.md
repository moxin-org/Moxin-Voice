# Moxin Studio Shared Components Architecture

## Overview

This document outlines the architecture for extracting reusable components from moxin-studio to enable multiple apps to share UI widgets, Dora bridges, and shell layouts.

## Goals

1. **Reusability** - Share components across moxin-fm, moxin-settings, and future apps
2. **Consistency** - Unified look and behavior across all Moxin apps
3. **Maintainability** - Single source of truth for shared logic
4. **Extensibility** - Easy to add new apps and components

---

## Current State Analysis

### Crate Structure

```
moxin-studio/
├── moxin-dora-bridge/          # Dora node bridges
│   ├── src/bridge.rs          # DoraBridge trait
│   ├── src/dispatcher.rs      # Message routing
│   ├── src/shared_state.rs    # SharedDoraState
│   └── src/widgets/           # Bridge widgets (aec_input, audio_player)
│
├── moxin-widgets/              # Basic shared widgets
│   └── src/app_trait.rs       # MoxinApp trait
│
├── moxin-studio-shell/         # Main app shell
│   └── src/widgets/           # Shell-specific widgets
│
├── apps/moxin-fm/              # Audio streaming app
│   └── src/screen/            # All UI logic embedded here
│       ├── audio_controls.rs
│       ├── chat_panel.rs
│       ├── log_panel.rs
│       ├── role_config.rs
│       └── design.rs          # 2500+ lines of DSL
│
└── apps/moxin-settings/        # Settings management app
    └── src/                   # Provider/model management
```

### Problems with Current Structure

| Issue                           | Impact                                |
| ------------------------------- | ------------------------------------- |
| UI widgets embedded in moxin-fm | Cannot reuse in other apps            |
| Large monolithic design.rs      | Hard to maintain, can't share styles  |
| No widget registry              | No dynamic composition                |
| Direct state access             | Tight coupling between components     |
| App-specific shell              | Each app rebuilds layout from scratch |

---

## Target Architecture

### Crate Structure

```
moxin-studio/
├── moxin-dora-bridge/          # UNCHANGED - Dora node bridges
│   ├── src/bridge.rs
│   ├── src/dispatcher.rs
│   ├── src/shared_state.rs
│   └── src/widgets/
│       ├── aec_input.rs       # AEC mic bridge
│       └── audio_player.rs    # Audio playback bridge
│
├── moxin-ui/                   # NEW - Shared UI component library
│   ├── src/lib.rs
│   ├── src/registry.rs        # Widget registry
│   ├── src/app_data.rs        # Shared app data for scope injection
│   ├── src/theme.rs           # Unified theming
│   ├── src/widgets/           # Reusable UI widgets
│   │   ├── mod.rs
│   │   ├── audio_controls.rs  # Mic, speaker, VU meter
│   │   ├── chat_panel.rs      # Chat display + input
│   │   ├── log_panel.rs       # Filterable log viewer
│   │   ├── role_editor.rs     # Role config editor
│   │   ├── status_bar.rs      # Connection status
│   │   └── dataflow_picker.rs # YAML selector
│   └── src/shell/             # Reusable shell components
│       ├── mod.rs
│       ├── layout.rs          # MoxinShell main layout
│       ├── sidebar.rs         # Collapsible sidebar
│       ├── tab_bar.rs         # Tab navigation
│       └── panel.rs           # Panel container
│
├── moxin-widgets/              # SLIM DOWN - Only base traits
│   └── src/app_trait.rs
│
├── moxin-studio-shell/         # SLIM DOWN - Just app composition
│   └── src/main.rs            # Composes moxin-ui components
│
├── apps/moxin-fm/              # REFACTOR - Use moxin-ui
│   └── src/
│       ├── lib.rs
│       ├── app.rs             # App-specific logic only
│       └── design.rs          # Minimal, imports moxin-ui
│
├── apps/moxin-settings/        # REFACTOR - Use moxin-ui
│   └── src/
│       ├── lib.rs
│       └── app.rs
│
└── apps/moxin-recorder/        # NEW - Future app example
    └── src/
        ├── lib.rs
        └── app.rs             # Composes moxin-ui widgets
```

---

## Key Abstractions

### 1. Widget Registry

```rust
// moxin-ui/src/registry.rs

/// Definition of a registerable widget
#[derive(Clone, Debug)]
pub struct MoxinWidgetDef {
    /// Unique identifier (e.g., "audio_controls", "chat_panel")
    pub id: String,

    /// Display name for UI
    pub title: String,

    /// Category for organization
    pub category: WidgetCategory,

    /// Whether this widget requires a Dora connection
    pub requires_dora: bool,

    /// Whether the widget can be maximized
    pub maximizable: bool,

    /// Default size hints
    pub default_size: WidgetSize,
}

#[derive(Clone, Debug)]
pub enum WidgetCategory {
    Audio,      // Mic, speaker, player
    Chat,       // Chat display, input
    Config,     // Role editors, settings
    Debug,      // Logs, status
    Custom,     // App-specific
}

#[derive(Clone, Debug)]
pub struct WidgetSize {
    pub min_width: f64,
    pub min_height: f64,
    pub preferred_width: f64,
    pub preferred_height: f64,
}

/// Registry for all available widgets
pub struct MoxinWidgetRegistry {
    definitions: HashMap<String, MoxinWidgetDef>,
    order: Vec<String>,
}

impl MoxinWidgetRegistry {
    pub fn new() -> Self { ... }
    pub fn register(&mut self, def: MoxinWidgetDef) { ... }
    pub fn get(&self, id: &str) -> Option<&MoxinWidgetDef> { ... }
    pub fn by_category(&self, cat: WidgetCategory) -> Vec<&MoxinWidgetDef> { ... }
}
```

### 2. Shared App Data (Scope Injection)

```rust
// moxin-ui/src/app_data.rs

use moxin_dora_bridge::SharedDoraState;

/// Shared data passed through Makepad's Scope mechanism
pub struct MoxinAppData {
    /// Dora bridge state (mic levels, connection status, etc.)
    pub dora_state: Arc<SharedDoraState>,

    /// Current theme settings
    pub theme: MoxinTheme,

    /// App-specific configuration
    pub config: AppConfig,

    /// Widget registry
    pub registry: Arc<MoxinWidgetRegistry>,
}

impl MoxinAppData {
    pub fn new(dora_state: Arc<SharedDoraState>) -> Self { ... }
}

// Usage in App:
impl AppMain for MoxinFmApp {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event) {
        // Pass shared data through scope
        self.ui.handle_event(cx, event, &mut Scope::with_data(&mut self.app_data));
    }
}

// Usage in Widget:
impl Widget for AudioControls {
    fn handle_event(&mut self, cx: &mut Cx, event: &Event, scope: &mut Scope) {
        // Access shared data
        if let Some(data) = scope.data.get::<MoxinAppData>() {
            let mic_level = data.dora_state.mic.level();
            // ...
        }
    }
}
```

### 3. Unified Theming

```rust
// moxin-ui/src/theme.rs

pub struct MoxinTheme {
    pub dark_mode: bool,
    pub dark_mode_anim: f64,  // 0.0 = light, 1.0 = dark
    pub accent_color: Vec4,
    pub font_sizes: FontSizes,
}

pub trait ThemeListener {
    fn apply_dark_mode(&self, cx: &mut Cx, dark_mode: f64);
}

// All moxin-ui widgets implement ThemeListener
```

### 4. Composable Shell

```rust
// moxin-ui/src/shell/layout.rs

live_design! {
    pub MoxinShell = {{MoxinShell}} {
        width: Fill, height: Fill
        flow: Down

        // Slots for app-specific content
        header = <ShellHeader> {}

        body = <Dock> {
            left_sidebar = <ShellSidebar> {}
            center = <View> {}      // App injects content here
            right_sidebar = <ShellSidebar> {}
            footer = <View> {}      // App injects content here
        }

        status_bar = <StatusBar> {}
    }
}
```

---

## Development Plan

### Phase 1: Foundation (Priority: HIGH)

**Goal**: Create moxin-ui crate with core infrastructure

| Task                                         | Effort | Dependencies |
| -------------------------------------------- | ------ | ------------ |
| 1.1 Create moxin-ui crate structure          | 2h     | None         |
| 1.2 Implement MoxinWidgetRegistry            | 3h     | 1.1          |
| 1.3 Implement MoxinAppData + scope injection | 4h     | 1.1          |
| 1.4 Implement MoxinTheme                     | 2h     | 1.1          |
| 1.5 Create base widget traits                | 2h     | 1.1          |

**Deliverable**: Empty moxin-ui crate with registry, app data, and theme infrastructure

### Phase 2: Extract Audio Widgets (Priority: HIGH)

**Goal**: Move audio controls from moxin-fm to moxin-ui

| Task                                                | Effort | Dependencies |
| --------------------------------------------------- | ------ | ------------ |
| 2.1 Extract AudioControls widget                    | 4h     | Phase 1      |
| 2.2 Extract VuMeter widget                          | 2h     | Phase 1      |
| 2.3 Extract MicButton widget                        | 2h     | Phase 1      |
| 2.4 Extract SpeakerButton widget                    | 2h     | Phase 1      |
| 2.5 Refactor moxin-fm to use moxin-ui audio widgets | 4h     | 2.1-2.4      |
| 2.6 Test audio functionality                        | 2h     | 2.5          |

**Deliverable**: Audio widgets in moxin-ui, moxin-fm using them

### Phase 3: Extract Chat & Log Widgets (Priority: MEDIUM)

**Goal**: Move chat panel and log panel to moxin-ui

| Task                                           | Effort | Dependencies |
| ---------------------------------------------- | ------ | ------------ |
| 3.1 Extract ChatPanel widget                   | 4h     | Phase 1      |
| 3.2 Extract ChatInput widget                   | 2h     | 3.1          |
| 3.3 Extract LogPanel widget                    | 4h     | Phase 1      |
| 3.4 Extract LogFilter widget                   | 2h     | 3.3          |
| 3.5 Refactor moxin-fm to use extracted widgets | 3h     | 3.1-3.4      |

**Deliverable**: Chat and log widgets in moxin-ui

### Phase 4: Extract Config Widgets (Priority: MEDIUM)

**Goal**: Move role editor and config widgets to moxin-ui

| Task                                           | Effort | Dependencies |
| ---------------------------------------------- | ------ | ------------ |
| 4.1 Extract RoleEditor widget                  | 6h     | Phase 1      |
| 4.2 Extract DataflowPicker widget              | 3h     | Phase 1      |
| 4.3 Extract ProviderSelector widget            | 3h     | Phase 1      |
| 4.4 Refactor moxin-fm to use extracted widgets | 4h     | 4.1-4.3      |

**Deliverable**: Config widgets in moxin-ui

### Phase 5: Shell Components (Priority: MEDIUM)

**Goal**: Create reusable shell layout

| Task                                              | Effort | Dependencies |
| ------------------------------------------------- | ------ | ------------ |
| 5.1 Create MoxinShell layout component            | 6h     | Phase 1      |
| 5.2 Create ShellSidebar component                 | 3h     | 5.1          |
| 5.3 Create ShellHeader component                  | 3h     | 5.1          |
| 5.4 Create StatusBar component                    | 2h     | 5.1          |
| 5.5 Refactor moxin-studio-shell to use MoxinShell | 4h     | 5.1-5.4      |

**Deliverable**: Reusable shell in moxin-ui

### Phase 6: New App Validation (Priority: LOW)

**Goal**: Validate architecture with a new app

| Task                                   | Effort | Dependencies |
| -------------------------------------- | ------ | ------------ |
| 6.1 Create moxin-recorder app skeleton | 2h     | Phase 2-4    |
| 6.2 Compose UI from moxin-ui widgets   | 4h     | 6.1          |
| 6.3 Add recording-specific logic       | 4h     | 6.2          |
| 6.4 Document patterns and learnings    | 2h     | 6.3          |

**Deliverable**: New app demonstrating reusability

---

## Priority Summary

```
HIGH PRIORITY (Do First)
├── Phase 1: Foundation (13h)
└── Phase 2: Audio Widgets (16h)

MEDIUM PRIORITY (Do Next)
├── Phase 3: Chat & Log (15h)
├── Phase 4: Config Widgets (16h)
└── Phase 5: Shell Components (18h)

LOW PRIORITY (Do Later)
└── Phase 6: Validation App (12h)

Total Estimated Effort: ~90 hours
```

---

## Migration Strategy

### Incremental Approach

1. **Create moxin-ui alongside existing code** - No breaking changes initially
2. **Extract one widget at a time** - Validate each extraction works
3. **Update apps to use moxin-ui** - Gradual migration
4. **Remove duplicated code** - Only after validation

### Compatibility Layer

During migration, maintain backward compatibility:

```rust
// moxin-fm/src/screen/audio_controls.rs

// DEPRECATED: Use moxin_ui::widgets::AudioControls instead
#[deprecated(note = "Use moxin_ui::widgets::AudioControls")]
pub use moxin_ui::widgets::AudioControls;
```

### Testing Strategy

| Level       | What to Test              |
| ----------- | ------------------------- |
| Unit        | Widget logic in isolation |
| Integration | Widget + Dora bridge      |
| E2E         | Full app with all widgets |

---

## Risks & Mitigations

| Risk                   | Mitigation                                           |
| ---------------------- | ---------------------------------------------------- |
| Breaking existing apps | Incremental migration, keep old code until validated |
| Over-abstraction       | Start with concrete widgets, generalize later        |
| Performance regression | Profile before/after extraction                      |
| Scope creep            | Stick to phases, don't add features during refactor  |

---

## Success Criteria

1. **moxin-fm works identically** after using moxin-ui widgets
2. **moxin-settings can use** chat/log widgets from moxin-ui
3. **New app (moxin-recorder)** built in < 1 day using moxin-ui
4. **No duplicated widget code** across apps
5. **Consistent theming** across all apps

---

## Next Steps

1. Review and approve this architecture
2. Create moxin-ui crate (Phase 1.1)
3. Start with AudioControls extraction (Phase 2.1)

---

_Document created: 2026-01-17_
_Author: Claude + Human collaboration_
