# Library

Phase 3E. Library is the second authenticated root destination, beside
Home. It browses every movie or every series the person can see, sorted and
filtered on the server, a page at a time, in a virtualized poster grid that
costs the same to draw at 100 titles or 10,000. Details and the Player open
above it, and Back returns to the same Library.

```text
Launch → Login or restored session → Home ⇄ Library → Details → Player
                                       ↑        ↑         ↑         │
                                       └────────┴── Back ─┴─────────┘

MatineeRoot
  ├─ roots (created once per sign-in, kept alive while hidden)
  │    ├─ home: HomeScreen
  │    └─ library: LibraryScreen ── LibraryModel (no GPUI, no HTTP)
  │          Request + Ticket ──► ServiceRuntime ── library::load ── JellyfinClient
  │          Response + Ticket ◄─┘     library_page / library_views /
  │                                    library_genres / item_details
  │          VirtualGrid (Atelier) ── frame(): visible + overscan range
  │          ArtworkLoader (shared 96 MiB LRU) ── posters for that range only
  └─ pages: Navigation<Page>  (Details, Player) above whichever root is current
```

## Shipping Library audited

Reference: `src/components/Library.tsx`, `AppNav.tsx`, `MediaCard.tsx`,
`Home.tsx` / `HomeEditorial.tsx` (entry points), `App.tsx` (routing),
`getLibraryItems` in `src/lib/jellyfin.ts`, and the `.library-*` and
`.media-card*` rules in `src/styles.css`.

| Area | Shipping behavior |
|---|---|
| Entry points | The top nav's **Movies** and **Series** (`AppNav`), Home's library shortcut tickets (Movies, Series), the Movies and Series rows' "View all", and the footer links. Each sets `view` to `movies` or `series`. |
| Movies vs Series | Two views of one component, `type: 'Movie' \| 'Series'`. No tabs inside the page; the nav is the switch. |
| Jellyfin libraries | Not used. `GET /Users/{id}/Items?Recursive=true&IncludeItemTypes=Movie` across every library; no `ParentId`, no view picker. Multiple movie or TV libraries merge. |
| Sorting | A `<select>`: Title (`SortName`, ascending, default), Recently added (`DateCreated`), Release year (`ProductionYear`), Rating (`CommunityRating`), the last three descending. Server-side. No tie-breaker. |
| Filtering | None. No genre, year, played, or favorite filter. |
| Search | Not in Library. The nav's search button opens `SearchOverlay`, a global Jellyfin search across movies, series, and episodes (Phase 3F). |
| Pagination | None. One request, `Limit=240`. A library over 240 titles is silently cut at 240. |
| Count | "N titles from Jellyfin", where N is the number returned (so at most 240, not the library size). |
| Grid | CSS `repeat(auto-fill, minmax(150px, 1fr))`, gap 24 × 14, side padding 5vw. Every item is in the DOM. |
| Card | `MediaCard`: poster at 360 px, hover play glyph, `★ 7.8 · Genre` pill, progress bar from `PlayedPercentage`. No visible title or year (only `aria-label`), no watched mark. |
| Loading | "Loading movies…" text in place of the grid. |
| Empty | "No movies found in this Jellyfin library." |
| Error | The request's error message (server text) painted in red. No retry. |
| Activation | Click opens Details (`setSelectedItem`). Never plays. No context menu. |
| Keyboard | Native button tab order through every card; no arrow-key grid movement. |
| Refresh | None. Changing the sort refetches. |
| State across Details | `App` replaces Library with Details, so Back remounts it: the sort resets to Title, the request runs again, scroll returns to the top. |
| Header | A 310 px header with the theater artwork, eyebrow "Your library", the title, and the count; the sort select on the right. |

## Parity

### Implemented

| Shipping behavior | Native |
|---|---|
| Movies and Series reached from the top nav | The shared app bar (Home · Movies · Series) on both roots. |
| Movies / Series from Home | Recently Added Movies / Series rows' "View all" open Library for that kind. |
| Every library merged by default | Default query has no `ParentId`, exactly as shipping. |
| Four sorts, same directions, Title default | Same four, same labels and directions, server-side. |
| Poster grid at 360 px artwork | Same `TILE_POSTER_WIDTH` address as Home's poster cards. |
| Progress on cards | The quiet amber line Home uses. |
| Card opens Details, never plays | Same. |
| "Your library", title, count | Same header text; the count is the server's total. |

### Intentionally changed

- **Paged, not capped.** 100 titles per request, the next page requested as
  the grid nears the end of what is loaded. A 5,000-title library shows all
  5,000; shipping stops at 240 without saying so.
- **The count is the library's**, from `TotalRecordCount`, not the number
  returned.
- **Ties are ordered.** Every sort but Title adds `SortName` as a second key
  (`SortBy=ProductionYear,SortName&SortOrder=Descending,Ascending`), so
  titles with one year or rating keep one order across pages.
- **Lean fields.** Grid pages ask only for what a card and its order need
  (`Genres,ProductionYear,CommunityRating,OfficialRating,PrimaryImageAspectRatio,DateCreated,PremiereDate`
  plus user data, one image of each type), not People, MediaSources, and
  Chapters as shipping does.
- **Filters** (shipping has none): *Show* — All titles, Unwatched, Watched,
  Favorites (`Filters=IsUnplayed|IsPlayed|IsFavorite`) — and *Genre*, one at a
  time (`GenreIds`). Both server-side. Year, studio, rating, and the rest
  stay out: they are what Jellyfin exposes, not what Matinee needs.
- **Library choice** (shipping has none): a *Library* menu appears only when
  the person has more than one library that can hold the current kind.
  Default is all libraries.
- **Visible titles and years.** Each card shows the title, `year · ★ rating`,
  and a small check when watched. The rating/genre pill and hover glyph are
  left out (as on native Home).
- **State survives Details and the Player.** Query, pages, scroll offset,
  and focused card are where they were; shipping reloads from the top with
  the default sort.
- **Typed copy, no server text.** Errors are fixed sentences with Try again.
- **Explicit Refresh** in the app bar.
- **Keyboard grid.** Arrow keys, Home, End, Page Up, Page Down, Enter, and
  Space.

### Deferred

- Home's library shortcut tickets, footer links, Top rated "view all":
  Home's app bar and the two "View all" buttons cover the routes. The
  tickets come back with Home's editorial pass.
- Year, decade, studio, and official-rating filters; multi-select genres.
- Search inside Library: shipping has none; global search is Phase 3F and
  will be its own screen. No second search architecture.
- Context actions (mark watched, favorite) on cards; collections and people
  browsing; Poster Studio's custom posters on cards.
- Metadata eviction for very deep scrolls (see Performance).

## Navigation

`nav.rs` now has two layers:

```rust
enum RootDestination { Home, Library(LibraryKind) }   // the app bar
struct Navigation<Page>                                // Details, Player
```

- A **root destination** is a place in the app bar. `RootDestination::BAR`
  lists only native ones (Home, Movies, Series). Search, Calendar, and
  Settings join the enum when they exist; nothing else changes.
- Each root screen (`Root::Home`, `Root::Library`) is created once per
  sign-in (Library on first use) and kept alive while another root or a
  page is showing. Switching roots never touches the page stack, and the
  app bar is only drawn on roots.
- Pages push above the current root. Back from the last page shows that
  root again — Library, not Home.
- `StaleRoots`: opening the Player marks the stack's root stale as before
  (`Navigation::mark_root_stale`); when the stack empties, the shell marks
  **every** root, the visible root takes its mark at once, and the other
  takes its mark when it next shows. Home then reloads its shelves; Library
  reconciles one title (below).
- Sign-out and session end drop both roots and the client.

## Home integration

- Home's top bar is now the shared app bar (`app_bar.rs`): MATINEE ·
  Home · Movies · Series · name · Refresh · Sign out. Movies and Series emit
  `HomeEvent::Navigate(RootDestination::Library(kind))`.
- Recently Added Movies and Recently Added Series show **View all** once
  they have titles. It emits `HomeEvent::Browse(kind, LibrarySort::DateCreated)`:
  Library for that kind, newest first, matching the row. The same sort keeps
  the loaded pages; a different one starts a new query.
- Continue Watching, Next Up, and Favorites have no Library equivalent
  (Favorites mixes movies and series), so they get no "View all". No dead
  buttons.
- Home stays alive while Library shows: its page and row offsets and
  focused card survive (tested).

## LibraryModel

`apps/matinee-next/src/library/model.rs`. No GPUI, no HTTP. One `Catalog`
per kind, so Movies → Series → Movies lands where it was:

| Field | Meaning |
|---|---|
| `query: LibraryQuery` | kind, library (`Option<LibraryId>`), sort, filter (watch + genre) |
| `items`, `ids` | the loaded titles, in server order, deduplicated |
| `total`, `next_start`, `has_more` | from the server's page answer |
| `state: CatalogState` | `Loading`, `Ready`, `Empty(Library \| Filtered)`, `Failed(failure)` |
| `pending: Option<(Ticket, start)>` | the one page in flight |
| `more_failed` | the next page failed; pages already shown stay |
| `genres: Fetch<…>`, `genres_ticket` | the genre menu for this kind and library |
| `focused`, `opened` | the card with logical focus; the card last opened |
| `reconcile` | the ticket of a one-title refresh after playback |

Libraries (`views`) are fetched once and shared by both kinds. The screen
asks the model for `Request`s and hands answers back with `apply`, which
returns `Ignored`, `Replaced`, `Appended`, `Updated`, or `SessionExpired`.
No request is built in a render function, and render functions make no
product decisions; the screen reads `state()`, `footer()`, `items()`.

## Typed Jellyfin queries

`matinee-core` (`library.rs`):

- `LibraryQuery { kind, view, sort, filter }`, `LibraryFilter { watch, genre }`,
  `WatchFilter`, `LibrarySort` (unchanged four, now with labels).
- `LibraryPageRequest { start, limit }` and `LibraryPage { items, start, total }`
  with `has_more(limit)`: by the total when present, else a full page means
  maybe more. An empty page is always the end.
- `LibraryView { id, name, content }`, `LibraryContent::{Movies, Series, Mixed}`
  with `holds(kind)`; `LibraryGenre { id, name }`.

`matinee-jellyfin`:

| Call | Request |
|---|---|
| `library_page(&LibraryQuery, LibraryPageRequest)` | `GET /Users/{id}/Items?Recursive=true&IncludeItemTypes=…[&ParentId=…]&SortBy=…&SortOrder=…[&Filters=…][&GenreIds=…]&StartIndex=…&Limit=…&Fields=…&ImageTypeLimit=1&EnableUserData=true&EnableTotalRecordCount=true` |
| `library_views()` | `GET /Users/{id}/Views`. `movies` → Movies, `tvshows` → Series, absent or `mixed` → Mixed; music, books, photos, box sets, and the rest are left out. |
| `library_genres(kind, view)` | `GET /Genres?userId=…&includeItemTypes=…&recursive=true&sortBy=SortName[&parentId=…]` |

The shipping `library_items` (240) is unchanged. DTOs stay private; the app
sees only `matinee-core` types. A 401/403 is `Unauthorized`; nothing returns
server text.

## Pagination

- **Page size 100** (`PAGE_SIZE`). Shipping asks for 240 at once with heavy
  fields. The widest window (1920 × 1080) shows ten columns by about four
  rows, so 100 fills it more than twice; at 960 × 620 it is about eight
  screens. A smaller page would mean several requests before the first
  screen settles on a large window; a larger one makes every query change
  slower. With the lean fields a page is roughly the size of shipping's
  heavy 40-item search answer. Full scroll through 10,000 titles is 100
  requests, made only as the person scrolls.
- **Initial page**: on first show of a kind, with the libraries and genres.
- **Next page**: `want_more(last_built)` when the last card the grid builds
  is within `PREFETCH_ITEMS` (40, about four rows at the widest) of the end
  of what is loaded. Not the final pixel.
- **One at a time**: nothing while a page is pending, after the end, or
  after a failure (that waits for Try again). Called on every frame, it
  issues at most one request per page; a render never creates a request by
  itself.
- **End**: `has_more == false` → the footer is empty. No spinner.
- **Retry**: the footer's Try again asks for the same `start` again.
- **Duplicates**: a title already loaded is skipped if a later page repeats
  it (the library changed underneath); paging follows the server's offsets,
  so nothing loops.

## Stale-response protection

Every request carries a `Ticket` from one counter. A catalog applies a page
only if it is the ticket it is waiting for (`pending`); likewise genres,
libraries, and the reconcile request. Then:

| Situation | Result |
|---|---|
| Sort, filter, genre, or library changes | `clear()`: titles, total, focus, failures gone; a new first-page ticket. The grid resets to the top. |
| The old query's page 2 arrives afterwards | `Ignored`; nothing appended. |
| Several quick changes, answers in any order | Only the last query's first page applies. |
| Refresh A, refresh B, A arrives last | A is ignored. |
| A Movies page arrives while Series shows | Applied to Movies, which is a separate catalog. |
| A screen-level page task is replaced | The previous task is aborted (its answer could not apply anyway). |

Tests: `rapid_query_changes_apply_only_the_last`,
`a_query_change_ignores_the_old_query_s_late_pages`,
`refresh_keeps_titles_until_the_new_first_page_and_ignores_stale_answers`.

## Loading, refresh, empty, and errors

- **First load / query change**: the header (title, menus) stays; two rows
  of card-shaped placeholders at the real card size. The count is blank. Old
  titles are never shown under a new query's label: a query change clears
  them at once.
- **Next page**: the loaded cards stay; a quiet "Loading more titles…" line
  under the last row.
- **Refresh**: same selections, from page 1. The current titles stay on
  screen until the new page 1 arrives, then replace every page. If the
  focused title is in the new page 1, focus and scroll follow it; otherwise
  the grid returns to the top (a refresh may reorder anything). A failed
  refresh keeps what is shown. The libraries and genres are asked again.
- **Empty library**: "Nothing is in this library yet." (plus a quiet note
  that titles appear once Jellyfin has scanned them).
- **Filtered empty**: "Nothing matches these filters." with Clear filters.
  Neither uses error styling.
- **First page failed**: "Jellyfin isn't answering" / "This library couldn't
  be loaded" with a fixed sentence and Try again.
- **Next page failed**: pages 1–3 stay; "More titles couldn't be loaded."
  with Try again under the grid. Scrolling does not retry by itself.
- **Genre or library list failed**: that menu is left out; browsing works.
- **Artwork failed or missing**: the calm placeholder with the title.

## Session expiration

An authorization failure on any Library request — a page, the libraries,
the genres, or the reconcile — makes `apply` return `SessionExpired`
without changing what is shown. The screen emits
`LibraryEvent::SessionExpired`; the shell's existing
`MatineeRoot::expire_session` acts once (see
[application.md](application.md#session-end)). Several Library requests
failing together, or a Library that is no longer current reporting late, do
nothing more (tested). Unreachable servers never sign anyone out.

## Virtual grid (Atelier)

`atelier_ui::VirtualGrid`, `VirtualGridState`, `GridSizing`, `GridLayout`,
`GridStep`, `GridViewport`, `GridCell` — generic, no media concepts (an
architecture rule checks the file).

- **Layout is arithmetic** (`GridLayout`, pure): columns from width, cell
  size, row pitch, content height, visible rows, materialized range (visible
  plus `overscan_rows`), reveal offset, keyboard steps, and resize anchoring.
  The element is one scroll container holding a single tall content box;
  only materialized cells exist, absolutely positioned.
- **Measurement**: a canvas records the viewport size at paint. Before the
  first paint the owner's fallback size is used; when the measured size
  differs from what a frame used, the owner is redrawn once (on resize too).
- **Owners read the same window**: `VirtualGridState::frame(sizing, count,
  footer, fallback)` applies anchoring and pending reveals and returns the
  visible and materialized ranges. Library calls it in render to sync
  artwork and pagination; the grid calls it again with the same inputs and
  gets the same answer.
- **Focus**: one tab stop for the whole grid. The focused cell is a logical
  index in the state ("active descendant"), not a handle per cell, so it
  survives its cell being unbuilt. Arrows move by cell and row, Page Up /
  Page Down by a viewport of rows, Home / End to the ends; movement does
  not wrap, and Down into a shorter final row lands on its last item. The
  destination is scrolled into view (least movement) and built on the next
  frame. Enter and Space activate. A click focuses the grid, moves the
  logical focus there, and activates. The ring is drawn by the owner's cell
  only in keyboard modality.
- **Resize**: when the column count changes, the focused cell — or the first
  visible cell if focus is off screen — keeps its height on screen. The
  logical index is untouched, so 6 → 4 columns keeps the same title.
- **Footer**: optional fixed-height content below the last row.
- **Gallery**: "Virtual Grid" story with 100 / 1,000 / 10,000 items, the
  built-cell count, focus, and activation.

Atelier also gained `Menu::max_height` / `Popover::menu_max_height` so a long
menu (genres) scrolls inside its panel with the keyboard cursor kept in
view; the Menu story has a 30-row example.

## Responsive columns

`GridSizing` for Library: cards 148–180 points wide (as many 148-point
columns as fit, then widened to share the row, never past 180, where a
360-pixel poster still covers a 2x display), 20-point column gap, 24-point
row gap, the Home gutters (40 under 1100 wide, 64 above), at most 10
columns, one overscan row each side.

| Window | Columns | Card width |
|---|---|---|
| 960 × 620 | 5 | 160 |
| 1200 × 760 | 6 | 162 |
| 1440 × 900 | 7 | 170 |
| 1920 × 1080 | 10 | 161 |

Pinned by `columns_follow_the_window_and_cards_stay_readable`.

## Artwork

- Library uses the existing `ArtworkLoader`, `ArtworkRequest`,
  `DecodedImage`, and the shared 96 MiB LRU. There is no Library cache.
- Posters are `Primary` at `TILE_POSTER_WIDTH` (360), the same address as
  Home's poster cards and Details' related titles: one decode serves all
  three. Cards are drawn 160–180 points wide, so no new width was needed.
- **Window**: `artwork_window(items, materialized)` — only the built cards
  (visible plus one overscan row each side). When the window moves, slots
  that left are dropped and their in-flight fetches aborted; the decoded
  images stay in the shared LRU, so a card coming back is a cache hit
  (`artwork_follows_the_built_cards_and_the_cache_serves_returns`). Work is
  done only when the window's addresses change, not per frame.
- **Ceiling**: at most 25 slots at 960 × 620 and 60 at 1920 × 1080 (ten
  columns × six rows). 60 posters at 360 × 540 are about 44.5 MiB decoded,
  under half the cache. 300 loaded titles with 30 on screen means about 30–50
  poster loads, never 300.
- The shared LRU may evict Home's images while a large Library window is
  showing; Home then fetches them again. Bounded memory and reuse, not
  residency, is the guarantee.

## Scroll, focus, and the Player round trip

- Library stays alive under Details and the Player, and while Home shows,
  so the query, pages, scroll offset (`VirtualGridState`'s `ScrollControl`),
  and focused index are exactly where they were; Details → Back fetches
  nothing (tested: `library_survives_details_and_the_player`).
- Focus returns to the grid, whose logical focus is the card that was
  opened; if that card is off screen it is revealed.
- **Playback**: Back to Library after the Player asks for **one title**
  again — the card that was opened — with `item_details`, and replaces it
  in place (progress, watched mark). No page is reloaded and nothing moves.
  If that title no longer matches an active Show filter (now watched under
  Unwatched), it stays until the next Refresh or query change rather than
  vanishing under the pointer. Other titles changed by the same session
  (another episode of a different series) refresh with the next Refresh.

## Keyboard and mouse

Tab walks the app bar, the header menus, then the grid (one stop). In the
grid: arrows, Home, End, Page Up, Page Down, Enter, Space. Menus keep their
own keys; the grid's keys are bound only in its key context, so nothing is
taken from a text field. Escape belongs to open menus, then the shell.
Mouse: wheel and trackpad scroll the grid; a click opens Details; hover
dims the card slightly.

## Performance

`MATINEE_LIBRARY_STATS=1` prints, per answer: titles held, pages requested,
total, artwork slots, fetches in flight, and the shared cache's hits,
misses, entries, and bytes; and once, the time from opening Library to the
first grid with the number of cards built. Nothing is sent anywhere; it is
off by default.

Scale results (deterministic tests, no images):

| Library | Cards built (most) | Pages to reach the end | Titles held at the end |
|---|---|---|---|
| 100 | ≤ 35 (grid tests) / < 100 (screen) | 1 | 100 |
| 1,000 | ≤ 35 | 10 | 1,000 |
| 10,000 | ≤ 35, tens at any depth | 100 | 10,000 |

Opening a 10,000-title library fetches one page (100 titles) and builds
tens of cards. **Metadata retention**: titles loaded for the current query
stay until the query changes or Library is dropped (sign-out). With the
lean fields that is estimated at the order of 1 KB per title, so a full
scroll of 10,000 is on the order of 10 MB of metadata. Posters are bounded by the window, not by what is loaded.

## Review scenes

`MATINEE_PREVIEW` accepts `library` and `library-movies` (two movie
libraries, the genre menu, 1,284 titles), `library-series`,
`library-loading`, `library-empty`, `library-filtered-empty`,
`library-partial-page` (page 4 failed), `library-error`,
`library-small-window` (960 × 620), `library-large-window` (1920 × 1080),
and `library-many-items` (10,000 titles, six pages loaded, keyboard focus on
card 432). Generated titles (one very long), years, ratings, progress and
watched marks, and the abstract review posters; no socket. In review
scenes the app bar's Movies and Series open this fixture Library.

## Architecture guards

`scripts/check-architecture.sh` adds: Library requests (`library_page`,
`library_views`, `library_genres`) are made only by `library/load.rs`; the
virtual grid file names no product concept. Existing rules still hold: no
`reqwest` in the app, one Tokio runtime, no `block_on`, no GPUI in
`matinee-core` or `matinee-jellyfin`, no Matinee terms in Atelier, no token
in artwork URLs, artwork only through `artwork.rs`, playback planning only in
the Player, clients built only by the shell.
