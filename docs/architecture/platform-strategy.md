# Platform Strategy

Matinee Next targets macOS, Windows, and Linux with **one coherent Matinee
design language and platform-appropriate desktop behavior**. It does not try
to make Windows or Linux look like macOS. macOS (SwiftUI/AppKit-grade polish)
is the quality benchmark for craft: typography, spacing rhythm, pointer and
focus states, keyboard navigation, motion, resizing, HiDPI, and reduced
motion.

## What is shared vs. adaptive

| Shared on all platforms | Adapts per platform |
|---|---|
| Visual hierarchy, design tokens, components and their behavior, content structure, motion vocabulary | Window decorations and titlebar, menus, shortcuts and their labels, app lifecycle (quit on last window), system services, reduced-motion source |

## Where platform differences live

Only `crates/atelier-app/src/platform.rs` branches on the OS (`Platform::current()`).
Everything else asks about a *convention*:

| API | macOS | Windows | Linux |
|---|---|---|---|
| `primary_modifier()` | ⌘ | Ctrl | Ctrl |
| `quits_when_last_window_closes()` | no | yes | yes |
| `has_global_menu_bar()` | yes | no | no |

Components never contain platform checks. `scripts/check-architecture.sh`
does not grep for `cfg(target_os)` yet; reviewers should reject it outside
`platform.rs` and `atelier-app`.

## Commands and shortcuts

Apps and menus refer to an `atelier_app::Command`; `Command::shortcut(platform)`
decides the chord once, and `Shortcut::label(platform)` renders it in platform
style (`⌃⌘F` vs `Ctrl+Shift+F`). `None` means the OS or window manager owns
the gesture, so the app must not bind it.

| Command | macOS | Windows | Linux | Menu title (mac / win / linux) |
|---|---|---|---|---|
| Quit | ⌘Q | — (Alt+F4 is the OS's) | Ctrl+Q | Quit *App* / Exit / Quit |
| Close Window | ⌘W | Ctrl+W | Ctrl+W | Close Window |
| Minimize | ⌘M | — (WM) | — (WM) | Minimize |
| Full Screen | ⌃⌘F | F11 | F11 | Enter Full Screen / Full Screen |
| Settings | ⌘, | Ctrl+, | Ctrl+, | Settings… / Settings / Preferences |

Tests assert that no two commands collide on any platform. Tab / Shift-Tab
focus traversal is installed by `atelier-app` on all platforms. On macOS,
"Full Keyboard Access" should eventually decide whether Tab reaches buttons
(see open work).

## Menus

On macOS, `AtelierApp` installs a native menu bar: App menu (Settings, Services,
Quit) and Window menu (Minimize, Full Screen, Close). On Windows and Linux
GPUI 0.2.2 renders no menu bar. `set_menus` is only called where a global
menu bar exists. In-window menus for Windows and Linux (a hamburger or
overflow menu following platform conventions) are Phase 1+ and need an
overlay/popover primitive.

## Window architecture

Phase 0 deliberately uses **native, server-side decorations everywhere**
(`atelier_app::open_window`). The Tauri app's custom frameless chrome and
macOS-style traffic lights on all platforms are *not* recreated. That is the
"one design language, platform-appropriate behavior" decision applied to
windows.

GPUI 0.2.2 capabilities relevant to the eventual design:

| Need | GPUI 0.2.2 API |
|---|---|
| Hide system titlebar, draw content under it | `TitlebarOptions { appears_transparent: true, traffic_light_position }` (macOS, Windows) |
| Client-side decorations (Linux) | `WindowOptions::window_decorations = Client`, `Window::request_decorations`, `window_decorations()`, `set_client_inset` |
| Drag / resize from custom chrome | `start_window_move`, `start_window_resize(ResizeEdge)`, `window_control_area(WindowControlArea::{Drag,Close,Max,Min})` (Windows hit-testing) |
| Window menu / zoom | `show_window_menu`, `zoom_window` |
| Fullscreen | `toggle_fullscreen`, `is_fullscreen` |
| Platform window-control availability | `window_controls()` (fullscreen, maximize, minimize, window_menu) |
| Translucency | `WindowBackgroundAppearance::{Opaque, Transparent, Blurred}` |
| Appearance and scale | `appearance()` (light/dark), `scale_factor()` |
| App grouping on Linux | `WindowOptions::app_id` (set from `AppInfo::app_id`) |

### macOS (target)

- **Unified titlebar:** `appears_transparent: true`, a toolbar row drawn by the
  app, and real traffic lights positioned with `traffic_light_position` to
  align with the toolbar's baseline. No fake traffic lights.
- **Native menus** (already in place), Services menu, ⌘-shortcuts, and the
  Window menu. Native fullscreen via `toggle_fullscreen`.
- Focus: honor key-window state (`is_window_active`) by dimming chrome
  accents when inactive, like AppKit.
- Gaps to verify in Phase 1: vibrancy material behind the sidebar (`Blurred`)
  and its legibility, plus the reduced-transparency preference (no GPUI API).

### Windows (target)

- Keep **native caption buttons** and Snap layouts. If a custom titlebar is
  adopted, use `appears_transparent` plus `window_control_area` so the OS
  still hit-tests Close/Max/Min and Snap flyouts keep working.
- **DPI:** GPUI works in logical pixels and reports `scale_factor()`. Verify
  per-monitor DPI changes when dragging between monitors in Phase 1.
- Conventions: Alt+F4 / Exit, F11 fullscreen, no global menu bar.

### Linux (target)

- **Wayland and X11** are both compiled in (GPUI default features); GPUI picks
  the backend at runtime. Phase 0 was exercised on X11 (Xfce) with Mesa
  lavapipe.
- **Decorations:** server-side by default. On compositors without SSD (GNOME
  Wayland), GPUI falls back to client-side decorations, which the app would
  have to draw. A minimal CSD titlebar component is required before shipping
  on GNOME Wayland.
- **HiDPI:** fractional scaling via the Wayland backend. Verify on Fedora
  (the current primary platform) in Phase 1.
- **Fedora compatibility:** the Tauri app needs `WEBKIT_DISABLE_DMABUF_RENDERER=1`;
  GPUI does not use WebKit, so that workaround does not apply. GPUI needs
  a working Vulkan driver (Mesa is fine).

## Reduced motion and other system preferences

GPUI 0.2.2 exposes no reduced-motion API. Today the preference comes from
`ATELIER_REDUCED_MOTION=1` or an in-app setting
(`atelier_app::set_motion_preference`). Components read only
`UiPreferences::motion`, so wiring the OS source later changes one function:

| Platform | Source to wire (Phase 1) |
|---|---|
| macOS | `NSWorkspace.accessibilityDisplayShouldReduceMotion` (+ change notification) |
| Windows | `SystemParametersInfo(SPI_GETCLIENTAREAANIMATION)` |
| Linux | `org.gnome.desktop.interface enable-animations` / `gtk-enable-animations` (XDG settings portal) |

Light/dark appearance is available (`Window::appearance`) but unused because
Matinee is dark-only. The neutral themes exist to validate the token system.

## CI and build validation

`.github/workflows/rust-workspace.yml`:

- **Lint job (Ubuntu 24.04):** architecture boundaries, `cargo fmt --check`,
  `cargo clippy --workspace --all-targets --locked -D warnings`,
  `cargo test --workspace --locked`.
- **Build matrix:** `ubuntu-24.04`, `macos-15`, and `windows-2022` each run
  `cargo build --workspace --all-targets --locked` and `cargo test --workspace --locked`.
  This compiles every platform-specific path (Metal shaders on macOS, HLSL on
  Windows, Vulkan/Wayland/X11 on Linux) and runs the platform-convention tests
  natively.
- Pinned to Rust 1.90.0; dependencies cached with `Swatinem/rust-cache`.
- The existing `security.yml` (pnpm audit + RustSec for `src-tauri`) is unchanged.

Not done in CI: launching windows (no display or GPU on hosted runners) and
packaging or signing (no distributable yet).

## Open work

- Wire OS reduced-motion and reduced-transparency sources.
- macOS Full Keyboard Access semantics for Tab.
- In-window menus for Windows/Linux; CSD titlebar for GNOME Wayland.
- `Platform` injection so the Gallery can preview other platforms' conventions.
- Packaging per platform (bundle, signing/notarization, AppImage/Flatpak/RPM).
  Native playback adds libmpv packaging; see [playback.md](playback.md).
