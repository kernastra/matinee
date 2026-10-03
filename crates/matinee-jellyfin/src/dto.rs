//! Private Jellyfin JSON shapes.
//!
//! Unknown fields are ignored. Identity fields that must exist are required.
//! Optional metadata stays optional, including null.

use std::collections::BTreeMap;

use serde::Deserialize;
use serde_json::Value;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct AuthDto {
    pub(crate) access_token: String,
    pub(crate) user: UserDto,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct UserDto {
    pub(crate) id: String,
    pub(crate) name: String,
    pub(crate) primary_image_tag: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ServerInfoDto {
    pub(crate) server_name: Option<String>,
    pub(crate) version: Option<String>,
    pub(crate) operating_system: Option<String>,
    pub(crate) product_name: Option<String>,
    pub(crate) id: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ItemsDto {
    pub(crate) items: Option<Vec<ItemDto>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ItemDto {
    pub(crate) id: String,
    pub(crate) name: String,
    #[serde(rename = "Type")]
    pub(crate) kind: Option<String>,
    pub(crate) overview: Option<String>,
    pub(crate) production_year: Option<i32>,
    pub(crate) run_time_ticks: Option<Value>,
    pub(crate) community_rating: Option<f64>,
    pub(crate) official_rating: Option<String>,
    pub(crate) critic_rating: Option<f64>,
    #[serde(default)]
    pub(crate) genres: Option<Vec<String>>,
    pub(crate) image_tags: Option<ImageTagsDto>,
    #[serde(default)]
    pub(crate) backdrop_image_tags: Option<Vec<String>>,
    pub(crate) parent_id: Option<String>,
    pub(crate) series_id: Option<String>,
    pub(crate) series_name: Option<String>,
    pub(crate) season_id: Option<String>,
    pub(crate) season_name: Option<String>,
    pub(crate) index_number: Option<i32>,
    pub(crate) parent_index_number: Option<i32>,
    pub(crate) premiere_date: Option<String>,
    pub(crate) date_created: Option<String>,
    pub(crate) end_date: Option<String>,
    #[serde(default)]
    pub(crate) taglines: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) studios: Option<Vec<NameDto>>,
    #[serde(default)]
    pub(crate) people: Option<Vec<PersonDto>>,
    #[serde(default)]
    pub(crate) production_locations: Option<Vec<String>>,
    #[serde(default)]
    pub(crate) provider_ids: Option<BTreeMap<String, String>>,
    #[serde(default)]
    pub(crate) chapters: Option<Vec<ChapterDto>>,
    #[serde(default)]
    pub(crate) media_sources: Option<Vec<MediaSourceDto>>,
    #[serde(default)]
    pub(crate) media_streams: Option<Vec<MediaStreamDto>>,
    pub(crate) user_data: Option<UserDataDto>,
}

#[derive(Debug, Deserialize, Default)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ImageTagsDto {
    pub(crate) primary: Option<String>,
    pub(crate) logo: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct NameDto {
    pub(crate) name: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct PersonDto {
    pub(crate) id: Option<String>,
    pub(crate) name: Option<String>,
    #[serde(rename = "Type")]
    pub(crate) credit: Option<String>,
    pub(crate) role: Option<String>,
    pub(crate) primary_image_tag: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct ChapterDto {
    pub(crate) name: Option<String>,
    pub(crate) start_position_ticks: Option<Value>,
    pub(crate) image_tag: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct UserDataDto {
    pub(crate) playback_position_ticks: Option<Value>,
    pub(crate) played_percentage: Option<f64>,
    #[serde(default)]
    pub(crate) is_favorite: bool,
    #[serde(default)]
    pub(crate) played: bool,
    #[serde(default)]
    pub(crate) play_count: u32,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct MediaSourceDto {
    pub(crate) id: Option<String>,
    pub(crate) name: Option<String>,
    pub(crate) path: Option<String>,
    pub(crate) container: Option<String>,
    pub(crate) size: Option<i64>,
    pub(crate) bitrate: Option<i64>,
    pub(crate) run_time_ticks: Option<Value>,
    #[serde(default)]
    pub(crate) supports_direct_play: bool,
    #[serde(default)]
    pub(crate) supports_direct_stream: bool,
    #[serde(default)]
    pub(crate) supports_transcoding: bool,
    pub(crate) transcoding_url: Option<String>,
    pub(crate) transcode_reasons: Option<serde_json::Value>,
    #[serde(default)]
    pub(crate) media_streams: Option<Vec<MediaStreamDto>>,
    pub(crate) default_audio_stream_index: Option<i32>,
    pub(crate) default_subtitle_stream_index: Option<i32>,
}

#[derive(Clone, Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct MediaStreamDto {
    pub(crate) index: Option<i32>,
    #[serde(rename = "Type")]
    pub(crate) kind: Option<String>,
    pub(crate) codec: Option<String>,
    pub(crate) width: Option<i32>,
    pub(crate) height: Option<i32>,
    pub(crate) video_range: Option<String>,
    pub(crate) video_range_type: Option<String>,
    pub(crate) language: Option<String>,
    pub(crate) title: Option<String>,
    pub(crate) display_title: Option<String>,
    pub(crate) channels: Option<i32>,
    pub(crate) channel_layout: Option<String>,
    pub(crate) bit_rate: Option<i64>,
    pub(crate) bit_depth: Option<i32>,
    pub(crate) sample_rate: Option<i32>,
    pub(crate) profile: Option<String>,
    pub(crate) pixel_format: Option<String>,
    pub(crate) average_frame_rate: Option<f64>,
    pub(crate) real_frame_rate: Option<f64>,
    pub(crate) color_space: Option<String>,
    #[serde(default)]
    pub(crate) is_default: bool,
    #[serde(default)]
    pub(crate) is_forced: bool,
    #[serde(default)]
    pub(crate) is_external: bool,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
pub(crate) struct PlaybackDto {
    #[serde(default)]
    pub(crate) media_sources: Option<Vec<MediaSourceDto>>,
    pub(crate) play_session_id: Option<String>,
    pub(crate) error_code: Option<String>,
}
