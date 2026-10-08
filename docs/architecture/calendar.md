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
| Next up: 120-day agenda, five items | Yes, own request | **Deferred** | Deferred (see below) |
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
`X-Api-Key` header. Neither the address nor a log line carries it, and errors
arrive redacted. Calendar writes no log lines (guard 24).

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
- **Show.** Each time Calendar is chosen, both connections are read again and
  failures are retried. This is how a connection added in Settings shows up
  without restarting.
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
  and Enter selects. Tab leaves the month for the release panel.
- On opening Calendar, focus goes to the selected day.

## Presentation

- A seven-column, six-row grid at every standard width. Cells are 84 points
  tall, so a day holds its number and up to three chips.
- A release panel of 336 points beside the grid. At 960 points wide both fit,
  and the grid stays seven columns.
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

Covers load only for the selected day's releases. Choosing another day releases
the covers no longer shown and cancels their loads. Loading a whole month's
covers would start dozens of downloads for a grid that shows none of them.

Known limitation: redirects are not followed, because the shared transport
disables them. An image host that answers with a redirect shows the fallback.

## Known limitations

- **No visual capture.** Pixel review is not available here. The scenes are
  checked by drawing them at all four standard sizes and by state assertions.
  Phase 3F reports the same limit: headless capture is blank.
- **Zone changes while open.** Each air time is placed with the system zone's
  offset for that instant, so daylight-saving changes are handled per event.
  A change to the system zone itself is applied when the next answer arrives,
  not immediately. Day-of-week labels follow the zone at the time they render.
- **Covers and redirects.** See Artwork.
- **Corrupt saved connections** read as not connected, as `key_status` reports
  them. Native guidance does not yet say to reconnect.
- **Two Calendars in a process** are two screens with their own requests.
  There is one service and one cache per process, as for the other roots.
- **Keyboard focus on a narrow window** follows the tab order. The release
  panel scrolls on its own, so Tab can move to a row below the fold.

## Intentional deferrals

- **Next up (120-day agenda and its filter).** Shipping's strip is a second,
  separate request for the next 120 days. The native pass uses the month grid
  and the selected day's list instead. The month grid already holds the
  Radarr and Sonarr data, and a second range would add a second cache policy
  and a second set of tickets. Opus should decide whether the strip is worth
  that.
- **Connecting a source from Calendar.** Settings is a later phase. Calendar
  guides the person and reads whatever the shipping app saved.
- **Modal release detail and Escape.** The panel replaces the modal. A modal
  would be a second way to see the same data.
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
| Episode-heavy month, several on one day | `calendar-episodes` | 1200 × 760 |
| Movies and episodes on the same days | `calendar-mixed` | 1200 × 760 |
| Radarr only | `calendar-radarr-only` | 1200 × 760 |
| Sonarr only | `calendar-sonarr-only` | 1200 × 760 |
| Neither configured | `calendar-disconnected` | 1200 × 760 |
| Radarr answered, Sonarr unreachable | `calendar-partial-error` | 1200 × 760 |
| Connected, nothing answered yet | `calendar-loading` | 1200 × 760 |
| Both unreachable | `calendar-error` | 1200 × 760 |
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
- `calendar/model_tests.rs` (52): navigation, the selected day, the year
  boundary and leap day, freshness, source independence, partial success,
  failure and retry, stale and repeated answers, refresh races, connection
  changes, duplicates, filters, focus, and retained months and eviction.
- `calendar/load_tests.rs` (10): the HTTP boundary on loopback. It covers the
  vault and connection check, the grid range and the header-only key, Sonarr's
  extra parameters, each failure kind, a refused connection, and an aborted
  request that cannot answer.
- `calendar/view_tests.rs` (21): the rendered screen with real focus and
  keystrokes. It covers opening on today, arrows, Home and Down, Enter, the
  month controls, the selected day's order, every source state, refresh,
  Tab, and every scene at all four standard sizes.
- `view.rs` (3): Calendar as a retained root, no page above it, and sign-out
  dropping it.
- `matinee-integrations/src/calendar_tests.rs` (16): request parameters, the
  key in a header only, 401, 5xx, timeouts with redaction, malformed bodies,
  partial bodies, ordering, timing, civil days, and the artwork fetch.

## Validation

See the pull request for the commands and results on the final head.
