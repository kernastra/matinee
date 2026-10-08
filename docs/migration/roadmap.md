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

**Phase 3C: Native Details — landed.**

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

**Phase 3D: Native Home — landed.**

- Home is the signed-in root: a restored session or a successful sign-in
  lands on it, and Details and the Player open above it. The temporary
  item-ID entry is removed.
- Five independently loaded shelves (Continue Watching, Next Up, Recently
  Added Movies, Recently Added Series, Favorites) through
  `JellyfinClient::home_shelf`, all requested at once, each with its own
  loading, empty, and failure state and stale-answer tickets.
- A deterministic, pinned hero with Resume/Play (the existing Player) and
  Details. No rotation.
- Home stays alive under pages: page and row scroll positions survive, focus
  returns to the opened card, and shelves refresh after playback only.
- An authorization failure on any authenticated request (Home, Details,
  or the Player's preparation) ends the session once, application-wide:
  Login with the form kept, the dead session removed. Reports after
  playback has started stay non-fatal.
- Atelier `Rail` (horizontal row, arrow-key focus movement, reveal on
  focus, vertical wheel left to the page) with a Gallery story;
  `ScrollView` keeps every child so `ScrollControl::reveal_child` can
  address them, and `restrict_to_axis` keeps a sideways swipe over a row
  from scrolling the page.
- Deferred: library shortcuts and "View all" (Library is Phase 3E), Top
  rated and the editorial showcases, Coming soon, hero rotation. See
  [home.md](../architecture/home.md).

**Phase 3E: Native Library — in review.**

- Library is a second root destination beside Home (`RootDestination`),
  reached from a shared app bar (Home · Movies · Series) and from Home's
  Recently Added rows ("View all", newest first). Pages (Details, the
  Player) stack above whichever root is current; Back returns to it.
- Server-side sort (the shipping four, with a title tie-break), Show
  (All, Unwatched, Watched, Favorites), Genre, and — when there is more than
  one — the person's libraries, through typed `LibraryQuery` /
  `LibraryPage` / `LibraryView` / `LibraryGenre` and
  `JellyfinClient::library_page`, `library_views`, `library_genres`.
- Pages of 100 requested as the grid nears the end, with ticketed
  stale-answer protection, retry for a failed page, and an end state. The
  shipping 240-title cap is gone.
- Atelier `VirtualGrid`: responsive fixed-size cells, only visible rows plus
  overscan built, one-tab-stop keyboard focus that survives virtualization
  and resize; Gallery story. `Menu::max_height` for long menus.
- Posters only for built cards through the shared loader and 96 MiB cache;
  query, pages, scroll, and focus survive Details and the Player; playback
  refreshes the one opened title.
- Deferred: year and other filters, card context actions, search (Phase
  3F). See [library.md](../architecture/library.md).

**Phase 3F: Native Search — in review.**

- Search is a third root destination beside Home and Library, in the app
  bar. It is retained: text, results, scroll, and the focused title survive
  Details, the Player, and other roots, and it reconciles only the title that
  was opened after playback. Opening it focuses the field.
- A pure `SearchModel` keeps the typed input, the effective query, and the
  shown query apart; a 300 ms debounce; Enter to search now; two-character
  minimum; query-generation tickets so stale answers never apply; server-side
  pages of 60 with one request in flight.
- Typed `SearchQuery` and `JellyfinClient::search_page` (Movie, Series,
  Episode; `StartIndex`/`Limit`/total). The unpaged `search_library` is gone.
- Reuses Atelier's `VirtualGrid`, `SearchField`, and the Library poster card
  (moved to a shared `media_grid` module; episodes read as series and
  `S2 E5 · name`, Library's movie and series cards are unchanged). Atelier
  gains two generic hooks: `TextField`/`SearchField` accept an owner's
  `FocusHandle` (still a tab stop), and `VirtualGrid::on_edge` reports a key
  that cannot move, so Up from the first row returns to the field.
- Hardening review: stale titles are inert while a new query loads; Enter,
  Refresh, and Try again recover from a failed first page and never race;
  a new query starts at the top; typing back to the held query shows it at
  once; abandoned pages are aborted. Thirteen review scenes; headless GPUI
  tests for focus, keys, clicks, navigation round trips, and scale; HTTP
  boundary tests against a loopback Jellyfin.
- Deferred: People, Collections, a type label on movie and series cards.
  Visual review of the scenes on a real display (headless capture is blank).
  See [search.md](../architecture/search.md), which includes the shipping
  audit.

**Phase 3G: Native Calendar — in review.**

- Calendar is a fourth root destination beside Home, Library, and Search. It
  shows Radarr movie releases and Sonarr episodes by day in a seven-column
  month grid with a release panel for the selected day, the shipping All,
  Movies, and Series filter, and month, today, and day navigation.
- A pure `CalendarModel` with generation tickets per source: one slow source
  never holds back another, a late answer for a month no longer shown is
  ignored, and each source keeps up to four months with a five-minute
  freshness rule (the shipping cache's). A failed source keeps the other
  source's releases, and each failure names its cause.
- Date policy: a Radarr stamp and a Sonarr `airDate` are the days they name,
  never converted through UTC; a Sonarr `airDateUtc` belongs to the viewer's
  local day. The shipping calendar moves both kinds a day early for viewers
  west of UTC; native does not. See [calendar.md](../architecture/calendar.md).
- `matinee-integrations` gains a typed `ReleaseTiming`, a native
  `fetch_calendar` that fails on a non-calendar body, and `fetch_image` for
  cover art with no credential. The shipping payloads are unchanged.
- Covers load through the shared artwork loader for the selected day only.
  Calendar never opens Details or the Player: a Radarr or Sonarr id is not a
  Jellyfin item id.
- Deferred: the 120-day "Next up" strip (shipping's second request), Settings
  (connections come from the shipping app's vault), and the modal detail.
  Visual capture of the review scenes is not available in this environment.

Settings, Poster Studio, and later screens are not started. The compatibility
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
