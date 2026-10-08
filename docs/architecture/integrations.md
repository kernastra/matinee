# Radarr and Sonarr

`matinee-integrations` owns connection tests, calendar normalization, the
five-minute cache, and the Home selection. It depends on `matinee-secrets`
and does not depend on `matinee-core`. A calendar instant is not a domain
`CalendarDate`. UI, Tauri, GPUI, and Atelier do not appear here.

## HTTP

`ReqwestTransport` uses a 20-second timeout and does not follow redirects. A
3xx response is a failed status. The API key is the `X-Api-Key` header.
Requests are not logged with that header. The log line is scheme, host,
port, and path, without the query or fragment. A non-2xx provider response
becomes `HTTP {status}` only. The body is not stored on the error, so
`Display` and `Debug` cannot repeat an API key, an `Authorization` value, a
password, HTML that echoes a secret, or a control character. A transport
failure keeps a short redacted message with query strings removed. Status
401, 403, 404, and 500 stay visible.

The crate does not create a Tokio runtime. The shipping Tauri command
runtime polls `ReqwestTransport`. Tests use a scripted `Transport` and
`futures::executor::block_on`.

## Addresses

HTTP and HTTPS are both valid. A LAN `http://` address is a normal home
server. The URL must have a host. Userinfo, a query, and a fragment are
rejected. A trailing slash is removed. IPv4, IPv6, `localhost`, `.local`,
and a reverse-proxy base path are accepted.

The saved API key is bound to the exact stored server string. Changing the
address requires the key again. A stored payload is camelCase
`{ "serverUrl", "apiKey" }`. A corrupt payload makes key status report "not
configured" and makes a fetch ask the user to reconnect. Keys shorter than
8 characters are rejected. Supplied keys are trimmed. A blank supplied key
means "use the stored key."

`GET /api/v3/system/status` must return an `appName` that contains the
provider name.

## Calendar

`GET /api/v3/calendar` sends `start`, `end`, and `unmonitored=false`. Sonarr
also sends `includeSeries=true` and `includeEpisodeImages=true`.

Radarr keeps monitored movies. Each of theatrical, digital, and physical is
its own event when the date parses and falls in the window. One movie can
produce several events. `hasFile`, overview, genres, and poster-then-fanart
are preserved. An id of `0` or an empty title is skipped.

Sonarr keeps a monitored episode of a monitored series. A non-empty
`airDateUtc` is used even when it does not parse, which then skips the
episode. An empty `airDateUtc` falls back to `airDate`. Season, episode,
series id, titles, overview, genres, `hasFile`, and screenshot-then-poster-or-fanart
are preserved. A series id of `0` is treated as missing.

Dates are parsed as RFC3339 (including an explicit numeric offset, converted
to UTC), as `YYYY-MM-DD` at UTC midnight, or as a zone-less date-time treated
as UTC. The original server string is kept for display. A bad date on one
item is skipped. Tests use `FixedOffset` and UTC only, so they do not depend
on the machine zone. A daylight-saving change is the offset the caller
supplies: the same civil day under UTC−4 and UTC−5 produces different UTC
midnights. There is no timezone database. A bad response for one provider
does not drop the other provider: `UpcomingResult.errors` is keyed by
`radarr` or `sonarr`, and `events` holds the successes. A partial result is
what the cache stores.

The window is inclusive at the start and exclusive at the end, including
midnight and the exact end instant. `upcoming_window` starts at local
midnight and adds N civil days. A missing or ambiguous midnight uses the
earliest offset and returns `None` when the local day does not exist. The
shipping UI still builds the window with `upcomingWindow` in TypeScript and
sends those ISO strings. The Rust helper is the contract for a native caller.

## Cache and Home

`Integrations::upcoming` caches for five minutes. The key is the Radarr URL,
Sonarr URL, and range on the query. A different range or a different provider
URL does not share an entry. The request itself uses the server URL stored
with the key. Concurrent identical calls share one future. The cache holds
that work with a weak reference, so dropping every waiter does not leave the
fetch running. `test_and_save`, `remove`, and `invalidate_cache` bump a
generation. A fetch that finishes after that bump is not stored, and the
next call fetches again. TTL uses the injected clock. A partial provider
failure is cached with the successes.

`home` on the result excludes downloaded items, keeps one Radarr event per
movie (Digital or Physical over Theatrical), keeps one Sonarr episode per
series, sorts by instant, and limits to 12. The shipping Home shelf reads
`home`. `homeUpcoming` remains in TypeScript as the synchronous helper and
is covered by the Vitest file. Provider JSON parsing lives only in Rust.

`fetch_upcoming_releases` is the aggregated command the React calendar calls.
`fetch_integration_calendar` is registered and returns one provider's events
without the cache, but the shipping UI does not invoke it. Its only behaviour
change is the native one below: a body that is not a JSON array is an error
there, where the aggregated path reads it as no releases.

## Native calendar contract

The native Calendar (see [calendar.md](calendar.md)) uses four additions. The
shipping payloads are unchanged: none of these fields is serialized.

- **`UpcomingRelease.timing: ReleaseTiming`** says what `date` means.
  `CivilDay` is a day as the source wrote it: Radarr's release fields (UTC
  midnight stamps that name a day) and Sonarr's `airDate`. `Instant` is a
  moment: Sonarr's `airDateUtc`. Normalization sets it from the field that
  supplied the date. A present but malformed `airDateUtc` still never falls
  back to `airDate`.
- **`UpcomingRelease::civil_day()`** and **`ReleaseMilestone::civil_day()`**
  return the `YYYY-MM-DD` the source wrote, ignoring any time and zone after
  it. They return `None` for an instant, or for text that does not start with a
  day. Placing a release on a local day is the application's policy, not this
  crate's.
- **`Integrations::fetch_calendar(provider, start, end)`** is the native path.
  It reads the saved connection from the vault, makes one request, and does
  not use the five-minute cache. Unlike `upcoming`, it returns
  `MalformedResponse` for a body that is not a JSON array. Results are
  ordered by instant, then by id. Radarr and Sonarr are independent calls, so
  a slow one does not hold up the other.
- **`Integrations::fetch_image(provider, url)`** fetches one artwork address a
  calendar response named. Only `http` and `https` addresses without
  credentials are fetched. No header or query is sent, so the key never goes to
  an image host. Redirects are not followed, which the shared transport
  enforces. A non-2xx status is `ImageUnavailable { provider, status }`. An
  unsupported address or a body over 16 MiB is `InvalidImage { provider }`.
  A timeout or refused connection is `Unreachable`, as for the API.

`IntegrationError` gained `InvalidImage` and `ImageUnavailable`. Matching on
the enum is the supported way to tell an unauthorized key (401,
`AuthenticationRejected`), an unreachable server (`Unreachable`), a server
failure (`Server`), an unreadable body (`MalformedResponse`), and a missing
connection (`NotConfigured`) apart. Display text is for people and is not a
contract.
