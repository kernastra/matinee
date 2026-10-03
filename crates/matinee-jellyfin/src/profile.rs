//! Device profile posted to PlaybackInfo.
//!
//! The facts come from [`matinee_core::native_playback`]. This module only
//! turns them into the server's JSON. The playback engine does not build
//! that document and does not depend on this crate.

use matinee_core::native_playback;
use serde_json::{Value, json};

pub(crate) fn native_device_profile() -> Value {
    let caps = native_playback();
    let mut subtitles = Vec::new();
    for format in caps.subtitles.formats {
        if caps.subtitles.embedded {
            subtitles.push(json!({ "Format": format, "Method": "Embed" }));
        }
        if caps.subtitles.external {
            subtitles.push(json!({ "Format": format, "Method": "External" }));
        }
    }
    let protocol = if caps.transcode.segmented_mp4 {
        "hls"
    } else {
        "http"
    };
    json!({
        "Name": caps.name,
        "MaxStreamingBitrate": caps.max_bitrate,
        "MaxStaticBitrate": caps.max_bitrate,
        "DirectPlayProfiles": [
            {
                "Container": caps.containers.join(","),
                "Type": "Video",
                "VideoCodec": caps.video_codecs.join(","),
                "AudioCodec": caps.audio_codecs.join(","),
            }
        ],
        "TranscodingProfiles": [
            {
                "Container": caps.transcode.container,
                "Type": "Video",
                "VideoCodec": caps.transcode.video_codec,
                "AudioCodec": caps.transcode.audio_codec,
                "Context": "Streaming",
                "Protocol": protocol,
                "MaxAudioChannels": caps.transcode.max_audio_channels.to_string(),
                "MinSegments": 1,
                "BreakOnNonKeyFrames": true,
                "SegmentContainer": caps.transcode.container,
            }
        ],
        "ContainerProfiles": [],
        "CodecProfiles": [],
        "SubtitleProfiles": subtitles,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_is_built_from_the_shared_capabilities() {
        let caps = native_playback();
        let profile = native_device_profile();
        assert_eq!(profile["Name"], caps.name);
        assert_ne!(profile["Name"], "Matinee WebKit");
        let direct = &profile["DirectPlayProfiles"][0];
        assert_eq!(direct["Container"], caps.containers.join(","));
        assert_eq!(direct["VideoCodec"], caps.video_codecs.join(","));
        assert_eq!(direct["AudioCodec"], caps.audio_codecs.join(","));
        assert!(direct.get("MaxAudioChannels").is_none());
        let transcode = &profile["TranscodingProfiles"][0];
        assert_eq!(transcode["Protocol"], "hls");
        assert_eq!(transcode["Container"], caps.transcode.container);
        assert_eq!(transcode["VideoCodec"], caps.transcode.video_codec);
        assert_eq!(
            transcode["MaxAudioChannels"],
            caps.transcode.max_audio_channels.to_string()
        );
        let subs = profile["SubtitleProfiles"].as_array().unwrap();
        assert!(subs.iter().all(|entry| entry["Method"] != "Encode"));
        assert!(subs.iter().any(|entry| entry["Method"] == "External"));
    }
}
