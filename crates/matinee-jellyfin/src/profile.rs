//! Jellyfin device profile posted by this client.
//!
//! The document is negotiation policy. The same declaration lives in
//! `matinee-player` as the engine's capability list (`native_device_profile`).
//! The player crate does not depend on this one, and this one does not depend
//! on the player, so the JSON is built here. The two copies are the Phase 1D
//! `Matinee Native` profile and have to move together.
//!
//! It is not the shipping WebKit profile. WebKit direct-plays only MP4/H.264
//! and asks Jellyfin to burn subtitles. This profile direct-plays the
//! containers and codecs Phase 1D opened, and it asks for external or embedded
//! SubRip rather than burn-in.

use serde_json::{Value, json};

pub(crate) fn native_device_profile() -> Value {
    json!({
        "Name": "Matinee Native",
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
    fn profile_is_the_native_one() {
        let profile = native_device_profile();
        assert_eq!(profile["Name"], "Matinee Native");
        assert_ne!(profile["Name"], "Matinee WebKit");
        let direct = &profile["DirectPlayProfiles"][0];
        let video = direct["VideoCodec"].as_str().unwrap();
        assert!(video.contains("h264") && video.contains("hevc") && video.contains("av1"));
        let audio = direct["AudioCodec"].as_str().unwrap();
        for codec in ["aac", "ac3", "eac3", "opus"] {
            assert!(audio.contains(codec), "{codec}");
        }
        let transcode = &profile["TranscodingProfiles"][0];
        assert_eq!(transcode["Protocol"], "hls");
        assert_eq!(transcode["Container"], "mp4");
        assert_eq!(transcode["MaxAudioChannels"], "6");
        let subs = profile["SubtitleProfiles"].as_array().unwrap();
        assert!(subs.iter().all(|entry| entry["Method"] != "Encode"));
    }
}
