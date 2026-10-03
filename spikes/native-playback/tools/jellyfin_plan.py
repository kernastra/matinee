#!/usr/bin/env python3
"""Reproduce Matinee's Jellyfin playback negotiation (src/lib/jellyfin.ts
`getPlaybackPlan`) for a given device profile and print the resulting URL.

Usage:
  jellyfin_plan.py --server URL --user NAME --password PW --item ITEM_ID \
      [--profile webkit|native] [--start-seconds N] [--audio INDEX] [--subtitle INDEX]

Only the Python standard library is used. Prints JSON to stdout.
"""

import argparse
import json
import sys
import urllib.parse
import urllib.request

DEVICE_ID = "matinee-native-playback-spike"

# Copied from src/lib/jellyfin.ts `videoDeviceProfile` (WebKit/HTML5 limits).
WEBKIT_PROFILE = {
    "Name": "Matinee WebKit",
    "MaxStreamingBitrate": 120_000_000,
    "MaxStaticBitrate": 120_000_000,
    "DirectPlayProfiles": [
        {"Container": "mp4,m4v", "Type": "Video", "VideoCodec": "h264", "AudioCodec": "aac,mp3"},
        {"Container": "hls", "Type": "Video", "VideoCodec": "h264", "AudioCodec": "aac"},
    ],
    "TranscodingProfiles": [
        {
            "Container": "ts",
            "Type": "Video",
            "VideoCodec": "h264",
            "AudioCodec": "aac",
            "Context": "Streaming",
            "Protocol": "hls",
            "MaxAudioChannels": "2",
            "MinSegments": 1,
            "BreakOnNonKeyFrames": True,
        }
    ],
    "ContainerProfiles": [],
    "CodecProfiles": [],
    "SubtitleProfiles": [],
}

# What a libmpv-backed player can claim: any container/codec FFmpeg decodes,
# embedded subtitles rendered locally, HEVC/AV1-capable HLS fallback.
NATIVE_PROFILE = {
    "Name": "Matinee Native (libmpv)",
    "MaxStreamingBitrate": 200_000_000,
    "MaxStaticBitrate": 200_000_000,
    "DirectPlayProfiles": [{"Type": "Video"}, {"Type": "Audio"}],
    "TranscodingProfiles": [
        {
            "Container": "mp4",
            "Type": "Video",
            "VideoCodec": "hevc,h264",
            "AudioCodec": "aac,ac3,eac3,opus,flac",
            "Context": "Streaming",
            "Protocol": "hls",
            "MaxAudioChannels": "8",
            "MinSegments": 1,
            "BreakOnNonKeyFrames": True,
        }
    ],
    "ContainerProfiles": [],
    "CodecProfiles": [],
    "SubtitleProfiles": [
        {"Format": fmt, "Method": "Embed"} for fmt in ("srt", "subrip", "ass", "ssa", "pgssub", "dvdsub", "vtt")
    ]
    + [{"Format": fmt, "Method": "External"} for fmt in ("srt", "ass", "vtt")],
}


def request(server, path, token=None, body=None):
    auth = f'MediaBrowser Client="Matinee Spike", Device="spike", DeviceId="{DEVICE_ID}", Version="0.0.0"'
    if token:
        auth += f', Token="{token}"'
    data = json.dumps(body).encode() if body is not None else None
    req = urllib.request.Request(
        f"{server}{path}",
        data=data,
        method="POST" if data is not None else "GET",
        headers={"Authorization": auth, "Content-Type": "application/json"},
    )
    with urllib.request.urlopen(req, timeout=30) as response:
        return json.load(response)


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--server", required=True)
    parser.add_argument("--user", required=True)
    parser.add_argument("--password", required=True)
    parser.add_argument("--item", required=True)
    parser.add_argument("--profile", choices=["webkit", "native"], default="native")
    parser.add_argument("--start-seconds", type=float, default=0)
    parser.add_argument("--audio", type=int)
    parser.add_argument("--subtitle", type=int)
    parser.add_argument("--max-bitrate", type=int)
    args = parser.parse_args()
    server = args.server.rstrip("/")

    auth = request(server, "/Users/AuthenticateByName", body={"Username": args.user, "Pw": args.password})
    token, user_id = auth["AccessToken"], auth["User"]["Id"]

    info = request(
        server,
        f"/Items/{args.item}/PlaybackInfo",
        token,
        {
            "UserId": user_id,
            "StartTimeTicks": int(args.start_seconds * 10_000_000),
            "MaxStreamingBitrate": args.max_bitrate,
            "AudioStreamIndex": args.audio,
            "SubtitleStreamIndex": args.subtitle,
            "IsPlayback": True,
            "AutoOpenLiveStream": True,
            "EnableDirectPlay": True,
            "EnableDirectStream": True,
            "EnableTranscoding": True,
            "AllowVideoStreamCopy": True,
            "AllowAudioStreamCopy": True,
            "DeviceProfile": WEBKIT_PROFILE if args.profile == "webkit" else NATIVE_PROFILE,
        },
    )
    if info.get("ErrorCode"):
        print(json.dumps({"error": info["ErrorCode"]}))
        return 1

    sources = info.get("MediaSources") or []
    source = next((s for s in sources if s.get("SupportsDirectPlay")), None) or next(
        (s for s in sources if s.get("TranscodingUrl")), sources[0] if sources else None
    )
    if source is None:
        print(json.dumps({"error": "no media source"}))
        return 1

    play_session = info.get("PlaySessionId", "")
    if source.get("SupportsDirectPlay"):
        query = urllib.parse.urlencode(
            {
                "Static": "true",
                "DeviceId": DEVICE_ID,
                "PlaySessionId": play_session,
                "MediaSourceId": source.get("Id", ""),
                "api_key": token,
            }
        )
        method, url = "DirectPlay", f"{server}/Videos/{args.item}/stream?{query}"
    else:
        url = urllib.parse.urljoin(f"{server}/", source["TranscodingUrl"].lstrip("/"))
        if "api_key=" not in url:
            url += f"&api_key={token}"
        method = "DirectStream" if source.get("SupportsDirectStream") else "Transcode"

    subtitles = [
        {
            "index": s["Index"],
            "codec": s.get("Codec"),
            "lang": s.get("Language"),
            "deliveryUrl": urllib.parse.urljoin(f"{server}/", s["DeliveryUrl"].lstrip("/")) if s.get("DeliveryUrl") else None,
            "deliveryMethod": s.get("DeliveryMethod"),
        }
        for s in source.get("MediaStreams", [])
        if s.get("Type") == "Subtitle"
    ]
    print(
        json.dumps(
            {
                "profile": args.profile,
                "method": method,
                "container": source.get("Container"),
                "transcodeReasons": source.get("TranscodeReasons"),
                "streams": [
                    f'{s.get("Type")}#{s["Index"]}:{s.get("Codec")}:{s.get("Language") or "-"}'
                    for s in source.get("MediaStreams", [])
                ],
                "subtitles": subtitles,
                "url": url,
                "token": token,
                "userId": user_id,
                "playSessionId": play_session,
                "mediaSourceId": source.get("Id"),
            },
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    sys.exit(main())
