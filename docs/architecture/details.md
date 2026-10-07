# Details

Phase 3C. Details is the bridge between a Jellyfin item and the native
Player: item → Details → Play or Resume → Player → progress reports → Back
→ Details with the new position.

```text
MatineeRoot (Navigation<Page>)
  └─ DetailsScreen ── paints ── DetailsModel (no GPUI, no HTTP)
        │ Request + Ticket                ▲ Response + Ticket
        ▼                                 │
     ServiceRuntime ── details::load ── JellyfinClient (matinee-jellyfin)
        │
        └─ ArtworkLoader ── fetch_artwork (session header) ── DecodedImage (Atelier)
  └─ PlayerScreen (Phase 3B, unchanged entry: an ItemId)
```

## Shipping behavior audited

The reference is `src/components/Details.tsx`, `SeriesDetails.tsx`,
`DetailsSupplemental.tsx`, `DetailsModal.tsx`, and `App.tsx`.

- **Movies and episodes** share one Details: backdrop with a scrim, logo or
  title, tagline, `year · rating · runtime · two genres`, a thin progress
  bar, overview, director and writers, studio, Play or Resume, Trailer
  (disabled), Watchlist (favorite), Mark as Played, More (play from the
  beginning, clear progress, copy title, media info), audience and critic
  percentages, video and audio labels, six cast portraits, chapters with
  images that play from that point, collection rows, and twelve similar
  titles. Choosing a related title replaces the current one; Back returns
  to Home.
- **Series** have their own page: backdrop, poster, “Series” eyebrow,
  title, `year · rating · genres`, overview, a Play next or Resume button
  for the next-up episode, season tabs (next-up season first), and episode
  cards with thumbnail, progress line, `S01 E02`, title, and overview.
  Choosing an episode plays it.
- **Play** opens the Player over Details. Closing the Player remounts
  Details, which reloads the item, so the new progress shows.

## Parity in Phase 3C

| Shipping behavior | Native |
|---|---|
| Backdrop, scrim, title, tagline, metadata line, genres, overview | Yes. Typography carries the hierarchy; no badge wall. |
| Poster (series) | Poster for movies and series, still frame for episodes. |
| Director, writers | “Directed by”, “Written by”. |
| Audience and critic scores | Same wording: `78% audience   91% critics`. |
| Video and audio labels | `TechnicalMedia::summary()`: `4K · HDR10 · HEVC`, `Dolby Atmos · 7.1`. |
| Play / Resume | Resume uses the Player's own rule (30-second completion tail). Label `Resume · 42:18`. |
| Progress bar | A quiet amber line beside Resume, from Jellyfin's percentage or resume ÷ runtime. |
| Played | “Watched” with a check when played and not resumable. |
| Cast | Ten actors, circular portraits, informational. |
| Chapters | Listed with times, informational. |
| Collections, similar | Poster rows. Choosing one replaces the title in place, as shipping does. |
| Series next up | `Resume S2 E5 · 18:02` or `Play S2 E5`; no button when Jellyfin has no next up. |
| Seasons and episodes | Season picker (keyboard: arrows) with episode rows; next-up season first. |
| Episode play | Each row plays its episode. |
| Return from Player | Targeted refresh, no remount. |

## Deferred parity

- Watchlist (favorite), Mark as Played, Clear progress, Play from the
  beginning, Copy title, and the More and Media Info dialogs. The client
  calls exist (`set_item_favorite`, `set_item_played`, `clear_item_progress`).
- Logo artwork in place of the title. The title is typographic.
- Studio line.
- Chapter images, and playing from a chapter. The Player starts at the
  server's resume position; it has no start-position input yet.
- Trailer (disabled in shipping too).
- People navigation and a collection browser.
- Next-episode autoplay (a shipping setting) and Up Next.

## Domain and client additions

- `matinee-core`: `TechnicalMedia::summary()` and `TechnicalSummary`, so the
  screen never reads raw stream structures.
- `matinee-jellyfin`: `ArtworkUrls::item_request` (tagged, `None` when the
  item has no such image), `ArtworkUrls::person_request`, and
  `JellyfinClient::fetch_artwork` with `MAX_ARTWORK_BYTES`. The rest of
  Details uses calls that already existed: `item_details`, `similar_items`,
  `item_collections`, `series_seasons`, `next_up_episode`, `season_episodes`.
- `atelier-ui` (generic): `DecodedImage` and `Image::decoded` for bytes a
  caller fetched itself, and `Pressable`, a focusable container for rows and
  tiles. Both have Gallery stories.

Phase 3D: Details opens from Home cards and returns to the same Home. Its
related-title tiles (collections, more like this) now request 360 px
posters (`TILE_POSTER_WIDTH`) instead of 480; they are drawn 136 px wide,
and the smaller address is shared with Home's poster cards. The hero poster
stays at 480. `art_frame`, `progress_line`, and `backdrop_request` moved to
`tiles.rs` so Home uses the same pieces.

## State

`DetailsModel` owns everything Details shows:

- `Hero`: `Loading`, `Ready(MediaItem)`, or `Failed(DetailsFailure)`.
- Secondary `Section`s: similar, collections, seasons, episodes. Each is
  `Loading`, `Ready`, or `Unavailable`, independently.
- The selected season and the next-up episode.

The model issues a `Request` with a `Ticket`; the screen runs it on the
service runtime and hands the `Response` back with that ticket. A section
applies only the ticket it is waiting for. That one rule covers a late
answer for a previous title (a related title replaced it), a previous
season (another season was chosen), and a previous refresh.

Order: the item first. A series then asks for seasons and next up together
and opens the next-up season (or the first). Anything else asks for similar
titles and collections together.

## Player round trip

Play emits `DetailsEvent::Play(ItemId)`; the shell opens the Phase 3B
Player with that id. Details does not plan sources, resume, or report.
When the Player leaves, the shell finishes it (final stop report) and calls
`DetailsScreen::resume`, which runs `DetailsModel::refresh`: the item again
and, for a series, next up and the visible season. The previous values stay
on screen until the answers arrive, so there is no flash. A failed refresh
keeps them. Focus returns to Play.

The refresh can arrive before Jellyfin has processed the final stop report
(it is sent without waiting). Jellyfin's progress reports during playback,
at most 10 seconds apart, mean the refreshed position is at most that far
behind in that case.

## Failures

- The title: not found, signed out, Jellyfin unreachable, or unreadable.
  Each has fixed copy, Try again, and Back. No server text, URL, or token
  is painted.
- A secondary section that fails says so quietly in its place (“More like
  this isn't available right now.”, “Episodes couldn't be loaded.” with Try
  again). The hero stays.
- Artwork that is missing or fails shows a calm placeholder with the title.

## Layout

The hero fills about 80% of the window height (460–820 px), copy at the
bottom left over a backdrop with a left-to-right and a top-to-bottom scrim.
Below it: episodes (series), cast, chapters, collections, more like this.
Under 1100 px wide the gutter, poster, still, and episode thumbnails get
smaller and the title uses the Title role; from 1600 px the poster and copy
column grow. Long titles always use the Title role.

## Keyboard and focus

Opening Details focuses Play (or Try again) once the title is ready. Tab
moves through Back, Play, the season picker, episode rows, and poster tiles.
Arrows move between seasons. Enter or Space activates the focused row or
tile. Escape is Back. Returning from the Player focuses Play again.

## Review scenes

`MATINEE_PREVIEW` accepts `details-movie`, `details-movie-resume`,
`details-series`, `details-season`, `details-loading`, and `details-error`
(and `MATINEE_PREVIEW_SIZE=WxH`). They use fixture metadata and generated
abstract artwork in `apps/matinee-next/assets/review/`; they open no socket.
