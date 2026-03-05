# App workflow

## 1. Scaffold a new app

1. Create crate:
   ```bash
   cd apps
   cargo new moxin-myapp --lib
   ```
2. Add dependencies in `apps/moxin-myapp/Cargo.toml`:
   ```toml
   [dependencies]
   makepad-widgets.workspace = true
   moxin-widgets = { path = "../../moxin-widgets" }
   moxin-dora-bridge = { path = "../../moxin-dora-bridge" }
   moxin-settings = { path = "../moxin-settings" }
   ```
3. Implement `MoxinApp` in `apps/moxin-myapp/src/lib.rs`:
   ```rust
   pub struct MoxinMyApp;
   impl MoxinApp for MoxinMyApp { /* info + live_design */ }
   ```
4. Create `apps/moxin-myapp/src/screen/mod.rs` with `live_design!` and `Widget` impl.

## 2. Optional: clone an existing app

- Copy `apps/moxin-debate` and rename the crate and types.
- Update dataflow paths and node IDs.
