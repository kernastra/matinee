//! Formats the native engine can play.
//!
//! A server client turns this into its own request document. Nothing here is
//! a URL, a header, or a server payload.

/// Name of the native capability set.
pub const NATIVE_PLAYBACK_NAME: &str = "Matinee Native";

/// Containers, codecs, and the transcode target validated with the engine.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct PlaybackCapabilities {
    pub name: &'static str,
    pub containers: &'static [&'static str],
    pub video_codecs: &'static [&'static str],
    pub audio_codecs: &'static [&'static str],
    pub subtitles: SubtitleCapabilities,
    pub transcode: TranscodeTarget,
    pub max_bitrate: u64,
}

/// Subtitles the engine draws itself. A server is not asked to burn them in.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct SubtitleCapabilities {
    pub formats: &'static [&'static str],
    pub embedded: bool,
    pub external: bool,
}

/// What to ask for when the original file cannot be played as-is.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct TranscodeTarget {
    pub container: &'static str,
    pub video_codec: &'static str,
    pub audio_codec: &'static str,
    pub max_audio_channels: u8,
    /// Segmented MP4. The engine plays that URL the same way it plays a file.
    pub segmented_mp4: bool,
}

/// The one declaration of native playback capabilities.
pub fn native_playback() -> PlaybackCapabilities {
    PlaybackCapabilities {
        name: NATIVE_PLAYBACK_NAME,
        containers: &["mkv", "mp4", "m4v"],
        video_codecs: &["h264", "hevc", "av1"],
        audio_codecs: &["aac", "ac3", "eac3", "opus"],
        subtitles: SubtitleCapabilities {
            formats: &["srt", "subrip"],
            embedded: true,
            external: true,
        },
        transcode: TranscodeTarget {
            container: "mp4",
            video_codec: "h264",
            audio_codec: "aac",
            max_audio_channels: 6,
            segmented_mp4: true,
        },
        max_bitrate: 120_000_000,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn native_playback_lists_the_validated_formats() {
        let caps = native_playback();
        assert_eq!(caps.name, NATIVE_PLAYBACK_NAME);
        assert!(caps.containers.contains(&"mkv"));
        assert!(caps.video_codecs.contains(&"hevc"));
        assert!(caps.video_codecs.contains(&"av1"));
        assert!(caps.audio_codecs.contains(&"opus"));
        assert!(caps.subtitles.embedded && caps.subtitles.external);
        assert!(caps.subtitles.formats.contains(&"srt"));
        assert_eq!(caps.transcode.max_audio_channels, 6);
        assert!(caps.transcode.segmented_mp4);
        assert_eq!(caps.transcode.video_codec, "h264");
    }
}
