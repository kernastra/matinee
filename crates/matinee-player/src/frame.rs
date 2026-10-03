//! CPU frames produced for an external surface.

#![forbid(unsafe_code)]
//!
//! The software path caps the rendered picture at 1920×1080. A larger source
//! is scaled down here so the UI never uploads a 4K buffer. This is not a
//! claim that 4K software upload works.

use std::sync::{Arc, Mutex};
use std::time::Duration;

/// One BGRA picture. Alpha has been forced opaque for the software renderer,
/// whose padding byte is undefined.
#[derive(Clone, Debug)]
pub struct CpuFrame {
    width: u32,
    height: u32,
    stride: u32,
    pixels: Vec<u8>,
    generation: u64,
}

impl CpuFrame {
    pub(crate) fn new(
        width: u32,
        height: u32,
        stride: u32,
        pixels: Vec<u8>,
        generation: u64,
    ) -> Self {
        Self {
            width,
            height,
            stride,
            pixels,
            generation,
        }
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    pub fn stride(&self) -> u32 {
        self.stride
    }

    pub fn generation(&self) -> u64 {
        self.generation
    }

    pub fn pixels(&self) -> &[u8] {
        &self.pixels
    }

    pub fn into_pixels(self) -> Vec<u8> {
        self.pixels
    }
}

/// Latest frame only. A newer publish drops the previous unpublished frame.
pub struct CpuMailbox {
    latest: Mutex<Option<CpuFrame>>,
    produced: std::sync::atomic::AtomicU64,
    replaced: std::sync::atomic::AtomicU64,
    listener: Mutex<Option<Arc<dyn Fn() + Send + Sync>>>,
}

impl CpuMailbox {
    pub fn new() -> Self {
        Self {
            latest: Mutex::new(None),
            produced: std::sync::atomic::AtomicU64::new(0),
            replaced: std::sync::atomic::AtomicU64::new(0),
            listener: Mutex::new(None),
        }
    }

    pub fn set_listener(&self, listener: impl Fn() + Send + Sync + 'static) {
        *self.listener_lock() = Some(Arc::new(listener));
    }

    pub fn publish(&self, frame: CpuFrame) {
        let replaced = self.frame_lock().replace(frame).is_some();
        self.produced
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        if replaced {
            self.replaced
                .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
        }
        if let Some(listener) = self.listener_lock().clone() {
            listener();
        }
    }

    pub fn take(&self) -> Option<CpuFrame> {
        self.frame_lock().take()
    }

    pub fn produced(&self) -> u64 {
        self.produced.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn replaced(&self) -> u64 {
        self.replaced.load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn depth(&self) -> u32 {
        u32::from(self.frame_lock().is_some())
    }

    fn frame_lock(&self) -> std::sync::MutexGuard<'_, Option<CpuFrame>> {
        self.latest
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }

    fn listener_lock(&self) -> std::sync::MutexGuard<'_, Option<Arc<dyn Fn() + Send + Sync>>> {
        self.listener
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

impl Default for CpuMailbox {
    fn default() -> Self {
        Self::new()
    }
}

/// Software render target. Never upscales. Width is a multiple of 16 and
/// height is even so the stride can stay SIMD-friendly.
pub fn capped_render_size(source_w: u32, source_h: u32) -> (u32, u32) {
    const CAP_W: f64 = 1920.0;
    const CAP_H: f64 = 1080.0;
    if source_w == 0 || source_h == 0 {
        return (0, 0);
    }
    let (sw, sh) = (f64::from(source_w), f64::from(source_h));
    let scale = (CAP_W / sw).min(CAP_H / sh).min(1.0);
    let width = ((sw * scale).floor() as u32).max(16) & !15;
    let mut height = ((f64::from(width) * sh / sw).round() as u32).max(2) & !1;
    if height == 0 {
        height = 2;
    }
    (width, height)
}

/// The software renderer leaves the fourth byte undefined (`bgr0`).
pub fn force_opaque_bgra(buffer: &mut [u8]) {
    for pixel in buffer.chunks_exact_mut(4) {
        pixel[3] = 0xff;
    }
}

/// Lab-facing counters. Not a telemetry sink.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Diagnostics {
    pub produced: u64,
    pub replaced: u64,
    pub mailbox_depth: u32,
    pub frame_width: u32,
    pub frame_height: u32,
    pub render_mean: Duration,
    pub opaque_mean: Duration,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn cap_keeps_1080p_and_shrinks_4k() {
        assert_eq!(capped_render_size(1920, 1080), (1920, 1080));
        assert_eq!(capped_render_size(3840, 2160), (1920, 1080));
        assert_eq!(capped_render_size(1280, 720), (1280, 720));
        assert_eq!(capped_render_size(0, 1080), (0, 0));
        let (w, h) = capped_render_size(1918, 1080);
        assert_eq!(w % 16, 0);
        assert_eq!(h % 2, 0);
        assert!(w <= 1920);
        assert!(h <= 1080);
    }

    #[test]
    fn opaque_keeps_color() {
        let mut pixels = vec![1, 2, 3, 0, 4, 5, 6, 7];
        force_opaque_bgra(&mut pixels);
        assert_eq!(pixels, vec![1, 2, 3, 255, 4, 5, 6, 255]);
    }

    #[test]
    fn mailbox_keeps_only_the_latest() {
        let mailbox = CpuMailbox::new();
        mailbox.publish(CpuFrame::new(2, 2, 8, vec![1; 16], 1));
        mailbox.publish(CpuFrame::new(2, 2, 8, vec![2; 16], 2));
        assert_eq!(mailbox.depth(), 1);
        assert_eq!(mailbox.produced(), 2);
        assert_eq!(mailbox.replaced(), 1);
        assert_eq!(mailbox.take().unwrap().generation(), 2);
        assert_eq!(mailbox.depth(), 0);
    }
}
