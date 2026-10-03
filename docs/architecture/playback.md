# Native playback: feasibility and recommendation

Status: Phase 0 spike. The spike lives in [`spikes/native-playback/`](../../spikes/native-playback/)
and is **not** the production player. Production video code belongs in
`crates/matinee-player`; nothing video-specific belongs in `atelier-ui`.

## Recommendation

**Use libmpv as Matinee Next's playback engine.** Integrate it in Phase 1 through
mpv's *software render API* feeding GPUI `RenderImage`s. That path works today on
GPUI 0.2.2 on every platform and was demonstrated end to end in this spike. Keep a
GPU zero-copy path as a later, isolated upgrade that requires a small GPUI extension.

- Ship an **LGPL build of libmpv** (`-Dgpl=false`, LGPL FFmpeg) and link it
  dynamically. Prefer loading it at runtime so that a missing library degrades to an
  error screen rather than a launch failure.
- **Change the Jellyfin device profile** so that the native player direct-plays
  everything FFmpeg decodes. Keep HLS transcoding only for bitrate caps and remote
  links.
- Hide every engine detail behind a `matinee-player` boundary: a `Player` controller,
  `Track` and `Event` models, and a `VideoSurface` element. A future engine or GPU-path
  swap should not touch UI code.

GStreamer is the credible runner-up. It has better adaptive HLS and a closer match to
native GPU memory types (D3D11, VA/DMABuf, CVPixelBuffer). However, it needs far more
integration code, has rougher runtime track switching, and has a messier plugin
licensing and packaging story. FFmpeg alone is a decoder library, not a player. See
the comparison below.

## Answers to the eight questions

| # | Question | Answer (libmpv) | Evidence |
|---|----------|-----------------|----------|
| 1 | Direct-play common Jellyfin media? | **Yes.** H.264 + AAC/AC-3 MKV with 2 audio and 2 subtitle tracks, HEVC Main10 + E-AC-3 (6ch), and AV1 + Opus all direct-play from a real Jellyfin 12.1 `/Videos/{id}/stream?Static=true` URL, with first frame in 150–210 ms. With today's WebKit profile, Jellyfin **transcodes all three** (`ContainerNotSupported`, `VideoCodecNotSupported`, `AudioCodecNotSupported`) and burns in subtitles (`SubtitleMethod=Encode`). | [Jellyfin negotiation](#jellyfin-negotiation), [mpv probe results](#mpv-probe-results) |
| 2 | Play Jellyfin HLS / transcoded streams? | **Yes.** Jellyfin `master.m3u8` (TS segments) and fMP4 HLS (native profile + bitrate cap) both play, as do a local ffmpeg HLS and the public Mux HLS. Resume, pause, and exact seek all pass. The first frame for a fresh transcode takes about 4 s, which is server CPU transcode time. *Limitation:* mpv uses FFmpeg's HLS demuxer, which has no adaptive bitrate switching and no WebVTT renditions, and it failed on Apple's multi-variant fMP4 sample. This doesn't affect Jellyfin, which serves single-variant playlists and delivers subtitles separately. | [mpv probe results](#mpv-probe-results) |
| 3 | Integrate decoded video cleanly into GPUI? | **Yes, via CPU frames, today.** mpv SW render → BGRA buffer → `RenderImage` → `Window::paint_image`, with old frames evicted via `Window::drop_image`. GPUI overlays composite normally over the video. 1080p runs at the full 24 fps with about 5 ms CPU per frame plus about 1 ms upload. 4K costs about 36 ms per frame, so it is not viable on this path. Zero-copy needs a GPUI API that GPUI doesn't have outside macOS. | [GPUI integration](#gpui-integration) |
| 4 | Seek, pause/play, volume, audio, subtitles, fullscreen, resume? | **All yes.** mpv properties and commands: `pause`, `seek … absolute+exact` and `relative+exact`, `volume`/`mute`, `aid`/`sid` (local switching, no renegotiation when direct-playing), `sub-add <Jellyfin DeliveryUrl>`, `start=<secs>` for resume. Fullscreen is GPUI's `Window::toggle_fullscreen`. All of these were exercised in the probes and the recorded GPUI demo. | [mpv probe results](#mpv-probe-results), demo video |
| 5 | Distribution requirements per platform? | macOS: bundle `libmpv.2.dylib` and its dependencies in `Contents/Frameworks`, then codesign and notarize each dylib (IINA's 1.4.4 DMG is 104 MB). Windows: ship one self-contained `libmpv-2.dll` (an LGPL CI build measures 96 MB, or 26 MB as 7z). Linux: depend on the system `libmpv2`/`mpv-libs`, or bundle it for AppImage/Flatpak. **Fedora note:** mpv and full FFmpeg live in RPM Fusion, not Fedora proper. | [Packaging](#distribution-and-packaging) |
| 6 | Licensing implications? | mpv is GPLv2+ by default and LGPLv2.1+ with `-Dgpl=false`. FFmpeg is LGPL unless built with `--enable-gpl` or `--enable-nonfree`. Ubuntu's libmpv (`gpl` feature) and FFmpeg (`--enable-gpl`, reported as "GPL version 2 or later") are both **GPL builds**, so they must not be what Matinee redistributes. MIT Matinee plus dynamically linked LGPL libmpv/FFmpeg is fine with notices and a source offer. GStreamer adds per-plugin license auditing: this spike auto-plugged GPL `a52dec` from the ugly plugin set. Codec patents (H.264/HEVC/AAC) are a separate question. | [Licensing](#licensing) |
| 7 | Hardware decoding options? | mpv `hwdec`: VideoToolbox (macOS), D3D11VA/NVDEC/DXVA2 (Windows), VA-API/NVDEC/Vulkan (Linux). The SW render path can only use **copy-back** modes (`auto-copy-safe`), which save decode CPU but still copy frames to RAM. True zero-copy needs mpv's OpenGL render API plus a GPUI external-texture primitive. Not measurable here: the VM has no GPU (`vainfo` fails, so `hwdec-current=no`). | [Hardware decoding](#hardware-decoding) |
| 8 | Packaging consequences? | +26–100 MB per platform; a pinned, per-platform prebuilt libmpv in CI (hash-checked); dylib signing and notarization on macOS; an import lib on Windows MSVC; an RPM Fusion/Flatpak story for Fedora; and FFmpeg security updates become Matinee's responsibility wherever it bundles. | [Packaging](#distribution-and-packaging) |

## Current Matinee playback (reference implementation)

The current player is `src/components/Player.tsx` plus `getPlaybackPlan` in `src/lib/jellyfin.ts`:

- `POST /Items/{id}/PlaybackInfo` with the **"Matinee WebKit" device profile**. It
  direct-plays only `mp4,m4v` + H.264 + AAC/MP3 (or HLS with H.264/AAC), and
  everything else transcodes to HLS MPEG-TS H.264/AAC with at most 2 audio channels.
  `SubtitleProfiles` is empty.
- Direct play uses `/Videos/{id}/stream?Static=true&MediaSourceId&PlaySessionId&api_key`.
  Transcodes use Jellyfin's `TranscodingUrl` (`master.m3u8`), played through hls.js,
  or natively where the WebView supports HLS.
- Changing audio, subtitles, or quality **renegotiates** a new `PlaybackInfo` with
  `AudioStreamIndex`/`SubtitleStreamIndex`/`StartTimeTicks` and restarts the stream.
  Resume seeks to `UserData.PlaybackPositionTicks` on load. Progress is reported to
  `/Sessions/Playing{,/Progress,/Stopped}` every 10 s.

A native player removes the WebKit codec ceiling. That is the biggest user-visible
win: server load drops, and quality, surround audio, and selectable (non-burned)
subtitles are preserved.

## Jellyfin negotiation

I installed Jellyfin 12.1 locally (`repo.jellyfin.org`, jellyfin-ffmpeg 8.1.3) with a
three-title library. [`tools/jellyfin_plan.py`](../../spikes/native-playback/tools/jellyfin_plan.py)
replays Matinee's exact `PlaybackInfo` request with the current WebKit profile and
with a proposed native profile (`DirectPlayProfiles: [{Type: Video}]`, embedded and
external subtitle profiles, and an fMP4 HLS HEVC/H.264 fallback):

| Title | WebKit profile (today) | Native profile |
|-------|------------------------|----------------|
| H.264 / AAC + AC-3 5.1 / 2× SRT, MKV | **Transcode** HLS-TS h264/aac, `ContainerNotSupported, SubtitleCodecNotSupported`, `SubtitleMethod=Encode` | **DirectPlay** `Static=true` |
| HEVC Main10 / E-AC-3 5.1, MKV | **Transcode**, `ContainerNotSupported, VideoCodecNotSupported, AudioCodecNotSupported` | **DirectPlay** |
| AV1 / Opus, MKV | **Transcode**, same three reasons | **DirectPlay** |
| HEVC, native profile, `MaxStreamingBitrate=2 Mbps` | — | **Transcode** fMP4 HLS (`SegmentContainer=mp4`), `ContainerBitrateExceedsLimit` |

Tooling gotcha: `AuthenticateByName` with the same `DeviceId` revokes the previous
token (my first HLS probe got HTTP 401). The production client must keep one session
per device.

## mpv probe results

`mpv-probe` uses libmpv client API 2.2 (mpv 0.37, Ubuntu 24.04) with `vo=null` and `ao=null`. All rows PASS unless noted.

| Source | First frame | Resume (`start=`) | Seek abs / −10 s exact | Audio switch | Subtitles |
|--------|-------------|-------------------|------------------------|--------------|-----------|
| Local H.264 MKV | 268 ms | 12.00 s ✓ | 220 ms / 400 ms | aac(eng) ↔ ac3 5.1(spa) ✓ | eng/spa ✓ with correct `sub-text`, off ✓ |
| Jellyfin DirectPlay H.264 MKV | 163 ms | ✓ | 16 ms / 20 ms | ✓ | ✓ (embedded) |
| Jellyfin DirectPlay HEVC Main10 / E-AC-3 | 153 ms | ✓ | 179 ms / 271 ms | eac3 6ch ✓ | n/a |
| Jellyfin DirectPlay AV1 / Opus | 211 ms | ✓ | 108 ms / 24 ms | ✓ | n/a |
| Jellyfin HLS-TS transcode (H.264 source) | 3.86 s | 12.02 s ✓ | 357 ms / 231 ms | single track* | burned in* |
| Jellyfin HLS-TS transcode (HEVC source) | 4.35 s | 12.08 s ✓ | 102 ms / 144 ms | single track* | n/a |
| Jellyfin fMP4 HLS transcode (native profile, capped) | 1.37 s | — | 46 ms / 38 ms | aac 6ch ✓ | n/a |
| Local ffmpeg HLS over HTTP | 56 ms | ✓ | 92 ms / 67 ms | ✓ | n/a |
| Public Mux HLS (`x36xhzz`) | 1.22 s | 60.07 s ✓ | 974 ms / 1159 ms | 5 renditions ✓ | n/a |
| Apple bipbop fMP4 multi-variant | **FAIL** | — | — | — | VTT rendition unsupported |

\* For transcodes, Jellyfin only includes the requested audio and subtitle tracks, so
switching still renegotiates, as it does today.

Notes:

- The default relative seek is keyframe-based; with x265's 10 s GOP, a −10 s seek
  landed on 0 s. The player should use `+exact` (or set `hr-seek=yes`).
- A Jellyfin subtitle `DeliveryUrl` (`…/Subtitles/4/0/Stream.srt`) loads with
  `sub-file`/`sub-add` and renders the correct cue. Put the token in
  `http-header-fields` rather than `api_key=`, which otherwise leaks into track titles
  and logs.
- `osd-level=0` lets GPUI own all chrome while mpv still blends subtitles (libass)
  into the frame. `sub-margin-y` keeps subtitles clear of the overlay controls.

## GPUI integration

GPUI version: `gpui = "=0.2.2"` from crates.io (edition 2024, Rust 1.99.0 pinned in the
spike). Renderers: Blade/Vulkan on Linux, Metal on macOS, Direct3D 11 on Windows.

### What exists in GPUI 0.2.2

| API | Platforms | Use for video |
|-----|-----------|---------------|
| `RenderImage::new(Vec<image::Frame>)` (BGRA8) + `Window::paint_image` / `img(ImageSource::Render)` | all | **Works.** Each new `RenderImage` is uploaded into the sprite atlas. A 1080p frame exceeds the 1024² atlas page, so **every frame allocates and destroys its own GPU texture**. |
| `Window::drop_image` / `App::drop_image` | all | Required, or the atlas leaks one texture per frame. |
| `surface(CVPixelBuffer)` / `Window::paint_surface` | **macOS only** | Zero-copy NV12 sampling, but it **asserts `kCVPixelFormatType_420YpCbCr8BiPlanarFullRange`**. Most video is video-range, and 10-bit is P010, so even this path needs a GPUI change before it can show real video correctly. |
| `PrimitiveBatch::Surfaces` on Windows (`directx_renderer.rs::draw_surfaces`) | Windows | No-op stub. |
| `HasWindowHandle` for `Window` | all | Enables the native-child-window alternative (below). |

### What the spike does (`gpui-player`)

```text
mpv decode thread ──(vo=libmpv update callback)──▶ render thread
   render thread: mpv_render_context_render(SW, "bgr0", BLOCK_FOR_TARGET_TIME=0)
                  → force alpha=0xFF → RgbaImage → Arc<RenderImage>
                  → latest-frame mailbox (drops superseded frames before upload)
UI (spawn_in task): take mailbox → cx.notify(); drop_image(frame from 2 updates ago)
paint (canvas): Window::paint_image(contain-fit bounds) → GPUI atlas upload
GPUI overlay (HUD, progress, controls) is ordinary elements on top
```

These measurements come from a 4-vCPU VM with **no GPU**. GPUI runs on lavapipe
(software Vulkan), so "upload" is a CPU memcpy and overall CPU use is pessimistic.

| Stage (mean per frame) | 1080p H.264 | 1080p HEVC 10-bit | 4K HEVC 10-bit |
|------------------------|-------------|-------------------|----------------|
| mpv SW render (YUV→BGRA, scale, subtitle blend) | 2.1–3.7 ms | 2.5 ms | 11–17 ms |
| Force opaque alpha (`bgr0` padding is garbage) | 0.8–1.0 ms | 0.8 ms | 3.7–4.6 ms |
| Allocate buffer + wrap `RenderImage` | 0.6–0.8 ms | 0.6 ms | 2.4–7.3 ms |
| GPUI `paint_image` for a new frame (atlas alloc + upload) | 0.8–1.0 ms | 1.6 ms | 6.4 ms |
| Result | 24 fps sustained; 611 produced, 608 shown, 3 superseded | 24.3 fps | about 36 ms per frame: **not viable** |

Whole process at 1080p: 140% CPU (mostly lavapipe compositing), against 15% for
headless mpv decode alone.

Caveat: `mpv_render_context_render` blocks until the frame's display time by default.
A naive measurement reported 39 ms per frame before I set
`MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME=0`.

### GPUI APIs Matinee would need (in priority order)

1. **Persistent, updatable texture handle.** Write each frame into one long-lived
   texture instead of allocating and destroying an atlas texture per frame. Ideally it
   accepts **NV12/P010 planes** with a GPU YUV→RGB shader, which moves color conversion
   off the CPU and removes the alpha-fix pass.
2. **Cross-platform external-surface primitive.** Generalize `paint_surface`:
   - macOS: `CVPixelBuffer`/IOSurface, with video-range NV12, P010, and BGRA, plus
     color-matrix and range metadata.
   - Windows: shared-handle `ID3D11Texture2D` on GPUI's D3D11 device.
   - Linux: DMABuf import into Blade/Vulkan via `VK_EXT_external_memory_dma_buf`.

   This enables zero-copy hardware decode, using mpv's OpenGL render API with
   platform interop, or GStreamer's native memory types.
3. **Frame-pacing hooks.** Expose presentation timestamps and vsync feedback
   (`request_animation_frame`/`on_next_frame` exist; swap feedback doesn't) for mpv's
   `display-resample` and A/V sync.
4. **HDR.** The GPUI swapchain is 8-bit SDR. HDR content is tone-mapped to SDR until
   GPUI supports HDR surfaces.

All of this should live in a small, upstreamable GPUI patch exposed to Matinee only
through `matinee-player`'s `VideoSurface` element.

### Alternative: native child window (`wid`)

mpv's own GPU VO (libplacebo, zero-copy hwdec, HDR) can render into a native child
view (NSView, child HWND, X11 child window, or Wayland subsurface) positioned under
GPUI. GPUI exposes `HasWindowHandle`, but not child views. The cost is "airspace":
GPUI cannot draw over the video, so controls would need separate overlay windows,
which are especially awkward on Wayland. This is a fallback for 4K/HDR fidelity, not
the default.

## Hardware decoding

| Platform | mpv `hwdec` | Usable with SW render API | Zero-copy path |
|----------|-------------|---------------------------|----------------|
| macOS | `videotoolbox` | `videotoolbox-copy` | GL render API + IOSurface → GPUI `surface` (needs fix 2) |
| Windows | `d3d11va`, `nvdec`, `dxva2` | `d3d11va-copy`, `nvdec-copy` | D3D11 shared texture → GPUI D3D11 (needs fix 2) |
| Linux | `vaapi`, `nvdec`, `vulkan` | `vaapi-copy`, `nvdec-copy` | VA-API → DMABuf → Vulkan import (needs fix 2) |

The spike defaults to `hwdec=auto-copy-safe`. On this VM `hwdec-current=no`, because
there is no VA/VDPAU device. At 1080p, software decode (H.264, about 183 fps
decode-only in `ffmpeg-probe`) is comfortably sufficient. 4K HEVC 10-bit software
decode dropped frames on 4 vCPUs, which is exactly where copy-back hwdec helps first.

## Engine comparison

| | libmpv | GStreamer (gstreamer-rs 0.23, GStreamer 1.24.2) | FFmpeg direct (ffmpeg-next 7.1, FFmpeg 6.1) |
|-|--------|-----------------------------------------------|---------------------------------------------|
| What you get | Complete player: demux, decode, A/V sync, audio output, libass subtitles, HLS, caching, hwdec | Pipeline framework: `playbin3`/`playbin`, `appsink`, adaptive demuxers, platform decoders | Demux and decode only. You build clocks, A/V sync, audio output, subtitles, network, and HLS sessions |
| Spike result | Every control passed on local, Jellyfin direct play, and Jellyfin HLS | Local and Jellyfin direct play passed with `playbin`. **`playbin3` runtime `select-streams` stalled video delivery** after audio/subtitle switches. Jellyfin HLS seek returned no position. Apple bipbop worked via `hlsdemux2` (VTT exposed, 1080p60) after a **28 s preroll**. GPL `a52dec` was auto-plugged for AC-3 | 183 fps H.264, 89 fps HEVC10, 382 fps AV1 decode + swscale; 0.8–3.7 ms BGRA convert |
| Frame path into GPUI | SW render into a caller buffer (one conversion) | appsink BGRA (`videoconvert`) plus one copy out of `GstBuffer` (0.7–1.8 ms) | swscale into your buffer |
| HLS | FFmpeg demuxer (no ABR, no VTT renditions); fine for Jellyfin | Best of the three (`hlsdemux2`) | FFmpeg demuxer |
| GPU zero-copy future | GL render API + interop | **Native memory types** (`d3d11`/`d3d12`, `va`/DMABuf, `vtdec` → CVPixelBuffer) | DIY hwaccel interop |
| Integration effort | Low (≈15 FFI functions in `src/mpv.rs`) | Medium–high (bus, collections, sparse streams, plugin ranks) | Very high |
| Packaging | One library (Windows: one DLL) | Core plus dozens of plugins and a registry. Large, fragile on macOS/Windows | FFmpeg libs, plus everything you build |

Other options considered:

- **libVLC.** LGPL with a mature player and video callbacks (CPU frames). Its Rust
  bindings are stale, it ships a heavy plugin tree, and its APIs are no better than
  mpv's for GPUI.
- **Platform frameworks** (AVFoundation, Media Foundation). They give the best power
  use and HDR, and AVPlayer → CVPixelBuffer → GPUI `surface` is attractive on macOS.
  But there is no MKV, ASS, or broad codec support, and each OS needs a separate
  implementation. They would push Jellyfin back toward transcoding.
- **Pure Rust.** Not credible for H.264/HEVC playback (dav1d/rav1d covers AV1 only).

## Distribution and packaging

- **macOS.** Build libmpv with `-Dgpl=false` against an LGPL FFmpeg; Homebrew's mpv is
  a GPL build. Bundle the dylib closure in `Contents/Frameworks` and fix install names
  (`@rpath`). Sign every dylib with the hardened runtime and notarize. Universal2
  needs two builds or lipo. For reference, IINA (bundled libmpv) ships a 104 MB DMG.
- **Windows.** Use an LGPL CI build: `mpv-dev-lgpl-x86_64-*.7z` (26 MB) unpacks to a
  single 96 MB `libmpv-2.dll` plus `libmpv.dll.a`. Generate an MSVC import `.lib` from
  a `.def` file, or load it at runtime. The spike's `build.rs` honors `MPV_LIB_DIR`.
- **Linux.** Ubuntu's `libmpv2` links 227 shared libraries (≈240 MB closure) because it
  is a kitchen-sink GPL build. Use it only for development. For release:
  - **deb/rpm:** depend on the distro's `libmpv2` (Debian/Ubuntu) or `mpv-libs`
    (**Fedora: RPM Fusion**). Fedora's own `ffmpeg-free` lacks patent-encumbered
    decoders.
  - **Flatpak:** add a libmpv module plus the `org.freedesktop.Platform.codecs-extra`
    / ffmpeg-full extension, as Celluloid and Haruna do.
  - **AppImage:** bundle a lean LGPL libmpv build.
- **CI.** Pin libmpv per platform by version and hash. Unlike WebKit's codecs,
  security updates to FFmpeg/libmpv become Matinee's release responsibility on
  macOS/Windows/AppImage.
- **Runtime loading.** Load libmpv at runtime (`libloading`) rather than linking it,
  so the app starts without it, can show a clear "video engine missing" state on Linux,
  and can prefer the system library.

## Licensing

Not legal advice; confirm before shipping.

- **Matinee is MIT.** Dynamically linking LGPL libraries is compatible. Obligations:
  ship the LGPL texts and notices, provide (or offer) the exact source of the LGPL
  libraries you distribute, allow users to swap the dynamic library, and don't forbid
  reverse engineering for that purpose.
- **mpv** is GPLv2+ by default; `-Dgpl=false` makes it LGPLv2.1+ and drops a few
  GPL-only features that Matinee doesn't need. Evidence: Ubuntu's libmpv feature list
  includes `gpl`, while third-party Windows CI publishes explicit `-lgpl` artifacts.
- **FFmpeg** is LGPL by default. `--enable-gpl` (x264, x265, some filters) turns the
  whole build GPL, and `--enable-nonfree` (e.g. fdk-aac) makes it **non-redistributable**.
  A player needs only decoders, so LGPL FFmpeg plus dav1d (BSD), libass (ISC),
  libplacebo (LGPL), and FreeType/HarfBuzz/FriBidi is sufficient. Evidence:
  `ffmpeg-probe` reports Ubuntu's libavcodec license as "GPL version 2 or later" with
  `--enable-gpl --enable-libx264 --enable-libx265`.
- Bundling a **GPL** libmpv or FFmpeg would require distributing the combined binary
  under GPL terms. MIT code allows that, but it also rules out the Mac App Store.
  Avoid it.
- **GStreamer** core/base/good are LGPL, but ugly/bad contain GPL elements; this spike
  auto-plugged `a52dec` (liba52, GPL). Each plugin would need auditing and rank
  pinning.
- **Patents.** H.264, HEVC, and AAC decoders may need patent licenses in some
  jurisdictions; OS decoders are licensed by the OS vendor. This is why Fedora ships
  `ffmpeg-free`. It is a distribution-policy decision, independent of the copyright
  license.

## Proposed `matinee-player` shape (Phase 1 input, not implemented)

- `Engine` (libmpv, runtime-loaded) → `Player` (commands: load(url, start, headers),
  play/pause, seek(exact), volume/mute, select_audio/select_subtitle, add_subtitle(url)),
  plus an observable state stream (position, duration, paused, buffering, tracks,
  errors, end-of-file).
- `VideoSurface` GPUI element. Phase 1 uses the CPU `RenderImage` path from this
  spike (render thread, latest-frame mailbox, delayed `drop_image`). The GPU path
  arrives later behind the same element.
- `matinee-jellyfin` owns the device profile, `PlaybackInfo`, progress reporting, and
  subtitle `DeliveryUrl`s. Direct play switches tracks locally; transcodes renegotiate.

## Reproducing

See [`spikes/native-playback/README.md`](../../spikes/native-playback/README.md).
