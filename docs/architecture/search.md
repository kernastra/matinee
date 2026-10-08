# Search

Phase 3F. Search is the third authenticated root destination, beside Home
and Library. The person types a title, sees the movies, series, and episodes
that match, opens one into Details, and comes back to the same results. The
query, the results, the scroll offset, and the focused title survive Details
and the Player.

```text
Launch → Login or restored session → Home ⇄ Library ⇄ Search
                                       ↑        ↑         ↑
                                       └────────┴── Back ─┴─→ Details → Player
                                                               (Back returns
                                                                to Search)

SearchScreen (root) ── SearchModel (no GPUI, no HTTP)
      │   input (typed) ≠ effective (asked) ≠ shown (held)
      │   Request + Ticket ──► ServiceRuntime ── search::load ── JellyfinClient
      │   Response + Ticket ◄─┘                   search_page / item_details
      │   SearchField (Atelier) ── typing, Enter, Escape
      │   VirtualGrid (Atelier) ── frame(): visible + overscan range
      │   media_grid (shared with Library): card, Layout, skeleton, artwork window
      └── ArtworkLoader (shared 96 MiB LRU) ── posters for that range only

SearchField (focus_handle: the screen's) ──Down──► VirtualGrid
            ▲                                         │
            └──────── on_edge(Up) from the first row ─┘
```

## Shipping Search audited

Reference: `src/components/SearchOverlay.tsx` (the overlay),
`src/components/AppNav.tsx` (the trigger), `src/App.tsx` (where it mounts),
and `searchLibrary` in `src/lib/jellyfin.ts`. Native Matinee was checked
against the Jellyfin Items endpoint and `crates/matinee-jellyfin`.

| Area | Shipping behavior |
|---|---|
| Entry point | The magnifier icon in the top nav (`AppNav`, `aria-label="Search library"`) on every signed-in view. Sets `searchOpen`. |
| Route | None. A modal overlay (`role="dialog"`, `aria-modal`) above the current view. Not a page and not a route. |
| Search field | One `<input>`, placeholder "Search movies, series, and episodes", with an **Esc** button. No clear button. |
| Focus on open | The input is focused on mount (`inputRef.focus()`). |
| Debounce | 220 ms `setTimeout` after the last keystroke. Each change aborts the previous request (`AbortController`). |
| Minimum query length | Two characters after `trim()` (`trimmed.length < 2` clears results and sends nothing). |
| Submit | None. Enter does nothing; results appear from the debounce alone. |
| Keyboard | **Escape** closes the overlay (window listener). No arrow-key movement into results, no keyboard selection. |
| Search API | `GET /Users/{id}/Items` with `Recursive=true`, `SearchTerm`, `IncludeItemTypes=Movie,Series,Episode`, `Limit=40`, `Fields`, `EnableUserData=true`. Authorization header; no `StartIndex`, no total. |
| Item types | Movie, Series, Episode. Collections, People, and every other searchable type are not requested. |
| Result ordering | Server order (Jellyfin relevance). No client re-sort. |
| Result limit | 40, one request. No pagination, no "load more". |
| Result card | Poster (`Primary`, width 160) plus a line: name, and `SeriesName · Type · ProductionYear` (empty parts dropped). No progress, no watched mark. |
| Artwork | `Primary` for every type, through the shared `Artwork` component. Episodes use their own Primary image when Jellyfin has one. |
| Empty query | Results cleared; nothing else is shown. No idle copy. |
| Too short | Results cleared; nothing shown. |
| No results | "No matching titles." under the field, shown only when the trimmed query has two or more characters. |
| Loading | "Searching Jellyfin…" line in place of results; old results are cleared when loading begins. |
| Errors | A failed request sets results to `[]`. The user sees "No matching titles." — the error is silent and indistinguishable from zero matches. |
| Choosing a result | `onSelect(item)` sets the selected item (the shared Details view) and closes the overlay. Every result type opens Details; nothing plays directly. |
| Episodes | Returned and opened through the same Details path as movies and series. |
| Clearing | Erase the text. Escape closes the overlay. |
| Query persistence | None. The overlay unmounts on close and its state is discarded. |
| Focus return | None defined; focus returns to the page behind the overlay. |
| People navigation | None. People are not searched. |
| Collection navigation | None. Collections are not searched. |
| Telemetry / logs | None. |

### What the audit means for native Search

- **Minimum length: two characters**, kept. One letter matches most of a
  library and Jellyfin returns a long list nobody reads. Encoded once in
  `matinee_core::SEARCH_MIN_CHARS` with tests.
- **Types: Movie, Series, Episode**, kept. Each opens the existing native
  Details screen. Episodes open Details and do not start playback.
- **Debounce: 300 ms** (shipping 220 ms). Chosen inside the 250–350 ms range
  the brief asks for: a word typed at normal speed sends one search, and the
  wait is short enough to feel live. Documented in `search/model.rs`.
- **Pagination: added.** Shipping stops at 40 with no way to see more.
  Native Search pages server-side, like Library, so a query with 10,000
  matches is reachable without fetching them all.
- **Page size: 60.** Six rows at the widest grid, which covers the window
  with overscan. Search answers arrive on each query and each scroll to the
  end, so a smaller first page keeps the first results quick. Library uses
  100 because it is a whole browse, not a search.
- **Enter submits now.** Shipping has no submit. Native Enter bypasses the
  debounce, because a person who presses Enter expects the results now.
- **Errors are shown, not hidden.** Shipping turns a failed search into
  "No matching titles." Native Search separates "no matches" from "could not
  search" and offers Try again, without echoing server text.
- **Escape on an empty field leaves Search for Home.** Escape with text
  clears the field (the `SearchField` behavior). Shipping closes the overlay;
  native Search is a root, so it returns to the previous kind of place.

## Native parity

| Behavior | Native Search |
|---|---|
| Open | A root destination in the app bar (**Search**), like Home and Library. Focus goes straight into the field. |
| Type | Input is kept as typed; the search waits 300 ms, then sends. |
| Enter | Sends the input now. A query already held sends nothing. |
| Results | `VirtualGrid` of the shared media cards: poster, title, year, rating. An episode reads as its series over `S2 E5 · Pilot`. |
| Open a title | Details (the existing screen). |
| Return | Search is still on screen, with the same text, results, scroll, and focused title. |
| Escape | Clears the text when there is some; on an empty field, goes to Home. |
| Clear | The field's clear button, or erasing the text. |

## Intentional differences

- **Search is a root, not an overlay.** The brief names it a root destination,
  and a root keeps its state between visits. Shipping's overlay forgets.
- **Retained results.** Search → Home → Search and Search → Library → Search
  keep the query and results for the session. No history is saved to disk.
- **Two-level focus.** Down from the field enters the results; Up from the
  first row returns to the field. Shipping had no keyboard movement.
- **Stale results are dimmed and inert while a new query loads.** Shipping
  cleared the list the moment loading began.
- **Pagination and total count**, described above.
- **Failures are distinct from zero matches.**

## Deferred result types

| Type | Status | Reason |
|---|---|---|
| People | Deferred | Native Matinee has no People destination. Building a People browser is out of scope for 3F. Shipping does not search people either. |
| Collections (box sets) | Deferred | Native Matinee has no Collections browser. Shipping does not search collections either. |
| Seasons | Excluded | Not in shipping's results; a season opens inside its series, which Search does not offer yet. |
| Music, books, photos, other | Excluded | Matinee is a movies-and-series app. |

Each deferred type is simply absent from the request (`IncludeItemTypes`),
so no dead result is ever drawn.

## SearchModel

`apps/matinee-next/src/search/model.rs`. Pure state; no GPUI, no HTTP.

- `input`: what the person has typed, as typed.
- `effective`: the newest query asked of Jellyfin. `None` when the input is
  blank or too short.
- `shown`: the query whose titles are held. Equal to `effective` once its
  first page has arrived.
- `items`, `ids`, `total`, `next_start`, `has_more`: the loaded pages.
- `pending`: the request in flight and where its page starts.
- `failure` (the first page of `effective` failed), `more_failed` (a later
  page failed), `focused`, `opened`.

`SearchState` is what the results area shows: `Idle(Empty | TooShort)`,
`Loading`, `Ready`, `NoResults`, or `Failed`. `is_stale()` is true while
titles from an older query are held during a new query's load.

## Input query vs effective query

The field and the results are different things and are kept apart.

```text
type "alien"     input = "alien"    effective = (none, debounce pending)
debounce ends    input = "alien"    effective = "alien"   shown = (none)  → Loading
first page       input = "alien"    effective = "alien"   shown = "alien" → Ready
type "aliens"   input = "aliens"   effective = "alien"   shown = "alien"  (debounce)
debounce ends    input = "aliens"   effective = "aliens"  shown = "alien"  → Loading, stale
first page       input = "aliens"   effective = "aliens"  shown = "aliens" → Ready
```

While debounce is pending, the results on screen still belong to the query
they were loaded for: `shown` does not move. Titles from `alien` are never
shown under `aliens`. When the effective query changes, the old titles stay
on screen, dimmed and inert, until the new first page replaces them.

Typing back to the query whose titles are held (`alien → aliens → alien`)
shows them again at once, undimmed, with no wait and no request: they are
that query's results. The request for `aliens` is abandoned, its HTTP call
is stopped, and its late answer is ignored.

## Debounce

- Duration: **300 ms** (`DEBOUNCE_MILLIS`).
- Each keystroke calls `SearchModel::type_text`, which returns a `Debounce`
  token. The screen starts a timer for that token. A new keystroke drops the
  old timer task (cancelling it) and bumps the token, so an old timer that
  still fires does nothing.
- Tests fire tokens by hand (no clock), so timing is deterministic.

## Immediate Enter

`SearchModel::submit()` bumps the debounce token, so the pending wait cannot
send a second time, then sends the input. If the input is already the
effective query, nothing is sent, including while its first page is on its
way. If the effective query's first page failed, Enter is Try again: the
failure is cleared and the first page asked once more.

## Query normalization

`matinee_core::SearchQuery::parse` trims leading and trailing whitespace and
counts characters, not bytes, for the minimum. Case, punctuation, and inner
spacing are kept as typed. Jellyfin does the matching; nothing is fuzzy-matched
on the device.

## Empty query and no results

- Empty or blank input: the calm start, "Search your library", with the hint
  "Movies, series, and episodes appear as you type." No request is sent. No
  "No results" message is shown before a search has run.
- One letter: "Keep typing" with "Two letters or more to search." Nothing is
  sent, and held titles are dropped.
- A searched query with zero matches: "No results for “term”" and "Check the
  spelling, or try fewer words." The term is the user's own text, shown on
  screen only.

## Query generations and stale responses

Every request carries a `Ticket` from a counter that never repeats. The model
applies a response only when its ticket is the one in flight (`pending` for a
page, `reconcile` for an opened title). Consequences, all tested:

- A late answer for an earlier query is ignored (`a_new_query_gets_a_new_request...`).
- A late answer never overwrites newer results (`a_late_answer_never_overwrites...`).
- A next-page answer for a query that has since changed is dropped
  (`changing_the_query_during_a_next_page_drops_the_late_page`).
- Typing back to the query already in flight keeps that request
  (`typing_back_to_the_query_in_flight_keeps_its_request`).
- `alien → aliens → alien`, `matrix → mat → matrix` (with the answer in
  between), and `A → B → clear → C` apply only what the field asks for now
  (`alien_aliens_alien_…`, `matrix_mat_matrix_…`, `a_then_b_then_clear_…`).
- A failure for an earlier query never touches the current state
  (`a_failed_earlier_query_cannot_touch_the_current_error_state`).
- Tickets come from one counter and are never reused, so returning to an
  earlier query can never accept that query's old ticket.

Cancellation is an optimization, not correctness. The screen aborts the
previous page task when a new page is asked, aborts the page the model
abandons (the field cleared, or back to the held query), and drops in-flight
artwork for posters that leave the window. Correctness does not depend on any
of these.

## Pagination

- `PAGE_SIZE = 60`, `PREFETCH_ITEMS = 40`.
- Server-side: `StartIndex`, `Limit`, `EnableTotalRecordCount=true`.
- `want_more(last_built)` asks for the next page only when the last built
  card is within `PREFETCH_ITEMS` of the loaded end, the state is `Ready`,
  the titles are not stale, and no page is in flight. Renders that change
  nothing send nothing.
- At most one next-page request is in flight. The end is reached when the
  server reports no more, when a page comes back empty, or, without a total,
  when a page comes back short. Offsets follow what the server sent, so a
  page that repeats titles (the library changed) neither loops nor skips.
- A next-page failure keeps everything loaded and offers Try again at the
  footer. The first page is never lost.
- Try again sends nothing while a page is on its way. Refresh clears any
  failure (first page or later page) because it supersedes it, so the two
  never ask for different pages at once. A failed refresh of the query on
  screen keeps its titles, as Library does.

## Jellyfin search API

`crates/matinee-jellyfin`:

- `JellyfinClient::search_page(&SearchQuery, LibraryPageRequest, Option<&CancelFlag>) -> LibraryPage`
- Query: `Recursive=true`, `SearchTerm`, `IncludeItemTypes=Movie,Series,Episode`,
  `StartIndex`, `Limit`, `Fields` (the Library grid fields), `ImageTypeLimit=1`,
  `EnableUserData=true`, `EnableTotalRecordCount=true`.
- Returns the shared `LibraryPage` type. No raw DTO reaches the UI.
- The old unpaged `search_library` is removed; it had no caller outside its
  tests.

## VirtualGrid reuse

Search uses Atelier's `VirtualGrid` and `VirtualGridState` unchanged: the
same sizing (`Layout::sizing`), the same footer slot, the same focus and
keyboard movement (Left, Right, Up, Down, Home, End, Page Up, Page Down,
Enter, Space). Atelier gains nothing search-specific.

The card and grid presentation moved from `library/screen.rs` to
`apps/matinee-next/src/media_grid.rs` so both roots
share one poster card, one layout, one skeleton, and one artwork-window rule.
Library's element ids are unchanged (`library-card`, `library-art`,
`library-skeleton`); Search's are `search-card`, `search-art`, and
`search-skeleton`.

## Result presentation

Each card is the Library card: a poster, the title in one line, and
`2019 · ★ 7.8`. Progress and the watched mark appear when the title has them.

An episode reads as Home's episode cards do: its series as the title and
`S2 E5 · Pilot` below (`home::card_title`, `home::card_detail`), so a search
for "pilot" is not a wall of identical "Pilot" cards. Library holds only
movies and series, so its cards are unchanged (pinned by
`media_grid::tests::movies_and_series_read_as_library_always_has`). Movies
and series still carry no type label; the series/episode line resolves the
real ambiguity, and a label waits until a result shows the need.

## Artwork roles

| Result | Artwork | Address |
|---|---|---|
| Movie | Primary (poster) | `ArtworkUrls::item_request(item, Primary, TILE_POSTER_WIDTH)` |
| Series | Primary (poster) | same |
| Episode | Primary, as the domain supplies it | same; a title without art has no request and shows a placeholder |

The addresses are the same ones Home and Library build, so the shared cache
serves all three. No access token is ever placed on an artwork URL.

## Artwork windowing

- `artwork_window(items, built, urls)` returns requests only for the cards the
  grid builds (`frame.materialized`), never for all loaded results.
- A card that leaves the window loses its slot and its in-flight fetch
  (`sync_artwork`). The decoded image stays in the shared 96 MiB LRU.
- A query change does not flush the cache. Titles that stay wanted keep their
  slots; those that do not are dropped. No Search artwork cache exists.
- While stale titles show, the grid keeps their window as usual; nothing else
  drops and restarts their posters on each render.
- Clearing the field releases every slot (no grid, no wants).
- Posters decoded by this screen that the cache did not keep are released
  from the atlas when they leave the window, and the rest when the screen is
  dropped (sign-out, session end, window close).

## State restoration

- Search is one retained `SearchScreen` per sign-in. It is created the first
  time Search is chosen and dropped with the session.
- Details and the Player cover Search as pages; the screen is not torn down.
- `show()` (chosen from the bar) puts focus in the field; `resume()` (back from
  Details) puts focus on the focused title, as Library does.
- `Search → Home → Search` and `Search → Library → Search` restore the same
  state, because the root is retained.

## Targeted playback reconciliation

After playback, only the opened title is asked for again
(`SearchModel::reconcile` → `Request::Item`). The loaded results are not
re-searched and the grid is not reset. The same mechanism as Library, with the
same ticket rule.

## Loading

- First search, no titles yet: card-shaped placeholders (`skeleton`) under the
  stable field and header, with "Searching…" in the header. No centered spinner.
- New query over titles already shown: the old titles stay on screen, dimmed to
  half opacity, and "Searching…" appears in the header. They are replaced when
  the first page arrives, and the grid starts at the top of the new results.
  Until then they are inert: a layer over them takes the pointer (no hover,
  no click; the wheel still scrolls), activation and logical-focus moves are
  ignored, and Down from the field does not enter them.
- Next page: existing titles stay; a small footer line with a spinner reads
  "Loading more results…".

## No results

"No results for “term”" with a quiet hint. The term is the person's own text.
Server text never appears.

## Errors

Handled independently, through `SearchFailure` (`SignedOut`, `Unreachable`,
`Unreadable`), each with fixed copy and no server text:

- **First page failed**: the results area shows the failure and **Try again**
  (`retry` re-asks the first page; Enter in the field and Refresh do the
  same). Old titles are not shown under it.
- **Next page failed**: the loaded titles stay; the footer says "More results
  couldn't be loaded." with **Try again**. Page one is not lost.
- **Artwork failed**: the card shows the placeholder; the shared loader reports
  nothing user-visible, as Library.
- **Session ended**: `SearchFailure::SignedOut` reports
  `SearchEvent::SessionExpired`, which the shell routes to the application-wide
  session-expiration path. Concurrent auth failures do nothing more after the
  first (the ticket is no longer current).

## Session expiration

Any authoritative authenticated Search request that the server rejects with an
auth error becomes `SearchFailure::SignedOut`, which the model reports as
`Applied::SessionExpired`. The shell's existing `expire_session` handles it.
No second logout path exists.

## Keyboard and focus

- Opening Search focuses the field. Typing goes straight in.
- **Tab** reaches the field like any field (the owner's handle is a tab stop).
- **Enter** in the field searches immediately.
- **Down** in the field moves focus to the grid (the focused title, or the
  first one) and records keyboard modality, so the card shows its ring.
  Nothing happens without results, or while stale titles show.
- **Up** on the grid's first row returns focus to the field. The grid binds
  Up itself (`NudgeUp`), and GPUI runs bound actions before key listeners, so
  a screen key listener can never see that Up; the grid reports it through
  `VirtualGrid::on_edge` instead.
- **Escape** on the field clears it when there is text, and goes to Home when
  it is empty.
- Once the grid has focus, its own keys (arrows, Home, End, Page Up/Down,
  Enter to open, Space) belong to `VirtualGrid`. Arrow keys are never taken
  from the text field while it is focused.
- Focus is restored after Details closes (`resume`): the grid, at the title
  that was opened. Chosen from the bar (`show`), focus goes to the field.
  Focus settles with `Window::defer`, once the frame that draws the target is
  done.

Atelier changes, both generic, both with no search behavior:

- `TextField::focus_handle` / `SearchField::focus_handle`: the owner holds
  the field's focus, so it can move focus into the field and see when it has
  it. The field makes the handle a tab stop, as its own handle is. Tests in
  `crates/atelier-ui/tests/text_field_behavior.rs`, including
  `an_owners_focus_handle_stays_in_the_tab_order`.
- `VirtualGrid::on_edge(GridStep)`: a movement key that cannot move (Up on
  the first row, Left at a row start) is reported; the grid keeps focus unless
  the owner moves it. `on_focus` is no longer called for a step that did not
  move. Test: `a_key_that_cannot_move_is_reported_as_an_edge` in
  `crates/atelier-ui/tests/virtual_grid_behavior.rs`.

The Gallery's Virtual Grid story has a "Focus handoff" example that uses both.

## Responsive layout

- The field is capped at 560 points (`FIELD_MAX_WIDTH`), so it does not grow
  absurdly wide on large displays.
- Results use Library's `Layout::for_width` and grid sizing unchanged.

## Security and privacy

- Search text is not logged. The search module writes no log lines at all
  (`scripts/check-architecture.sh` rule 20), and `search/load.rs` never
  formats the query. The Jellyfin client's request log line keeps only the
  scheme, host, port, and path, never the query string. A transport error's
  technical detail can include the address; Search maps every error to a
  fixed `SearchFailure` and drops that detail.
- Query text does not go into error messages, telemetry, cache filenames, or
  review artifacts. There is no telemetry or analytics.
- Failures are typed; Jellyfin's response bodies never reach the UI.
- No disk search history. Nothing about a query is written to the store.
- Artwork URLs carry no access token.
- The Jellyfin client's error type maps to `SearchFailure`; `session_ended` is
  the shared check that routes auth rejection to session expiration.

## Scale behavior

The rendered cell count follows the viewport, not the result count, because
the grid builds only the materialized window. The loaded titles are held in
memory as Library holds them: the pages the person has scrolled to, at 60 per
page, not the whole result. A 10,000-title query is reachable without loading
10,000 titles.

Model tests (no GPUI, no network):

| Results | What the test proves |
|---|---|
| 0, 1, 60, 61, 100, 120, 121 | Exactly the pages needed (`result_sizes_around_the_page_size_…`). |
| 100 | At most two pages requested while scrolling the whole result. |
| 1,000 | Pages requested equal `ceil(1000 / 60)` and track the window; never more than one in flight. |
| 10,000 | Only one page is loaded for a short look; scrolling to the end loads every page, one at a time. |

Rendered-screen tests (headless GPUI, `search/view_tests.rs`):

| Results | What the test proves |
|---|---|
| 0, 1, 60, 61, 100, 1,000, 10,000 (all loaded) | Fewer than 60 cards built at 1200 × 760, at the top and at the very end; poster slots never exceed built cards (`built_cards_follow_the_window_not_the_result_count`). |
| 10,000 (scrolled) | Forty jumps to the end ask forty pages, in order, never the same page twice; under 60 cards built throughout (`scrolling_ten_thousand_results_pages_one_at_a_time`). |

## Verification

Evidence is kept apart by kind; one never stands in for another.

| Kind | Where | Covers |
|---|---|---|
| Pure model | `search/model_tests.rs` | Debounce tokens, Enter, tickets, stale answers, rapid replacement, clear, paging, duplicates, short/empty pages, failures, retry/refresh, session end, reconcile, error mapping. |
| Jellyfin transport | `matinee-jellyfin` `api_tests.rs` | Path and user scope, `SearchTerm` encoding, fields, paging parameters, header-only token, totals, error classes. |
| HTTP boundary | `search/load.rs` tests | The real client and transport against a loopback Jellyfin: request line and header, typed answers, 401/403/500/malformed/missing/unreachable, reconcile, an aborted page. |
| Headless GPUI, screen | `search/view_tests.rs` | Focus on open, typing without a click, the debounce on the fake clock, Enter, editing keys in the field, Down/Up handoff, Escape, stale titles inert under a real click and Enter, clicks, resize, artwork window and the shared cache, every review scene at all four review sizes. |
| Headless GPUI, shell | `view.rs` tests | Home → Search, Search → Home → Search, Search → Library → Search, Search → Details → Search, Search → Details → Player → Details → Search (one reconcile, no search), playback while hidden, session end, review scenes. |
| Atelier | `text_field_behavior.rs`, `virtual_grid_behavior.rs` | The two generic hooks. |

Not verified: pixels. GPUI's headless test window never asks for a frame
and paints nothing. In the Linux container used for the 3F review, the app
under Xvfb with lavapipe opened its window at the requested size, but every
capture was solid black, for Library's merged scenes as well as Search's.
Layout, opacity, and the focus ring are asserted through state, not seen; the
scenes still need a look on a real display.

## Review scenes

`MATINEE_PREVIEW` accepts `search` (the populated result), `search-empty`,
`search-short`, `search-typing` (results for "harbor" held, the field reading
"harbor li", its wait not ended), `search-loading`, `search-many-results`
(10,000 matches, five pages loaded, keyboard focus on index 251),
`search-no-results`, `search-error`, `search-partial-page` (the third page
failed), `search-stale` ("harbor" dimmed while "harbor lights" loads),
`search-episodes` ("pilot": episodes of six series), `search-small-window`
(960 × 620), and `search-large-window` (1920 × 1080). `MATINEE_PREVIEW_SIZE`
gives any other size (1440x900, say). The fixture server answers any query,
with titles that follow from its text; generated names, the abstract review
artwork, no socket. In review scenes the app bar's Search opens this fixture
Search.

## Known limitations

- Loaded titles accumulate per query, as in Library. A very long scroll holds
  every page visited; this is bounded by how far the person scrolls, not by the
  result size.
- Episodes use their own Primary image (a still) in the poster frame, as
  shipping does; the series poster is not substituted.
- Movies and series carry no type label (see Result presentation).
- People and Collections are not searched (see Deferred result types).
- The Escape-to-Home rule is a product choice; there is no "previous root" memory.
- Escape in the results grid does nothing Search-specific (it reaches the
  shell's full-screen Escape); Up from the first row is the way back.
- The artwork sync glue in `search/screen.rs` follows the per-screen pattern
  Home, Details, and Library each have. The cache and the loader are shared;
  unifying the bookkeeping is a cross-screen change, not a Search one.
- No visual verification (see Verification).
