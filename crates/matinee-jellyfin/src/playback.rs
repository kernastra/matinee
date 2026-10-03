//! Playback negotiation and progress reporting.
//!
//! The native device profile replaces the shipping WebKit profile. When that
//! profile still produces `NoCompatibleStream`, this client returns
//! [`JellyfinError::NoCompatibleSource`] instead of the shipping static
//! `/Videos/{id}/stream?Static=true` fallback. A static URL would ignore the
//! negotiation the native profile just performed, including a file the server
//! refused. The difference is intentional and tested.
//!
//! Direct play still builds an authenticated `/Videos/{id}/stream` URL when
//! the chosen source says direct play is supported. That URL is the negotiated
//! result for this client, not an assumption that every response looks like
//! that. A transcode uses the server's `TranscodingUrl`, resolved against the
//! server and rejected if it leaves the origin.
//!
//! Reporting is a single request. Nothing here schedules the next one.

use std::time::Duration;

use matinee_core::{
    MediaItem, MediaSourceId, PlaySessionId, PlaybackMethod, PlaybackOptions, PlaybackPlan,
    PlaybackReport, ReportKind,
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
        let source = choose_source(&sources).ok_or_else(|| {
            log::warn!(target: "matinee_jellyfin::playback", "compatibility failure: no media source");
            JellyfinError::playback_unavailable(
                "No playable media source was returned by Jellyfin.",
            )
        })?;
        let play_session_id = play_session_id(dto.play_session_id.as_deref())?;
        let streams = source
            .media_streams
            .clone()
            .unwrap_or_default()
            .into_iter()
            .map(stream_from_dto)
            .collect::<Result<Vec<_>, _>>()?;
        let selected_audio = options
            .audio_stream_index
            .or(source.default_audio_stream_index);
        let selected_subtitle = options
            .subtitle_stream_index
            .or(source.default_subtitle_stream_index);
        let source_id = match source.id.as_deref() {
            Some(value) if !value.trim().is_empty() => Some(
                MediaSourceId::parse(value.trim())
                    .map_err(|_| JellyfinError::malformed("media source"))?,
            ),
            _ => None,
        };
        if source.supports_direct_play {
            let url = direct_play_url(
                self.session().server_url(),
                item.id().as_str(),
                source_id.as_ref(),
                play_session_id.as_str(),
                self.session().access_token(),
            )?;
            log::info!(target: "matinee_jellyfin::playback", "negotiation DirectPlay");
            return Ok(PlaybackPlan {
                url,
                source_id,
                play_session_id,
                method: PlaybackMethod::DirectPlay,
                streams,
                selected_audio,
                selected_subtitle,
                start_position: start,
            });
        }
        let transcoding = source
            .transcoding_url
            .as_deref()
            .filter(|url| !url.trim().is_empty())
            .ok_or_else(|| {
                log::warn!(
                    target: "matinee_jellyfin::playback",
                    "compatibility failure: media source has no playable address"
                );
                JellyfinError::playback_unavailable(
                    "Jellyfin returned media information without a playable stream URL.",
                )
            })?;
        let url = resolve_transcoding_url(
            self.session().server_url(),
            transcoding,
            self.session().access_token(),
        )?;
        let method = if source.supports_direct_stream {
            PlaybackMethod::DirectStream
        } else {
            PlaybackMethod::Transcode
        };
        log::info!(
            target: "matinee_jellyfin::playback",
            "negotiation {}",
            method.as_str()
        );
        Ok(PlaybackPlan {
            url,
            source_id,
            play_session_id,
            method,
            streams,
            selected_audio,
            selected_subtitle,
            start_position: start,
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
            "PlaySessionId": report.play_session_id.as_str(),
            "PositionTicks": position,
            "IsPaused": report.paused,
            "IsMuted": report.muted,
            "VolumeLevel": report.volume_percent(),
            "CanSeek": true,
            "PlayMethod": report.method.as_str(),
        });
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

fn choose_source(sources: &[MediaSourceDto]) -> Option<&MediaSourceDto> {
    sources
        .iter()
        .find(|source| source.supports_direct_play)
        .or_else(|| {
            sources.iter().find(|source| {
                source
                    .transcoding_url
                    .as_ref()
                    .is_some_and(|url| !url.trim().is_empty())
            })
        })
        .or_else(|| sources.first())
}

fn play_session_id(value: Option<&str>) -> Result<PlaySessionId, JellyfinError> {
    if let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) {
        return PlaySessionId::parse(value).map_err(|_| JellyfinError::malformed("play session"));
    }
    PlaySessionId::parse(uuid::Uuid::new_v4().to_string())
        .map_err(|_| JellyfinError::malformed("play session"))
}

fn direct_play_url(
    server: &str,
    item_id: &str,
    source_id: Option<&MediaSourceId>,
    play_session_id: &str,
    token: &str,
) -> Result<String, JellyfinError> {
    let mut query = Query::new()
        .pair("Static", "true")
        .pair("DeviceId", DEVICE_ID)
        .pair("PlaySessionId", play_session_id)
        .pair("api_key", token);
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

fn resolve_transcoding_url(
    server: &str,
    transcoding: &str,
    token: &str,
) -> Result<String, JellyfinError> {
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
    let mut pairs: Vec<(String, String)> = resolved
        .query_pairs()
        .map(|(key, value)| (key.into_owned(), value.into_owned()))
        .collect();
    if !pairs.iter().any(|(key, _)| key == "api_key") {
        pairs.push(("api_key".to_string(), token.to_string()));
    }
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
