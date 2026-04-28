# Contributing to Moxin Voice

Thank you for your interest in contributing to Moxin Studio! This document provides guidelines and instructions for contributing to the project.

## Table of Contents

- [Code of Conduct](#code-of-conduct)
- [Getting Started](#getting-started)
- [Development Setup](#development-setup)
- [Project Structure](#project-structure)
- [Creating a New App](#creating-a-new-app)
- [Code Style](#code-style)
- [Testing](#testing)
- [Pull Request Process](#pull-request-process)
- [Architecture Guidelines](#architecture-guidelines)

## Code of Conduct

Be respectful, professional, and inclusive. We welcome contributions from everyone.

## Getting Started

1. **Fork the repository** on GitHub
2. **Clone your fork** locally:
   ```bash
   git clone https://github.com/YOUR_USERNAME/moxin-studio.git
   cd moxin-studio
   ```
3. **Create a branch** for your feature:
   ```bash
   git checkout -b feature/my-new-feature
   ```

## Development Setup

### Prerequisites

- **Rust** 1.70+ (2021 edition)
- **Cargo** package manager
- **Git** for version control

### Build the Project

```bash
# Development build (faster compilation)
cargo build

# Release build (optimized)
cargo build --release

# Run the application
cargo run
```

### Enable Debug Logging

```bash
RUST_LOG=debug cargo run
```

## Project Structure

Moxin Voice is organized as a Cargo workspace:

```
moxin-tts/
├── apps/moxin-voice/      # Main desktop application UI
├── moxin-voice-shell/     # Launcher / shell integration
├── moxin-widgets/         # Shared UI components
├── moxin-dora-bridge/     # Dora bridge layer
└── node-hub/              # Runtime node binaries
```

See [README.md](README.md) and [docs/getting-started/QUICKSTART_MACOS.md](docs/getting-started/QUICKSTART_MACOS.md) for the current project overview and setup flow.

## Working In The App

Most contributions happen inside the existing Moxin Voice app and runtime pipeline:

1. **Read the README**: Start with [README.md](README.md)
2. **Follow the existing app structure**: Use `apps/moxin-voice` as the main reference
3. **Reuse shared widgets**: Prefer `moxin-widgets` before adding one-off UI primitives
4. **Support dark mode**: Use the existing `instance dark_mode` shader pattern where applicable
5. **Keep distribution in sync**: If you add or rename runtime nodes, update packaging/bootstrap scripts too

### Quick Example

```rust
// Your app's lib.rs
use moxin_widgets::{MoxinApp, AppInfo};

pub struct MyApp;

impl MoxinApp for MyApp {
    fn info() -> AppInfo {
        AppInfo {
            name: "My App",
            id: "my-app",
            description: "Description of my app"
        }
    }

    fn live_design(cx: &mut Cx) {
        screen::live_design(cx);
    }
}
```

## Code Style

### Rust Conventions

- Follow [Rust API Guidelines](https://rust-lang.github.io/api-guidelines/)
- Use `cargo fmt` before committing
- Run `cargo clippy` and fix warnings
- Use meaningful variable and function names
- Document public APIs with `///` doc comments

### Makepad Conventions

- **Widget IDs**: Use `snake_case` (e.g., `ids!(sidebar_menu)`)
- **Widget Types**: Use `PascalCase` (e.g., `MyAppScreen`)
- **Theme colors**: Use constants from `moxin-widgets::theme`
- **Dark mode**: Always support both light and dark themes

### Naming Patterns

```rust
// Good
ids!(user_menu_overlay.user_profile)
pub struct SettingsScreen { ... }

// Bad
ids!(userMenuOverlay.userProfile)  // camelCase not allowed
pub struct settings_screen { ... }  // should be PascalCase
```

## Testing

### Manual Testing

Since Moxin Studio is a GUI application, testing is primarily manual:

1. **Build succeeds**: `cargo build`
2. **Application runs**: `cargo run`
3. **Test all features**:
   - [ ] FM app navigation works
   - [ ] Settings app navigation works
   - [ ] Dark mode toggle works
   - [ ] Audio device selection works
   - [ ] All buttons respond to clicks
   - [ ] Hover states work correctly

### Unit Tests (Future)

We plan to add unit tests for:

- Data models (`Provider`, `Preferences`)
- Pure logic functions
- AppRegistry operations

## Pull Request Process

1. **Update documentation** if you changed APIs or architecture
2. **Test thoroughly**:
   ```bash
   cargo build
   cargo run
   # Manually test all affected features
   ```
3. **Format your code**:
   ```bash
   cargo fmt
   ```
4. **Check for warnings**:
   ```bash
   cargo clippy
   ```
5. **Write a clear PR description**:
   - What does this PR do?
   - Why is this change needed?
   - How was it tested?
6. **Link to related issues** if applicable
7. **Be responsive** to review feedback

### PR Title Format

Use conventional commit style:

- `feat: Add voice recording feature`
- `fix: Resolve dark mode color issues`
- `docs: Update architecture guide`
- `refactor: Simplify sidebar navigation`
- `style: Format code with rustfmt`

## Architecture Guidelines

### Black-Box App Principle

Apps should be **self-contained** with minimal shell coupling:

✅ **Do:**

- Keep all app logic inside the app crate
- Use the `MoxinApp` trait for registration
- Implement dark mode support
- Own your app's state

❌ **Don't:**

- Access shell internals from apps
- Store app state in the shell
- Create tight coupling with other apps
- Bypass the plugin system

### Theme System

Always use the centralized theme:

```rust
// Good - uses theme constants
use moxin_widgets::theme::*;

draw_bg: {
    fn pixel(self) -> vec4 {
        return mix((PANEL_BG), (PANEL_BG_DARK), self.dark_mode);
    }
}

// Bad - hardcoded colors
draw_bg: { color: #ffffff }  // Don't do this!
```

### State Management

See [STATE_MANAGEMENT_ANALYSIS.md](STATE_MANAGEMENT_ANALYSIS.md) for patterns.

**Key principle**: Shell coordinates, apps own their state.

```rust
// Shell propagates state to apps
impl App {
    fn notify_dark_mode_change(&mut self, cx: &mut Cx) {
        self.ui.settings_screen(ids!(settings_page))
            .update_dark_mode(cx, self.dark_mode);
    }
}
```

## Questions?

- **Project overview**: See [README.md](README.md)
- **macOS setup**: See [docs/getting-started/MACOS_SETUP.md](docs/getting-started/MACOS_SETUP.md)
- **Migration background**: See [docs/development/MLX_CORE_MIGRATION.md](docs/development/MLX_CORE_MIGRATION.md)
- **Bug reports**: Open an issue on GitHub
- **Feature requests**: Open an issue with the `enhancement` label

## License

By contributing, you agree that your contributions will be licensed under the Apache License 2.0.

---

Thank you for contributing to Moxin Studio! 🎉
