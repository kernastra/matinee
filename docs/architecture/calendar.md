# Calendar

Phase 3G. Calendar is the fourth authenticated root destination, beside Home,
Library, and Search. It shows upcoming movie releases from Radarr and episodes
from Sonarr by day. Choosing a day lists what releases on it, and the chosen
release shows its details. Each source is independent: one can be connected,
slow, unauthorized, or down without hiding the other.

```text
Launch → Login or restored session → Home ⇄ Library ⇄ Search ⇄ Calendar
                                                              (no page opens
                                                               above Calendar)

CalendarScreen (root) ── CalendarModel<Local> (no GPUI, no HTTP, no clock)
      │   month · selected day · filter · focused release
      │   per source: link · one request in flight · failure · retained months
      │   plan() → Request { ticket } ──► calendar::load ──► Integrations
      │                                   (ServiceRuntime)    (matinee-integrations)
      │   apply(Response { ticket }) ◄──┘                     ├─ key_status (vault)
      │                                                       └─ fetch_calendar (HTTP)
      │   VirtualGrid (Atelier): 7 × 6 day cells, one focus stop, arrows, Enter
      │   tiles::art_frame + ArtworkLoader: covers for the selected day only
      └── calendar::event: presentation_day(release, zone) — the day rule
```

The integration crate says what each source sent. The application decides
where a release sits on the calendar. The model is testable without GPUI or
HTTP, and it takes the zone as a parameter.

## Shipping Calendar audited

Read from `src/components/Calendar.tsx`, `src/lib/integrations.ts`,
`src-tauri/src/media_calendar.rs`, and `src-tauri/src/services.rs`.

- The nav item reads **Calendar**. The page heading reads **Coming Soon**,
  with the note *Only monitored titles from your connected Radarr and Sonarr
  libraries.*
- A **month grid** of 42 cells, Sunday first, with out-of-month days shown,
  and Previous, Today, and Next controls.
- A **Next up** strip: the first five releases in the next 120 days, from
  their own request, with an All, Movies, or Series filter.
- A **sync bar**: a status for each *configured* provider (connected,
  connecting, or unavailable), *Updated <time>*, and Refresh. Refresh clears
  the five-minute cache.
- A **warning line** that joins the per-provider error messages.
- A **detail dialog** for a release: poster, source label, title, subtitle,
  date, kind, milestones for movies, overview (or *No overview is available
  from the connected service.*), and genres. Escape closes it.
- Each day cell shows up to three chips. Several episodes of one series read
  *N episodes*, and the rest read *+N more*.
- Requests: `fetch_upcoming_releases` takes both URLs and both providers run
  one after the other in the backend. Keys come from the OS vault in the
  `dev.sean.matinee.media-integrations` namespace, and the same namespace is
  read natively.
- Shipping has no Jellyfin identity for a release. Clicking one never opens a
  Jellyfin item.

## Parity matrix

| Behaviour | Shipping | Native | Status |
|---|---|---|---|
| Month grid, 42 cells, Sunday first | Yes | Yes, `VirtualGrid`, seven columns | Parity |
| Previous, Today, Next | Yes | Yes, plus arrow-key month focus | Parity |
| Selected day | No (days are not selectable) | Yes: click or Enter selects; the month follows | Native addition |
| Releases for the selected day | Only in the detail dialog | Panel beside the grid, movies first, then episodes by air time | Native addition |
| Release detail | Modal dialog | Panel: source, title, subtitle, kind, milestone dates, overview, genres, in-library state | Intentional difference: no modal, no Escape to close |
| Movie vs episode distinction | Chip colour and source label | Chip colour bar, source line, and the panel's source label | Parity |
| Filter All / Movies / Series | Yes, in the agenda and the grid | Yes, in the grid and the panel | Parity |
| Next up: 120-day agenda, five items | Yes, own request | **Deferred** | Deferred: a separate enhancement (see below) |
| Per-provider status | Connected, connecting, or unavailable for configured providers | Checking, not connected, loading, connected, or the specific failure | Parity, with the cause named |
| Last updated time | Yes | Yes, in the local zone | Parity |
| Refresh | Clears the 5-minute cache and refetches | Refetches the shown month and both connections; the old events stay until replaced | Native difference: no TTL cache, see lifecycle |
| Empty month | *No monitored releases this month.* | *Nothing is scheduled for this day.* once both sources have answered | Parity, but only when both have answered |
| Warning line for errors | One line joined from provider errors | Per source, in the status line, with *Try again* | Intentional difference: names the cause per source |
| Destination visible with no connection | Hidden from the nav until one integration passes *Test & save* | Always in the app bar; the panel explains how to connect, with *Check again* | Intentional difference: the guidance has to be reachable |
| Posters on cards | Every visible card, `<img>` | Covers for the selected day only | Intentional difference: no month-wide artwork load |
| Sign-out | Not applicable | The calendar is dropped with the other roots and the connections stay saved | Native rule |
| Escape closes detail | Yes | Not applicable: no modal | Intentional difference |

## Date and time semantics

Every source field is one of two kinds. The integration crate records which one
in `UpcomingRelease.timing`:

| Source field | What it means | `ReleaseTiming` | The calendar day |
|---|---|---|---|
| Radarr `inCinemas`, `digitalRelease`, `physicalRelease` | A release *day*, stored as UTC midnight | `CivilDay` | The date as written, in no zone |
| Sonarr `airDate` | The network's *day*, with no time | `CivilDay` | The date as written |
| Sonarr `airDateUtc` | An *instant* (air time) | `Instant` | The viewer's local day of that instant |

The shipping calendar converts every string with `new Date(value)`. That moves
two kinds of release to the wrong day:

- A Radarr stamp of `2026-08-20T00:00:00Z` shows on **19 August** for a viewer
  west of UTC.
- A date-only `airDate` of `2026-08-16` parses as UTC midnight and shows on
  **15 August** in the Americas.

The native rule keeps the written day for both. This is an intentional
difference, and it is tested for offsets from UTC−12 to UTC+14 in
`calendar/event.rs` and `calendar/model_tests.rs`.

An air time follows the viewer. An episode at 9 pm in New York on 10 August
(01:00 UTC on 11 August) is on the 10th in New York and on the 11th in Tokyo.
That matches shipping and what someone in each zone would see when it airs.

**Fetch window.** A request covers the 42 grid days, padded by one day on each
side, as UTC midnights: from 00:00 UTC on the day before the first grid day to
00:00 UTC on the second day after the last. A local day starts no earlier than
14 hours before its UTC midnight and ends no later than 12 hours after the
next one, because offsets run from UTC−12 to UTC+14. So every instant whose
local day is in the grid lies inside that padding. The application then drops
any event whose day is outside the grid. Shipping sends a local-midnight window
and filters by instant, which can drop or keep an edge day for a UTC-stamped
release. Native does not filter by instant.

**Clock and zone.** `CalendarModel<Z: TimeZone>` takes `now` from the caller and
the zone as a type. The screen uses `Local`, so DST follows the offset in force
at each instant. Tests use fixed offsets. `Local` needs chrono's `clock`
feature, which adds `iana-time-zone` to the lock file.

**Today** is the local date of `now`. It moves at local midnight without moving
the month or the selection.

**Month arithmetic** (`calendar/grid.rs`) covers the year boundary in both
directions, leap years (including 2000 and 2100), and a selected day that
falls past the end of a shorter month: 31 January becomes 28 or 29 February.
The grid starts on the Sunday on or before the first and always has 42 days.

## Data normalization

`calendar/event.rs` turns one `UpcomingRelease` into a `CalendarEvent`:

- `id` is the integration's id: `radarr-{movie}-{kind}` or `sonarr-{episode}`.
  Ids are stable across refreshes.
- `day` is the presentation day. An event whose day cannot be placed is not
  kept.
- `instant` is set for episodes with an air time, and `None` for movies.
- `milestones` lists a movie's release days in day order, with unreadable
  ones dropped.
- A movie keeps its overview and genres, and an episode keeps its series
  title, `S02E03 · Title` subtitle, and series id.
- Deduplication is by id, with the first kept. Within a month answer, each
  day's events are replaced, so a release that disappears from the server
  disappears from the calendar on the next answer.
- Within a day, movies come first, then episodes by air time, then title and
  id. Equal rows keep their order between refreshes.

A movie's release days are separate events, as in shipping. The fixture
*The Long Corridor* shows on the 4th (theatrical) and again on the 19th
(digital), each with the same overview and each selectable.

## Integration availability

The integration crate owns the vault, the connection test, and the API calls.
The calendar reads them through `CalendarService`, which is one
`Integrations<SharedStore, ReqwestTransport>` built in `view.rs`.

| State | Meaning | Shown as | Left by |
|---|---|---|---|
| Checking | Connection not read yet | *Checking Radarr…* | First show |
| Not connected | No saved connection, or a releases answer said so | *Radarr isn't connected*, and the guidance panel when neither is connected | The next show, which reads it again |
| Connection failed | The vault could not be read | *Radarr couldn't be reached* | The next show or Refresh |
| Loading | Connected, and the shown month has not been answered | *Loading Radarr…* | An answer |
| Connected | The shown month is answered, possibly with no releases | *Radarr connected* | A month change or a refresh |
| Unauthorized | 401: the saved key was refused | *Radarr rejected the saved key* | Refresh, after the key changes |
| Unavailable | Unreachable, or a 5xx | *Radarr couldn't be reached* | Refresh |
| Malformed | A body that is not a calendar array | *Radarr sent a calendar Matinee could not read* | Refresh |

Only these states decide what the screen shows. A 401 stays on that source.
It does not end the Jellyfin session, and it does not move the person to Login.
Only a Jellyfin authorization failure does that, and Calendar never makes one.

**Empty versus unknown.** A day is empty only when every connected source has
answered the shown month. While a source loads or has failed, the panel says
it is still waiting and names the source. This is why a slow or failed Sonarr
never makes an empty Radarr day look like a day with no episodes.

**Partial success.** A failed source keeps the other source's releases on the
grid and in the panel. A source that fails for a month keeps the events it had
for days that other loaded months still cover, until a new answer replaces
them.

**Credentials.** The key is read from the vault and sent only as the
`X-Api-Key` header, to the address saved with it. Redirects are not followed,
so the key never travels to another host. Neither the address nor a log line
carries it, and errors arrive redacted. Calendar writes no log lines (guard
24).

## Request lifecycle

- **One request per source per shown month.** `CalendarModel::plan` returns a
  request only when a source is connected, the shown month is not fresh, and
  no request for that month is in flight or failed. It runs after each action
  and each answer, never during render.
- **Tickets.** Every request has a ticket. An answer applies only if the
  source is still waiting on that ticket. A late answer for a month that is no
  longer shown is ignored (`an_answer_for_a_month_no_longer_shown_is_ignored`).
- **Cancellation.** The screen aborts a request when the model stops waiting on
  its ticket: a new month, a refresh, or a lost connection. An aborted call's
  receiver closes, so its answer cannot land.
- **Independence.** Radarr and Sonarr have separate tickets and separate
  tasks. A slow Sonarr does not hold a Radarr answer (`a_slow_sonarr_never_holds_back_a_radarr_answer`).
- **Retained months.** Each source keeps up to four months, least recently used
  first. The shown month is never evicted. When a month is evicted, events on
  days no kept month covers are dropped. Going back and forth among four
  neighbouring months asks for nothing.
- **Freshness.** An answer is fresh for five minutes, as the shipping cache is.
  After that, showing the calendar asks again, but the old events stay on the
  grid until the new answer replaces them, so nothing flashes.
- **Refresh** (the app bar's Refresh, or *Try again*) re-reads both connections
  and asks again for the shown month. Failures are cleared. Events stay until
  replaced.
- **Show.** Each time Calendar is chosen, both connections are read again, so
  a connection added in Settings shows up without restarting. A failed month
  is retried by a visit only once the failure is `RETRY_AFTER` (30 seconds)
  old: going back and forth between roots does not ask a failing server again
  on every visit (`coming_back_right_after_a_failure_does_not_retry_it_but_a_later_visit_does`).
  A month still in flight is never asked for twice
  (`coming_back_while_the_month_is_loading_asks_for_nothing_more`). Refresh,
  *Try again*, and choosing another month retry at once.
- **Abandoned requests.** If a request's task ends without an answer (it
  panicked, or the runtime stopped), the screen tells the model
  (`CalendarModel::abandon`). The month shows as unavailable with *Try again*,
  rather than loading forever with Refresh disabled. A first connection check
  that is lost reads as failed; a lost recheck keeps the known connection.
- **Sign-out** drops the Calendar root, as it drops Home, Library, and Search.
  The saved connections are not Jellyfin credentials, so they stay saved.
- **Window close** aborts every request and cover task in the screen's `Drop`.

## Navigation

- Calendar is a `RootDestination`, listed in the app bar after Search. Pages do
  not open above it, and the shell does not resume one.
- Moving between roots keeps each root's state. Calendar's month, selected day,
  filter, focused release, and answers are the same when it is chosen again
  (`calendar_is_a_retained_root_beside_home_library_and_search`).
- Calendar never opens Details or the Player. A Radarr or Sonarr id is not a
  Jellyfin item id, and no cross-service mapping is invented. Guard 23 keeps
  Details and the Player out of `calendar/`.
- Keyboard: the month is one tab stop (`VirtualGrid`). Arrows move a day,
  Up and Down move a week, Home and End move to the first and last cell,
  and Enter selects. Left and Right continue across the week (Saturday to
  the next Sunday). Past the grid's first or last day, and Up or Down past
  its first or last week, the day reached is selected and its month shown,
  through the grid's `on_edge`. Tab leaves the month for the release panel.
- Choosing a day of another month (Enter, a click, or crossing an edge) shows
  that month and moves the grid's focus to the chosen day's new cell, so the
  focus ring never sits on an unrelated date.
- The release panel's rows are direct children of its scroll view. When Tab
  reaches a row below the fold, the panel scrolls to it
  (`ScrollControl::reveal_child`). At 960 × 620 a day with six releases puts
  rows below the fold
  (`tabbing_to_a_release_below_the_fold_scrolls_the_panel_to_it`).
- On opening Calendar, and on every return to it, focus goes to the selected
  day.

## Presentation

- A seven-column, six-row grid at every standard width. The six weeks share
  the grid area whole (`CellMetrics::row_height`), so a tall window has no
  empty band under the grid, and no row is shorter than the day number and
  two lines. The lines a cell shows follow its height, from the theme's type
  metrics: every chip if they fit, otherwise one line less and *+N more*.
  Measured: 62-point rows with 2 lines at 960 × 620, 85 with 3 at
  1200 × 760, 108 with 4 at 1440 × 900, and 138 with 6 at 1920 × 1080. The
  month does not scroll at any standard size; the bands above it are 12
  points apart so that the minimum size fits two lines. Each chip is one
  line, truncated, with a series' episode count (`×2`) kept beside it: a
  wrapped title ran into the chip below it in a narrow cell.
- A day the grid borrows from a neighbouring month has a muted number.
- The app bar, header, and status line keep their height; the month and the
  panel share the rest. The month column is `h_full`, so the grid area is
  bounded by it and scrolls inside it. Without that, at 960 × 620 the grid
  area took its full 528-point content height and was centred over the
  header and the status line, hiding each source's state
  (`the_header_status_and_month_stack_without_overlap_at_every_standard_size`).
- A release panel of 336 points beside the grid. At 960 points wide both fit,
  and the grid stays seven columns. Release rows span the panel whether or not
  it scrolls (`a_release_row_spans_the_panel_whether_or_not_the_panel_scrolls`).
- Covers are drawn at 40 × 60 points through the shared `art_frame`, with the
  title as the fallback text.
- The palette and type are Matinee's: amber for movies, faded teal for series,
  and the shared text roles. No new colour is introduced.

## Artwork

Covers come from Radarr's and Sonarr's public image addresses. They are
fetched by `Integrations::fetch_image` with no credential and go through the
shared `ArtworkLoader`. The loader uses the same 96 MiB cache, the same decode,
and the same release of atlas images. Calendar keys its covers under
`release\n{url}`, so they cannot collide with a Jellyfin address.

Covers load only for the selected day's releases, as the filter shows them.
Choosing another day releases the covers no longer shown and cancels their
loads. Loading a whole month's covers would start dozens of downloads for a
grid that shows none of them. The covers wanted are worked out in `drive`,
after an action or an answer, never while drawing; a review scene sets its
fixture covers when it is built.

### Artwork trust policy

A cover address comes from the server's JSON, so the server names the host.
Matinee fetches covers only from the public internet
(`matinee-integrations/src/destination.rs`):

1. **The address.** `http` or `https`, no user or password, not `localhost`
   or `*.localhost`, and any IP literal must be public (`public_url`).
2. **The client.** Artwork requests are `public_only` and go through a
   separate reqwest client from the Radarr and Sonarr API client.
3. **Resolution.** That client's resolver (`PublicResolver`) looks a name up
   once per connection and refuses the whole name if any answer is not
   public. "Public" means global IPv4, or IPv6 in 2000::/3 outside
   documentation; loopback, private, link-local (including
   169.254.169.254), shared, unspecified, broadcast, multicast, benchmarking,
   and reserved ranges are refused, and an IPv4 address carried in IPv6
   (mapped, NAT64, 6to4) is judged as that IPv4 address.
4. **Connection.** The connector dials only the addresses the resolver
   returned. There is no separate validation lookup, so a DNS server that
   answers differently the second time (rebinding) has no second time to
   answer: each new connection is resolved and checked afresh. IP literals
   never reach a resolver, which is why step 1 checks them.
5. **No proxy.** The artwork client ignores proxy settings, because a proxy
   would resolve the name itself (guard 25).
6. **No redirects.** A 3xx is the answer (the fallback cover); no hop is
   followed, so a redirect cannot reach a host the rules refuse.
7. **No credential.** No header and no query: no key goes to a cover host.
8. **Bounded.** The body is capped at 16 MiB from its announced length or as
   it streams; 20 seconds covers DNS, connection, and body; decoding refuses
   images over 2560 pixels on a side before allocating them.

A refused host is `InvalidImage`, and the cover shows its fallback. The API
client is unchanged: Radarr and Sonarr are usually on the person's own
network.

**Lifecycle.** A lookup runs on its own short-lived thread, as reqwest's own
resolver runs `getaddrinfo` off the async workers. Lookups happen per new
connection, and connections are pooled per host, so a day's covers cost a
few. Dropping a cover's task drops its request, which closes the connection
at once (`a_cancelled_fetch_closes_its_connection_at_once`); a lookup already
running finishes and its answer is discarded.

## Known limitations

- **Visual capture.** GPUI's headless test window never paints, so pixels
  come from the real app: every scene was run under XWayland
  (`WAYLAND_DISPLAY` unset) on GNOME and captured with ImageMagick
  `import -window <id>`, at 960 × 620, 1200 × 760, and 1440 × 900 (resized
  with `XResizeWindow`, since Mutter ignores the requested size), and at
  1920 × 1011, the largest the 1920 × 1080 display's work area allows. Layout
  is also asserted headlessly with GPUI debug bounds at all four sizes.
- **Zone changes while open.** Each air time is placed with the system zone's
  offset for that instant, so daylight-saving changes are handled per event.
  A change to the system zone itself is applied when the next answer arrives,
  not immediately. Day-of-week labels follow the zone at the time they render.
- **Covers and redirects.** See Artwork.
- **Corrupt saved connections** read as not connected, as `key_status` reports
  them. Native guidance does not yet say to reconnect.
- **Two Calendars in a process** are two screens with their own requests.
  There is one service and one cache per process, as for the other roots.
- **A connection changed in the shipping app** while native Calendar runs is
  read on the next visit, but months answered by the old server stay until
  they are five minutes old or Refresh is used. Native Settings (Phase 3H)
  is the place to refresh Calendar when a connection is saved.
- **Artwork and proxies.** Covers are fetched directly, never through a
  configured proxy. Where only a proxy reaches the internet, covers show
  their fallback.
- **Trust in the resolver.** The check applies to what the system resolver
  answers. An address that is public but routes into a private network
  (unusual, and outside what DNS can show) is not detected.
- **Home and End** move to the grid's first and last cells, as `VirtualGrid`
  defines them, not to the start and end of the week.

## Intentional deferrals

- **Next up (120-day agenda and its filter).** Deferred after review. The
  strip is a separate enhancement, not part of a coherent calendar: the grid,
  Today, and the selected day's panel already answer "what releases when".
  It needs its own 120-day request per source, its own freshness and tickets,
  and its own failure line, which is a second request lifecycle beside the
  month's. It can be added later without changing the month model.
- **Connecting a source from Calendar.** Settings is a later phase. Calendar
  guides the person and reads whatever the shipping app saved.
- **Modal release detail and Escape.** Not needed. The panel shows everything
  the shipping modal does for the chosen release: source, title, subtitle,
  kind and day, every milestone of a movie, overview, genres, and whether it
  is in the library. A release never opens Jellyfin Details.
- **Notifications, calendar export, and marking releases watched.** Not part
  of Phase 3G.
- **Poster Studio, Settings, and Phase 4 packaging.** Unchanged.

## Review scenes

Set `MATINEE_PREVIEW` to one of these names, optionally with
`MATINEE_PREVIEW_SIZE=WxH`. Each scene is fixture answers only.

| Scene | Name | Size |
|---|---|---|
| Populated month, both sources | `calendar` | 1200 × 760 |
| Empty month | `calendar-empty` | 1200 × 760 |
| Selected day with a movie and episodes | `calendar-selected` | 1200 × 760 |
| Movie-heavy month | `calendar-movies` | 1200 × 760 |
| Episode-heavy month, four a day and six on the 15th | `calendar-episodes` | 1200 × 760 |
| Movies and episodes on the same days | `calendar-mixed` | 1200 × 760 |
| Radarr only | `calendar-radarr-only` | 1200 × 760 |
| Sonarr only | `calendar-sonarr-only` | 1200 × 760 |
| Neither configured | `calendar-disconnected` | 1200 × 760 |
| Radarr answered, Sonarr unreachable | `calendar-partial-error` | 1200 × 760 |
| Connected, nothing answered yet | `calendar-loading` | 1200 × 760 |
| Both unreachable | `calendar-error` | 1200 × 760 |
| Next month, releases on borrowed grid days | `calendar-next-month` | 1200 × 760 |
| Narrow window | `calendar-small-window` | 960 × 620 |
| Wide window | `calendar-large-window` | 1920 × 1080 |

The 1440 × 900 standard size is covered by the drawing test, not by a named
scene.

## Tests

- `calendar/grid.rs` (7): grid start and length, membership, months across the
  year boundary, leap years and centuries, and the fetch padding.
- `calendar/event.rs` (10): the day rule for each timing kind at several
  offsets, the exact local midnight, unreadable days, milestones, ordering,
  filters.
- `calendar/model_tests.rs` (58): navigation, the selected day, the year
  boundary and leap day, freshness, source independence, partial success,
  failure, retry and the retry interval, repeated visits, abandoned requests,
  stale and repeated answers, refresh races, connection changes, duplicates,
  filters, focus, retained months and eviction, the fetch padding for every
  quarter-hour offset from UTC−12 to UTC+14 over 2024–2030, and daylight
  saving in both directions with a US Eastern 2026 zone.
- `calendar/load_tests.rs` (15): the HTTP boundary on loopback: the vault and
  connection check, the grid range and the header-only key, Sonarr's extra
  parameters, each failure kind, a refused connection, an aborted request, a
  release at the first and last local moment of the grid surviving the real
  request in eight zones and five months, and the transport under hostile
  answers (an announced oversized body, an endless stream, a body at the cap,
  and a redirect that must not carry the key).
- `calendar/view_tests.rs` (32): the rendered screen with real focus and
  keystrokes: opening on today, arrows across weeks and months, Home and
  Down, Enter on a day of the next month, the month controls, the selected
  day's order, covers for the selected day only, every source state and the
  empty-day line, refresh, Tab into the panel and its scrolling at 960 × 620,
  the layout bounds of the header, status line, grid, and release rows, the
  month filling its area without scrolling at every size, chips that fit and
  *+N more*, muted borrowed days, and every scene at all four standard sizes.
- `view.rs` (4): Calendar as a retained root that keeps its month, day,
  filter, releases, and focus; no page above it; sign-out and a Jellyfin
  session ending both drop it.
- `matinee-integrations/src/destination_tests.rs` (10), on the real client
  with loopback stand-ins for public and private hosts and a scripted DNS:
  the address policy table, a public host reached once per connection with
  no credential, private answers (IPv4, IPv6, mapped, metadata, ULA,
  link-local) refused with nothing reaching them, mixed answers refused
  whole, rebinding between connections, redirects toward private hosts,
  IP literals, the API path still reaching a private server, the size cap
  and timeout, and cancellation closing the connection.
- `matinee-integrations/src/calendar_tests.rs` (17): request parameters, the
  key in a header only, the body cap on covers and not on the API, 401, 5xx,
  timeouts with redaction, malformed bodies, partial bodies, ordering, timing,
  civil days, the artwork fetch and its failures, and the local and private
  hosts a cover address may not name.

## Validation

See the pull request for the commands and results on the final head.
