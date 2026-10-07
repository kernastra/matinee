# Native application

`apps/matinee-next` is the native Matinee window. Phase 3A gives it one
service runtime, Login, and a minimal authenticated shell. Phase 3B adds
the Player on that same runtime. Phase 3C adds Details, the first screen
that loads Jellyfin artwork, and a small navigation stack. Phase 3D adds
Home, which replaces the minimal shell as the signed-in root. Phase 3E adds
Library as a second root destination. It is not Search, Calendar, Settings,
or Poster Studio.

The shipping Tauri app remains the usable reference. This binary does not
replace it.

## Ownership

```text
GPUI window
  → AppModel (Starting, Unauthenticated, Authenticating, Authenticated, SigningOut)
  → ServiceRuntime (one Tokio runtime)
  → matinee-jellyfin (ReqwestTransport, persist, playback plan, reports)
  → matinee-secrets (KeyringStore in production, MemoryStore in tests and review)
  → root destinations (RootDestination: Home, Library(Movies | Series))
       → HomeModel + HomeScreen (kept alive under pages and other roots)
            → JellyfinClient::home_shelf, one request per shelf
       → LibraryModel + LibraryScreen (created on first use, kept alive)
            → JellyfinClient::library_page / library_views / library_genres
            → Atelier VirtualGrid; ArtworkLoader for the built cards only
  → Navigation<Page> (above the current root: Details, then the Player)
  → DetailsModel + DetailsScreen
       → ArtworkLoader (shared, bounded cache) → JellyfinClient::fetch_artwork
  → PlayerModel + PlayerScreen
       → matinee-player (existing engine; no second backend)
       → ExternalFrameSurface
```

Home is described in [home.md](home.md), Library in [library.md](library.md),
Details in [details.md](details.md).

`PlayerModel` in `apps/matinee-next` is the playback model. `PlayerScreen`
paints it and forwards input. The GPUI view does not choose a source, invent
a URL, or decide when to report. `matinee-player`, `matinee-jellyfin`, and
`matinee-core` do not depend on GPUI. Atelier does not know about Jellyfin
or playback plans.

GPUI does not poll `reqwest` futures, call `block_on`, or construct a Tokio
runtime. `scripts/check-architecture.sh` rejects those patterns. The only
`tokio::runtime` constructor is `apps/matinee-next/src/runtime.rs`.

`ServiceRuntime::spawn` runs the future on that runtime and returns a
`JoinHandle` plus a oneshot receiver. The window awaits the receiver on the
GPUI executor. The handle is the only in-flight slot. Starting another vault
or HTTP task aborts the previous one. Dropping the window aborts it too.
Abort drops the HTTP future at its next await, which cancels an in-flight
`reqwest` call. It does not interrupt `save_session` once that call is
already inside the vault.

Vault reads and writes use `spawn_blocking` so a D-Bus round trip does not
sit on a Tokio worker or the GPUI thread.

Production constructs `KeyringStore` and the namespace
`dev.sean.matinee.jellyfin-session`, account `default`. Review captures set
`MATINEE_PREVIEW` and use `MemoryStore`. Unit tests use `MemoryStore` and
never open the OS vault. The app id is `dev.sean.matinee.next`, distinct from
the shipping Tauri id `dev.sean.matinee`.

## Shutdown

`main` holds one `Arc<ServiceRuntime>` until `AtelierApp::run` returns, then
drops it. The last `Drop` drains final work (below), then calls
`Runtime::shutdown_timeout` (2 seconds), which aborts leftover tasks.
Closing the window aborts that view's handle first. On macOS the process can
outlive the last window; the runtime stays until the process quits.

### Final work and orderly exit

Some work must outlive the screen that started it. Today that is only the
Player's final Jellyfin stop report. It is started with
`ServiceRuntime::spawn_final`, which counts it as pending until it finishes
or is cancelled.

- **Player → Details or Home** (Back, Escape, sign-out, session end). `MatineeRoot::release_player`
  calls `PlayerScreen::finish`, which starts the stop report and returns.
  Nothing waits: the runtime stays alive and delivers it in the background.
- **Window close.** `Window::on_window_should_close` calls the same
  `release_player` before the window is torn down, so the report does not
  depend on drop order. The window then closes. The Close Window command
  removes the window without that check; there `Drop for PlayerScreen`
  starts the same tracked report, so the exit drain below still covers it.
  On Linux and Windows the last window closing quits the app, which
  continues as below.
- **Orderly application exit** (Quit, or the last window closing where that
  quits). GPUI runs its `on_app_quit` handlers before it drops windows. The
  window's handler finishes the Player if it is still open, then calls
  `ServiceRuntime::drain_final`. An application-level handler registered in
  `lib.rs` drains as well, for the case where the window, and its Player,
  closed first. Drain blocks the GPUI thread until pending final work
  finishes or `FINAL_WORK_BOUND` (2 seconds) passes, counted once from the
  first drain however many handlers call it. Exit then continues whether the
  report succeeded, failed, or timed out. A failed or abandoned final report
  is not shown; the window is gone. This is the only place the GPUI thread
  waits on the service runtime, and only during termination. It is not
  `block_on` and does not poll HTTP: it waits on a condition variable that
  the runtime's tasks signal. GPUI's own 100 ms wait for quit futures is
  not used for this, because the handlers return completed futures.
- The runtime is destroyed only after that: on Linux and Windows when `run`
  returns, on macOS when the process exits after the quit handlers.
  `ServiceRuntime`'s `Drop` drains again in case no quit handler ran; after
  an orderly drain it returns at once.
- `Drop for PlayerScreen` calls `finish` too. It is defensive cleanup that
  normally finds the Player already closed and only stops the engine and
  releases resources.

Matinee cannot deliver the final report when the process is killed
(`SIGKILL`, Task Manager end task), crashes, or the OS crashes or loses
power. Jellyfin then keeps the last progress report, at most 10 seconds old
while playing, and expires the session on its own schedule.

If sign-in is aborted after `save_session` has already returned, the vault
keeps the session and the next launch restores it. If it is aborted before
the save, the vault is unchanged.

## Startup

The visible phase begins at `Starting`. The first task is `load_session`.
That read is local. The app does not contact the server to decide whether a
saved session is still valid, and it does not delete a session because the
server is offline.

| Vault result | Visible state |
|---|---|
| No entry | Login |
| A session `load_session` accepts | Home |
| `CorruptSession` | Login, with the corrupt-session sentence. The payload stays in the vault. |
| Any other vault error | Login, with the credential-failure sentence |

Login is not painted during `Starting`, so a restored session does not flash
the form first.

## Login

Fields are Jellyfin server, Username, and Password. The primary action is
Enter Matinee. The server field uses `matinee_jellyfin::normalize_server_url`
and no second parser. An empty or whitespace username is rejected locally
with “Enter your username.” A username that is not empty after trim is sent
as typed. The password may be empty.

An informational HTTP warning appears when the normalized URL is `http` and
the host is not localhost, `127.0.0.1`, or `::1`:

> This server uses unencrypted HTTP. Prefer HTTPS when connecting beyond this computer.

Incomplete or invalid text does not warn. The warning does not block submit.

Tab order follows the element tree: server, username, password, Enter
Matinee. Enter submits when the form can start sign-in. Escape does not
clear the fields. While `Authenticating`, the three fields are disabled
with Atelier’s existing disabled-field behavior, so the visible form cannot
diverge from the request already in flight. The values stay. Focus moves to
the loading button, which remains focusable and is not activatable. Its
label is “Connecting…”. A second submit does not start another task. A
failed attempt returns the fields to normal editing.

Errors shown beside the form are `JellyfinError`'s `Display` text, or the
local validation sentence. The UI does not render `context()`, the password,
the access token, or the authorization header.

The password field is Atelier `TextField::masked`. It paints a bullet per
Unicode scalar. Copy and cut do nothing. Paste still inserts. The controlled
value is the real password so submit can send it. This is not an
operating-system secure field. `SharedString` and the platform input handler
can still see the characters. `AppModel` zeroizes the `String` it owns when
the password changes, when sign-in succeeds, when sign-out succeeds, and on
drop. `Password` zeroizes the copy it owns after the request body is built.
Those owned copies are not a claim that every allocator or GPUI buffer is
wiped.

## Persistence and sign-out

A successful `authenticate` is followed by `save_session` before Home
appears. If the save fails, the phase stays `Unauthenticated` and the notice
is the vault error. The UI does not claim the sign-in succeeded.

Sign-out calls `remove_session`, drops the in-memory `Session`, clears the
password and username, and returns to Login. If removal fails, the notice
is shown and the phase returns to `Authenticated` with the same session.
`SigningOut` is only the in-flight removal. It is not another screen.

Signed in, the window shows Home. The app bar on Home and Library has the
MATINEE wordmark, Home · Movies · Series, the signed-in name, Refresh, and
Sign out (“Signing out…” while the vault
removal runs). The Phase 3C temporary item-ID entry is removed; Home is the
way into Details. If the session cannot make an HTTP client, a small panel
says Jellyfin could not be reached and offers Sign out instead of Home.
Nothing renders the access token.

### Session end

One rule for every authenticated screen:

```text
authenticated request → Unauthorized / AuthRejected (401, 403)
  → the screen reports it → MatineeRoot::expire_session → Login
```

Timeouts, an unreachable server, and malformed answers are not this; they
stay local or partial failures on the screen that saw them.

- **Home.** Any shelf: `HomeEvent::SessionExpired` (see [home.md](home.md)).
- **Library.** Any page, the library list, the genre list, or the one-title
  refresh after playback: `LibraryEvent::SessionExpired` (see
  [library.md](library.md#session-expiration)). What is shown stays.
- **Details.** Any request, primary or secondary (the title, similar,
  collections, seasons, next up, episodes): `details::load` answers
  `SessionEnded` instead of a response, and the screen emits
  `DetailsEvent::SessionExpired`. A dead session never shows up as "More
  like this isn't available". The title's own "session has ended" copy
  remains only as a fallback.
- **Player.** Preparation (the item and the playback plan) is
  authoritative: a 401/403 there becomes `PlanFailure::SessionEnded`, the
  Player shows "Your Jellyfin session has ended." and emits
  `PlayerSessionEnded`. Nothing has played, so no stop report is due.
  Progress and stop reports after playback has started stay non-fatal
  notices and never end the session.

`MatineeRoot::expire_session` is the only place that acts. It finishes
every page (a Player that started sends its stop report, which may itself
fail), drops Home, Library, and the session's client, and shows Login with the
server and username kept, the password cleared, and the notice “Your
Jellyfin session has ended. Sign in again to keep browsing.” The dead
session is removed from the vault as final work on the service runtime
(an orderly exit gives it the usual bounded drain). The removal takes the
vault turn synchronously, before Login is shown, and sign-in saves only
after taking that turn, so a quick sign-in can never be undone by the
removal.

It is idempotent. `AppModel::expire_session` is true only while
`Authenticated`; Home's five shelves reporting together, a Details request
reporting after Home, or a report arriving while the person types or
signs in again all find it false and do nothing: one vault removal, one
transition, one release of pages, and the new form is untouched. Reports
from a Home or page that is no longer current (already closed, or from a
previous sign-in) are dropped before that check.

## Navigation

Two layers (`nav.rs`). **Root destinations** are the places in the app bar:
`RootDestination::Home` and `RootDestination::Library(Movies | Series)`.
`MatineeRoot` keeps `home` and `library` (one Library screen for both
kinds) and which one is current. Each is created once per sign-in (Library
the first time it is chosen) and dropped on sign-out or session end.
Switching roots keeps each exactly as it was. Search, Calendar, and
Settings will be further destinations; the bar lists only ones that exist.

A `Navigation<Page>` stack sits above whichever root is current. A page is
Details or the Player. A Home or Library card pushes Details; Details'
Play, or the Home hero's Play, pushes the Player. Back, Escape, or the
Player's own Leave removes the top page. When the Player is removed, its
final stop report starts (see Shutdown) and the page underneath is told it
is visible again: Details refreshes the title's user data and, for a
series, next up and the visible season, then focuses Play. When the stack
empties, the current root is told it is visible again. If the Player was
opened since (`Navigation::mark_root_stale` / `take_root_stale`), every
root is marked (`StaleRoots`): the visible one acts now — Home reloads its
shelves, Library asks again for the one title that was opened — and the
other acts when it next shows. Scroll positions and focus are untouched.
Sign-out, window close, and exit finish every page.

## Player

Opening the Player (from Details or the Home hero):

1. `item_details` and `playback_plan` run on `ServiceRuntime`. Source
   selection stays in `matinee-jellyfin`: valid DirectPlay, then genuine
   DirectStream/remux, then Transcode, otherwise typed `NoCompatibleSource`.
   The Player does not invent a fallback URL.
2. `Player::open` uses the existing `matinee-player` engine. A missing or
   incompatible libmpv becomes an error on the Player. Packaging that
   library remains Phase 4. The rest of the application stays usable.
3. `set_frame_listener` only sends on a channel. The listener does not
   render GPUI, mutate entities, call back into `Player`, or block. A GPUI
   task then `take_frame`s and publishes the newest BGRA frame to
   `ExternalFrameSurface`. The mailbox keeps one unpublished frame.
4. `LoadRequest` carries the plan URL with credential query pairs removed,
   the resume start, and `Authorization` from `Session::authorization_header`
   when the plan's `StreamAuthorization` is `Session`. The header is built
   at load time and is not stored on `PlayerModel`. The access token is not
   painted or logged.
5. After the file loads, the model sends play and a start report.

Leaving the Player, including window close, sign-out, and application exit,
reports stop when playback had started, stops the engine, aborts in-flight
requests, and drops `Player`. Drop joins the engine's owner and render
threads. The stop report body is captured by `PlayerModel::close` before it
resets anything, after taking position, volume, and mute from the engine's
last snapshot (an idle or failed snapshot is ignored, so teardown never
reports position zero). It carries the last seek target, pause state, and
audio and subtitle indices. The report runs as final work on the same
`ServiceRuntime` (see Shutdown) and is not aborted with the screen's other
tasks. A stop already sent at natural completion is not sent again, and
playback that never started (resolving, loading, or a startup failure) sends
none. There is no second Tokio runtime.

Resume follows the shipping player. A zero position starts at the beginning.
A position inside the last 30 seconds of a known runtime starts over. If the
runtime is unknown, the file is loaded at the resume point and seeked to
the start once duration shows the position is inside that tail.

Reports use `ReportKind` and `PlaybackReport` through `JellyfinClient` on
the service runtime. Start is sent once when the file loads. Progress is
sent immediately on pause, resume, and seek, and every 10 seconds of wall
time while the snapshot is `Playing`. The cadence is checked on the
Player's 250 ms UI tick, so there is no second timer to drift against the
last report. Stop is sent on natural completion and on close. Pressing Play
after the end rewinds and starts a new Start/Stop pair. A failed report is
a notice until the next report succeeds. It does not stop playback or crash
the process. All reports share one `JellyfinClient` per Player.

Scrubber seeks are throttled to one engine seek per 200 ms while the
pointer is moving. Keyboard seeks become absolute targets. Either way the
clock and the report show the target until the engine reaches it, and one
progress report is sent 200 ms after the last seek. Play and pause hold
their requested state until a snapshot agrees, so a stale snapshot does
not send a contradicting report. Audio and subtitle changes update the
reported Jellyfin stream index when the engine's track list matches
Jellyfin's; otherwise the index is omitted. A command the engine rejects
because the snapshot moved on (`InvalidCommand`) is ignored, not fatal.

Controls sit on the picture: title and episode context, timeline, position,
duration, play/pause, ±10 seconds, volume, mute, audio, subtitles,
fullscreen, and Back. Fullscreen is `Window::toggle_fullscreen` from Phase
1C. Escape closes an open track menu, then leaves fullscreen, then returns
to Details or Home. Space, Left, Right, Up, and Down apply when the video surface
is focused and no menu is open. M and F apply when no menu is open. A
focused button, slider, or menu keeps its own keys. Controls show on
pointer and keyboard activity, hide after 5 seconds while playing, and stay
visible while paused, buffering, loading, ended, failed, hovered, or while
a menu is open. Show and hide are instant.

Audio and subtitle rows come from `Snapshot` and use Matinee `TrackId`s.
Labels use title and language. Subtitles are drawn by libmpv into the
frame. Off sets the reported subtitle index to `-1`. Selecting another
subtitle or audio track does not invent a Jellyfin stream index from the
engine id; the negotiated index stays until Off.

The picture uses `ImageFit::Fit` at the window's content size, including
960×620, 1200×760, 1440×900, and fullscreen. The software path remains the
1080p BGRA cap documented in [playback.md](playback.md). The Player does
not load poster or backdrop artwork.

## Artwork

Details was the first native screen to load Jellyfin artwork; Home and
Library use the same loader and cache (sizes and memory in
[home.md](home.md) and [library.md](library.md#artwork)). Addresses
are `ArtworkRequest`s built with `ArtworkUrls::item_request`,
`person_request`, or `image_request`: no token in the URL, and the image
tag on it so a cached copy changes when the artwork does.
`apps/matinee-next/src/artwork.rs` owns fetching. `ArtworkLoader::load`
answers from a shared cache or runs `JellyfinClient::fetch_artwork` on the
service runtime, which sends `Session::authorization_header`, refuses an
address on another server, and caps the body at 16 MiB. Bytes are decoded
on a blocking thread into an Atelier `DecodedImage` (JPEG, PNG, WebP; the
longest side is capped). A 404 is “missing”, not an error; a failed fetch
or decode is “failed”. Both show a designed placeholder.

The cache is bounded by decoded bytes (96 MiB), least-recently-used, and
lives on the GPUI thread. An evicted image is released from every window
atlas; a screen releases images it holds that the cache did not keep when
it is dropped. The screen that asked owns the fetch task: dropping the
screen, or leaving the slot (another season, another title), aborts it, and
an answer for a slot that is gone is dropped.

`scripts/check-architecture.sh` rejects the `api_key` URL builders and the
raw access token in `apps/matinee-next`, and any `fetch_artwork` call
outside `artwork.rs`. It also keeps Jellyfin client construction in the
shell (`view.rs`) and the Player, Player construction in the shell,
Home's shelf requests in `home/load.rs`, and Library's requests in
`library/load.rs`. The Login and Player screens load
no artwork.
