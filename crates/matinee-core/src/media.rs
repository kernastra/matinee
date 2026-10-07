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

    /// Short labels a viewer recognises, such as `4K · HDR10 · HEVC` and
    /// `Dolby Atmos · 7.1`. Raw stream fields stay out of the presentation.
    pub fn summary(&self) -> TechnicalSummary {
        TechnicalSummary {
            video: self.primary_video().and_then(video_label),
            audio: self.primary_audio().and_then(audio_label),
        }
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

/// Viewer-facing technical labels for the first source.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TechnicalSummary {
    pub video: Option<String>,
    pub audio: Option<String>,
}

fn video_label(video: &VideoStream) -> Option<String> {
    let mut parts = Vec::new();
    if let Some(resolution) = resolution_label(video.width, video.height) {
        parts.push(resolution);
    }
    if let DynamicRange::Hdr { label } = &video.range {
        parts.push(label.clone());
    }
    if let Some(codec) = video.codec.as_deref().map(video_codec_label) {
        parts.push(codec);
    }
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// Width counts as well as height, so a 1920×800 scope film is 1080p, not 800p.
fn resolution_label(width: Option<u32>, height: Option<u32>) -> Option<String> {
    let (width, height) = (width.unwrap_or(0), height.unwrap_or(0));
    let label = if width >= 3800 || height >= 2100 {
        "4K".to_string()
    } else if width >= 1900 || height >= 1060 {
        "1080p".to_string()
    } else if width >= 1260 || height >= 700 {
        "720p".to_string()
    } else if height > 0 {
        "SD".to_string()
    } else {
        return None;
    };
    Some(label)
}

fn video_codec_label(codec: &str) -> String {
    match codec.to_ascii_lowercase().as_str() {
        "hevc" | "h265" => "HEVC".into(),
        "h264" | "avc" => "H.264".into(),
        "av1" => "AV1".into(),
        "vp9" => "VP9".into(),
        "mpeg2video" => "MPEG-2".into(),
        other => other.to_ascii_uppercase(),
    }
}

fn audio_label(audio: &AudioStream) -> Option<String> {
    let described = audio
        .display_title
        .as_deref()
        .or(audio.title.as_deref())
        .unwrap_or_default()
        .to_ascii_lowercase();
    let codec = audio.codec.as_deref().map(str::to_ascii_lowercase);
    let format = if described.contains("atmos") {
        Some("Dolby Atmos".to_string())
    } else if described.contains("dts:x") || described.contains("dts-x") {
        Some("DTS:X".to_string())
    } else if described.contains("dts-hd") || described.contains("dts hd") {
        Some("DTS-HD".to_string())
    } else {
        codec.as_deref().map(|codec| match codec {
            "truehd" => "Dolby TrueHD".to_string(),
            "eac3" => "Dolby Digital+".to_string(),
            "ac3" => "Dolby Digital".to_string(),
            "dts" | "dca" => "DTS".to_string(),
            "aac" => "AAC".to_string(),
            "flac" => "FLAC".to_string(),
            "opus" => "Opus".to_string(),
            "mp3" => "MP3".to_string(),
            other => other.to_ascii_uppercase(),
        })
    };
    let channels = audio.channels.map(|channels| match channels {
        1 => "Mono".to_string(),
        2 => "Stereo".to_string(),
        6 => "5.1".to_string(),
        8 => "7.1".to_string(),
        other => format!("{other} ch"),
    });
    let parts: Vec<String> = format.into_iter().chain(channels).collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
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

    fn video(width: u32, height: u32, codec: &str, range: DynamicRange) -> MediaStream {
        MediaStream::Video(VideoStream {
            index: 0,
            codec: Some(codec.into()),
            title: None,
            language: None,
            width: Some(width),
            height: Some(height),
            range,
            bit_depth: None,
            frame_rate: None,
            bitrate: None,
            profile: None,
            pixel_format: None,
            color_space: None,
            is_default: true,
        })
    }

    fn audio(codec: &str, channels: u32, display: Option<&str>) -> MediaStream {
        MediaStream::Audio(AudioStream {
            index: 1,
            codec: Some(codec.into()),
            title: None,
            display_title: display.map(str::to_string),
            language: None,
            channels: Some(channels),
            channel_layout: None,
            sample_rate: None,
            bitrate: None,
            is_default: true,
        })
    }

    #[test]
    fn technical_summary_uses_viewer_labels() {
        let uhd = TechnicalMedia::new(
            Vec::new(),
            vec![
                video(
                    3840,
                    2160,
                    "hevc",
                    DynamicRange::Hdr {
                        label: "HDR10".into(),
                    },
                ),
                audio("truehd", 8, Some("English - Dolby TrueHD Atmos 7.1")),
            ],
        );
        assert_eq!(
            uhd.summary(),
            TechnicalSummary {
                video: Some("4K · HDR10 · HEVC".into()),
                audio: Some("Dolby Atmos · 7.1".into()),
            }
        );
        let scope = TechnicalMedia::new(
            Vec::new(),
            vec![
                video(1920, 800, "h264", DynamicRange::Sdr),
                audio("aac", 2, None),
            ],
        );
        assert_eq!(scope.summary().video.as_deref(), Some("1080p · H.264"));
        assert_eq!(scope.summary().audio.as_deref(), Some("AAC · Stereo"));
        assert_eq!(
            TechnicalMedia::default().summary(),
            TechnicalSummary::default()
        );
    }

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
