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
| `reopens_when_activated_without_windows()` | yes | no | no |
| `has_global_menu_bar()` | yes | no | no |
| `restores_window_origin()` | yes | yes | X11 yes, Wayland no |
| `word_key()` / emacs line keys / character palette | Option, yes, yes | Ctrl, no, no | Ctrl, no, no |

Components never contain platform checks. `scripts/check-architecture.sh`
rejects `cfg(target_os)` under `crates/atelier-ui/src` and
`crates/atelier-app/src` except `platform.rs`.

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
| Hide | ⌘H | — | — | Hide |
| Hide Others | ⌥⌘H | — | — | Hide Others |

Tests assert that no two commands collide on any platform. Tab / Shift-Tab
focus traversal is installed by `atelier-app` on all platforms. On macOS,
"Full Keyboard Access" should eventually decide whether Tab reaches buttons
(see open work).

## Menus

On macOS, `AtelierApp` installs a native menu bar: App (Settings, Services,
Hide, Hide Others, Quit), Edit (Undo, Redo, Cut, Copy, Paste, Select All),
and Window (Minimize, Full Screen, Close). Edit items use GPUI `OsAction` so
the system selector is `cut:` / `copy:` / `paste:` / `selectAll:`. Undo and
Redo stay disabled: there is no undo stack, and GPUI notes that `undo:` /
`redo:` do not attach to an `NSTextView`. There is no About item.
`SystemMenuType` only offers Services, and GPUI 0.2.2 has no About-panel API.

On Windows and Linux GPUI 0.2.2 renders no menu bar. `set_menus` is only
called where a global menu bar exists. An in-window stand-in for that menu
bar is still not wired. Phase 1B added the Popover and Menu primitives that
stand-in would use. The same commands and shortcuts still exist.

## Window architecture

Applications pass a `WindowSpec` (title, size, min, optional max, resizable,
restoration key, `ChromeIntent`). `open_window` maps that intent per
platform. Views read `resolve_chrome` for the titlebar band and leading
inset. They do not embed traffic-light offsets. The Tauri app's frameless
chrome is not recreated.

`ChromeIntent::PlatformDefault` and `Unified` draw a unified titlebar on
macOS only. Windows and Linux stay on the system frame even if an app asks
for `Unified`. GPUI has no maximum-size field, so max size is applied when
placing and when saving a normal frame. The OS can still resize past it
during the session.

GPUI 0.2.2 window APIs that this layer actually uses:

| Need | GPUI 0.2.2 API | What Atelier does with it |
|---|---|---|
| Native frame | `WindowDecorations::Server` | Always. Client decorations are not requested. |
| Unified macOS titlebar | `TitlebarOptions { appears_transparent, traffic_light_position }` | macOS unified only. Real traffic lights at (12, 12) in a 38px band; content starts after a 78px leading inset. |
| Windows caption / Snap | default caption when `appears_transparent` is false | Kept. `window_control_area` is not used on Windows, because a custom caption would own hit-testing. |
| Drag / double-click | `WindowControlArea::Drag` (Windows `WM_NCHITTEST` only), `start_window_move` (X11 and Wayland only), `titlebar_double_click` (macOS) | Wired on empty in-client titlebar regions. That path is live only for the macOS unified band. |
| Fullscreen | `toggle_fullscreen`, `is_fullscreen`, `WindowBounds::Fullscreen` | F11 / ⌃⌘F. Escape exits when no dialog, menu, or search field consumed it. Fullscreen bounds are not written to the saved frame. |
| Maximize | `is_maximized`, `WindowBounds::Maximized` | The saved file keeps the last windowed size and a maximized flag. |
| Bounds and displays | `Window::bounds`, `App::displays`, `PlatformDisplay::bounds` | Logical pixels. Off-screen frames are moved so the top edge stays reachable. |
| Scale | `Window::scale_factor` (read-only) | Shown in the Gallery and Window Lab. Not simulated. |
| Lifecycle | `on_window_closed`, `Application::on_reopen`, `hide`, `hide_other_apps` | Windows and Linux quit when the last window closes. macOS stays running and reopens on dock click. |
| Linux app id | `WindowOptions::app_id` | `AppInfo::app_id`. |

Not used, and not papered over: `WindowDecorations::Client`, `set_client_inset`, `show_window_menu`, `zoom_window`, `WindowBackgroundAppearance::Blurred`. GPUI's macOS `on_hit_test_window_control` is a no-op, and `start_window_move` is unimplemented on macOS, so a unified titlebar cannot be dragged by pointer. Double-click still calls `titlebar_double_click`, which honors `AppleActionOnDoubleClick`. Traffic lights cannot be moved while fullscreen (GPUI skips that; Zed issue 4712). There is no public titlebar height; the 38px band is Atelier's layout, not a measured AppKit value.

### macOS

Unified titlebar with native traffic lights and a toolbar row in the band.
Sidebar content starts under that band. No drawn stand-in for the traffic
lights. Native menus are the App, Edit, and Window menus above. Fullscreen
uses the system transition. On macOS 15.3+, GPUI temporarily turns the
transparent titlebar off while fullscreen.

Pointer-dragging the unified band does not move the window. That is a GPUI
0.2.2 gap, not an Atelier drawing fallback. `ChromeIntent::Native` keeps the
opaque system titlebar, which AppKit drags itself, and reports a zero inset.

### Windows

Native caption, Snap layouts, resize, and DPI stay with the system. Atelier
does not draw caption buttons. Alt+F4 and the caption close the window.
F11 is fullscreen. There is no global menu bar. Per-monitor DPI has not been
dragged between displays in this environment; `scale_factor` is the live value.

### Linux

GPUI compiles both Wayland and X11 and prefers Wayland when `WAYLAND_DISPLAY`
is non-empty. Decorations are server-side. The toolbar is ordinary client
content under the window-manager titlebar, so it is not a drag region.
GNOME Wayland often has no SSD; GPUI can fall back to client decorations, and
this layer still does not draw them. A CSD titlebar is required before that
desktop is a supported target.

Saved positions are restored on X11 and ignored on Wayland (the window is
centered, size and maximized flag kept). Wayland position restore is not
reliable. HiDPI fractional scale is a Wayland concern and was not exercised
here; the development session is X11 at scale 1.

Fedora note, unchanged: the Tauri app needs `WEBKIT_DISABLE_DMABUF_RENDERER=1`.
GPUI does not use WebKit. It needs a Vulkan driver (Mesa is fine).

### Restoration and fullscreen

One text file per restoration key under `ATELIER_STATE_DIR`, or the platform
state directory (`~/Library/Application Support/Atelier`, `%APPDATA%/Atelier`,
`$XDG_STATE_HOME/atelier` or `~/.local/state/atelier`). The file stores the
normal origin and size plus a maximized flag. A restored frame that does not
leave its top edge on a display is moved onto the nearest display. Fullscreen
does not overwrite that file. A bounds event that arrives before
`is_fullscreen` flips, or a frame larger than the requested maximum, is
treated as transient and left unsaved. A window that is exactly the display
size is also left unsaved. This is not a settings database.

### Window Lab

`apps/atelier-window-lab` is the manual window harness. It is not a Gallery
story. It shows the resolved platform, insets, scale, frame, fullscreen, and
a toolbar whose empty regions are drag surfaces only when the chrome says so.

## Reduced motion and other system preferences

GPUI 0.2.2 exposes no reduced-motion API and no change notification.
`UiPreferences::motion()` resolves in layers: an app or Gallery override,
then the system probe, then full motion when the probe returns nothing.
`system_motion: None` means detection is unavailable. It does not mean the
OS asked for full motion.

`ATELIER_REDUCED_MOTION` is an override (`1`/`true` reduced, `0`/`false`
full). `set_motion_preference` sets that same override and leaves the system
value in place. Components call `motion()`, not the fields.

| Platform | What the probe actually reads | Limitation |
|---|---|---|
| macOS | `defaults read com.apple.universalaccess reduceMotion` | A missing key is full motion. No live notification. |
| Windows | PowerShell `SystemParametersInfo(0x1042)` (`SPI_GETCLIENTAREAANIMATION`) | **Temporary technical debt.** The probe spawns PowerShell. The replacement should call the Win32 API directly and not start a shell. It still reads client-area animation, not the Ease of Access "show animations" toggle. No `unsafe` in the current probe. |
| Linux | `gsettings get org.gnome.desktop.interface enable-animations`, then KDE `AnimationDurationFactor` (`0` is reduced) via `kreadconfig6`/`kreadconfig5` | Xfce, Sway, and other desktops are unsupported and stay `None`. |

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

- Reduced-transparency source (no GPUI API). Reduced motion is probed at launch only.
- Windows reduced-motion detection still shells out to PowerShell. Replace it with a direct Win32 `SystemParametersInfo` call. See the roadmap.
- macOS Full Keyboard Access semantics for Tab.
- In-window menus for Windows/Linux. GNOME Wayland still needs a CSD titlebar; Phase 1C kept server decorations on purpose.
- macOS unified-titlebar dragging, once GPUI exposes a drag path. About panel, once GPUI has an API for it.
- `Platform` injection so the Gallery can preview other platforms' conventions.
- Packaging per platform (bundle, signing/notarization, AppImage/Flatpak/RPM).
  Native playback adds libmpv packaging; see [playback.md](playback.md).
