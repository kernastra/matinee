# Native application

`apps/matinee-next` is the native Matinee window. Phase 3A gives it one
service runtime, Login, and a minimal authenticated shell. Phase 3B adds
the Player on that same runtime. It is not Home, Details, Library, Search,
Calendar, Settings, or Poster Studio.

The shipping Tauri app remains the usable reference. This binary does not
replace it.

## Ownership

```text
GPUI window
  → AppModel (Starting, Unauthenticated, Authenticating, Authenticated, SigningOut)
  → ServiceRuntime (one Tokio runtime)
  → matinee-jellyfin (ReqwestTransport, persist, playback plan, reports)
  → matinee-secrets (KeyringStore in production, MemoryStore in tests and review)
  → PlayerModel + PlayerScreen
       → matinee-player (existing engine; no second backend)
       → ExternalFrameSurface
```

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
drops it. The last `Drop` calls `Runtime::shutdown_timeout` (2 seconds),
which aborts leftover tasks. Closing the window aborts that view's handle
first. On macOS the process can outlive the last window; the runtime stays
until the process quits.

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
| A session `load_session` accepts | Authenticated shell |
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

A successful `authenticate` is followed by `save_session` before the shell
appears. If the save fails, the phase stays `Unauthenticated` and the notice
is the vault error. The UI does not claim the sign-in succeeded.

Sign-out calls `remove_session`, drops the in-memory `Session`, clears the
password and username, and returns to Login. If removal fails, the notice
is shown and the phase returns to `Authenticated` with the same session.
`SigningOut` is only the in-flight removal. It is not another screen.

The shell shows Matinee, “Connected as &lt;username&gt;”, the server, Sign
out, and a temporary playback entry. The entry is labeled temporary Phase
3B infrastructure. It takes a Jellyfin item ID typed by the person at the
keyboard. It does not embed a server address, user ID, media ID, token, or
library name. An empty field asks for an item ID. An ID `ItemId` cannot
parse is rejected before any request. The shell does not render the access
token.

## Player

Opening the Player from the shell:

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

Leaving the Player, including window close and sign-out, reports stop when
playback had started, stops the engine, aborts the progress timer, and
drops `Player`. Drop joins the engine's owner and render threads. The final
stop report is spawned on the same `ServiceRuntime` and is not aborted with
the screen's other tasks. There is no second Tokio runtime.

Resume follows the shipping player. A zero position starts at the beginning.
A position inside the last 30 seconds of a known runtime starts over. If the
runtime is unknown, the file is loaded at the resume point and seeked to
the start once duration shows the position is inside that tail.

Reports use `ReportKind` and `PlaybackReport` through `JellyfinClient` on
the service runtime. Start is sent once when the file loads. Progress is
sent immediately on pause, resume, and seek, and every 10 seconds of wall
time while the snapshot is `Playing`. The cadence timer is
`ServiceRuntime::interval` and is aborted when the Player closes. Stop is
sent on natural completion and on close. A failed report is a notice. It
does not stop playback or crash the process. Scrubber seeks are throttled
to one engine seek per 200 ms while the pointer is moving, and one progress
report after the pointer settles. Keyboard seeks report immediately.

Controls sit on the picture: title and episode context, timeline, position,
duration, play/pause, ±10 seconds, volume, mute, audio, subtitles,
fullscreen, and Back. Fullscreen is `Window::toggle_fullscreen` from Phase
1C. Escape closes an open track menu, then leaves fullscreen, then returns
to the shell. Space, Left, Right, Up, and Down apply when the video surface
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
1080p BGRA cap documented in [playback.md](playback.md). This screen does
not load poster or backdrop artwork.

## Artwork

Login and the Player do not load artwork. The next native screen that does
must build `ArtworkRequest` and send `Session::authorization_header` from
the service runtime. Do not put `api_key` back on those URLs.
