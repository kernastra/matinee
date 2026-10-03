# Domain model

`matinee-core` is Matinee's domain. It does not know Jellyfin JSON, HTTP,
GPUI, Atelier, libmpv, or Tauri. Time is `std::time::Duration`. Tick counts
never enter this crate.

```text
Jellyfin server → matinee-jellyfin → matinee-core
                                      ├─ future Matinee UI
                                      └─ matinee-player (via a future app adapter)
```

`matinee-player` stays independent of both crates. A future screen translates
a `PlaybackPlan` into a `LoadRequest`. The player does not import domain
types, and the domain does not import the player.

## What a title is

`MediaItem` is assembled from parts rather than one bag of optional fields:

| Part | Type | Holds |
|---|---|---|
| Identity | `ItemIdentity` | `ItemId`, display name |
| Kind | `ItemKind` | Movie, Series, Season, Episode, Collection, Other |
| Metadata | `ItemMetadata` | Overview, year, rating, runtime, genres, studios, official rating |
| Artwork | `ItemArtwork` | Primary, backdrop, and logo tags when Jellyfin supplied them |
| User state | `UserItemState` | Resume, played percentage, favorite, played, play count |
| Hierarchy | `ItemHierarchy` | Series, season, parent, collection membership, index numbers |
| Technical media | `TechnicalMedia` | Sources and streams |
| People | `Person` | Id when present, name, role, credit type, image tag |
| Chapters | `Chapter` | Name, start time, image tag |

`ItemKind::Other` keeps the server's label. An unknown type is a degraded
item, not a parse failure. Missing artwork is empty, not an error.

Ids (`ItemId`, `UserId`, `LibraryId`, `MediaSourceId`, `PlaySessionId`) are
newtypes over the server string. They reject empty values, whitespace, and
characters that would break a path or a query. They are not a second
identifier scheme.

## Progress

`UserItemState::from_parts` drops a zero resume position. A non-zero resume
stays available even when played percentage is missing, and a positive
percentage is progress even without a resume position. `viewing_progress()`
returns a `ViewingProgress` when either is present. Played and favorite are
independent flags.

## Libraries and the home feed

`LibraryKind` is Movies or Series. `LibrarySort` is one of four typed sorts:
name ascending, and date created, production year, and community rating
descending. Callers do not pass Jellyfin query strings.

`HomeFeed` has six shelves: resume, latest, movies, series, top rated, and
favorites. The feed type does not know which shelves are allowed to fail.
That policy lives in `matinee-jellyfin`.

## Technical media

`MediaSource` records container, bitrate, size, runtime, and whether the
server marked the source direct-play, direct-stream, or transcode capable.
Streams are a `MediaStream` enum: video, audio, subtitle, or other. Video
carries codec, size, bit depth, frame rate, and `DynamicRange` (`Sdr`,
`Hdr { label }`, or `Unknown`). Audio carries codec, language, channels, and
layout. Subtitles carry codec, language, and default, forced, and external
flags. Stream indexes are Jellyfin's indexes. They are not
`matinee-player::TrackId`.

## Playback plan

Premiere, date created, and end date are a `CalendarDate` (year, month,
day). A client accepts `YYYY-MM-DD` and ignores a time after `T` or a space.
A malformed day becomes absent. The UI does not parse a server timestamp.

`native_playback()` is the one declaration of native playback capabilities:
containers, video codecs, audio codecs, subtitle formats, and the transcode
target. A server client turns that into its own request document. The
playback engine implements those formats and does not import this crate.

`PlaybackPlan` is what a future player adapter consumes:

- stream URL, with no access token and no generic headers
- `StreamAuthorization`: `None`, or `Session` when the adapter must copy the
  session authorization header onto the load request
- `PlaySessionId` when the server sent one, otherwise absent
- `PlaybackMethod`: `DirectPlay`, `DirectStream`, or `Transcode`
- the source and its streams
- selected audio and subtitle indexes
- start position

`PlaybackReport` is one sample: item, media source, optional play session,
position, paused, mute, volume (0–1, with a 0–100 percent helper), selected
streams, and method. The domain does not schedule the next sample.
