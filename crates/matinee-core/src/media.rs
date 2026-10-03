//! Technical media: containers, streams, and how a file can be delivered.
//!
//! Stream indexes are the server's indexes. They are not player track ids.

use std::time::Duration;

use crate::id::MediaSourceId;

#[derive(Clone, Debug, PartialEq, Default)]
pub struct TechnicalMedia {
    sources: Vec<MediaSource>,
    streams: Vec<MediaStream>,
}

impl TechnicalMedia {
    pub fn new(sources: Vec<MediaSource>, streams: Vec<MediaStream>) -> Self {
        Self { sources, streams }
    }

    pub fn sources(&self) -> &[MediaSource] {
        &self.sources
    }

    pub fn item_streams(&self) -> &[MediaStream] {
        &self.streams
    }

    /// Streams for one source, falling back to the item-level list when that
    /// source did not repeat them.
    pub fn source_streams(&self, index: usize) -> &[MediaStream] {
        match self.sources.get(index) {
            Some(source) if !source.streams.is_empty() => &source.streams,
            Some(_) => &self.streams,
            None => &[],
        }
    }

    pub fn primary_video(&self) -> Option<&VideoStream> {
        self.presentation_streams()
            .iter()
            .find_map(MediaStream::as_video)
    }

    pub fn primary_audio(&self) -> Option<&AudioStream> {
        self.presentation_streams()
            .iter()
            .find_map(MediaStream::as_audio)
    }

    fn presentation_streams(&self) -> &[MediaStream] {
        if self.sources.is_empty() {
            &self.streams
        } else {
            self.source_streams(0)
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct MediaSource {
    pub id: Option<MediaSourceId>,
    pub name: Option<String>,
    pub path: Option<String>,
    pub container: Option<String>,
    pub bitrate: Option<u64>,
    pub runtime: Option<Duration>,
    pub size_bytes: Option<u64>,
    pub delivery: Delivery,
    pub streams: Vec<MediaStream>,
    pub default_audio_index: Option<i32>,
    pub default_subtitle_index: Option<i32>,
}

/// What the server said this source can do. Not a promise about the player.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Default)]
pub struct Delivery {
    pub direct_play: bool,
    pub direct_stream: bool,
    pub transcode: bool,
}

#[derive(Clone, Debug, PartialEq)]
pub enum MediaStream {
    Video(VideoStream),
    Audio(AudioStream),
    Subtitle(SubtitleStream),
    Other { index: i32, type_name: String },
}

impl MediaStream {
    pub fn index(&self) -> i32 {
        match self {
            Self::Video(stream) => stream.index,
            Self::Audio(stream) => stream.index,
            Self::Subtitle(stream) => stream.index,
            Self::Other { index, .. } => *index,
        }
    }

    pub fn as_video(&self) -> Option<&VideoStream> {
        match self {
            Self::Video(stream) => Some(stream),
            _ => None,
        }
    }

    pub fn as_audio(&self) -> Option<&AudioStream> {
        match self {
            Self::Audio(stream) => Some(stream),
            _ => None,
        }
    }

    pub fn as_subtitle(&self) -> Option<&SubtitleStream> {
        match self {
            Self::Subtitle(stream) => Some(stream),
            _ => None,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct VideoStream {
    pub index: i32,
    pub codec: Option<String>,
    pub title: Option<String>,
    pub language: Option<String>,
    pub width: Option<u32>,
    pub height: Option<u32>,
    pub range: DynamicRange,
    pub bit_depth: Option<u32>,
    pub frame_rate: Option<f64>,
    pub bitrate: Option<u64>,
    pub profile: Option<String>,
    pub pixel_format: Option<String>,
    pub color_space: Option<String>,
    pub is_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum DynamicRange {
    Sdr,
    Hdr { label: String },
    Unknown,
}

impl DynamicRange {
    pub fn is_hdr(&self) -> bool {
        matches!(self, Self::Hdr { .. })
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct AudioStream {
    pub index: i32,
    pub codec: Option<String>,
    pub title: Option<String>,
    pub display_title: Option<String>,
    pub language: Option<String>,
    pub channels: Option<u32>,
    pub channel_layout: Option<String>,
    pub sample_rate: Option<u32>,
    pub bitrate: Option<u64>,
    pub is_default: bool,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct SubtitleStream {
    pub index: i32,
    pub codec: Option<String>,
    pub title: Option<String>,
    pub language: Option<String>,
    pub is_default: bool,
    pub forced: bool,
    pub external: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hdr_and_multichannel_facts_stay_on_the_stream() {
        let video = VideoStream {
            index: 0,
            codec: Some("hevc".into()),
            title: None,
            language: None,
            width: Some(3840),
            height: Some(2160),
            range: DynamicRange::Hdr {
                label: "HDR10".into(),
            },
            bit_depth: Some(10),
            frame_rate: Some(23.976),
            bitrate: None,
            profile: Some("Main 10".into()),
            pixel_format: None,
            color_space: None,
            is_default: true,
        };
        assert!(video.range.is_hdr());
        let audio = AudioStream {
            index: 1,
            codec: Some("eac3".into()),
            title: None,
            display_title: None,
            language: Some("eng".into()),
            channels: Some(6),
            channel_layout: Some("5.1".into()),
            sample_rate: Some(48_000),
            bitrate: None,
            is_default: true,
        };
        let media = TechnicalMedia::new(
            Vec::new(),
            vec![MediaStream::Video(video), MediaStream::Audio(audio)],
        );
        assert_eq!(media.primary_video().unwrap().bit_depth, Some(10));
        assert_eq!(media.primary_audio().unwrap().channels, Some(6));
    }

    #[test]
    fn unknown_stream_keeps_its_index() {
        let stream = MediaStream::Other {
            index: 4,
            type_name: "Data".into(),
        };
        assert_eq!(stream.index(), 4);
        assert!(stream.as_video().is_none());
    }
}
