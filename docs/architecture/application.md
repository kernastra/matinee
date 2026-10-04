# Native application

`apps/matinee-next` is the native Matinee window. Phase 3A gives it one
service runtime, Login, and a minimal authenticated shell. It is not Home,
Player, or any other library screen.

The shipping Tauri app remains the usable reference. This binary does not
replace it.

## Ownership

```text
GPUI window
  → AppModel (Starting, Unauthenticated, Authenticating, Authenticated, SigningOut)
  → ServiceRuntime (one Tokio runtime)
  → matinee-jellyfin (ReqwestTransport, persist)
  → matinee-secrets (KeyringStore in production, MemoryStore in tests and review)
```

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
clear the fields. While `Authenticating`, the fields stay enabled so focus
is not dropped, the button is `ButtonStatus::Loading` with the label
“Connecting…”, and a second submit does not start another task.

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
out, and a note that native library screens are still being migrated. It
does not render the access token.

## Artwork

Login does not load artwork. The next native screen that does must build
`ArtworkRequest` and send `Session::authorization_header` from the service
runtime. Do not put `api_key` back on those URLs.
