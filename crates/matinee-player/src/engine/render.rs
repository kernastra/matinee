//! Render thread. It owns the software render context and never calls the
//! client API. The owner publishes the target size; this thread publishes
//! the latest BGRA frame.

#![forbid(unsafe_code)]

use std::path::Path;
use std::sync::atomic::{AtomicBool, AtomicU32, AtomicU64, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Mutex};
use std::thread::{self, JoinHandle};
use std::time::{Duration, Instant};

use crate::engine::ffi::{self, Api, Mpv, MpvHandle, SwRender};
use crate::engine::library;
use crate::error::PlayerError;
use crate::frame::{CpuFrame, CpuMailbox, capped_render_size, force_opaque_bgra};

pub struct FrameStats {
    pub render_ns: AtomicU64,
    pub render_count: AtomicU64,
    pub opaque_ns: AtomicU64,
    pub width: AtomicU32,
    pub height: AtomicU32,
}

impl FrameStats {
    pub fn new() -> Self {
        Self {
            render_ns: AtomicU64::new(0),
            render_count: AtomicU64::new(0),
            opaque_ns: AtomicU64::new(0),
            width: AtomicU32::new(0),
            height: AtomicU32::new(0),
        }
    }
}

pub struct RenderThread {
    wake: Sender<()>,
    stop: Arc<AtomicBool>,
    size: Arc<Mutex<(u32, u32)>>,
    join: Option<JoinHandle<()>>,
}

impl RenderThread {
    pub fn start(
        api: Api,
        mpv: MpvHandle,
        frames: Arc<CpuMailbox>,
        stats: Arc<FrameStats>,
    ) -> Result<Self, PlayerError> {
        let (wake, wake_rx) = mpsc::channel();
        let (ready_tx, ready_rx) = mpsc::channel();
        let stop = Arc::new(AtomicBool::new(false));
        let size = Arc::new(Mutex::new((0u32, 0u32)));
        let stop_flag = stop.clone();
        let size_slot = size.clone();
        let wake_for_callback = wake.clone();
        let join = thread::Builder::new()
            .name("matinee-frames".into())
            .spawn(move || {
                let mut renderer = match SwRender::new(api, mpv.as_ptr()) {
                    Ok(renderer) => renderer,
                    Err(error) => {
                        let _ = ready_tx.send(Err(error));
                        return;
                    }
                };
                renderer.set_update_callback(move || {
                    let _ = wake_for_callback.send(());
                });
                let _ = ready_tx.send(Ok(()));
                loop {
                    if stop_flag.load(Ordering::Relaxed) {
                        break;
                    }
                    if wake_rx.recv().is_err() {
                        break;
                    }
                    if stop_flag.load(Ordering::Relaxed) || !renderer.needs_frame() {
                        continue;
                    }
                    let (width, height) = *size_slot
                        .lock()
                        .unwrap_or_else(|poison| poison.into_inner());
                    if width == 0 || height == 0 {
                        continue;
                    }
                    if let Some(frame) = draw(&renderer, width, height, &stats) {
                        frames.publish(frame);
                    }
                }
                drop(renderer);
            })
            .map_err(|error| PlayerError::Initialization(error.to_string()))?;
        match ready_rx.recv() {
            Ok(Ok(())) => Ok(Self {
                wake,
                stop,
                size,
                join: Some(join),
            }),
            Ok(Err(error)) => {
                let _ = join.join();
                Err(error)
            }
            Err(_) => {
                let _ = join.join();
                Err(PlayerError::Initialization(
                    "render thread exited before it was ready".into(),
                ))
            }
        }
    }

    pub fn set_size(&self, source_w: u32, source_h: u32) {
        let size = capped_render_size(source_w, source_h);
        *self
            .size
            .lock()
            .unwrap_or_else(|poison| poison.into_inner()) = size;
    }

    pub fn shutdown(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.wake.send(());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

impl Drop for RenderThread {
    fn drop(&mut self) {
        self.stop.store(true, Ordering::Relaxed);
        let _ = self.wake.send(());
        if let Some(join) = self.join.take() {
            let _ = join.join();
        }
    }
}

fn draw(renderer: &SwRender, width: u32, height: u32, stats: &FrameStats) -> Option<CpuFrame> {
    let len = (width as usize)
        .checked_mul(height as usize)?
        .checked_mul(4)?;
    let (mut buffer, offset) = aligned_buffer(len);
    let started = Instant::now();
    let slice = &mut buffer[offset..offset + len];
    if renderer.render_bgr0(width, height, slice).is_err() {
        return None;
    }
    let rendered = started.elapsed();
    let started = Instant::now();
    force_opaque_bgra(slice);
    let opaque = started.elapsed();
    let pixels = slice.to_vec();
    let generation = stats.render_count.fetch_add(1, Ordering::Relaxed) + 1;
    stats
        .render_ns
        .fetch_add(nanos(rendered), Ordering::Relaxed);
    stats.opaque_ns.fetch_add(nanos(opaque), Ordering::Relaxed);
    stats.width.store(width, Ordering::Relaxed);
    stats.height.store(height, Ordering::Relaxed);
    Some(CpuFrame::new(width, height, width * 4, pixels, generation))
}

fn aligned_buffer(len: usize) -> (Vec<u8>, usize) {
    const ALIGN: usize = 64;
    let buffer = vec![0u8; len + ALIGN];
    let address = buffer.as_ptr() as usize;
    let aligned = (address + ALIGN - 1) & !(ALIGN - 1);
    (buffer, aligned - address)
}

fn nanos(duration: Duration) -> u64 {
    u64::try_from(duration.as_nanos()).unwrap_or(u64::MAX)
}

/// Render `frames` stills at `width`×`height` into a CPU buffer.
///
/// This does not upload anything. It is how the lab checks a size the
/// playback path refuses to present, including 4K.
pub fn measure_software_render(
    source: &Path,
    width: u32,
    height: u32,
    frames: u32,
) -> Result<crate::frame::Diagnostics, PlayerError> {
    if width == 0 || height == 0 || frames == 0 {
        return Err(PlayerError::InvalidCommand(
            "measurement size is empty".into(),
        ));
    }
    let loaded = library::load()?;
    let mut options = playback_options(false);
    options.push(("pause".into(), "no".into()));
    let mpv = Mpv::create(loaded.api, &pairs(&options))?;
    let renderer = SwRender::new(loaded.api, mpv.handle().as_ptr())?;
    mpv.command(&[
        "loadfile",
        source
            .to_str()
            .ok_or_else(|| PlayerError::InvalidCommand("measurement path is not UTF-8".into()))?,
        "replace",
    ])?;
    let stats = FrameStats::new();
    let mut produced = 0u32;
    let deadline = Instant::now() + Duration::from_secs(30);
    while produced < frames && Instant::now() < deadline {
        match mpv.wait_event(0.05) {
            ffi::EngineEvent::EndFile { error, .. } if error < 0 => {
                return Err(PlayerError::Engine(format!("measurement ended ({error})")));
            }
            ffi::EngineEvent::Shutdown => {
                return Err(PlayerError::Engine("measurement shut down".into()));
            }
            _ => {}
        }
        if renderer.needs_frame() && draw(&renderer, width, height, &stats).is_some() {
            produced += 1;
        }
    }
    if produced == 0 {
        return Err(PlayerError::Engine(
            "software render produced no frames".into(),
        ));
    }
    let count = stats.render_count.load(Ordering::Relaxed).max(1);
    Ok(crate::frame::Diagnostics {
        produced: u64::from(produced),
        replaced: 0,
        mailbox_depth: 0,
        frame_width: width,
        frame_height: height,
        render_mean: Duration::from_nanos(stats.render_ns.load(Ordering::Relaxed) / count),
        opaque_mean: Duration::from_nanos(stats.opaque_ns.load(Ordering::Relaxed) / count),
    })
}

pub fn playback_options(audio: bool) -> Vec<(String, String)> {
    let mut options = vec![
        ("vo", "libmpv"),
        ("hwdec", "auto-copy-safe"),
        ("keep-open", "yes"),
        ("idle", "yes"),
        ("terminal", "no"),
        ("msg-level", "all=warn"),
        ("ytdl", "no"),
        ("osd-level", "0"),
        ("osc", "no"),
        ("hr-seek", "yes"),
        ("audio-display", "no"),
        ("force-window", "no"),
    ]
    .into_iter()
    .map(|(key, value)| (key.to_string(), value.to_string()))
    .collect::<Vec<_>>();
    if !audio {
        options.push(("ao".into(), "null".into()));
    }
    options
}

pub fn pairs(options: &[(String, String)]) -> Vec<(&str, &str)> {
    options
        .iter()
        .map(|(key, value)| (key.as_str(), value.as_str()))
        .collect()
}
