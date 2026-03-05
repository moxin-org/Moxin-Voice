# Shell integration

## 1. Cargo features

Append to `moxin-studio-shell/Cargo.toml`:

```toml
[features]
default = ["moxin-fm", "moxin-settings", "moxin-debate", "moxin-myapp"]
moxin-myapp = ["dep:moxin-myapp"]

[dependencies]
moxin-myapp = { path = "../apps/moxin-myapp", optional = true }
```

## 2. Register widgets

In `moxin-studio-shell/src/app.rs`:

- Import the app type and ScreenWidgetRefExt.
- Register in `LiveHook::after_new_from_doc`.
- Register in `LiveRegister::live_register`.

## 3. Add the page

In `moxin-studio-shell/src/widgets/dashboard.rs`:

- Add `<MyAppScreen>` page in the overlay stack.

## 4. Add navigation

In `moxin-studio-shell/src/widgets/sidebar.rs`:

- Add sidebar button and click handler.
- Ensure selection state updates and pinned app behavior are consistent.

## 5. Lifecycle hooks

- Start/stop timers when switching pages.
- Apply dark mode updates when the theme toggles.
