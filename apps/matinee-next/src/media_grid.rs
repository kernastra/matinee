//! Matinee's media grid: the poster card, its measurements, the artwork
//! window, and the placeholders for a first page.
//!
//! Library and Search paint titles the same way, so a title looks the same
//! wherever it is found. The grid itself is Atelier's `VirtualGrid`; this
//! module decides only what each cell shows. Each screen passes its own
//! [`MediaIds`], so element ids stay unique per screen.

use std::collections::HashSet;
use std::ops::Range;

use atelier_ui::prelude::*;
use matinee_core::{ImageRole, MediaItem};
use matinee_jellyfin::{ArtworkRequest, ArtworkUrls};

use crate::artwork::{Artwork, TILE_POSTER_WIDTH};
use crate::details::progress_fraction;
use crate::tiles::{art_frame, progress_line, stable_index};

/// Element ids for one screen's cards. Each screen uses its own, so ids stay
/// unique within the window; the names are static because `ElementId` takes
/// static strings.
#[derive(Clone, Copy, Debug)]
pub(crate) struct MediaIds {
    pub card: &'static str,
    pub art: &'static str,
    pub skeleton: &'static str,
}

pub(crate) const LIBRARY_IDS: MediaIds = MediaIds {
    card: "library-card",
    art: "library-art",
    skeleton: "library-skeleton",
};

pub(crate) const SEARCH_IDS: MediaIds = MediaIds {
    card: "search-card",
    art: "search-art",
    skeleton: "search-skeleton",
};

/// Poster requests for the built cards, in grid order. Never more than the
/// window; titles without art have no request.
pub(crate) fn artwork_window(
    items: &[MediaItem],
    built: Range<usize>,
    urls: &ArtworkUrls<'_>,
) -> Vec<ArtworkRequest> {
    let end = built.end.min(items.len());
    let start = built.start.min(end);
    let mut seen = HashSet::new();
    items[start..end]
        .iter()
        .filter_map(|item| poster_request(item, urls))
        .filter(|request| seen.insert(request.url.clone()))
        .collect()
}

/// A card's poster. The same address Home's poster cards and Details'
/// related titles use, so the shared cache serves all three.
pub(crate) fn poster_request(item: &MediaItem, urls: &ArtworkUrls<'_>) -> Option<ArtworkRequest> {
    urls.item_request(item, ImageRole::Primary, TILE_POSTER_WIDTH)
}

/// Measurements that follow the window width.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct Layout {
    pub gutter: Space,
}

impl Layout {
    pub(crate) fn for_width(width: f32) -> Self {
        Self {
            gutter: if width < 1100.0 {
                Space::S10
            } else {
                Space::S16
            },
        }
    }

    /// Cards are 148 to 180 points wide: as many columns as fit, then
    /// widened. 180 points is the widest a 360-pixel poster covers on a 2x
    /// display. The caption below the poster is two lines.
    pub(crate) fn sizing(self) -> GridSizing {
        GridSizing {
            min_cell_width: 148.0,
            max_cell_width: 180.0,
            aspect: 1.5,
            extra_height: Space::S12.value(),
            column_gap: Space::S5.value(),
            row_gap: Space::S6.value(),
            inset_x: self.gutter.value(),
            inset_top: Space::S2.value(),
            inset_bottom: Space::S10.value(),
            max_columns: 10,
            overscan_rows: 1,
        }
    }
}

/// What one card shows, gathered before the grid asks for it.
#[derive(Clone)]
pub(crate) struct CardData {
    key: usize,
    title: String,
    detail: Option<String>,
    art: Artwork,
    progress: Option<f32>,
    watched: bool,
}

impl CardData {
    pub(crate) fn new(item: &MediaItem, art: Artwork) -> Self {
        let progress = item
            .is_resumable()
            .then(|| progress_fraction(item))
            .flatten();
        let mut detail = Vec::new();
        if let Some(year) = item.metadata.year {
            detail.push(year.to_string());
        }
        if let Some(rating) = item.metadata.community_rating.filter(|r| *r > 0.0) {
            detail.push(format!("★ {rating:.1}"));
        }
        Self {
            key: stable_index(item.id()),
            title: item.name().to_string(),
            detail: (!detail.is_empty()).then(|| detail.join(" · ")),
            art,
            progress,
            watched: item.user.is_played() && progress.is_none(),
        }
    }
}

/// One card: the poster, a quiet progress line or watched mark, the title,
/// and `2019 · ★ 7.8`. Focus is the grid's; the ring is drawn here.
pub(crate) fn card(
    theme: &Theme,
    ids: MediaIds,
    data: &CardData,
    width: f32,
    art_height: f32,
    focused: bool,
) -> AnyElement {
    let radius = theme.radius.get(Radius::Medium);
    v_stack(Space::S2)
        .id((ids.card, data.key))
        .w(px(width))
        .child(
            div()
                .relative()
                .rounded(px(radius))
                .child(art_frame(
                    theme,
                    (ids.art, data.key),
                    &data.art,
                    width,
                    art_height,
                    Radius::Medium,
                    &data.title,
                ))
                .when_some(data.progress, |frame, fraction| {
                    frame.child(
                        div()
                            .absolute()
                            .left(Space::S2.px())
                            .right(Space::S2.px())
                            .bottom(Space::S2.px())
                            .child(progress_line(theme, fraction, width - 16.0)),
                    )
                })
                .when(focused, |frame| frame.child(FocusRing::new(radius, 0.0))),
        )
        .child(
            Text::new(data.title.clone())
                .role(TextRole::Label)
                .truncate(),
        )
        .child(
            h_stack(Space::S1)
                .items_center()
                .when_some(data.detail.clone(), |row, detail| {
                    row.child(
                        Text::new(detail)
                            .role(TextRole::Caption)
                            .tone(TextTone::Muted)
                            .truncate(),
                    )
                })
                .when(data.watched, |row| {
                    row.child(
                        Icon::new(IconName::Check)
                            .size(IconSize::Small)
                            .color(theme.colors.text.muted),
                    )
                }),
        )
        .hover(|style| style.opacity(0.92))
        .into_any_element()
}

pub(crate) fn quiet(theme: &Theme, text: &str) -> impl IntoElement {
    Text::new(text.to_string())
        .role(TextRole::Caption)
        .color(theme.colors.text.muted)
}

/// Card-shaped placeholders for the first page: two rows at the real size,
/// with the header already in place above. No shimmer.
pub(crate) fn skeleton(
    theme: &Theme,
    ids: MediaIds,
    layout: Layout,
    width: f32,
) -> impl IntoElement {
    let grid = GridLayout::new(layout.sizing(), width, 0, 0.0);
    let (w, h) = (grid.cell_width, grid.cell_width * 1.5);
    let row = |row: usize| {
        h_stack(Space::S5)
            .items_start()
            .children((0..grid.columns).map(move |column| {
                v_stack(Space::S2)
                    .id((ids.skeleton, row * 16 + column))
                    .flex_none()
                    .child(
                        div()
                            .w(px(w))
                            .h(px(h))
                            .rounded(px(theme.radius.get(Radius::Medium)))
                            .bg(theme.colors.surface.panel),
                    )
                    .child(
                        div()
                            .w(px(w * 0.6))
                            .h(px(10.0))
                            .rounded(px(theme.radius.get(Radius::Small)))
                            .bg(theme.colors.surface.elevated),
                    )
            }))
    };
    v_stack(Space::S6)
        .px(layout.gutter.px())
        .pt(Space::S2.px())
        .overflow_hidden()
        .size_full()
        .child(row(0))
        .child(row(1))
}
