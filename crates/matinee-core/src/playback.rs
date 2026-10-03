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

/// A playable result. `url` is ready for a player that can load an HTTP URL.
#[derive(Clone, Debug, PartialEq)]
pub struct PlaybackPlan {
    pub url: String,
    pub source_id: Option<MediaSourceId>,
    pub play_session_id: PlaySessionId,
    pub method: PlaybackMethod,
    pub streams: Vec<MediaStream>,
    pub selected_audio: Option<i32>,
    pub selected_subtitle: Option<i32>,
    pub start_position: Duration,
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
    pub play_session_id: PlaySessionId,
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
    /// Jellyfin's volume field is an integer percentage.
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
            play_session_id: session,
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
