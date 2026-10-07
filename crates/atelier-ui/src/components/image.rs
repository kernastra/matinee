//! Fixed-frame image.
//!
//! Presents a GPUI image source: an embedded asset path, a decoded image, a
//! pending load, or a failure. There is no network policy here. Callers that
//! fetch bytes themselves decode them with [`DecodedImage::decode`], on any
//! thread, and present the result. The frame is clipped to the chosen radius.
//!
//! # Decoded images
//!
//! A [`DecodedImage`] is CPU pixels until a window paints it; painting
//! uploads it to that window's sprite atlas. GPUI keeps the upload until it
//! is released, so the owner of a decoded image calls
//! [`DecodedImage::release`] when it is done with it, for example when a
//! cache evicts it. Releasing an image that is still painted is safe: the
//! next paint uploads it again.

use std::io::Cursor;
use std::sync::Arc;

use gpui::{
    App, ImageCacheError, InteractiveElement, IntoElement, ObjectFit, ParentElement, RenderImage,
    RenderOnce, SharedString, Styled, StyledImage, Window, div, img, px,
};
use image::{Frame, ImageReader, Limits};

use crate::{
    ActiveTheme,
    components::{Icon, IconName, IconSize, Text},
    tokens::{Radius, Space, TextRole},
};

/// How the source sits inside the frame.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub enum ImageFit {
    /// The whole image stays visible. Empty bands use the placeholder fill.
    #[default]
    Fit,
    /// The frame is filled. Overflow is clipped.
    Fill,
}

/// Largest decoded side, in pixels, unless the caller asks for less.
pub const MAX_DECODED_SIDE: u32 = 4096;

/// Why bytes did not become a [`DecodedImage`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DecodeError {
    /// Not a format this build decodes (JPEG, PNG, WebP).
    Unsupported,
    /// Wider or taller than the limit.
    TooLarge,
    /// The format was recognised and the data was damaged.
    Corrupt,
}

impl std::fmt::Display for DecodeError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            Self::Unsupported => "the image format is not supported",
            Self::TooLarge => "the image is too large",
            Self::Corrupt => "the image data is damaged",
        })
    }
}

impl std::error::Error for DecodeError {}

/// Encoded image bytes turned into pixels GPUI can paint.
#[derive(Clone)]
pub struct DecodedImage {
    image: Arc<RenderImage>,
    width: u32,
    height: u32,
}

impl DecodedImage {
    /// Decode JPEG, PNG, or WebP bytes. Runs on any thread and never touches
    /// a window. Sides over `max_side` (capped at [`MAX_DECODED_SIDE`]) are
    /// refused before the pixels are allocated.
    pub fn decode(bytes: &[u8], max_side: u32) -> Result<Self, DecodeError> {
        let max_side = max_side.min(MAX_DECODED_SIDE);
        let mut reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|_| DecodeError::Corrupt)?;
        if reader.format().is_none() {
            return Err(DecodeError::Unsupported);
        }
        let (width, height) = reader
            .into_dimensions()
            .map_err(|error| decode_error(&error))?;
        if width == 0 || height == 0 {
            return Err(DecodeError::Corrupt);
        }
        if width > max_side || height > max_side {
            return Err(DecodeError::TooLarge);
        }
        reader = ImageReader::new(Cursor::new(bytes))
            .with_guessed_format()
            .map_err(|_| DecodeError::Corrupt)?;
        let mut limits = Limits::default();
        limits.max_image_width = Some(max_side);
        limits.max_image_height = Some(max_side);
        reader.limits(limits);
        let mut pixels = reader
            .decode()
            .map_err(|error| decode_error(&error))?
            .into_rgba8();
        // GPUI paints BGRA.
        for pixel in pixels.chunks_exact_mut(4) {
            pixel.swap(0, 2);
        }
        Ok(Self {
            image: Arc::new(RenderImage::new(vec![Frame::new(pixels)])),
            width,
            height,
        })
    }

    pub fn width(&self) -> u32 {
        self.width
    }

    pub fn height(&self) -> u32 {
        self.height
    }

    /// Bytes of pixel memory, for a caller's cache budget.
    pub fn byte_size(&self) -> usize {
        self.width as usize * self.height as usize * 4
    }

    /// Free the uploaded copy in every window. CPU pixels stay with clones.
    pub fn release(&self, cx: &mut App) {
        cx.drop_image(Arc::clone(&self.image), None);
    }
}

fn decode_error(error: &image::ImageError) -> DecodeError {
    match error {
        image::ImageError::Unsupported(_) => DecodeError::Unsupported,
        image::ImageError::Limits(_) => DecodeError::TooLarge,
        _ => DecodeError::Corrupt,
    }
}

impl PartialEq for DecodedImage {
    fn eq(&self, other: &Self) -> bool {
        Arc::ptr_eq(&self.image, &other.image)
    }
}

impl Eq for DecodedImage {}

impl std::fmt::Debug for DecodedImage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("DecodedImage")
            .field("width", &self.width)
            .field("height", &self.height)
            .finish()
    }
}

/// What the frame should show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageContent {
    /// Path served by the application asset source, including framework samples.
    Asset(SharedString),
    /// Pixels the caller decoded.
    Decoded(DecodedImage),
    /// Stays on the loading placeholder.
    Pending,
    /// Resolves immediately as a failure.
    Failed,
}

/// Built-in sample frames for catalogs and stress stories. Neutral geometry,
/// not product artwork.
pub const SAMPLE_COUNT: usize = 4;

pub fn sample_asset(index: usize) -> SharedString {
    format!("atelier/samples/swatch-{}.png", index % SAMPLE_COUNT).into()
}

/// A sized, clipped image frame with placeholder, loading, and failure states.
#[derive(IntoElement)]
pub struct Image {
    id: gpui::ElementId,
    content: ImageContent,
    width: f32,
    height: f32,
    fit: ImageFit,
    radius: Radius,
    /// Accessible name. Shown in the placeholder and failure states.
    label: SharedString,
}

impl Image {
    pub fn asset(id: impl Into<gpui::ElementId>, path: impl Into<SharedString>) -> Self {
        Self::new(id, ImageContent::Asset(path.into()))
    }

    pub fn decoded(id: impl Into<gpui::ElementId>, image: DecodedImage) -> Self {
        Self::new(id, ImageContent::Decoded(image))
    }

    pub fn pending(id: impl Into<gpui::ElementId>) -> Self {
        Self::new(id, ImageContent::Pending)
    }

    pub fn failed(id: impl Into<gpui::ElementId>) -> Self {
        Self::new(id, ImageContent::Failed)
    }

    pub fn sample(id: impl Into<gpui::ElementId>, index: usize) -> Self {
        Self::asset(id, sample_asset(index))
    }

    fn new(id: impl Into<gpui::ElementId>, content: ImageContent) -> Self {
        Self {
            id: id.into(),
            content,
            width: 96.0,
            height: 72.0,
            fit: ImageFit::Fit,
            radius: Radius::Medium,
            label: SharedString::from("Image"),
        }
    }

    pub fn frame(mut self, width: f32, height: f32) -> Self {
        self.width = width;
        self.height = height;
        self
    }

    pub fn fit(mut self, fit: ImageFit) -> Self {
        self.fit = fit;
        self
    }

    pub fn radius(mut self, radius: Radius) -> Self {
        self.radius = radius;
        self
    }

    pub fn label(mut self, label: impl Into<SharedString>) -> Self {
        self.label = label.into();
        self
    }

    pub fn name(&self) -> &str {
        &self.label
    }
}

impl RenderOnce for Image {
    fn render(self, _window: &mut Window, cx: &mut App) -> impl IntoElement {
        let theme = cx.theme().clone();
        let radius = theme.radius.get(self.radius);
        let label = self.label.clone();
        let pending_label = label.clone();
        let failed_label = label.clone();
        let fit = match self.fit {
            ImageFit::Fit => ObjectFit::Contain,
            ImageFit::Fill => ObjectFit::Cover,
        };
        let source = match self.content {
            ImageContent::Asset(path) => img(path),
            ImageContent::Decoded(decoded) => img(decoded.image),
            ImageContent::Pending => img(pending_source),
            ImageContent::Failed => img(failed_source),
        };

        div()
            .id(self.id)
            .relative()
            .flex_none()
            .w(px(self.width))
            .h(px(self.height))
            .overflow_hidden()
            .rounded(px(radius))
            .bg(theme.colors.surface.panel)
            .child(
                source
                    .id("frame")
                    .size_full()
                    .object_fit(fit)
                    .with_loading({
                        let theme = theme.clone();
                        move || placeholder(&theme, &pending_label, false).into_any_element()
                    })
                    .with_fallback({
                        let theme = theme.clone();
                        move || placeholder(&theme, &failed_label, true).into_any_element()
                    }),
            )
    }
}

fn pending_source(
    _window: &mut Window,
    _cx: &mut App,
) -> Option<Result<std::sync::Arc<gpui::RenderImage>, ImageCacheError>> {
    None
}

fn failed_source(
    _window: &mut Window,
    _cx: &mut App,
) -> Option<Result<std::sync::Arc<gpui::RenderImage>, ImageCacheError>> {
    Some(Err(ImageCacheError::Asset("image failed".into())))
}

fn placeholder(theme: &crate::Theme, label: &SharedString, failed: bool) -> impl IntoElement {
    div()
        .size_full()
        .flex()
        .flex_col()
        .items_center()
        .justify_center()
        .gap(Space::S1.px())
        .bg(theme.colors.surface.elevated)
        .text_color(if failed {
            theme.colors.text.danger
        } else {
            theme.colors.text.muted
        })
        .child(
            Icon::new(if failed {
                IconName::Info
            } else {
                IconName::Folder
            })
            .size(IconSize::Small)
            .color(if failed {
                theme.colors.text.danger
            } else {
                theme.colors.text.muted
            }),
        )
        .child(
            Text::new(label.clone())
                .role(TextRole::Caption)
                .color(theme.colors.text.muted),
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    const SWATCH: &[u8] = include_bytes!("../../assets/samples/swatch-0.png");

    #[test]
    fn png_bytes_decode_to_bgra_pixels() {
        let decoded = DecodedImage::decode(SWATCH, MAX_DECODED_SIDE).unwrap();
        assert!(decoded.width() > 0 && decoded.height() > 0);
        assert_eq!(
            decoded.byte_size(),
            decoded.width() as usize * decoded.height() as usize * 4
        );
        let rgba = image::load_from_memory(SWATCH).unwrap().into_rgba8();
        let bgra = decoded.image.as_bytes(0).unwrap();
        assert_eq!(
            &bgra[..4],
            &[
                rgba[(0, 0)][2],
                rgba[(0, 0)][1],
                rgba[(0, 0)][0],
                rgba[(0, 0)][3]
            ]
        );
        assert_eq!(decoded.clone(), decoded, "a clone is the same image");
    }

    #[test]
    fn bad_bytes_are_typed_failures() {
        assert_eq!(
            DecodedImage::decode(b"not an image at all", MAX_DECODED_SIDE).unwrap_err(),
            DecodeError::Unsupported
        );
        assert_eq!(
            DecodedImage::decode(&SWATCH[..SWATCH.len() / 2], MAX_DECODED_SIDE).unwrap_err(),
            DecodeError::Corrupt
        );
        assert_eq!(
            DecodedImage::decode(SWATCH, 2).unwrap_err(),
            DecodeError::TooLarge,
            "the side limit applies before pixels are allocated"
        );
    }
}
