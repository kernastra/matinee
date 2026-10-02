//! Fixed-frame image.
//!
//! Presents a GPUI image source: an embedded asset path, a pending load, or
//! a failure. There is no network policy here. A `http` URL would use GPUI's
//! own loader; callers that need a policy should resolve bytes first and pass
//! an asset path. The frame is clipped to the chosen radius.

use gpui::{
    App, ImageCacheError, InteractiveElement, IntoElement, ObjectFit, ParentElement, RenderOnce,
    SharedString, Styled, StyledImage, Window, div, img, px,
};

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

/// What the frame should show.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum ImageContent {
    /// Path served by the application asset source, including framework samples.
    Asset(SharedString),
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
