# Native playback feasibility spike

This is throwaway Phase 0 evidence for Matinee Next. It is **not** the production
player. Findings and the recommendation are in
[`docs/architecture/playback.md`](../../docs/architecture/playback.md).

This is a standalone Cargo project. It has its own empty `[workspace]` table, so it
never joins the root Matinee Next workspace. Every probe is behind a Cargo feature, so
you only need the system libraries for the probe you build.

| Binary | Feature | What it proves |
|--------|---------|----------------|
| `mpv-probe` | `mpv` | Headless libmpv: load file/URL, first frame, resume (`start=`), pause, exact seek, volume/mute, audio and subtitle switching, SW-render cost (`--sw-render SECS`) |
| `gpui-player` | `gpui` | libmpv SW render → BGRA → GPUI `RenderImage` in a GPUI 0.2.2 window, with GPUI overlay controls, fullscreen, and per-stage timings |
| `gst-probe` | `gst` | The same checks with GStreamer `playbin3` (default) or `playbin` (`--playbin2`) and a BGRA appsink |
| `ffmpeg-probe` | `ffmpeg` | FFmpeg-direct decode + swscale to BGRA throughput and seek |
| `tools/jellyfin_plan.py` | none | Replays Matinee's `PlaybackInfo` negotiation (WebKit or native profile) and prints the resulting stream URL |

`src/mpv.rs` holds about 15 hand-written libmpv FFI declarations (client API 2.x plus
the `sw` render API). `src/frame.rs` holds the backend-agnostic frame helpers (unit
tested).

## Prerequisites

The toolchain is pinned by `rust-toolchain.toml` (Rust 1.99.0). GPUI 0.2.2 needs edition 2024.

Ubuntu 24.04 packages (used to produce the results in the doc):

```bash
sudo apt-get install -y \
  libmpv-dev mpv \
  libgstreamer1.0-dev libgstreamer-plugins-base1.0-dev gstreamer1.0-plugins-{base,good,bad,ugly} gstreamer1.0-libav \
  libavcodec-dev libavformat-dev libavutil-dev libswscale-dev libswresample-dev libavfilter-dev libavdevice-dev clang \
  libxkbcommon-dev libxkbcommon-x11-dev libwayland-dev libvulkan-dev mesa-vulkan-drivers \
  libfontconfig-dev libfreetype-dev libx11-xcb-dev libxcb1-dev libssl-dev libzstd-dev
```

On Fedora, libmpv comes from RPM Fusion (`mpv-libs-devel`). On macOS, use `brew install mpv pkgconf`
(a GPL build, fine for local experiments). On Windows, download an `mpv-dev-lgpl-x86_64-*.7z`
and set `MPV_LIB_DIR` to the folder containing `libmpv-2.dll` / the import library.

These Ubuntu packages are GPL builds of mpv and FFmpeg. They are fine for development
but not for redistribution; see the licensing section of the doc.

## Test media

```bash
mkdir -p /tmp/spike-media && cd /tmp/spike-media
printf '1\n00:00:01,000 --> 00:00:20,000\n[ENGLISH SUBTITLE] first cue\n\n2\n00:00:20,000 --> 00:00:40,000\n[ENGLISH SUBTITLE] second cue\n' > eng.srt
sed 's/ENGLISH/SPANISH/' eng.srt > spa.srt
ffmpeg -y -f lavfi -i testsrc2=size=1920x1080:rate=24:duration=60 \
  -f lavfi -i sine=frequency=440:duration=60 -f lavfi -i sine=frequency=880:duration=60 \
  -i eng.srt -i spa.srt -map 0:v -map 1:a -map 2:a -map 3 -map 4 \
  -c:v libx264 -preset veryfast -pix_fmt yuv420p -c:a:0 aac -c:a:1 ac3 -ac:a:1 6 -c:s srt \
  -metadata:s:a:0 language=eng -metadata:s:a:1 language=spa \
  -metadata:s:s:0 language=eng -metadata:s:s:1 language=spa sample-h264-2audio-2subs.mkv
ffmpeg -y -f lavfi -i testsrc2=size=1920x1080:rate=24:duration=30 -f lavfi -i sine=duration=30 \
  -c:v libx265 -preset ultrafast -pix_fmt yuv420p10le -c:a eac3 -ac 6 sample-hevc-main10-eac3.mkv
# Jellyfin-like HLS (MPEG-TS, H.264/AAC), served over HTTP:
mkdir -p hls && ffmpeg -y -i sample-h264-2audio-2subs.mkv -map 0:v -map 0:a:0 -c:v libx264 -g 72 \
  -c:a aac -ac 2 -f hls -hls_time 3 -hls_playlist_type vod -hls_segment_filename 'hls/seg%03d.ts' hls/main.m3u8
python3 -m http.server 8099 &
```

## Running

```bash
cd spikes/native-playback

cargo run --release --features mpv --bin mpv-probe -- /tmp/spike-media/sample-h264-2audio-2subs.mkv --start 12 --sw-render 4
cargo run --release --features mpv --bin mpv-probe -- http://localhost:8099/hls/main.m3u8 --start 12
cargo run --release --features mpv --bin mpv-probe -- https://test-streams.mux.dev/x36xhzz/x36xhzz.m3u8 --start 60

cargo run --release --features gst --bin gst-probe -- /tmp/spike-media/sample-h264-2audio-2subs.mkv --playbin2 --start 12
cargo run --release --features ffmpeg --bin ffmpeg-probe -- /tmp/spike-media/sample-hevc-main10-eac3.mkv --seek 10

# GPUI window (needs a display; Vulkan via a GPU or mesa lavapipe on Linux).
cargo run --release --features gpui --bin gpui-player -- /tmp/spike-media/sample-h264-2audio-2subs.mkv --start 5
#   keys: space, ←/→, ↑/↓, a (audio), s (subtitles), m, f (fullscreen), q
#   --demo [--demo-quit] runs a scripted seek/audio/subtitle/pause/volume/fullscreen tour and prints STATS
```

Probes print `PASS` / `FAIL` / `INFO` lines and exit non-zero on any failure.

### Against a Jellyfin server

```bash
URL=$(python3 tools/jellyfin_plan.py --server http://localhost:8096 --user USER --password PASS \
        --item ITEM_ID --profile native | python3 -c 'import json,sys; print(json.load(sys.stdin)["url"])')
cargo run --release --features mpv --bin mpv-probe -- "$URL" --start 12
```

Use `--profile webkit` to get today's Matinee transcode URL, and `--max-bitrate N` to force an HLS transcode.
Generate the URL right before playing it: each login with the same `DeviceId` revokes the previous token.

## Checks

```bash
cargo fmt --check
cargo clippy --release --features mpv,gst,ffmpeg,gpui --all-targets
cargo test --release --features mpv
```
