# Radarr and Sonarr

`matinee-integrations` owns connection tests, calendar normalization, the
five-minute cache, and the Home selection. It depends on `matinee-secrets`
and does not depend on `matinee-core`. A calendar instant is not a domain
`CalendarDate`. UI, Tauri, GPUI, and Atelier do not appear here.

## HTTP

`ReqwestTransport` uses a 20-second timeout and does not follow redirects. A
3xx response is a failed status. The API key is the `X-Api-Key` header.
Requests are not logged with that header. Error bodies are passed through
`redact` and truncated.

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

Dates are parsed as RFC3339, as `YYYY-MM-DD` at UTC midnight, or as a
zone-less date-time treated as UTC. The original server string is kept for
display. A bad date on one item is skipped. A bad response for one provider
does not drop the other provider: `UpcomingResult.errors` is keyed by
`radarr` or `sonarr`, and `events` holds the successes.

The window is inclusive at the start and exclusive at the end.
`upcoming_window` starts at local midnight and adds N civil days. A missing
or ambiguous midnight uses the earliest offset and returns `None` when the
local day does not exist. The shipping UI still builds the window with
`upcomingWindow` in TypeScript and sends those ISO strings. The Rust helper
is the contract for a native caller.

## Cache and Home

`Integrations::upcoming` caches for five minutes. The key is the configured
Radarr URL, Sonarr URL, and the range. The request itself uses the server
URL stored with the key. Concurrent identical calls share one future. The
cache holds that work with a weak reference, so dropping every waiter does
not leave the fetch running. `test_and_save`, `remove`, and
`invalidate_cache` bump a generation and discard a late store.

`home` on the result excludes downloaded items, keeps one Radarr event per
movie (Digital or Physical over Theatrical), keeps one Sonarr episode per
series, sorts by instant, and limits to 12. The shipping Home shelf reads
`home`. `homeUpcoming` remains in TypeScript as the synchronous helper and
is covered by the Vitest file. Provider JSON parsing lives only in Rust.

`fetch_integration_calendar` returns the normalized events for one provider
and does not use the cache. `fetch_upcoming_releases` is the aggregated
command the React calendar calls.
