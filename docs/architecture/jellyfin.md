# Jellyfin client

`matinee-jellyfin` speaks HTTP and Jellyfin JSON, then converts into
[`matinee-core`](domain.md). DTOs are crate-private. Future screens must not
depend on `Id`, `RunTimeTicks`, `UserData`, or `MediaSources`.

The crate depends on `matinee-core`. It does not depend on GPUI, Atelier,
`matinee-ui`, `matinee-player`, or Tauri. `apps/matinee-next` depends on both
new crates only so the crate graph records that direction. The shell does not
call them.

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

Cancellation is a `CancelFlag` checked before a request, plus drop of the
in-flight reqwest future. Search and playback negotiation take an optional
flag. There is no background timer in this crate.

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

The session lives in memory. This crate does not write a token file and does
not read the Tauri credential vault. Native persistent login stays open.

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

## Artwork

`ArtworkUrls` builds the shipping image, user, backdrop, indexed backdrop,
and chapter URLs. Query values are percent-encoded (`%20`, not `+`). Image
URLs include `api_key=<token>` because the shipping image loader cannot set
the authorization header. That token can land in a log, a proxy, or an image
cache. A future native loader should send `Session::authorization_header` and
omit `api_key`. Availability stays on `ItemArtwork`; the builders still emit
a URL when no tag is present, matching `imageUrl`.

## Playback

`playback_plan` posts `PlaybackInfo` with the Matinee Native device profile
built in this crate (`profile.rs`). The engine keeps
`matinee_player::native_device_profile()` so Phase 1D tests and the public
player API stay intact. The two JSON documents must move together. Neither
crate depends on the other.

The plan is a domain `PlaybackPlan`, not a `LoadRequest`.

- Direct play builds `/Videos/{id}/stream` with `Static=true`, the media
  source id, the play session id, the device id, and `api_key`. That URL is
  emitted only when the chosen source says direct play is supported.
- A relative `TranscodingUrl` is resolved with `Url::join` against the server
  base. The existing query is kept. `api_key` is added only when absent.
  Userinfo, control characters, and a different origin are rejected before
  the token is attached.
- `SupportsDirectStream` without a transcoding URL is `DirectStream` on the
  same native stream URL.
- Shipping `NoCompatibleStream` used to fall back to a static
  `/Videos/{id}/stream?Static=true` URL. With the native profile that
  fallback is unsafe: it would play a file the server just refused. This
  client returns `JellyfinError::NoCompatibleSource` instead.

`report_playback` posts one of `/Sessions/Playing`,
`/Sessions/Playing/Progress`, or `/Sessions/Playing/Stopped`. The body carries
item id, media source id, play session id, position in ticks, paused, mute,
volume percent, audio and subtitle indexes, and `PlayMethod`. The future app
decides how often to call it.

## Logging and secrets

Logs record endpoint category, method, path, and status. They do not record
the query string. `HttpRequest`'s `Debug` redacts `Authorization` and omits
the body. `Session`'s `Debug` redacts the token and its `Drop` zeroizes it.
`Password` and the login body zeroize on drop. The JSON string handed to the
transport is a copy; reqwest can retain it until the request is dropped.
Errors do not include response bodies. `api_key`, `Token`, and `Pw` are
redacted if they appear in a free-form detail string.
