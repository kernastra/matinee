//! Latest externally produced BGRA frame.
//!
//! A producer on any thread publishes packed or strided BGRA pixels. The
//! view keeps at most one unpublished frame, wakes the UI, and paints through
//! a GPUI image. Superseded frames that were never painted are dropped.
//! Painted images are released one update later so an in-flight scene can
//! still sample them.
//!
//! Frame delivery is not a transition. Reduced motion does not thin it out.
//! The type knows pixel size, stride, generation, and image resources. It does
//! not know codecs, clocks, or streams.

use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use futures::StreamExt;
use futures::channel::mpsc::{UnboundedSender, unbounded};
use gpui::prelude::FluentBuilder;
use gpui::{
    Bounds, Context, Corners, IntoElement, ParentElement, Render, RenderImage, SharedString,
    Styled, WeakEntity, Window, canvas, div, point, px, size,
};
use image::{Frame, RgbaImage};

use crate::{
    ActiveTheme,
    components::{ImageFit, Text, TextTone},
    tokens::TextRole,
};

const BYTES_PER_PIXEL: u32 = 4;

/// Why a frame was rejected.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FrameError {
    /// Width or height was zero.
    Empty,
    /// `stride` was below `width * 4`.
    StrideTooSmall { stride: u32, minimum: u32 },
    /// The byte buffer was shorter than `stride * height`.
    BufferTooSmall { len: usize, required: usize },
    /// Width, height, and stride overflow `usize`.
    DimensionOverflow,
}

/// Owned BGRA pixels. Byte 0 of each pixel is blue. Alpha is preserved.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BgraFrame {
    width: u32,
    height: u32,
    stride: u32,
    pixels: Vec<u8>,
    generation: u64,
}

impl BgraFrame {
    /// `stride` is bytes per row and may include padding.
    pub fn new(
        width: u32,
        height: u32,
        stride: u32,
        pixels: Vec<u8>,
        generation: u64,
    ) -> Result<Self, FrameError> {
        validate(width, height, stride, pixels.len())?;
        Ok(Self {
            width,
            height,
            stride,
            pixels,
            generation,
        })
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
}

fn validate(width: u32, height: u32, stride: u32, len: usize) -> Result<(), FrameError> {
    if width == 0 || height == 0 {
        return Err(FrameError::Empty);
    }
    let minimum = width
        .checked_mul(BYTES_PER_PIXEL)
        .ok_or(FrameError::DimensionOverflow)?;
    if stride < minimum {
        return Err(FrameError::StrideTooSmall { stride, minimum });
    }
    let required = (stride as usize)
        .checked_mul(height as usize)
        .ok_or(FrameError::DimensionOverflow)?;
    if len < required {
        return Err(FrameError::BufferTooSmall { len, required });
    }
    Ok(())
}

/// Tightly packed BGRA (`stride == width * 4`).
pub(crate) struct PackedFrame {
    width: u32,
    height: u32,
    pixels: Vec<u8>,
    generation: u64,
}

pub(crate) fn pack_bgra(frame: BgraFrame) -> Result<PackedFrame, FrameError> {
    let row = (frame.width as usize)
        .checked_mul(BYTES_PER_PIXEL as usize)
        .ok_or(FrameError::DimensionOverflow)?;
    let packed_len = row
        .checked_mul(frame.height as usize)
        .ok_or(FrameError::DimensionOverflow)?;
    let stride = frame.stride as usize;
    if stride == row {
        return Ok(PackedFrame {
            width: frame.width,
            height: frame.height,
            pixels: frame.pixels,
            generation: frame.generation,
        });
    }
    let mut pixels = vec![0u8; packed_len];
    for y in 0..frame.height as usize {
        let src = y * stride;
        let dst = y * row;
        pixels[dst..dst + row].copy_from_slice(&frame.pixels[src..src + row]);
    }
    Ok(PackedFrame {
        width: frame.width,
        height: frame.height,
        pixels,
        generation: frame.generation,
    })
}

/// Where a frame sits inside a rectangle, in the same units as the inputs.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct FramePlacement {
    pub x: f32,
    pub y: f32,
    pub width: f32,
    pub height: f32,
}

/// Fit or fill `content` inside `bounds`, centered. Non-positive inputs yield
/// an empty placement. Fill may extend past the rectangle; callers clip.
pub fn place_frame(
    content_w: f32,
    content_h: f32,
    bounds_w: f32,
    bounds_h: f32,
    fit: ImageFit,
) -> FramePlacement {
    if !(content_w > 0.0 && content_h > 0.0 && bounds_w > 0.0 && bounds_h > 0.0) {
        return FramePlacement {
            x: 0.0,
            y: 0.0,
            width: 0.0,
            height: 0.0,
        };
    }
    let scale = match fit {
        ImageFit::Fit => (bounds_w / content_w).min(bounds_h / content_h),
        ImageFit::Fill => (bounds_w / content_w).max(bounds_h / content_h),
    };
    let width = content_w * scale;
    let height = content_h * scale;
    FramePlacement {
        x: (bounds_w - width) / 2.0,
        y: (bounds_h - height) / 2.0,
        width,
        height,
    }
}

struct PendingFrame {
    image: Arc<RenderImage>,
    width: u32,
    height: u32,
    generation: u64,
}

struct MailboxInner {
    pending: Option<PendingFrame>,
    published: u64,
    replaced: u64,
    wake: Option<UnboundedSender<()>>,
}

/// Cloneable handle to a single-slot frame mailbox.
///
/// Publishing is cheap and non-blocking. The previous unpublished frame is
/// dropped without a GPU release, because it was never painted. Depth is 0 or 1.
#[derive(Clone)]
pub struct FrameMailbox {
    inner: Arc<Mutex<MailboxInner>>,
}

impl FrameMailbox {
    pub fn new() -> Self {
        Self {
            inner: Arc::new(Mutex::new(MailboxInner {
                pending: None,
                published: 0,
                replaced: 0,
                wake: None,
            })),
        }
    }

    fn bind_wake(&self, wake: UnboundedSender<()>) {
        self.lock().wake = Some(wake);
    }

    /// Replace the pending frame and wake the UI. Safe from a producer thread.
    pub fn publish(&self, frame: BgraFrame) -> Result<(), FrameError> {
        let packed = pack_bgra(frame)?;
        let image = RgbaImage::from_raw(packed.width, packed.height, packed.pixels).ok_or(
            FrameError::BufferTooSmall {
                len: 0,
                required: 1,
            },
        )?;
        let pending = PendingFrame {
            image: Arc::new(RenderImage::new(vec![Frame::new(image)])),
            width: packed.width,
            height: packed.height,
            generation: packed.generation,
        };
        let mut inner = self.lock();
        let replaced = inner.pending.replace(pending).is_some();
        inner.published += 1;
        if replaced {
            inner.replaced += 1;
        }
        if let Some(wake) = &inner.wake {
            let _ = wake.unbounded_send(());
        }
        Ok(())
    }

    pub fn stats(&self) -> MailboxStats {
        let inner = self.lock();
        MailboxStats {
            published: inner.published,
            replaced: inner.replaced,
            depth: u8::from(inner.pending.is_some()),
        }
    }

    fn take(&self) -> Option<PendingFrame> {
        self.lock().pending.take()
    }

    fn lock(&self) -> std::sync::MutexGuard<'_, MailboxInner> {
        self.inner
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
    }
}

impl Default for FrameMailbox {
    fn default() -> Self {
        Self::new()
    }
}

/// Counters for the single-slot mailbox. Depth never exceeds 1.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct MailboxStats {
    pub published: u64,
    pub replaced: u64,
    pub depth: u8,
}

struct Presented {
    image: Arc<RenderImage>,
    width: u32,
    height: u32,
    generation: u64,
}

struct UploadStats {
    count: u64,
    total: Duration,
    last: Duration,
    last_id: Option<usize>,
}

/// Paints the latest [`BgraFrame`] published to its [`FrameMailbox`].
///
/// Letterboxing uses the canvas color. The default fit preserves aspect ratio.
pub struct ExternalFrameSurface {
    mailbox: FrameMailbox,
    fit: ImageFit,
    label: SharedString,
    presented: Option<Presented>,
    /// Painted image from the previous update. Released on the update after that.
    retired: Option<Arc<RenderImage>>,
    presented_count: u64,
    upload: Arc<Mutex<UploadStats>>,
}

impl ExternalFrameSurface {
    /// `label` is the accessible name and the empty-state caption.
    pub fn new(
        label: impl Into<SharedString>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> Self {
        let mailbox = FrameMailbox::new();
        let (tx, mut rx) = unbounded();
        mailbox.bind_wake(tx);
        let upload = Arc::new(Mutex::new(UploadStats {
            count: 0,
            total: Duration::ZERO,
            last: Duration::ZERO,
            last_id: None,
        }));
        cx.spawn_in(window, async move |this: WeakEntity<Self>, cx| {
            while rx.next().await.is_some() {
                while rx.try_recv().is_ok() {}
                let alive = this.update_in(cx, |this, window, cx| this.consume(window, cx));
                if alive.is_err() {
                    break;
                }
            }
        })
        .detach();
        Self {
            mailbox,
            fit: ImageFit::Fit,
            label: label.into(),
            presented: None,
            retired: None,
            presented_count: 0,
            upload,
        }
    }

    pub fn mailbox(&self) -> FrameMailbox {
        self.mailbox.clone()
    }

    pub fn fit(&self) -> ImageFit {
        self.fit
    }

    pub fn set_fit(&mut self, fit: ImageFit, cx: &mut Context<Self>) {
        self.fit = fit;
        cx.notify();
    }

    pub fn generation(&self) -> Option<u64> {
        self.presented.as_ref().map(|frame| frame.generation)
    }

    pub fn frame_size(&self) -> Option<(u32, u32)> {
        self.presented
            .as_ref()
            .map(|frame| (frame.width, frame.height))
    }

    pub fn presented_count(&self) -> u64 {
        self.presented_count
    }

    pub fn mean_upload(&self) -> Duration {
        let stats = self
            .upload
            .lock()
            .unwrap_or_else(|poison| poison.into_inner());
        if stats.count == 0 {
            Duration::ZERO
        } else {
            stats.total / u32::try_from(stats.count).unwrap_or(u32::MAX)
        }
    }

    pub fn last_upload(&self) -> Duration {
        self.upload
            .lock()
            .unwrap_or_else(|poison| poison.into_inner())
            .last
    }

    /// Release painted images. Unpublished mailbox frames are dropped with the handle.
    pub fn release(&mut self, window: &mut Window) {
        if let Some(frame) = self.presented.take() {
            let _ = window.drop_image(frame.image);
        }
        if let Some(image) = self.retired.take() {
            let _ = window.drop_image(image);
        }
    }

    fn consume(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        let Some(pending) = self.mailbox.take() else {
            return;
        };
        if let Some(stale) = self.retired.take() {
            let _ = window.drop_image(stale);
        }
        self.retired = self.presented.take().map(|frame| frame.image);
        self.presented = Some(Presented {
            image: pending.image,
            width: pending.width,
            height: pending.height,
            generation: pending.generation,
        });
        self.presented_count += 1;
        cx.notify();
    }
}

impl Render for ExternalFrameSurface {
    fn render(&mut self, _window: &mut Window, cx: &mut Context<Self>) -> impl IntoElement {
        let theme = cx.theme().clone();
        let fit = self.fit;
        let presented = self.presented.as_ref().map(|frame| Presented {
            image: frame.image.clone(),
            width: frame.width,
            height: frame.height,
            generation: frame.generation,
        });
        let upload = self.upload.clone();
        let label = self.label.clone();
        let painter = canvas(
            |_, _, _| {},
            move |bounds, _, window, _| {
                let Some(frame) = presented else {
                    return;
                };
                let place = place_frame(
                    frame.width as f32,
                    frame.height as f32,
                    f32::from(bounds.size.width),
                    f32::from(bounds.size.height),
                    fit,
                );
                if place.width <= 0.0 || place.height <= 0.0 {
                    return;
                }
                let target = Bounds {
                    origin: point(bounds.origin.x + px(place.x), bounds.origin.y + px(place.y)),
                    size: size(px(place.width), px(place.height)),
                };
                let id = frame.image.id.0;
                let began = Instant::now();
                let _ = window.paint_image(target, Corners::default(), frame.image, 0, false);
                let mut stats = upload.lock().unwrap_or_else(|poison| poison.into_inner());
                if stats.last_id != Some(id) {
                    let elapsed = began.elapsed();
                    stats.last_id = Some(id);
                    stats.last = elapsed;
                    stats.count += 1;
                    stats.total += elapsed;
                }
            },
        )
        .size_full();

        div()
            .size_full()
            .overflow_hidden()
            .bg(theme.colors.surface.canvas)
            .child(painter)
            .when(self.presented.is_none(), |parent| {
                parent.child(
                    div()
                        .size_full()
                        .flex()
                        .items_center()
                        .justify_center()
                        .child(Text::new(label).role(TextRole::Body).tone(TextTone::Muted)),
                )
            })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn solid(width: u32, height: u32, generation: u64, byte: u8) -> BgraFrame {
        let len = (width * height * 4) as usize;
        BgraFrame::new(width, height, width * 4, vec![byte; len], generation).unwrap()
    }

    #[test]
    fn rejects_bad_dimensions_and_stride() {
        assert_eq!(
            BgraFrame::new(0, 2, 0, vec![], 0).unwrap_err(),
            FrameError::Empty
        );
        assert!(matches!(
            BgraFrame::new(2, 2, 4, vec![0; 16], 0),
            Err(FrameError::StrideTooSmall { minimum: 8, .. })
        ));
        assert!(matches!(
            BgraFrame::new(2, 2, 8, vec![0; 8], 0),
            Err(FrameError::BufferTooSmall { required: 16, .. })
        ));
    }

    #[test]
    fn packs_stride_padding_away() {
        let mut pixels = vec![0u8; 12];
        pixels[0..4].copy_from_slice(&[1, 2, 3, 4]);
        pixels[6..10].copy_from_slice(&[5, 6, 7, 8]);
        let frame = BgraFrame::new(1, 2, 6, pixels, 9).unwrap();
        let packed = pack_bgra(frame).unwrap();
        assert_eq!(packed.pixels, vec![1, 2, 3, 4, 5, 6, 7, 8]);
        assert_eq!(packed.generation, 9);
    }

    #[test]
    fn latest_frame_wins_and_depth_stays_one() {
        let mailbox = FrameMailbox::new();
        mailbox.publish(solid(2, 2, 1, 1)).unwrap();
        mailbox.publish(solid(4, 2, 2, 9)).unwrap();
        let stats = mailbox.stats();
        assert_eq!(stats.published, 2);
        assert_eq!(stats.replaced, 1);
        assert_eq!(stats.depth, 1);
        let pending = mailbox.take().unwrap();
        assert_eq!(pending.generation, 2);
        assert_eq!(pending.width, 4);
        assert_eq!(mailbox.stats().depth, 0);
        assert!(mailbox.take().is_none());
    }

    #[test]
    fn dropping_an_unpublished_frame_does_not_panic() {
        let mailbox = FrameMailbox::new();
        mailbox.publish(solid(2, 2, 1, 3)).unwrap();
        drop(mailbox);
    }

    #[test]
    fn fit_letterboxes_and_fill_covers() {
        let fit = place_frame(1920.0, 1080.0, 1000.0, 1000.0, ImageFit::Fit);
        assert!((fit.width - 1000.0).abs() < 0.01);
        assert!((fit.height - 562.5).abs() < 0.01);
        assert!((fit.x).abs() < 0.01);
        assert!(fit.y > 200.0);

        let square = place_frame(1000.0, 1000.0, 1920.0, 1080.0, ImageFit::Fit);
        assert!((square.height - 1080.0).abs() < 0.01);
        assert!(square.x > 0.0);

        let fill = place_frame(1920.0, 1080.0, 1000.0, 500.0, ImageFit::Fill);
        assert!(fill.height > 500.0);
        assert!(fill.y < 0.0);

        let empty = place_frame(0.0, 10.0, 10.0, 10.0, ImageFit::Fit);
        assert_eq!(empty.width, 0.0);
    }
}
