//! A playback plan and a progress report.
//!
//! The plan is the address and the choices a player should load. It is not
//! a player load request. Reporting is a value the app sends; this crate
//! does not schedule it.

use std::time::Duration;

use crate::id::{ItemId, MediaSourceId, PlaySessionId};
use crate::media::MediaStream;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PlaybackMethod {
    DirectPlay,
    DirectStream,
    Transcode,
}

impl PlaybackMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::DirectPlay => "DirectPlay",
            Self::DirectStream => "DirectStream",
            Self::Transcode => "Transcode",
        }
    }
}

/// What the app asked the server to negotiate.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct PlaybackOptions {
    pub max_bitrate: Option<u64>,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub start_position: Option<Duration>,
}

/// Whether the player must attach session authorization when it loads `url`.
///
/// The secret stays in the session. This plan does not carry a token or a
/// header value.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StreamAuthorization {
    /// The URL can be loaded as-is.
    None,
    /// The app copies the session authorization header onto the load request.
    Session,
}

/// A playable result. `url` is ready for a player that can load an HTTP URL.
///
/// `play_session_id` is the server-issued id when the server sent one. It is
/// absent when the server omitted it. This crate does not invent one.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackPlan {
    pub url: String,
    pub source_id: Option<MediaSourceId>,
    pub play_session_id: Option<PlaySessionId>,
    pub method: PlaybackMethod,
    pub streams: Vec<MediaStream>,
    pub selected_audio: Option<i32>,
    pub selected_subtitle: Option<i32>,
    pub start_position: Duration,
    pub authorization: StreamAuthorization,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReportKind {
    Start,
    Progress,
    Stopped,
}

/// One playback report. Volume is linear from 0 to 1, matching the player.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackReport {
    pub item_id: ItemId,
    pub media_source_id: Option<MediaSourceId>,
    pub play_session_id: Option<PlaySessionId>,
    pub position: Duration,
    pub paused: bool,
    pub muted: bool,
    /// Linear gain from 0 to 1. Values outside that range are clamped when sent.
    pub volume: f32,
    pub audio_stream_index: Option<i32>,
    pub subtitle_stream_index: Option<i32>,
    pub method: PlaybackMethod,
}

impl PlaybackReport {
    /// Servers that want a volume percentage use 0–100.
    pub fn volume_percent(&self) -> u32 {
        if !self.volume.is_finite() {
            return 0;
        }
        let scaled = (self.volume.clamp(0.0, 1.0) * 100.0).round();
        if scaled <= 0.0 {
            0
        } else if scaled >= 100.0 {
            100
        } else {
            scaled as u32
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn volume_percent_clamps() {
        let item = ItemId::parse("item").unwrap();
        let session = PlaySessionId::parse("play").unwrap();
        let mut report = PlaybackReport {
            item_id: item,
            media_source_id: None,
            play_session_id: Some(session),
            position: Duration::from_secs(3),
            paused: false,
            muted: true,
            volume: 0.5,
            audio_stream_index: Some(1),
            subtitle_stream_index: Some(-1),
            method: PlaybackMethod::DirectPlay,
        };
        assert_eq!(report.volume_percent(), 50);
        report.volume = 4.0;
        assert_eq!(report.volume_percent(), 100);
        report.volume = f32::NAN;
        assert_eq!(report.volume_percent(), 0);
    }
}
