# Migration Roadmap

The Tauri app stays the shipping product throughout. Each phase begins
**only with explicit instruction**. Phases build the native app alongside the
reference app and reach parity screen by screen; nothing in `src/` or
`src-tauri/` is removed until a replacement is approved.

## Phase 0: Foundation (landed)

Done:

- Rust workspace (`crates/`, `apps/`) beside the Tauri app, with GPUI pinned at `=0.2.2`.
- `atelier-ui`: tokens, validated themes, Text / Icon / Button / IconButton /
  Surface / Stack / FocusRing, motion resolver with reduced motion.
- `atelier-app`: bootstrap, native windows, platform conventions, commands
  and shortcuts, macOS menus, theme and motion coordination.
- `matinee-ui` (Matinee theme), `matinee-player` (boundary only).
- Atelier Gallery and a Matinee Next shell.
- Architecture boundary checks; CI on Linux, macOS, and Windows.
- Native playback feasibility spike: [playback.md](../architecture/playback.md).

## Phase 1A: Interaction controls (landed)

Done, without starting screen migration or playback:

- TextField, SearchField, Switch, Checkbox, SegmentedControl, Slider, each
  with a Gallery story and headless behavior tests.
- Keyboard-versus-pointer focus rings (`focus_visible`).
- OS reduced-motion probes, with the Gallery override kept as a separate layer.
- Matinee fonts bundled in `matinee-ui` (SIL OFL static faces).

### Interaction follow-ups (not started)

These stay inside the interaction model. They are not permission to start screen migration, window chrome, or playback. Overlays landed separately in Phase 1B.

- Blinking text caret. The caret is a solid 1px quad.
- Word-boundary movement that follows platform segmentation. Movement today is whitespace-delimited, not UAX #29.
- Manual IME validation on macOS, Windows, and Linux. Composition is implemented; headless tests do not drive a session.
- Windows reduced-motion detection via the Win32 API. The launch probe spawns PowerShell (`SystemParametersInfo`) and that shell-out is temporary technical debt.

## Phase 1B: Desktop composition and overlays (landed)

Done, without window chrome, screen migration, or playback:

- ScrollView, List/ListRow, Image, ProgressBar, Tooltip, Popover, Menu, ContextMenu, Dialog, Sidebar, Toolbar, SplitView, EmptyState.
- Shared overlay focus (open, move inside, restore on close; dialogs trap Tab) and linear keyboard movement that skips disabled rows.
- A Desktop Composition Gallery story, plus one story per component.
- Submenus, list virtualization, and a third split pane are explicitly deferred.

## Phase 1C: Native window (landed)

Done, without a frame surface, screen migration, or playback:

- `WindowSpec` and per-platform chrome. macOS uses a unified titlebar and native traffic lights. Windows keeps the system caption. Linux keeps server-side decorations.
- Titlebar insets, geometry restore with off-screen correction, generic fullscreen, macOS Hide / Hide Others / Edit menu.
- Window Lab (`apps/atelier-window-lab`). `matinee-next` uses the window infrastructure and stays featureless.
- Still open: macOS pointer-drag of the unified titlebar (no GPUI API), GNOME Wayland client-side decorations, an About panel.

## Phase 1: Framework depth and playback core (remainder, not started)

Framework (`atelier-ui` / `atelier-app`):

1. ~~Bundle Matinee fonts~~ and ~~OS reduced motion~~ (done in 1A).
2. ~~Scroll, list, image, progress, tooltip, and overlays~~ (done in 1B).
   Still open inside this item: submenu menus, virtualized lists, an inspector pane on SplitView.
3. ~~Window chrome~~ (done in 1C). Still open: GNOME Wayland client-side decorations, and macOS dragging of the unified titlebar.
4. Gallery preview axes still open: simulated platform. Scale factor cannot
   be overridden at runtime in GPUI 0.2.2 (the Gallery shows the live value).
   Focus-testing mode landed in 1A.
5. A generic **external frame surface** element: paints an externally produced
   BGRA frame from a latest-frame mailbox via GPUI `RenderImage`. It contains
   no codec or playback knowledge, so it fits `atelier-ui`'s rules. It lets
   `matinee-player` render video without depending on GPUI directly.

Playback (`matinee-player`), following the spike recommendation in
[playback.md](../architecture/playback.md):

7. libmpv engine behind a small Matinee-owned API: load URL, play/pause,
   seek, volume, audio and subtitle track selection, resume position,
   playback-state events. Use mpv's software render API on a render thread,
   feeding the frame surface. Adopt the spike's timing fix
   (`BLOCK_FOR_TARGET_TIME=0`) and delayed image release.
8. Distribution groundwork: LGPL libmpv/FFmpeg builds (`-Dgpl=false`), loaded
   at runtime and pinned and hash-checked per platform in CI. Never
   redistribute distro GPL builds.
9. A Jellyfin device profile for native playback that direct-plays what libmpv
   supports (the WebKit profile forces transcodes and burned-in subtitles).

## Phase 2: Domain extraction (proposed)

- `matinee-core`: domain types (items, libraries, progress, availability
  language), shared by both apps where practical.
- `matinee-jellyfin`: a Rust Jellyfin client mirroring `src/lib/jellyfin.ts`
  behavior and its tests.
- `matinee-integrations`, `matinee-studio`: extract the existing Rust in
  `src-tauri` (calendar, image generation, credentials) into UI-agnostic
  crates. The Tauri app can depend on them, which removes duplication without
  changing its behavior.

## Phase 3: Screens (proposed, screen by screen)

Login → Player (first, since native playback is the main motivation) →
Details → Home → Library → Search → Calendar → Settings → Poster Studio.
Each screen ships only after behavioral parity with the reference app,
including the product rules in `docs/design-spec.md`.

## Phase 4: Distribution and cut-over (proposed)

Per-platform packaging (macOS signing and notarization for every bundled
dylib, Windows installer with an LGPL `libmpv-2.dll`, Linux system libmpv,
Flatpak, or AppImage). Then a beta channel beside the Tauri app, and cut-over
only on explicit approval.

## Later

GPU zero-copy video needs a GPUI extension (a persistent/NV12 texture or a
cross-platform external surface); 4K software frames are too slow (see
[playback.md](../architecture/playback.md)). Also an accessibility tree once
GPUI gains AccessKit support.
