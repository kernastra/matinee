# Jellyfin client

`matinee-jellyfin` speaks HTTP and Jellyfin JSON, then converts into
[`matinee-core`](domain.md). DTOs are crate-private. Future screens must not
depend on `Id`, `RunTimeTicks`, `UserData`, or `MediaSources`.

The crate depends on `matinee-core`. It does not depend on GPUI, Atelier,
`matinee-ui`, `matinee-player`, or Tauri. `apps/matinee-next` links this
crate and calls it from the application service runtime described in
[application.md](application.md). The UI crate does not name `reqwest`.
CI still sets `CARGO_PROFILE_DEV_DEBUG=0` and `CARGO_PROFILE_TEST_DEBUG=0`
because a debug GPUI binary that also links this client is large.

## HTTP

Production transport is `reqwest` 0.12 with `rustls-tls-native-roots`, HTTP/2,
gzip, charset, and the system proxy. Default features are off, so the client
does not pull native TLS or cookies it does not use. Plain HTTP is allowed.
There is no browser CORS check.

`ReqwestTransport` uses a 10 second connect timeout and a 60 second request
timeout. Redirects stay on the same origin and stop after five. The user
agent is `Matinee/0.5.6`. `ReqwestTransport` must be polled on a Tokio
runtime. Tests inject the `Transport` trait and run on
`futures::executor::block_on`. They never open a socket.

The runtime owner is the application, not this crate and not a UI task:

```text
GPUI application → application/service runtime → matinee-jellyfin async calls
```

That adapter is `ServiceRuntime` in `apps/matinee-next`. Calling
`ReqwestTransport` from a UI task panics. The window awaits a oneshot; it
does not poll this transport.

`CancelFlag::cancel()` only stops a request that has not started. It does
not interrupt a request already in flight. Dropping the `ReqwestTransport`
future does. Search and playback negotiation take an optional flag. There
is no background timer in this crate.

Public errors are `JellyfinError`. `reqwest::Error` does not leave the
transport. Variants: invalid URL, unreachable, auth rejected, not found,
unauthorized, server failure (status plus redacted context), malformed
response, playback unavailable, no compatible source, cancelled.

## Addresses

`normalize_server_url` ports `normalizeServerUrl` and adds checks the
TypeScript helper does not:

- A bare `jellyfin.local:8096` becomes `http://jellyfin.local:8096`.
- Only `http` and `https`.
- Embedded username or password is rejected.
- A copied `/web/index.html#!/home.html` is removed. A base path in front of
  `/web` is kept (`https://media.example.com/jellyfin/web/` becomes
  `https://media.example.com/jellyfin`). `/webhook` is left alone.
- Search and hash are dropped. Trailing slashes are dropped. Default ports
  are omitted.
- Control characters are rejected before trim, so a trailing newline is not
  silently stripped.
- A scheme-relative `//host` is rejected. Prepending `http://` would hide it.

## Authentication and session

`POST /Users/AuthenticateByName` returns a `Session`: normalized server URL,
access token, and `User`. The authorization header is centralized:

```text
MediaBrowser Client="Matinee", Device="Desktop", DeviceId="matinee-desktop", Version="0.5.6", Token="…"
```

`DeviceId` is the fixed string the shipping app already sends. It is not an
installation identity. Login itself omits `Token`. A 401 on login is
`AuthRejected`. A 401 or 403 on any other call is `Unauthorized`. A 404 is
`NotFound`. A transport failure is `Unreachable`.

`authenticate` still returns an in-memory `Session` and does not touch a vault.
`persist` can save, load, and remove that session through a
`matinee_secrets::CredentialStore`. The namespace is
`dev.sean.matinee.jellyfin-session` and the account is `default`. The payload
is JSON (`serverUrl`, `userId`, `userName`, optional `avatarTag`,
`accessToken`). `save_session` serializes that JSON into one owned string,
moves the string into a `Secret`, and drops the `Secret` after the vault
`set` returns so that copy is zeroized. `StoredSession` also zeroizes its
token field on drop. The vault keeps its own copy until `remove_session`.
Loading parses the vault copy and does not erase it. `serde_json` may hold
short-lived buffers this crate does not zeroize.

`load_session` rebuilds the session with `Session::new`, so the server URL
and token go through the same checks as authentication. A persisted URL that
still has a query or fragment is `CorruptSession` even though a pasted login
URL would have those stripped. An invalid URL, a non-HTTP scheme, credentials
in the URL, a malformed user id, a malformed avatar tag, and a blank token
are also `CorruptSession`. The payload stays in the vault. This crate does
not call `keyring` and does not read the shipping browser `sessionStorage`
entry. The native Login screen calls `load_session`, `save_session`, and
`remove_session` through the application runtime. This crate still does not
know about that screen. A corrupt payload stays in the vault.

`GET /System/Info/Public` maps server name, version, operating system,
product, and server id. Missing fields stay empty.

## Catalog

Paths and query keys match `src/lib/jellyfin.ts`, including the mixed casing
(`Fields` vs `userId` vs `SortBy`).

| Operation | Behavior |
|---|---|
| `home_feed` | Six requests in one `join!`: resume, latest movies and series, movies, series, top rated, favorites. Top rated and favorites return an empty shelf on failure. Any other shelf failure fails the feed. |
| `library_items` | Movies or series, one of the four `LibrarySort` values, limit 240. |
| `item_details`, `similar_items`, `collection_context` | One item, similar titles, and up to two collections. Order is the server's order. |
| `series_seasons`, `season_episodes` | Order preserved. |
| `next_up_episode` | First item, or `None`. |
| `following_episode` | `None` without a request when the episode has no series id. Otherwise the first item whose id differs. |
| `search_library` | Movies, series, and episodes, limit 40. Optional `CancelFlag`. |
| `set_item_favorite`, `set_item_played` | The shipping user-data posts. |
| `clear_item_progress` | `GET` the user-data object, set position and played percentage to zero, `POST` the object back, including fields this client does not model. |

Serde ignores unknown fields. Optional metadata may be absent. An item with
no id, no name, or an empty name is rejected. Negative ticks are rejected.
A JSON integer wider than `i64` is an overflow, not a truncated duration.
Premiere, date created, and end date become a `CalendarDate`, or stay absent
when the string is not a real day. Non-finite or out-of-range community
rating, critic rating, dimensions, and frame rate become absent. A finite
played percentage below zero is absent; one above 100 is clamped. A usable
title is kept. Ticks stay strict.

## Artwork

`ArtworkUrls` builds the shipping image, user, backdrop, indexed backdrop,
and chapter URLs. Query values are percent-encoded (`%20`, not `+`). Those
compatibility URLs include `api_key=<token>` because the shipping image
loader cannot set the authorization header. That token can land in a log, a
proxy, or an image cache.

`ArtworkRequest` is the native shape: the same address with no `api_key`.
The next native screen that loads artwork sends
`Session::authorization_header` with it. Login does not. Availability stays on
`ItemArtwork`; the builders still emit a URL when no tag is present,
matching `imageUrl`.

## Playback

`playback_plan` posts `PlaybackInfo` with a device profile built in
`profile.rs` from `matinee_core::native_playback()`. The player does not
build that JSON and does not depend on this crate or on `matinee-core`.
This crate does not depend on the player.

The plan is a domain `PlaybackPlan`, not a `LoadRequest`. Every negotiated
plan sets `StreamAuthorization::Session`. The URL has no access token. The
future adapter copies `Session::authorization_header` onto the Phase 1D
`LoadRequest` headers. The domain does not carry the token or a header map.

`PlaySessionId` is the server value when the response includes one. A missing
or blank id stays `None`. This client does not invent a UUID. Reporting omits
the JSON key when the id is absent.

Source selection is the first valid DirectPlay, then the first valid
DirectStream, then the first valid Transcode. Anything else is
`PlaybackUnavailable`. There is no quality ranking and no
`sources.first()` fallback.

- Direct play builds `/Videos/{id}/stream` with `Static=true`, the device id,
  the media source id, and `PlaySessionId` when the server sent one. Item ids
  are percent-encoded.
- A `TranscodingUrl` is resolved with `Url::join` against the server base.
  An absolute path replaces the base path. A relative path stays under it.
  `//other-host/...` is another origin and is rejected. Userinfo, control
  characters, and a different origin are rejected. Query keys `api_key`,
  `apikey`, `accesstoken`, and `token` are stripped, including duplicates.
  Other query parameters stay.
- The method describes the address, not a single boolean.
  `TranscodeReasons` of only `ContainerNotSupported` or
  `ContainerBitrateExceedsLimit` (or those bits) is DirectStream. Any other
  reason is Transcode. Copied video and audio codecs, with no reasons, are
  DirectStream. `SupportsDirectStream` without `SupportsTranscoding` is
  DirectStream. Both flags and no remux evidence are Transcode. A source
  with neither direct play nor a transcoding URL is not playable.
- Shipping `NoCompatibleStream` used to fall back to a static
  `/Videos/{id}/stream?Static=true` URL. With the native profile that
  fallback is unsafe: it would play a file the server just refused. This
  client returns `JellyfinError::NoCompatibleSource` instead.

`report_playback` posts one of `/Sessions/Playing`,
`/Sessions/Playing/Progress`, or `/Sessions/Playing/Stopped`. The body carries
item id, media source id, play session id when present, position in ticks,
paused, mute, volume percent, audio and subtitle indexes, and `PlayMethod`.
The future app decides how often to call it.

## Logging and secrets

Logs record endpoint category, method, path, and status. They do not record
the query string. `HttpRequest`'s `Debug` redacts `Authorization` and omits
the body. `Session`'s `Debug` redacts the token and its `Drop` zeroizes the
copy this struct owns. `Password` and the login body zeroize the copies they
own on drop. Errors do not include response bodies. `api_key`, `Token`, and
`Pw` are redacted if they appear in a free-form detail string.

Zeroizing those owned copies is not a claim that the secret is gone from the
process. `serde_json` builds another string for the request body. The
transport receives that string, and reqwest can retain it until the request
is dropped. The allocator can reuse the memory afterward without wiping it.
Debug and logs are redacted; those other copies are not.
