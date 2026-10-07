//! Media artwork pieces shared by Home and Details.
//!
//! These are Matinee presentation, not framework components: they know the
//! app's [`Artwork`] slot states and Matinee's quiet progress line. Generic
//! interaction (focus, activation, scrolling) stays in Atelier.

use atelier_ui::prelude::*;
use matinee_core::{ImageRole, ItemId, MediaItem};
use matinee_jellyfin::{ArtworkRequest, ArtworkUrls};

use crate::artwork::{Artwork, BACKDROP_WIDTH};

/// A small stable number for element ids derived from an item id.
pub(crate) fn stable_index(id: &ItemId) -> usize {
    id.as_str().bytes().fold(0usize, |hash, byte| {
        hash.wrapping_mul(31).wrapping_add(byte as usize)
    })
}

/// A quiet progress line: amber over a faint track.
pub(crate) fn progress_line(theme: &Theme, fraction: f32, width: f32) -> impl IntoElement {
    let fraction = fraction.clamp(0.0, 1.0);
    div()
        .w(px(width))
        .h(px(3.0))
        .rounded(px(theme.radius.get(Radius::Full)))
        .bg(theme.colors.text.primary.with_alpha(0.18))
        .child(
            div()
                .h_full()
                .w(px(width * fraction))
                .rounded(px(theme.radius.get(Radius::Full)))
                .bg(theme.colors.control.accent),
        )
}

/// Artwork in a fixed frame. Loading and missing art are calm surfaces;
/// missing art carries the title so the frame still says something.
pub(crate) fn art_frame(
    theme: &Theme,
    id: impl Into<ElementId>,
    art: &Artwork,
    width: f32,
    height: f32,
    radius: Radius,
    title: &str,
) -> AnyElement {
    match art {
        Artwork::Ready(image) => Image::decoded(id, image.clone())
            .frame(width, height)
            .fit(ImageFit::Fill)
            .radius(radius)
            .label(title.to_string())
            .into_any_element(),
        Artwork::Loading => div()
            .id(id)
            .flex_none()
            .w(px(width))
            .h(px(height))
            .rounded(px(theme.radius.get(radius)))
            .bg(theme.colors.surface.elevated)
            .into_any_element(),
        Artwork::Missing | Artwork::Failed => div()
            .id(id)
            .flex_none()
            .w(px(width))
            .h(px(height))
            .p(Space::S3.px())
            .flex()
            .items_end()
            .rounded(px(theme.radius.get(radius)))
            .bg(theme.colors.surface.elevated)
            .border(px(1.0))
            .border_color(theme.colors.border.subtle)
            .child(
                Text::new(title.to_string())
                    .role(TextRole::Caption)
                    .tone(TextTone::Muted),
            )
            .into_any_element(),
    }
}

/// Item backdrop, or the series backdrop for an episode or season.
pub(crate) fn backdrop_request(item: &MediaItem, urls: &ArtworkUrls<'_>) -> Option<ArtworkRequest> {
    urls.item_request(item, ImageRole::Backdrop, BACKDROP_WIDTH)
        .or_else(|| {
            let series = item.hierarchy.series_id.as_ref()?;
            Some(urls.image_request(series, ImageRole::Backdrop, BACKDROP_WIDTH))
        })
}
