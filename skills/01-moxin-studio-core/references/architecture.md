# Architecture map

## 1. Plugin system

Apps implement `MoxinApp` and export their main screen type.

```rust
use moxin_widgets::{AppInfo, MoxinApp};

pub struct MoxinMyApp;

impl MoxinApp for MoxinMyApp {
    fn info() -> AppInfo {
        AppInfo { name: "My App", id: "moxin-myapp", description: "..." }
    }

    fn live_design(cx: &mut Cx) {
        screen::live_design(cx);
    }
}
```

## 2. Four coupling points (required)

1. Import screen type into shell live_design
2. Register widgets in `LiveRegister`
3. Instantiate widget in dashboard content
4. Toggle visibility and lifecycle (timers)

## 3. Core files

- `apps/<app>/src/lib.rs` - app descriptor and exports
- `moxin-studio-shell/src/app.rs` - registry, lifecycle, routing
- `moxin-studio-shell/src/widgets/dashboard.rs` - page instances
- `moxin-studio-shell/src/widgets/sidebar.rs` - navigation

## 4. Compile-time constraint

Makepad requires concrete widget types in `live_design!`; runtime app loading is not supported.
