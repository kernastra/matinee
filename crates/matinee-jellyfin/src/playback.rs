//! Playback negotiation and progress reporting.
//!
//! The native device profile replaces the shipping WebKit profile. When that
//! profile still produces `NoCompatibleStream`, this client returns
//! [`JellyfinError::NoCompatibleSource`] instead of the shipping static
//! `/Videos/{id}/stream?Static=true` fallback. A static URL would ignore the
//! negotiation the native profile just performed, including a file the server
//! refused. The difference is intentional and tested.
//!
//! Direct play builds `/Videos/{id}/stream` when the chosen source says direct
//! play is supported. A direct stream is a remux: the server returned a
//! transcoding address whose reasons are only about the container, or it set
//! direct stream without transcoding. A re-encode is `Transcode` even when
//! the same source also says direct stream is possible. The stream URL does
//! not carry the access token. The plan asks the future adapter to attach
//! the session authorization header instead.
//!
//! Reporting is a single request. Nothing here schedules the next one.

use std::time::Duration;

use matinee_core::{
    MediaItem, MediaSourceId, PlaySessionId, PlaybackMethod, PlaybackOptions, PlaybackPlan,
    PlaybackReport, ReportKind, StreamAuthorization,
};
use serde_json::json;
use url::Url;

use crate::DEVICE_ID;
use crate::client::{Endpoint, JellyfinClient};
use crate::convert::stream_from_dto;
use crate::dto::{MediaSourceDto, PlaybackDto};
use crate::error::JellyfinError;
use crate::profile::native_device_profile;
use crate::query::{Query, encode_component};
use crate::ticks::ticks_from_duration;
use crate::transport::{CancelFlag, Transport};
use crate::url::{join_server, same_origin};

const UNUSABLE_STREAM: &str = "Jellyfin returned an unusable stream address.";

impl<T: Transport> JellyfinClient<T> {
    pub async fn playback_plan(
        &self,
        item: &MediaItem,
        options: PlaybackOptions,
        cancel: Option<&CancelFlag>,
    ) -> Result<PlaybackPlan, JellyfinError> {
        let start = options
            .start_position
            .or_else(|| item.user.resume_position())
            .unwrap_or(Duration::ZERO);
        let start_ticks =
            ticks_from_duration(start).map_err(|_| JellyfinError::malformed("start position"))?;
        let mut body = json!({
            "UserId": self.session().user().id().as_str(),
            "StartTimeTicks": start_ticks,
            "IsPlayback": true,
            "AutoOpenLiveStream": true,
            "EnableDirectPlay": true,
            "EnableDirectStream": true,
            "EnableTranscoding": true,
            "AllowVideoStreamCopy": true,
            "AllowAudioStreamCopy": true,
            "DeviceProfile": native_device_profile(),
        });
        if let Some(bitrate) = options.max_bitrate {
            body["MaxStreamingBitrate"] = json!(bitrate);
        }
        if let Some(audio) = options.audio_stream_index {
            body["AudioStreamIndex"] = json!(audio);
        }
        if let Some(subtitle) = options.subtitle_stream_index {
            body["SubtitleStreamIndex"] = json!(subtitle);
        }
        let path = format!(
            "/Items/{}/PlaybackInfo",
            encode_component(item.id().as_str())
        );
        let response = self
            .post_json(Endpoint::Playback, &path, &body, cancel)
            .await?;
        let dto: PlaybackDto = serde_json::from_slice(&response.body)
            .map_err(|_| JellyfinError::malformed("playback"))?;
        if let Some(code) = dto.error_code.as_deref() {
            return Err(playback_error(code));
        }
        let sources = dto.media_sources.unwrap_or_default();
        let chosen = choose_source(&sources).ok_or_else(|| {
            log::warn!(target: "matinee_jellyfin::playback", "compatibility failure: no media source");
            JellyfinError::playback_unavailable(
                "No playable media source was returned by Jellyfin.",
            )
        })?;
        let play_session_id = optional_play_session(dto.play_session_id.as_deref())?;
        let streams = chosen
            .source
            .media_streams
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(stream_from_dto)
            .collect::<Result<Vec<_>, _>>()?;
        let selected_audio = options
            .audio_stream_index
            .or(chosen.source.default_audio_stream_index);
        let selected_subtitle = options
            .subtitle_stream_index
            .or(chosen.source.default_subtitle_stream_index);
        let source_id = match chosen.source.id.as_deref() {
            Some(value) if !value.trim().is_empty() => Some(
                MediaSourceId::parse(value.trim())
                    .map_err(|_| JellyfinError::malformed("media source"))?,
            ),
            _ => None,
        };
        let url = match chosen.method {
            PlaybackMethod::DirectPlay => direct_play_url(
                self.session().server_url(),
                item.id().as_str(),
                source_id.as_ref(),
                play_session_id.as_ref().map(PlaySessionId::as_str),
            )?,
            PlaybackMethod::DirectStream | PlaybackMethod::Transcode => {
                let transcoding = stream_address(chosen.source).ok_or_else(|| {
                    log::warn!(
                        target: "matinee_jellyfin::playback",
                        "compatibility failure: media source has no playable address"
                    );
                    JellyfinError::playback_unavailable(
                        "Jellyfin returned media information without a playable stream URL.",
                    )
                })?;
                resolve_transcoding_url(self.session().server_url(), transcoding)?
            }
        };
        log::info!(
            target: "matinee_jellyfin::playback",
            "negotiation {}",
            chosen.method.as_str()
        );
        Ok(PlaybackPlan {
            url,
            source_id,
            play_session_id,
            method: chosen.method,
            streams,
            selected_audio,
            selected_subtitle,
            start_position: start,
            authorization: StreamAuthorization::Session,
        })
    }

    pub async fn report_playback(
        &self,
        kind: ReportKind,
        report: &PlaybackReport,
    ) -> Result<(), JellyfinError> {
        let path = match kind {
            ReportKind::Start => "/Sessions/Playing",
            ReportKind::Progress => "/Sessions/Playing/Progress",
            ReportKind::Stopped => "/Sessions/Playing/Stopped",
        };
        let position = ticks_from_duration(report.position)
            .map_err(|_| JellyfinError::malformed("position"))?;
        let mut body = json!({
            "ItemId": report.item_id.as_str(),
            "PositionTicks": position,
            "IsPaused": report.paused,
            "IsMuted": report.muted,
            "VolumeLevel": report.volume_percent(),
            "CanSeek": true,
            "PlayMethod": report.method.as_str(),
        });
        if let Some(id) = &report.play_session_id {
            body["PlaySessionId"] = json!(id.as_str());
        }
        if let Some(source) = &report.media_source_id {
            body["MediaSourceId"] = json!(source.as_str());
        }
        if let Some(audio) = report.audio_stream_index {
            body["AudioStreamIndex"] = json!(audio);
        }
        if let Some(subtitle) = report.subtitle_stream_index {
            body["SubtitleStreamIndex"] = json!(subtitle);
        }
        self.post_json(Endpoint::Playback, path, &body, None)
            .await?;
        Ok(())
    }
}

fn playback_error(code: &str) -> JellyfinError {
    log::warn!(
        target: "matinee_jellyfin::playback",
        "compatibility failure: {code}"
    );
    match code {
        "NoCompatibleStream" => JellyfinError::NoCompatibleSource,
        "NotAllowed" => JellyfinError::playback_unavailable(
            "Your Jellyfin account is not allowed to play this title.",
        ),
        "RateLimitExceeded" => JellyfinError::playback_unavailable(
            "Jellyfin is busy preparing other streams. Try again shortly.",
        ),
        other => JellyfinError::playback_unavailable(format!(
            "Jellyfin could not start playback ({other})."
        )),
    }
}

struct Chosen<'a> {
    source: &'a MediaSourceDto,
    method: PlaybackMethod,
}

/// First playable direct play, then direct stream, then transcode.
fn choose_source(sources: &[MediaSourceDto]) -> Option<Chosen<'_>> {
    if let Some(source) = sources.iter().find(|source| source.supports_direct_play) {
        return Some(Chosen {
            source,
            method: PlaybackMethod::DirectPlay,
        });
    }
    if let Some(source) = sources.iter().find(|source| {
        stream_address(source)
            .is_some_and(|url| delivery_method(source, url) == PlaybackMethod::DirectStream)
    }) {
        return Some(Chosen {
            source,
            method: PlaybackMethod::DirectStream,
        });
    }
    sources.iter().find_map(|source| {
        let url = stream_address(source)?;
        (delivery_method(source, url) == PlaybackMethod::Transcode).then_some(Chosen {
            source,
            method: PlaybackMethod::Transcode,
        })
    })
}

fn stream_address(source: &MediaSourceDto) -> Option<&str> {
    source
        .transcoding_url
        .as_deref()
        .map(str::trim)
        .filter(|url| !url.is_empty())
}

/// What the address actually delivers.
///
/// `SupportsDirectStream` is not enough. The server sets that flag for direct
/// play as well, and a source can claim both direct stream and transcoding
/// while the address re-encodes. Container-only reasons, or copied codecs,
/// are a remux. Any other reason is a transcode.
fn delivery_method(source: &MediaSourceDto, url: &str) -> PlaybackMethod {
    if let Some(reasons) = transcode_reasons(source, url) {
        return if remux_only(&reasons) {
            PlaybackMethod::DirectStream
        } else {
            PlaybackMethod::Transcode
        };
    }
    if codecs_are_copy(url) {
        return PlaybackMethod::DirectStream;
    }
    if source.supports_direct_stream && !source.supports_transcoding {
        PlaybackMethod::DirectStream
    } else {
        PlaybackMethod::Transcode
    }
}

fn transcode_reasons(source: &MediaSourceDto, url: &str) -> Option<String> {
    query_value(url, "TranscodeReasons").or_else(|| match source.transcode_reasons.as_ref() {
        Some(serde_json::Value::String(value)) => Some(value.clone()),
        Some(serde_json::Value::Number(value)) => value.as_u64().map(|bits| bits.to_string()),
        _ => None,
    })
}

fn remux_only(reasons: &str) -> bool {
    let trimmed = reasons.trim();
    if trimmed.is_empty() {
        return false;
    }
    if let Ok(bits) = trimmed.parse::<u64>() {
        // ContainerNotSupported = 1, ContainerBitrateExceedsLimit = 8.
        return bits != 0 && bits & !9 == 0;
    }
    let mut any = false;
    for part in trimmed.split(|character: char| character == ',' || character.is_whitespace()) {
        if part.is_empty() {
            continue;
        }
        any = true;
        if !matches!(
            part,
            "ContainerNotSupported" | "ContainerBitrateExceedsLimit"
        ) {
            return false;
        }
    }
    any
}

fn codecs_are_copy(url: &str) -> bool {
    let Some(video) = query_value(url, "VideoCodec") else {
        return false;
    };
    if !video.eq_ignore_ascii_case("copy") {
        return false;
    }
    query_value(url, "AudioCodec").is_none_or(|audio| audio.eq_ignore_ascii_case("copy"))
}

fn query_value(url: &str, key: &str) -> Option<String> {
    let parsed = Url::parse(url).ok().or_else(|| {
        Url::parse("http://placeholder.invalid/")
            .ok()?
            .join(url)
            .ok()
    })?;
    parsed
        .query_pairs()
        .find(|(name, _)| name == key)
        .map(|(_, value)| value.into_owned())
}

fn optional_play_session(value: Option<&str>) -> Result<Option<PlaySessionId>, JellyfinError> {
    match value.map(str::trim).filter(|value| !value.is_empty()) {
        Some(value) => PlaySessionId::parse(value)
            .map(Some)
            .map_err(|_| JellyfinError::malformed("play session")),
        None => Ok(None),
    }
}

fn direct_play_url(
    server: &str,
    item_id: &str,
    source_id: Option<&MediaSourceId>,
    play_session_id: Option<&str>,
) -> Result<String, JellyfinError> {
    let mut query = Query::new()
        .pair("Static", "true")
        .pair("DeviceId", DEVICE_ID);
    if let Some(play_session_id) = play_session_id {
        query = query.pair("PlaySessionId", play_session_id);
    }
    if let Some(source_id) = source_id {
        query = query.pair("MediaSourceId", source_id.as_str());
    }
    let path = format!(
        "/Videos/{}/stream?{}",
        encode_component(item_id),
        query.encode()
    );
    join_server(server, &path)
}

fn resolve_transcoding_url(server: &str, transcoding: &str) -> Result<String, JellyfinError> {
    if transcoding.chars().any(char::is_control) {
        log::warn!(
            target: "matinee_jellyfin::playback",
            "compatibility failure: transcoding url contained control characters"
        );
        return Err(JellyfinError::playback_unavailable(UNUSABLE_STREAM));
    }
    let base = Url::parse(&format!("{server}/"))
        .map_err(|_| JellyfinError::playback_unavailable(UNUSABLE_STREAM))?;
    let mut resolved = base.join(transcoding).map_err(|_| {
        log::warn!(
            target: "matinee_jellyfin::playback",
            "compatibility failure: transcoding url did not resolve"
        );
        JellyfinError::playback_unavailable(UNUSABLE_STREAM)
    })?;
    if !resolved.username().is_empty() || resolved.password().is_some() {
        log::warn!(
            target: "matinee_jellyfin::playback",
            "compatibility failure: transcoding url contained credentials"
        );
        return Err(JellyfinError::playback_unavailable(UNUSABLE_STREAM));
    }
    if !same_origin(&base, &resolved) {
        log::warn!(
            target: "matinee_jellyfin::playback",
            "compatibility failure: transcoding url left the server origin"
        );
        return Err(JellyfinError::playback_unavailable(
            "Jellyfin returned a stream address outside the server.",
        ));
    }
    let pairs: Vec<(String, String)> = resolved
        .query_pairs()
        .filter(|(key, _)| !is_credential_query(key))
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    let mut encoded = String::new();
    for (index, (key, value)) in pairs.iter().enumerate() {
        if index > 0 {
            encoded.push('&');
        }
        encoded.push_str(&encode_component(key));
        encoded.push('=');
        encoded.push_str(&encode_component(value));
    }
    resolved.set_query(None);
    let mut address = resolved.to_string();
    if !encoded.is_empty() {
        address.push('?');
        address.push_str(&encoded);
    }
    Ok(address)
}

fn is_credential_query(key: &str) -> bool {
    matches!(
        key.to_ascii_lowercase().as_str(),
        "api_key" | "apikey" | "accesstoken" | "token"
    )
}
