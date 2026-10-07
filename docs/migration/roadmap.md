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
- `matinee-ui` (Matinee theme), `matinee-player` (crate reserved; engine landed in 1D).
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
- Window Lab (`apps/atelier-window-lab`). `matinee-next` uses the same window infrastructure. Login arrived in Phase 3A.
- Still open: a macOS client drag region (GPUI 0.2.2 cannot start one; the system titlebar still moves the window), GNOME Wayland client-side decorations, an About panel.

## Phase 1D: Native playback foundation (landed)

Done, without a Player screen, Jellyfin client, or packaging:

- `ExternalFrameSurface` in `atelier-ui`: latest BGRA frame, fit/fill, delayed image release. No codec knowledge.
- `matinee-player`: libmpv behind load / transport / seek / volume / tracks, one owner thread, software render capped at 1080p, runtime-loaded library, typed failure when it is missing.
- Jellyfin device profile `Matinee Native` for the formats this phase opened. Not the WebKit profile.
- Playback Lab (`apps/matinee-playback-lab`) and an External Frame gallery story.
- Linux CI installs libmpv and ffmpeg so integration tests run, and the interactive lab was run on Linux. macOS and Windows CI compile and run pure tests only. They do not install libmpv, so those jobs are not playback validation. The distro build is GPL and is not a redistributable artifact.

Details, licensing, and the 4K software limit: [playback.md](../architecture/playback.md).

## Phase 1: Framework depth (remainder, not started)

Framework (`atelier-ui` / `atelier-app`):

1. ~~Bundle Matinee fonts~~ and ~~OS reduced motion~~ (done in 1A).
2. ~~Scroll, list, image, progress, tooltip, and overlays~~ (done in 1B).
   Still open inside this item: submenu menus, virtualized lists, an inspector pane on SplitView.
3. ~~Window chrome~~ (done in 1C). Still open: GNOME Wayland client-side decorations, and a macOS client drag region. The system titlebar remains the drag surface.
4. Gallery preview axes still open: simulated platform. Scale factor cannot
   be overridden at runtime in GPUI 0.2.2 (the Gallery shows the live value).
   Focus-testing mode landed in 1A.
5. ~~External frame surface~~ (done in 1D).

Playback core landed in 1D. Still open, and not started here: LGPL artifact
pinning once those binaries exist (the loader and the license rules are in
place; no hash is published yet), and a GPUI path for frames larger than
the software cap.

## Phase 2: Domain extraction

**Phase 2A: Domain + Jellyfin foundation — landed.**

- `matinee-core`: items, libraries, progress, artwork identity, technical
  media, and playback plans. No HTTP and no GPUI.
- `matinee-jellyfin`: Rust client for the shipping `src/lib/jellyfin.ts`
  behaviors, converting into domain types. In-memory session only. The
  Matinee Native profile is built in this crate from
  `matinee_core::native_playback()`. The shipping static stream fallback is
  a typed `NoCompatibleSource` error.

**Phase 2B: Native application services — landed.** Screens are not started.

- `matinee-secrets`: OS keyring (Linux Secret Service, macOS Keychain,
  Windows Credential Manager) behind `CredentialStore`. Tests use
  `MemoryStore`. Shipping namespaces are unchanged.
- `matinee-integrations`: Radarr and Sonarr connection tests, calendar
  normalization, partial failure, the five-minute cache, and Home selection.
- `matinee-studio`: Poster Studio credentials, Codex and fal generation,
  manifests, and media-folder artwork. Higgsfield generation is still the
  shipping "not available" error.
- The shipping Tauri commands are adapters over those crates. Command names
  are unchanged. Two new commands feed the calendar UI:
  `fetch_upcoming_releases` and `clear_integration_calendar_cache`.
- `matinee-jellyfin::persist` can save and load one session through
  `CredentialStore`. `authenticate` is unchanged.

Phase 2 is structurally complete. The next work is a screen, not another
service boundary.

## Phase 3: Screens

Login → Player → Details → Home → Library → Search → Calendar → Settings →
Poster Studio. Each screen ships only after behavioral parity with the
reference app, including the product rules in `docs/design-spec.md`.

**Phase 3A: Native application runtime and Login — landed.**

- `apps/matinee-next` owns one Tokio service runtime and calls
  `matinee-jellyfin` there. The UI does not poll `ReqwestTransport`.
- Startup loads `dev.sean.matinee.jellyfin-session` / `default`. A valid
  session opens the authenticated shell. No session opens Login. A corrupt
  payload stays in the vault and Login explains it. A vault failure is
  visible.
- Login: server, username, password, Enter Matinee. HTTP warning, masked
  password, and sign-in errors follow the shipping form. Success saves the
  session before the shell appears. Sign-out removes it, or stays signed in
  when removal fails.
- The signed-in destination is a minimal shell: identity, server, and Sign
  out. It is not Home.

**Phase 3B: Native Player — landed.**

- The authenticated shell has a temporary item-ID entry (Phase 3C points
  it at Details). It is not Home. No server address, user ID, media ID, token, or library name is
  hard-coded.
- `PlayerModel` owns playback state. `PlayerScreen` paints it. The engine
  is the existing `matinee-player`. Frames go through `ExternalFrameSurface`
  with latest-frame-wins and `ImageFit::Fit`.
- The plan path is Session → item → `playback_plan` → DirectPlay, remux, or
  Transcode. `NoCompatibleSource` does not invent a URL. Stream auth uses
  `LoadRequest` headers. The token is not painted.
- Resume uses the plan start and the shipping 30-second completion tail.
  Reports are start, progress on pause/resume/seek, progress every 10
  seconds while playing, and stop on completion or close. A failed report
  is a notice. Play after the end starts a new Start/Stop pair.
- Leaving for the shell sends the final stop without waiting. An orderly
  application exit gives it up to 2 seconds before the service runtime is
  destroyed, then exits regardless. A killed or crashed process cannot.
- A missing libmpv is an error state. Packaging remains Phase 4. The
  software frame path stays capped at 1080p.

**Phase 3C: Native Details — in review.**

- The temporary item entry now opens Details; Play and Resume open the
  Phase 3B Player, and Back from the Player refreshes Details in place.
- Movies and episodes: backdrop, poster or still, title, tagline,
  metadata, scores, genres, overview, credits, Play/Resume with a quiet
  progress line, technical labels, cast, chapters, collections, and more
  like this. Series: next-up Play/Resume, a season picker, and episode rows.
- First native artwork: `ArtworkRequest` plus `fetch_artwork` with the
  session header, decoded off the UI thread into Atelier's `DecodedImage`,
  a 96 MiB shared cache, and cancellation when a slot goes away.
- A small `Navigation<Page>` stack: shell, Details, Player.
- Deferred: watchlist, played toggles, clear progress, play from the
  beginning, logo artwork, chapter images and chapter seek, people and
  collection browsing, autoplay. See [details.md](../architecture/details.md).

Phase 3D (Home) and later screens are not started. The compatibility
builders that still put `api_key` on artwork URLs stay for the shipping
app; the native app is guarded against them.

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
