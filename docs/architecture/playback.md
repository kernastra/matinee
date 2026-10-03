# Native playback

Status: **Phase 1D foundation**. The Matinee Player screen is not built.
Jellyfin login, Home, and Library are not built. Packaging is not built.

The feasibility spike (not merged) established the engine choice and the
cost of a software frame. This document is the production design. New
playback findings belong here.

## Path

```
future domain (Jellyfin URL) → matinee-player → libmpv
        → render thread → latest CPU frame
        → app adapter → atelier ExternalFrameSurface → GPUI
```

`matinee-player` loads a URL and returns a [`Snapshot`] plus BGRA frames.
It does not depend on GPUI, Atelier, or `matinee-ui`. `atelier-ui` paints
an externally produced BGRA picture. It does not know about codecs, clocks,
streams, or Matinee. The only adapter is `apps/matinee-playback-lab`, which
turns a `CpuFrame` into a `BgraFrame` and publishes it.

Audio stays inside libmpv. There is no Atelier audio layer. Subtitles are
drawn by libmpv (libass into the frame). The UI does not lay out subtitle text.

## What the spike settled

libmpv is the engine. GStreamer is not a second path, and the app does not
link its own FFmpeg.

GPUI 0.2.2 can paint a `RenderImage` (`Window::paint_image`). A 1920×1080
frame is larger than the 1024² atlas, so each frame is its own texture and
must be released with `Window::drop_image`. macOS `paint_surface` accepts a
`CVPixelBuffer` and asserts full-range 8-bit biplanar. The Windows surface
draw is a no-op. That is not a cross-platform zero-copy path. GPU surfaces
are not implemented.

The spike's software timings, on a 4-vCPU VM with lavapipe and no discrete
GPU, for 1920×1080 H.264 at 24 fps:

| Step | Cost |
|---|---|
| Software render | about 2–4 ms |
| Force opaque alpha | about 1 ms |
| GPUI upload | about 1 ms |

24 fps was sustained (611 produced, 608 shown, 3 replaced before paint).
A 3840×2160 software render was about 36 ms, which misses 24 fps before
upload. 4K is not a production target on this path. Phase 1D caps the
picture that is uploaded at 1920×1080 and can time a larger CPU render
without uploading it (`measure_software_render`).

`MPV_RENDER_PARAM_BLOCK_FOR_TARGET_TIME` must be 0. Leaving the default on
made a 24 fps frame look like a 39 ms render. The render thread returns the
newest frame; it does not sleep inside `mpv_render_context_render`.

## External frame surface

`ExternalFrameSurface` lives in `atelier-ui`. It is not a video view.

- Input is packed or strided BGRA (`BgraFrame`): width, height, stride,
  generation, bytes. Alpha is preserved. The player forces alpha to opaque
  first, because libmpv's `bgr0` padding byte is undefined.
- `FrameMailbox` holds one unpublished frame. A newer publish drops the
  previous unpublished frame. Dropped unpublished frames were never painted,
  so they are not passed to `drop_image`.
- The surface keeps the painted image and the one before it. On the next
  update it `drop_image`s the older of the two, matching the spike: release
  the frame from two updates ago so an in-flight scene can still sample it.
- Default placement is `ImageFit::Fit` (letterbox or pillarbox, centered,
  no stretch). `ImageFit::Fill` covers and the surface clips. A size change
  replaces the picture. An empty mailbox shows the accessible label on the
  canvas color.
- Publishing builds the CPU image off the UI thread, then wakes the surface
  on a channel. There is no polling loop and no 60 Hz timer. Reduced motion
  does not drop frames; it only collapses Atelier transitions.
- That off-thread build is safe on the pinned GPUI 0.2.2. `image::Frame::new`
  stores an `RgbaImage`. `RenderImage::new` (`gpui` `assets.rs`) stores those
  frames and an id from a process-wide atomic. Neither touches a window, the
  sprite atlas, or a GPU device. GPUI's own `App::fetch_asset` constructs
  `RenderImage` on `background_executor` the same way. GPU and window work
  starts later, on the window thread: `Window::paint_image` uploads into the
  sprite atlas, and `Window::drop_image` removes that atlas entry.
- Every image that was presented is released through GPUI. The surface
  `drop_image`s the frame from two updates ago so an in-flight scene can
  still sample the previous one. `release`, and entity drop via
  `App::drop_image`, release whatever is still presented or retired. GPUI
  flushes entity release after the window is back in the app map, so a live
  window does not keep the atlas entry. A window that has already closed
  drops its atlas with the window. An unpublished replacement is dropped as
  a CPU value and is not passed to `drop_image`. The mailbox never holds
  more than one unpublished frame. Presented plus the previous painted frame
  is that delayed release, not a frame queue.

## Player API

`Player` is the only handle future screens should hold. Commands are
checked against the current snapshot, then sent to the owner thread:

load URL (optional start position and HTTP headers), play, pause, toggle,
stop, seek absolute, seek relative, volume (0–1), mute, select audio,
select subtitle, disable subtitles, add an external subtitle file or URL.

`snapshot()` is one coherent value: state, position, duration, volume,
mute, audio tracks, subtitle tracks, selection, source size, presented
size, and error. Callers do not combine raw pause and eof flags.

`PlaybackState` is `Idle`, `Loading`, `Playing`, `Paused`, `Buffering`,
`Ended`, or `Error`. Priority is failure, then opening, idle, end of file,
buffering, pause, otherwise playing. Seek is not its own state. `Ended`
wins over pause so keep-open does not look like a user pause.

Discrete events, capped at 32, are loaded, ended, fatal error, track list
changed, and resolution changed. Position is an atomic so the snapshot lock
is not rewritten on every time update.

Track ids are assigned by Matinee when a track is first seen for the current
item and kept across refreshes. They are not libmpv ids. A new load starts
a new identity space. Image subtitle codecs (PGS and similar) are labeled
`SubtitleForm::Image` and still drawn by libmpv.

## Threads and ownership

One owner thread (`matinee-player`) owns the libmpv handle and is the only
thread that calls the client API, except `mpv_wakeup`.

- Commands arrive on a channel. The sender then calls `mpv_wakeup` through
  a slot that the owner clears, under the same mutex, before
  `mpv_terminate_destroy`. The owner blocks in `mpv_wait_event` with a
  timeout of 0 after each wake, so it does not poll.
- A second thread (`matinee-frames`) owns the render context. An mpv update
  callback wakes it. It renders into a 64-byte-aligned CPU buffer, forces
  opaque BGRA, and publishes to a one-slot mailbox. It does not call
  `get_property`. The owner publishes the target size.
- The mailbox listener may only signal another thread. The lab wakes its
  UI task, and that task takes the frame and publishes to Atelier.
- Shutdown sends `Shutdown`, pokes the owner, joins the render thread
  (which frees the render context), then drops the handle
  (`mpv_terminate_destroy`), then unloads the library. Drop of `Player`
  does the same. The render context is freed before the handle.

The GPUI thread does not call libmpv and does not wait for a frame.

## Dynamic library

libmpv is opened with `libloading` at `Player::open`. Nothing is linked at
compile time, and nothing is downloaded.

Search order:

1. `MATINEE_LIBMPV`, if set. A missing file is `LibraryMissing`.
2. Otherwise the platform soname: Linux `libmpv.so.2` then `libmpv.so`,
   macOS `libmpv.2.dylib` then `libmpv.dylib`, Windows `libmpv-2.dll`,
   `mpv-2.dll`, then `libmpv.dll`.

Client API major must be 2. Anything else is `LibraryIncompatible`. A
missing library is `LibraryMissing`. `matinee-next` does not open a player,
so the shell still launches when playback is unavailable. The lab shows the
typed error and stays open.

Development machines and Linux CI may use the distro package. Ubuntu's
libmpv and FFmpeg are GPL builds. They are for tests only and must not be
shipped. `scripts/check-libmpv.sh` reports which file the loader would see.
It does not download or copy one.

## Distribution groundwork

Release artifacts, when they exist, are a separately built LGPL libmpv plus
LGPL FFmpeg (`-Dgpl=false`), loaded dynamically. No binaries are committed.

| Platform | File the loader opens | Pin |
|---|---|---|
| Linux | `libmpv.so.2` | Not pinned yet. Set `MATINEE_LIBMPV` to the hashed file. |
| macOS | `libmpv.2.dylib` | Not pinned yet. |
| Windows | `libmpv-2.dll` | Not pinned yet. |

The pin is the file's sha256, recorded here when the artifact is produced.
There is no installer in this phase.

CI is split by platform. That split is not playback validation on every OS.

- Linux CI compiles the workspace and runs the architecture check, formatting,
  clippy, unit tests, and the real libmpv integration tests. That job installs
  `libmpv2` and `ffmpeg`. Linux CI panics if either is missing. The interactive
  Playback Lab was also run on a Linux host (`media/phase-1d/linux/`).
- macOS and Windows CI compile the workspace and run the pure tests. They do
  not install libmpv. Integration tests skip there. That is compile coverage
  plus pure tests, not playback validation, and not a libmpv integration run.

## Jellyfin

`matinee_core::native_playback()` is the capability declaration. The name
is `Matinee Native`. It is not the shipping WebKit profile.
`matinee-jellyfin` turns that declaration into the device-profile JSON it
posts. This crate does not build that JSON and does not depend on
`matinee-core` or `matinee-jellyfin`. The engine plays the formats the
declaration lists. The client does not use the shipping static
`/Videos/{id}/stream?Static=true` fallback when negotiation returns
`NoCompatibleStream`. It returns a typed `NoCompatibleSource` error instead.
See [jellyfin.md](jellyfin.md).

Direct play, because these combinations were opened with this engine:

- Containers: `mkv`, `mp4`, `m4v`
- Video: H.264, HEVC (including Main 10), AV1
- Audio: AAC, AC-3, E-AC-3, Opus. No channel cap on direct play.
- Subtitles: SubRip, embedded and external. No burn-in (`Encode`).

When Jellyfin refuses direct play, the transcode profile asks for fMP4 HLS,
H.264, AAC, up to six channels. `Player::load` plays the URL Jellyfin
returns and does not care whether it was a direct path or a transcode.
ASS, PGS, and other subtitle formats stay out of the profile until they
are validated the same way. libmpv can draw more than the profile claims;
the profile does not advertise that.

HTTP headers on `LoadRequest` are applied with `http-header-fields`, so a
future client can pass a media token without putting it in the URL.

## Options that differ from a stock mpv

`vo=libmpv`, `hwdec=auto-copy-safe`, `keep-open=yes`, `idle=yes`,
`hr-seek=yes`, `ytdl=no`, `osd-level=0`, `osc=no`, `audio-display=no`,
`force-window=no`. Tests pass `audio: false`, which sets `ao=null`.
`sub-margin-y` stays 0 because lab controls sit outside the picture, not
over it. The spike used a margin to clear an on-screen HUD. That HUD is
not this lab.

## Tests

Pure tests cover frame rejection, stride packing, latest-frame-wins,
placement, state priority, track identity, command validation, the device
profile, and a missing library path. They do not open a window or the network.

Integration tests generate short fixtures with ffmpeg (H.264 with two audio
tracks and SubRip, HEVC Main 10 with E-AC-3, AV1 with Opus, and a 1080p24
H.264 clip). They check transport, seek, replace, stop, shutdown, track
selection, mailbox depth, and that a few seconds of 1080p does not grow
the mailbox or resident memory without bound. They are skipped when libmpv
or ffmpeg is absent, except on Linux CI.

## Measurement on the Phase 1D Linux host

Release build, libmpv 0.37.0, FFmpeg 6.1.1, 4 vCPU, lavapipe, no audio device.
Source: generated 1920×1080 H.264 at 24 fps, local file, direct play.

| | Headless engine | Playback Lab |
|---|---|---|
| Software render | 1.54 ms mean | about 1.9–2.2 ms mean |
| Force opaque alpha | 0.55 ms mean | (included in the engine, not the upload) |
| GPUI upload | not uploaded | about 0.52 ms |
| Mailbox depth | 0 | 0 |
| Replaced before present | 0 | 0 |

The lab presented every produced frame across the sampled seconds, paused, seeked, and resumed. A separate 3840×2160 software render, not uploaded, averaged 8.88 ms plus 1.71 ms to force alpha. That is inside a 24 fps budget on this host before any GPU upload. It is not a 4K production result: the surface still refuses frames above the 1080p cap, and the earlier spike measured about 36 ms for the same class of work. 1080p is the target.

## Known limits

- 1080p24 software frames are the production target. 4K software upload is
  not, and the renderer will not upload a frame above the cap.
- A future GPU path needs a GPUI extension (a persistent texture or a
  cross-platform external surface). This phase does not add one.
- Interactive playback was exercised on Linux. macOS and Windows CI compile
  the workspace and run pure tests. They do not install libmpv, so those jobs
  are not playback validation and are not libmpv integration runs.
- The distro library used in development is GPL. Shipping it would violate
  the license plan above.
- HDR, tone-mapping UI, picture-in-picture, and remote-renderer playback
  are out of scope.
