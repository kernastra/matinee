//! DTO to domain conversion.
//!
//! A missing id or name is rejected. Unknown item and stream types degrade
//! to `Other`. Missing artwork and missing optional metadata stay empty.

use std::time::Duration;

use matinee_core::{
    AudioStream, CalendarDate, Chapter, Credit, Delivery, DynamicRange, ImageTag, ItemArtwork,
    ItemHierarchy, ItemId, ItemIdentity, ItemKind, ItemMetadata, MediaItem, MediaSource,
    MediaSourceId, MediaStream, Person, SubtitleStream, TechnicalMedia, User, UserId,
    UserItemState, VideoStream,
};
use serde_json::Value;

use crate::dto::{
    ChapterDto, ItemDto, MediaSourceDto, MediaStreamDto, PersonDto, UserDataDto, UserDto,
};
use crate::error::JellyfinError;
use crate::ticks::{TickError, duration_from_json_number};

pub(crate) fn user_from_dto(dto: UserDto) -> Result<User, JellyfinError> {
    let id = UserId::parse(dto.id.trim()).map_err(|_| JellyfinError::malformed("user"))?;
    let name = required_text(&dto.name, "user")?;
    Ok(User::new(id, name, tag(dto.primary_image_tag)))
}

pub(crate) fn items_from_dtos(items: Vec<ItemDto>) -> Result<Vec<MediaItem>, JellyfinError> {
    items.into_iter().map(item_from_dto).collect()
}

pub(crate) fn item_from_dto(dto: ItemDto) -> Result<MediaItem, JellyfinError> {
    let id = ItemId::parse(dto.id.trim()).map_err(|_| JellyfinError::malformed("item identity"))?;
    let name = required_text(&dto.name, "item identity")?;
    let people = dto
        .people
        .unwrap_or_default()
        .into_iter()
        .filter_map(person_from_dto)
        .collect();
    let chapters = dto
        .chapters
        .unwrap_or_default()
        .into_iter()
        .enumerate()
        .map(|(index, chapter)| chapter_from_dto(index, chapter))
        .collect::<Result<Vec<_>, _>>()?;
    let sources = dto
        .media_sources
        .unwrap_or_default()
        .into_iter()
        .map(source_from_dto)
        .collect::<Result<Vec<_>, _>>()?;
    let streams = dto
        .media_streams
        .unwrap_or_default()
        .into_iter()
        .map(stream_from_dto)
        .collect::<Result<Vec<_>, _>>()?;
    let image_tags = dto.image_tags.unwrap_or_default();
    Ok(MediaItem {
        identity: ItemIdentity { id, name },
        kind: map_kind(dto.kind.as_deref()),
        metadata: ItemMetadata {
            overview: blank(dto.overview),
            year: dto
                .production_year
                .filter(|year| (1..10_000).contains(year)),
            runtime: optional_ticks(dto.run_time_ticks.as_ref())?
                .filter(|runtime| !runtime.is_zero()),
            community_rating: optional_rating(dto.community_rating, 10.0),
            critic_rating: optional_rating(dto.critic_rating, 100.0),
            official_rating: blank(dto.official_rating),
            genres: strings(dto.genres),
            taglines: strings(dto.taglines),
            studios: dto
                .studios
                .unwrap_or_default()
                .into_iter()
                .filter_map(|studio| blank(studio.name))
                .collect(),
            production_locations: strings(dto.production_locations),
            provider_ids: dto
                .provider_ids
                .unwrap_or_default()
                .into_iter()
                .filter(|(key, value)| !key.trim().is_empty() && !value.trim().is_empty())
                .collect(),
            premiere: calendar_day(dto.premiere_date),
            date_created: calendar_day(dto.date_created),
            end_date: calendar_day(dto.end_date),
        },
        artwork: ItemArtwork {
            primary: tag(image_tags.primary),
            logo: tag(image_tags.logo),
            backdrops: dto
                .backdrop_image_tags
                .unwrap_or_default()
                .into_iter()
                .filter_map(ImageTag::parse)
                .collect(),
        },
        user: user_state(dto.user_data)?,
        hierarchy: ItemHierarchy {
            parent_id: optional_item_id(dto.parent_id)?,
            series_id: optional_item_id(dto.series_id)?,
            series_name: blank(dto.series_name),
            season_id: optional_item_id(dto.season_id)?,
            season_name: blank(dto.season_name),
            index: optional_index(dto.index_number),
            parent_index: optional_index(dto.parent_index_number),
        },
        media: TechnicalMedia::new(sources, streams),
        people,
        chapters,
    })
}

fn person_from_dto(dto: PersonDto) -> Option<Person> {
    let name = blank(dto.name)?;
    let id = match blank(dto.id) {
        Some(value) => Some(ItemId::parse(value).ok()?),
        None => None,
    };
    Some(Person {
        id,
        name,
        role: blank(dto.role),
        credit: map_credit(dto.credit.as_deref()),
        image: tag(dto.primary_image_tag),
    })
}

fn chapter_from_dto(index: usize, dto: ChapterDto) -> Result<Chapter, JellyfinError> {
    let index = u32::try_from(index).map_err(|_| JellyfinError::malformed("chapter"))?;
    Ok(Chapter {
        index,
        name: blank(dto.name),
        start: optional_ticks(dto.start_position_ticks.as_ref())?.unwrap_or(Duration::ZERO),
        image: tag(dto.image_tag),
    })
}

fn source_from_dto(dto: MediaSourceDto) -> Result<MediaSource, JellyfinError> {
    let id = match dto.id {
        Some(value) if !value.trim().is_empty() => Some(
            MediaSourceId::parse(value.trim())
                .map_err(|_| JellyfinError::malformed("media source"))?,
        ),
        _ => None,
    };
    let streams = dto
        .media_streams
        .unwrap_or_default()
        .into_iter()
        .map(stream_from_dto)
        .collect::<Result<Vec<_>, _>>()?;
    Ok(MediaSource {
        id,
        name: blank(dto.name),
        path: blank(dto.path),
        container: blank(dto.container),
        bitrate: optional_u64(dto.bitrate),
        runtime: optional_ticks(dto.run_time_ticks.as_ref())?.filter(|runtime| !runtime.is_zero()),
        size_bytes: optional_u64(dto.size),
        delivery: Delivery {
            direct_play: dto.supports_direct_play,
            direct_stream: dto.supports_direct_stream,
            transcode: dto.supports_transcoding,
        },
        streams,
        default_audio_index: dto.default_audio_stream_index,
        default_subtitle_index: dto.default_subtitle_stream_index,
    })
}

pub(crate) fn stream_from_dto(dto: MediaStreamDto) -> Result<MediaStream, JellyfinError> {
    let index = dto
        .index
        .ok_or_else(|| JellyfinError::malformed("media stream"))?;
    let kind = dto.kind.unwrap_or_default();
    match kind.trim() {
        "Video" => Ok(MediaStream::Video(VideoStream {
            index,
            codec: blank(dto.codec),
            title: blank(dto.title),
            language: blank(dto.language),
            width: optional_dimension(dto.width),
            height: optional_dimension(dto.height),
            range: dynamic_range(dto.video_range.as_deref(), dto.video_range_type.as_deref()),
            bit_depth: optional_u32(dto.bit_depth),
            frame_rate: frame_rate(dto.average_frame_rate, dto.real_frame_rate),
            bitrate: optional_u64(dto.bit_rate),
            profile: blank(dto.profile),
            pixel_format: blank(dto.pixel_format),
            color_space: blank(dto.color_space),
            is_default: dto.is_default,
        })),
        "Audio" => Ok(MediaStream::Audio(AudioStream {
            index,
            codec: blank(dto.codec),
            title: blank(dto.title),
            display_title: blank(dto.display_title),
            language: blank(dto.language),
            channels: optional_u32(dto.channels),
            channel_layout: blank(dto.channel_layout),
            sample_rate: optional_u32(dto.sample_rate),
            bitrate: optional_u64(dto.bit_rate),
            is_default: dto.is_default,
        })),
        "Subtitle" => Ok(MediaStream::Subtitle(SubtitleStream {
            index,
            codec: blank(dto.codec),
            title: blank(dto.title),
            language: blank(dto.language),
            is_default: dto.is_default,
            forced: dto.is_forced,
            external: dto.is_external,
        })),
        other => Ok(MediaStream::Other {
            index,
            type_name: if other.is_empty() {
                "Unknown".to_string()
            } else {
                other.to_string()
            },
        }),
    }
}

fn user_state(dto: Option<UserDataDto>) -> Result<UserItemState, JellyfinError> {
    let Some(dto) = dto else {
        return Ok(UserItemState::default());
    };
    let percentage = optional_percentage(dto.played_percentage);
    Ok(UserItemState::from_parts(
        optional_ticks(dto.playback_position_ticks.as_ref())?,
        percentage,
        dto.is_favorite,
        dto.played,
        dto.play_count,
    ))
}

fn map_kind(value: Option<&str>) -> ItemKind {
    match value.unwrap_or("").trim() {
        "Movie" => ItemKind::Movie,
        "Series" => ItemKind::Series,
        "Season" => ItemKind::Season,
        "Episode" => ItemKind::Episode,
        "BoxSet" | "Collection" => ItemKind::Collection,
        "" => ItemKind::Other("Unknown".to_string()),
        other => ItemKind::Other(other.to_string()),
    }
}

fn map_credit(value: Option<&str>) -> Credit {
    match value.unwrap_or("").trim() {
        "Actor" => Credit::Actor,
        "Director" => Credit::Director,
        "Writer" => Credit::Writer,
        "Producer" => Credit::Producer,
        "" => Credit::Other("Unknown".to_string()),
        other => Credit::Other(other.to_string()),
    }
}

fn dynamic_range(range: Option<&str>, range_type: Option<&str>) -> DynamicRange {
    let label = blank(range_type.map(str::to_string)).or_else(|| blank(range.map(str::to_string)));
    let Some(label) = label else {
        return DynamicRange::Unknown;
    };
    let upper = label.to_ascii_uppercase();
    if upper == "SDR" {
        DynamicRange::Sdr
    } else if upper.contains("HDR")
        || upper.contains("DOVI")
        || upper.contains("HLG")
        || upper.contains("DOLBY")
    {
        DynamicRange::Hdr { label }
    } else {
        DynamicRange::Unknown
    }
}

fn optional_ticks(value: Option<&Value>) -> Result<Option<Duration>, JellyfinError> {
    let Some(value) = value else {
        return Ok(None);
    };
    if value.is_null() {
        return Ok(None);
    }
    let number = value
        .as_number()
        .ok_or_else(|| JellyfinError::malformed("ticks"))?;
    match duration_from_json_number(number) {
        Ok(duration) => Ok(Some(duration)),
        Err(TickError::Overflow) => Err(JellyfinError::malformed("ticks overflow")),
        Err(TickError::Malformed) => Err(JellyfinError::malformed("ticks")),
    }
}

fn optional_item_id(value: Option<String>) -> Result<Option<ItemId>, JellyfinError> {
    match blank(value) {
        Some(value) => ItemId::parse(value)
            .map(Some)
            .map_err(|_| JellyfinError::malformed("item id")),
        None => Ok(None),
    }
}

fn optional_index(value: Option<i32>) -> Option<u32> {
    u32::try_from(value?).ok()
}

fn optional_u32(value: Option<i32>) -> Option<u32> {
    u32::try_from(value?).ok().filter(|value| *value > 0)
}

fn optional_u64(value: Option<i64>) -> Option<u64> {
    u64::try_from(value?).ok().filter(|value| *value > 0)
}

fn frame_rate(average: Option<f64>, real: Option<f64>) -> Option<f64> {
    average
        .or(real)
        .filter(|value| value.is_finite() && *value > 0.0 && *value <= 480.0)
}

fn optional_rating(value: Option<f64>, max: f64) -> Option<f64> {
    value.filter(|value| value.is_finite() && *value >= 0.0 && *value <= max)
}

fn optional_percentage(value: Option<f64>) -> Option<f32> {
    let value = value.filter(|value| value.is_finite() && *value >= 0.0)?;
    Some(value.min(100.0) as f32)
}

fn optional_dimension(value: Option<i32>) -> Option<u32> {
    optional_u32(value).filter(|value| *value <= 16_384)
}

fn calendar_day(value: Option<String>) -> Option<CalendarDate> {
    value.as_deref().and_then(CalendarDate::parse)
}

fn strings(values: Option<Vec<String>>) -> Vec<String> {
    values
        .unwrap_or_default()
        .into_iter()
        .filter_map(nonempty)
        .collect()
}

fn nonempty(value: String) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn blank(value: Option<String>) -> Option<String> {
    value.and_then(nonempty)
}

fn tag(value: Option<String>) -> Option<ImageTag> {
    value.and_then(ImageTag::parse)
}

fn required_text(value: &str, context: &str) -> Result<String, JellyfinError> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        Err(JellyfinError::malformed(context))
    } else {
        Ok(trimmed.to_string())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse_item(json: &str) -> Result<MediaItem, JellyfinError> {
        let dto: ItemDto = serde_json::from_slice(json.as_bytes())
            .map_err(|_| JellyfinError::malformed("item"))?;
        item_from_dto(dto)
    }

    fn item(json: &str) -> MediaItem {
        parse_item(json).unwrap()
    }

    #[test]
    fn converts_a_movie_with_people_chapters_and_resume() {
        let movie = item(
            r#"{
                "Id": "movie-1",
                "Name": "Andromeda",
                "Type": "Movie",
                "Overview": "A quiet ship.",
                "ProductionYear": 2024,
                "RunTimeTicks": 54000000000,
                "CommunityRating": 8.2,
                "OfficialRating": "PG-13",
                "Genres": ["Science Fiction"],
                "ImageTags": {"Primary": "poster", "Logo": "logo"},
                "BackdropImageTags": ["back"],
                "Taglines": ["Further"],
                "Studios": [{"Name": "Northlight"}],
                "ProviderIds": {"Imdb": "tt1"},
                "PremiereDate": "2024-05-01",
                "People": [
                    {"Id": "person-1", "Name": "Ada", "Type": "Actor", "Role": "Captain", "PrimaryImageTag": "face"},
                    {"Name": "", "Type": "Director"}
                ],
                "Chapters": [{"Name": "Opening", "StartPositionTicks": 0, "ImageTag": "chapter"}],
                "UserData": {
                    "PlaybackPositionTicks": 90000000,
                    "PlayedPercentage": 12.5,
                    "IsFavorite": true,
                    "Played": false,
                    "PlayCount": 3
                },
                "ExtraField": true
            }"#,
        );
        assert_eq!(movie.kind, ItemKind::Movie);
        assert_eq!(movie.name(), "Andromeda");
        assert_eq!(movie.metadata.year, Some(2024));
        assert_eq!(movie.metadata.runtime, Some(Duration::from_secs(5400)));
        assert_eq!(movie.metadata.genres, vec!["Science Fiction".to_string()]);
        assert!(movie.artwork.has_primary());
        assert!(movie.artwork.has_logo());
        assert!(movie.artwork.has_backdrop());
        assert_eq!(movie.people.len(), 1);
        assert_eq!(movie.people[0].credit, Credit::Actor);
        assert!(movie.people[0].has_image());
        assert!(movie.chapters[0].has_image());
        assert!(movie.is_resumable());
        assert_eq!(movie.user.resume_position(), Some(Duration::from_secs(9)));
        assert!(movie.user.is_favorite());
        assert_eq!(movie.user.play_count(), 3);
        assert!(movie.user.viewing_progress().is_some());
        assert_eq!(
            movie.metadata.provider_ids.get("Imdb").map(String::as_str),
            Some("tt1")
        );
    }

    #[test]
    fn converts_series_season_episode_and_collection() {
        let series = item(r#"{"Id":"series-1","Name":"Signal","Type":"Series"}"#);
        assert_eq!(series.kind, ItemKind::Series);
        let season = item(
            r#"{"Id":"season-1","Name":"Season 1","Type":"Season","SeriesId":"series-1","IndexNumber":1}"#,
        );
        assert_eq!(season.kind, ItemKind::Season);
        assert_eq!(season.hierarchy.series_id.unwrap().as_str(), "series-1");
        let episode = item(
            r#"{"Id":"episode-1","Name":"Pilot","Type":"Episode","SeriesId":"series-1","SeriesName":"Signal","SeasonId":"season-1","SeasonName":"Season 1","IndexNumber":1,"ParentIndexNumber":1}"#,
        );
        assert_eq!(episode.kind, ItemKind::Episode);
        assert_eq!(episode.hierarchy.episode_label().as_deref(), Some("S1 E1"));
        let collection = item(r#"{"Id":"box-1","Name":"The Trilogy","Type":"BoxSet"}"#);
        assert_eq!(collection.kind, ItemKind::Collection);
    }

    #[test]
    fn missing_artwork_and_optional_metadata_are_empty() {
        let movie = item(r#"{"Id":"movie-1","Name":"Bare","Type":"Movie"}"#);
        assert!(!movie.artwork.has_primary());
        assert!(movie.metadata.overview.is_none());
        assert!(movie.metadata.runtime.is_none());
        assert!(!movie.is_resumable());
        assert!(movie.people.is_empty());
        assert!(movie.chapters.is_empty());
        assert!(movie.media.sources().is_empty());
    }

    #[test]
    fn converts_hdr_multichannel_audio_and_subtitles() {
        let movie = item(
            r#"{
                "Id":"movie-1","Name":"Bright","Type":"Movie",
                "MediaStreams":[
                    {"Index":0,"Type":"Video","Codec":"hevc","Width":3840,"Height":2160,"VideoRange":"HDR","VideoRangeType":"HDR10","BitDepth":10,"AverageFrameRate":23.976,"IsDefault":true},
                    {"Index":1,"Type":"Audio","Codec":"eac3","Language":"eng","Channels":6,"ChannelLayout":"5.1","SampleRate":48000,"IsDefault":true},
                    {"Index":2,"Type":"Subtitle","Codec":"subrip","Language":"eng","IsDefault":false,"IsForced":true,"IsExternal":true},
                    {"Index":3,"Type":"Data"}
                ]
            }"#,
        );
        let video = movie.media.primary_video().unwrap();
        assert!(video.range.is_hdr());
        assert_eq!(video.bit_depth, Some(10));
        assert_eq!(video.width, Some(3840));
        let audio = movie.media.primary_audio().unwrap();
        assert_eq!(audio.channels, Some(6));
        assert_eq!(audio.channel_layout.as_deref(), Some("5.1"));
        assert_eq!(audio.codec.as_deref(), Some("eac3"));
        let subtitle = movie.media.item_streams()[2].as_subtitle().unwrap();
        assert!(subtitle.forced && subtitle.external);
        assert_eq!(subtitle.language.as_deref(), Some("eng"));
        assert_eq!(movie.media.item_streams()[3].index(), 3);
    }

    #[test]
    fn unknown_item_and_played_state_degrade_safely() {
        let item = item(
            r#"{"Id":"trailer-1","Name":"Teaser","Type":"Trailer","UserData":{"Played":true,"PlayCount":1,"PlaybackPositionTicks":0}}"#,
        );
        assert_eq!(item.kind, ItemKind::Other("Trailer".into()));
        assert!(item.user.is_played());
        assert!(!item.is_resumable());
    }

    #[test]
    fn rejects_missing_identity_and_malformed_ticks() {
        assert!(parse_item(r#"{"Name":"No Id","Type":"Movie"}"#).is_err());
        assert!(parse_item(r#"{"Id":"movie-1","Type":"Movie"}"#).is_err());
        assert!(
            parse_item(r#"{"Id":"movie-1","Name":"Bad","Type":"Movie","RunTimeTicks":-5}"#)
                .is_err()
        );
        let overflow =
            r#"{"Id":"movie-1","Name":"Big","Type":"Movie","RunTimeTicks":18446744073709551615}"#;
        let error = parse_item(overflow).unwrap_err();
        assert_eq!(error.context(), Some("ticks overflow"));
    }

    #[test]
    fn weird_metadata_does_not_drop_the_title() {
        let mut dto: ItemDto = serde_json::from_str(
            r#"{
                "Id":"movie-1",
                "Name":"Still Here",
                "Type":"Movie",
                "PremiereDate":"not-a-date",
                "DateCreated":"2024-05-01T03:04:05Z",
                "EndDate":"2023-02-29",
                "CommunityRating":80,
                "CriticRating":-4,
                "UserData":{"PlayedPercentage":140},
                "MediaSources":[{
                    "Id":"source-1",
                    "MediaStreams":[{
                        "Index":0,
                        "Type":"Video",
                        "Width":0,
                        "Height":999999,
                        "AverageFrameRate":9000
                    }]
                }]
            }"#,
        )
        .unwrap();
        dto.community_rating = Some(f64::NAN);
        let movie = item_from_dto(dto).unwrap();
        assert_eq!(movie.name(), "Still Here");
        assert!(movie.metadata.premiere.is_none());
        assert!(movie.metadata.end_date.is_none());
        assert!(movie.metadata.community_rating.is_none());
        assert!(movie.metadata.critic_rating.is_none());
        assert_eq!(
            movie.metadata.date_created,
            CalendarDate::parse("2024-05-01")
        );
        assert!(movie.user.viewing_progress().is_some());
        let video = movie.media.primary_video().unwrap();
        assert!(video.width.is_none());
        assert!(video.height.is_none());
        assert!(video.frame_rate.is_none());
    }
}
