//! Jellyfin device profile for the native engine.

#![forbid(unsafe_code)]
//!
//! This is a declaration of formats exercised with libmpv. It is not the
//! shipping WebKit profile, and it does not list every codec libmpv can open.
//! A future Jellyfin client posts this document to `PlaybackInfo`. The engine
//! then plays whatever URL that response supplies.

use serde_json::{Value, json};

/// Profile name sent to Jellyfin. Distinct from the WebKit player profile.
pub const PROFILE_NAME: &str = "Matinee Native";

/// Containers, codecs, and subtitle methods that Phase 1D validated.
///
/// Direct play:
/// - Matroska and MP4 with H.264, HEVC Main 10, or AV1 video.
/// - AAC, AC-3, E-AC-3, and Opus audio.
/// - Embedded and external SubRip subtitles. The engine draws them. Jellyfin
///   is not asked to burn them in.
///
/// Transcode fallback, when Jellyfin refuses direct play (for example a
/// bitrate cap): fMP4 HLS, H.264, AAC, up to six channels. The engine plays
/// that URL the same way it plays a direct URL.
pub fn native_device_profile() -> Value {
    json!({
        "Name": PROFILE_NAME,
        "MaxStreamingBitrate": 120_000_000,
        "MaxStaticBitrate": 120_000_000,
        "DirectPlayProfiles": [
            {
                "Container": "mkv,mp4,m4v",
                "Type": "Video",
                "VideoCodec": "h264,hevc,av1",
                "AudioCodec": "aac,ac3,eac3,opus"
            }
        ],
        "TranscodingProfiles": [
            {
                "Container": "mp4",
                "Type": "Video",
                "VideoCodec": "h264",
                "AudioCodec": "aac",
                "Context": "Streaming",
                "Protocol": "hls",
                "MaxAudioChannels": "6",
                "MinSegments": 1,
                "BreakOnNonKeyFrames": true,
                "SegmentContainer": "mp4"
            }
        ],
        "ContainerProfiles": [],
        "CodecProfiles": [],
        "SubtitleProfiles": [
            { "Format": "srt", "Method": "Embed" },
            { "Format": "srt", "Method": "External" },
            { "Format": "subrip", "Method": "Embed" },
            { "Format": "subrip", "Method": "External" }
        ]
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_is_native_and_not_webkit() {
        let profile = native_device_profile();
        assert_eq!(profile["Name"], PROFILE_NAME);
        assert_ne!(profile["Name"], "Matinee WebKit");
        let direct = &profile["DirectPlayProfiles"][0];
        let video = direct["VideoCodec"].as_str().unwrap();
        assert!(video.contains("h264"));
        assert!(video.contains("hevc"));
        assert!(video.contains("av1"));
        let audio = direct["AudioCodec"].as_str().unwrap();
        for codec in ["aac", "ac3", "eac3", "opus"] {
            assert!(audio.contains(codec), "{codec}");
        }
        assert!(direct.get("MaxAudioChannels").is_none());
        let subs = profile["SubtitleProfiles"].as_array().unwrap();
        assert!(!subs.is_empty());
        assert!(subs.iter().all(|entry| entry["Method"] != "Encode"));
        let transcode = &profile["TranscodingProfiles"][0];
        assert_eq!(transcode["Protocol"], "hls");
        assert_eq!(transcode["Container"], "mp4");
        assert_ne!(transcode["MaxAudioChannels"], "2");
    }
}
