# Home

Phase 3D. Home is the authenticated root of the native app. Startup with a
saved session, or a successful sign-in, lands here. Details and the Player
open above it, and Back returns to the same Home.

```text
Launch → Login or restored session → Home → Details → Player
                                       ↑        ↑         │
                                       └────────┴── Back ─┘

MatineeRoot
  ├─ home: HomeScreen (root destination, kept alive under pages)
  │     └─ HomeModel (no GPUI, no HTTP)
  │          Request + Ticket ──► ServiceRuntime ── home::load ── JellyfinClient::home_shelf
  │          Response + Ticket ◄─┘
  │     └─ ArtworkLoader (shared with Details) ── fetch_artwork (session header)
  └─ pages: Navigation<Page>  (Details, Player)
```

## Shipping Home audited

The reference is `src/components/Home.tsx`, `HomeEditorial.tsx`,
`MediaRow.tsx`, `MediaCard.tsx`, `Artwork.tsx`, `App.tsx`, and
`getHomeFeed` in `src/lib/jellyfin.ts`.

- **Data.** One `getHomeFeed` call runs six requests in parallel: resume
  (12, `/Items/Resume`), latest (6, movies and series by date added),
  movies (12, newest), series (12, newest), top rated (12, by community
  rating), favorites (12). Top rated and favorites fall back to empty; any
  of the other four failing fails all of Home.
- **Hero.** Up to eight candidates from resume, latest, movies, and series
  (deduplicated, those with a backdrop preferred). It rotates every 7
  seconds when the Hero rotation setting is on and reduced motion is off;
  otherwise the first candidate stays. Eyebrow "Continue watching" when
  the item has a played percentage, else "Tonight’s feature". Title,
  `year · rating · runtime · genre`, overview, "Resume" or "Play now"
  (plays; a series plays its next-up episode or opens Details), and
  "More information" (Details). Backdrop at 1800 px.
- **Order below the hero.** Library shortcuts (Continue, Just added,
  Movies, Series, Top rated: two route to the Library screen, three scroll
  the page), Continue watching, Recently added, Coming soon (calendar
  integration), Top rated (ranked 1–10), a Featured showcase carousel (top
  rated and latest, arrows, animated), Movies and Series (each with "View
  all" to the Library and scroll arrows), My favorites, a Collection
  spotlight (a backdrop feature plus "Top movies" and "Top series"
  lists), and a footer of links.
- **Rows.** Horizontal, poster cards (Continue watching too), hidden when
  empty, "N titles" beside the heading. Item order is the server's.
- **Cards.** Poster at 360 px, a hover play glyph, `★ 7.8 · Genre` overlay,
  a progress bar from the played percentage. Selecting a card opens
  Details; cards never start playback.
- **Loading.** A centered "Curating your evening…" mark until the whole
  feed answers.
- **Errors.** The request's message centered with Sign out. No retry.
- **Empty library.** "Your Jellyfin library is connected, but it does not
  contain any movies or series yet." with Sign out.
- **Refresh.** Only when the session changes. Details and the Player
  replace Home in the tree, so returning remounts it: every request runs
  again and the page is back at the top.
- **Responsive.** CSS breakpoints shrink the hero and cards.

## Native hierarchy

Home answers three questions in order: what was I watching, what should I
watch next, and what is new.

1. **Hero.** Top bar (MATINEE wordmark, the signed-in name, Refresh, Sign
   out) over a restrained backdrop, about 62% of the window height
   (360–620 px), so the first row always starts on screen.
2. **Continue Watching.** Landscape cards (still or backdrop) with a quiet
   amber progress line, the series or title, and `S2 E5 · Keeper` for an
   episode or `38m left` for a movie.
3. **Next Up.** Landscape cards: series name, `S2 E5 · Episode title`.
4. **Recently Added Movies** and **Recently Added Series.** Poster cards
   with the title and year, a progress line when resumable, and a small
   check when watched.
5. **Favorites.** Poster cards.

Every row is a [`Rail`](ui-framework.md) with ‹ › arrows when it overflows.
The page `ScrollView` uses `restrict_to_axis`, so a sideways trackpad swipe
over a row moves only that row and a vertical wheel over a row moves only
the page.

## Parity

### Implemented

| Shipping behavior | Native |
|---|---|
| Continue watching from `/Items/Resume`, 12 | Same query and limit. Landscape cards (see changes). |
| Movies and Series rows (newest first, 12) | "Recently Added Movies" and "Recently Added Series", same queries. |
| My favorites, 12 | "Favorites", same query. |
| Hero from resume, then newest titles, preferring a backdrop | Same candidates and preference, chosen deterministically (see changes). |
| Hero eyebrow, metadata line, overview | "Continue watching" / "Tonight's feature" (plus "Up next · S2 E5"), `year · rating · runtime`, overview shortened at a word. |
| Hero Resume/Play and More information | Resume/Play opens the existing Player; Details opens Details. |
| Cards open Details, never play | Same. |
| Progress on cards | A quiet amber line, Jellyfin's percentage or position ÷ runtime. |
| Empty rows hidden | Same, except Continue Watching (see changes). |
| Empty-library copy | Same sentence. |
| Sign out from Home | In the top bar. |

### Deferred

- **Library shortcuts, "View all", footer links.** They lead to Library,
  Settings, and other screens that are not native yet (Phase 3E and
  later). Dead controls are not shown.
- **Top rated, Featured showcase, Collection spotlight.** All three are
  built from the top-rated query; they return together as an editorial
  pass once the core rows have settled. Leaving them out also keeps Home
  well inside the artwork budget (below).
- **Mixed "Recently added" (6).** It repeats the first items of the Movies
  and Series rows, which are themselves newest-first. Not requested.
- **Coming soon.** Needs the native Calendar and integration settings.
- **Hero rotation** and its setting (see changes); there is no native
  Settings screen yet.
- **Rating and genre overlay on cards**, custom Poster Studio artwork,
  and the hover play glyph.
- **Playing a series from the hero.** Shipping resolves next up first;
  native shows "View series" and lets Details pick the episode.

### Intentionally changed

- **Sections load independently.** A failure in one shelf leaves the rest
  of Home; shipping blanks the page when any of four requests fails.
- **Next Up is new.** Shipping Home has no next-episode row. Native uses
  Jellyfin's `/Shows/NextUp` across series (no `seriesId`), limit 12, with
  `enableResumable=false`, and leaves out anything Continue Watching
  already shows. This is the "what should I watch next" row. Checked
  against Jellyfin's source (`TvShowsController.GetNextUp`, which accepts
  `enableResumable`, default true, and `TVSeriesManager.DetermineNextEpisode`):
  with it false, a series whose next episode has a saved position is
  dropped from Next Up, and that same episode is what `/Items/Resume`
  returns for Continue Watching, so no valid next episode is lost between
  the rows. The one gap: with more than 12 titles in progress, an
  in-progress next episode past Continue Watching's 12 is on neither row.
  The per-series Next Up that Details uses keeps `enableResumable=true`.
  A server that ignored the parameter would still be covered by the
  Continue Watching dedupe.
- **Landscape cards for Continue Watching and Next Up.** What you are in
  the middle of reads differently from what is new, and Home is not one
  wall of identical posters.
- **No hero rotation.** Rotation is a permanent 7-second timer that
  repaints Home forever and changes what Resume does under the pointer.
  The native hero is chosen once by a fixed rule and pinned by item id
  (below).
- **Calm loading.** The page structure appears at once with a hero
  placeholder and card-shaped placeholders per row; rows fill in as their
  answers arrive. No spinner, no shimmer.
- **Continue Watching empty** shows "Nothing in progress. Anything you
  start will wait for you here." instead of disappearing, so the first
  row's absence never looks like a failure.
- **Errors** are fixed copy with Try again (whole page) or a quiet line
  with Try again (one row). Server text is never painted.
- **Refresh** keeps the page and row positions (below), and there is an
  explicit Refresh.

## Model

`HomeModel` (`apps/matinee-next/src/home/model.rs`) owns:

- one `ShelfState` per `HomeShelf`: `Loading`, `Ready(items)`, or
  `Failed(HomeFailure)`;
- the current `Ticket` for each shelf;
- the pinned hero (shelf and item id);
- the card last opened, for focus on return.

It derives `PageState` (`Content`, `Empty` when every shelf answered with
nothing, `Failed` when every shelf failed) and `HeroState` (`Loading`,
`Ready`, `None`). `HomeScreen` paints that; no networking or product
decision is made in render functions.

## Requests

`HomeShelf` lives in `matinee-core`; `JellyfinClient::home_shelf(shelf)`
in `matinee-jellyfin` maps it to the shipping query (Next Up is the one new
path). The client's `home_feed` aggregate stays for the shipping parity
tests. No DTO reaches the app.

`HomeModel::open` returns five `Request`s. `HomeScreen::run` spawns all of
them on the `ServiceRuntime` at once, one `JoinHandle` each, using the one
`JellyfinClient` the shell made for the session. Each answer comes back
with its ticket. Nothing waits for another shelf.

**Stale answers.** Every refresh issues new tickets. A shelf applies only
the ticket it is waiting for, so refresh A, refresh B, A arriving last is
ignored. A failed refresh keeps the rows already shown.

## Hero

Order: Continue Watching, Next Up, Recently Added Movies, Recently Added
Series. The hero is the first item with a backdrop (an episode counts its
series' backdrop) in the highest-priority shelf, decided only once every
shelf above it has answered, so the hero never switches as later rows
arrive. Without any backdrop, the first item of those shelves stands in.
Once chosen it is pinned by id: a refresh that still contains it keeps it
(with fresh progress); if it has left Home, the same rule picks again.

Buttons: Resume · 42:18 or Play (the existing Player's resume rule and
30-second tail, via `PlayAction`), and Details. A series shows "View
series" as the primary action. Play emits `HomeEvent::Play`; the shell
opens the Phase 3B Player. Home does not choose sources, plan, or report.

## Navigation

`MatineeRoot` keeps `home: Option<Entity<HomeScreen>>` as the root
destination and `Navigation<Page>` (Details, Player) above it. Home is
created once per sign-in, kept alive under pages, and dropped on sign-out
or session end, which aborts its requests and artwork fetches.

- Card → `HomeEvent::Open(id)` → Details (the Phase 3C screen). Related
  titles inside Details work as before.
- Hero Play → `HomeEvent::Play(id)` → Player directly above Home.
- Back from the last page shows Home again; the shell calls
  `HomeScreen::resume(refresh)`.

The temporary Phase 3C item-ID entry is gone. Nothing replaced it in
production; Home is the way into Details.

## Refresh and the Player round trip

Home loads after sign-in and after a restored session, and again when
Refresh is pressed. Opening the Player marks the root stale
(`Navigation::mark_root_stale`). When the stack is empty again,
`take_root_stale` says whether playback happened since Home was last
showing; only then does Home reload its shelves. Details → Back alone does
not refetch anything. There is no timer and no polling.

```text
Home → Continue Watching card → Details → Resume → Player (20 minutes)
     → Back → Details refreshes itself → Back → Home refreshes every shelf
```

The old rows stay on screen until the answers arrive. Jellyfin may not have
processed the final stop report yet (it is sent without waiting); its
progress reports during playback are at most 10 seconds apart, so the new
position is at most that far behind.

## Scroll and focus

Home owns its page `ScrollControl` and one `RailState` per row. Because
the entity stays alive while Details covers it, the vertical position and
every row's horizontal position are exactly where they were on return.
`view.rs` tests prove this on the headless GPUI platform with the real
objects: after Home → Details → Home and Home → Details → Player → Details
→ Home, the root still holds the same `HomeScreen` entity, the page and
every row keep the offsets set before, and the handles taken before are
still the live ones. (They assert the state objects, not painted pixels.)

Focus returns to the card that was opened, if it is still on Home after any
refresh; otherwise to the hero's first button. On first load the hero's
first button receives focus once the hero is ready. Focus never lands on an
invisible element: a focused card is scrolled into view in its row by the
rail and in the page by Home.

Keyboard: Tab walks the top bar, the hero buttons, then every card.
Left and Right move within a row, Home and End to its ends. Up and Down
move between the hero and rows, keeping the column the row last had. Enter
and Space activate. Escape belongs to the shell (fullscreen).

## Failures and session end

- One shelf fails: "Next Up isn't available right now." with Try again in
  its place. The hero and other rows stay.
- Every shelf fails: "Jellyfin isn't answering" (or "Home couldn't be
  loaded") with Try again. No server text, URL, or token.
- Malformed data fails only its shelf.
- Artwork missing or failed: the calm placeholder with the title.
- An authorization failure (401/403, or a rejected token) on any Home
  request is not a shelf failure. The shelf is left as it was, the page
  never shows an ordinary failure for it, and `HomeModel::apply` returns
  `Applied::SessionExpired`. Home emits `HomeEvent::SessionExpired` and the
  shell ends the session (see [application.md](application.md#session-end)).
  Five shelves can 401 together; the shell acts once. An unreachable server
  never signs anyone out.
- When shelves fail for different reasons, the whole-page failure is
  "Jellyfin isn't answering" if any shelf found the server unreachable,
  otherwise "Home couldn't be loaded", whatever order the answers came in.

## Artwork and memory

Home uses `ArtworkRequest`, `ArtworkLoader`, and `DecodedImage` from Phase
3C: tokenless tagged addresses, the session header, the 16 MiB body cap,
decoding off the UI thread, the shared LRU cache, atlas release, task
cancellation when a slot goes away, and the stale-slot check.

| Slot | Request | Decoded |
|---|---|---|
| Hero backdrop | Backdrop, 1920 (same address as Details' backdrop) | ≈ 7.9 MiB |
| Landscape card | Episode still, or movie backdrop, 480 (`THUMB_WIDTH`, as Details' episode rows) | ≈ 0.5 MiB |
| Poster card | Primary, 360 (`TILE_POSTER_WIDTH`) | ≈ 0.74 MiB |

Planning estimate: a representative worst case, every row full of distinct
titles (one hero backdrop, 24 landscape cards, 36 poster cards) at the
expected 16:9 and 2:3 aspects, is about 46.5 MiB, roughly half of the
96 MiB cache. It is computed in a test from the requests Home actually builds
(`artwork_plan`, so the widths are `BACKDROP_WIDTH`, `THUMB_WIDTH`,
`TILE_POSTER_WIDTH`, and the counts come from `HomeShelf::ALL` and
`SHELF_LIMIT`); changing any of those changes the estimate, and the test
keeps it under 55% of the budget. Real artwork keeps its own aspect, so this
is a planning number, not a ceiling. The ceiling is `ARTWORK_CACHE_BYTES`:
the cache never holds more.

The shared 96 MiB LRU substantially reuses artwork between Home and
Details. Identical addresses are cache hits: the hero's backdrop is the
same address as Details' backdrop for that title, an episode still is the
same as Details' episode rows, and Details' related tiles now use 360 px
(they are drawn 136 px wide), the same address as Home's poster cards and
half the memory. Older entries can still be evicted while a heavy Details
page (many cast portraits, collections, episodes) is open, and a return to
Home then fetches them again. What is guaranteed is bounded memory and
useful cross-screen reuse, not that every Home image survives. The cache
budget is unchanged. `MATINEE_ARTWORK_STATS=1` prints hits, misses,
entries, and decoded bytes when Home finishes loading.

**One request per address.** `artwork_plan` lists every address once, even
when the same title is in several shelves (Recently Added and Favorites)
or is also the hero, so one fetch and one decode serve every slot. The
screen also skips any address it already has in flight. The hero keeps its
own 1920 px backdrop; a card's 480 px backdrop of the same title is a
different address and is fetched separately, because the hero genuinely
needs the larger image. Two screens loading the same address at the same
moment (rare: Home is not drawn while Details is open) can each fetch it;
the cache then keeps one.

## Rows and virtualization

Rows hold at most 12 cards, five rows: at most 60 cards and a few hundred
elements. That is laid out every render without measurable cost, so rows
are not virtualized. The generic part is Atelier's `Rail` (a clipped,
horizontally scrolling row; Left/Right/Home/End focus movement; focused
items revealed; vertical wheel left to the page). Its Gallery story and
headless tests are in Atelier. Card presentation (`landscape_card`,
`poster_card`, shared `art_frame` and `progress_line` in `tiles.rs`) is
Matinee's.

## Layout

| Width | Gutter | Poster card | Landscape card | Hero copy |
|---|---|---|---|---|
| < 1100 (960×620) | 40 | 128 × 192 | 240 × 135 | 460, Title role |
| 1100–1599 (1200×760, 1440×900) | 64 | 148 × 222 | 288 × 162 | 560, Display for short titles |
| ≥ 1600 (fullscreen) | 64 | 176 × 264 | 336 × 189 | 640 |

The hero is 62% of the window height, 360–620 px. At 960×620 the hero is
384 px and the Continue Watching heading and cards start above the fold.

## Repaint

Home repaints when a shelf answers, an image arrives, focus or hover
changes, or a row scrolls. There is no timer, animation, or polling loop;
an idle Home does no work.

## Review scenes

`MATINEE_PREVIEW` accepts `home`, `home-continue-watching` (keyboard focus
on the first card), `home-empty`, `home-partial-error`, `home-loading`,
`home-small-window` (960×620), `home-large-window` (1920×1080),
`home-hero-resume`, and `home-hero-fresh`. They use fixture metadata and
the generated abstract artwork in `apps/matinee-next/assets/review/`, a
memory vault, and no socket.
