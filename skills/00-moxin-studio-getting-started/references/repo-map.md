# Repo map

## 1. Top-level directories

- `apps/` - app crates (moxin-fm, moxin-debate, moxin-settings)
- `moxin-studio-shell/` - shell app and navigation
- `moxin-widgets/` - shared widgets and theme
- `moxin-dora-bridge/` - dynamic node bridges and dataflow parsing
- `node-hub/` - Dora nodes (Rust/Python)
- `models/` - model downloads and setup scripts

## 2. New app touch points

- `apps/<app>/` - new crate and UI
- `moxin-studio-shell/Cargo.toml` - add dependency + feature
- `moxin-studio-shell/src/app.rs` - register app + timers
- `moxin-studio-shell/src/widgets/dashboard.rs` - add page widget
- `moxin-studio-shell/src/widgets/sidebar.rs` - add sidebar entry
- `flake.nix` - dataflow directory checks

## 3. Key docs

- `ARCHITECTURE.md` and `APP_DEVELOPMENT_GUIDE.md`
- `MOFA_DORA_ARCHITECTURE.md` and `moxin-studio-dora-integration-checklist.md`
- `DEPLOY_WITH_NIX.md`
