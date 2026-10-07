//! Review scenes. Fixture metadata and generated abstract artwork; no
//! server, no socket, no real film frames.

use std::sync::Arc;
use std::time::Duration;

use atelier_ui::prelude::*;
use matinee_core::{
    HomeShelf, ImageTag, ItemHierarchy, ItemId, ItemIdentity, ItemKind, ItemMetadata, MediaItem,
    TechnicalMedia, UserItemState,
};

use super::model::{HomeFailure, HomeModel, Response};
use super::screen::HomeScreen;
use crate::artwork::{Artwork, ArtworkLoader};
use crate::runtime::ServiceRuntime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum HomePreview {
    /// A full Home; the hero resumes a movie.
    Home,
    /// Keyboard focus on the first Continue Watching card.
    ContinueWatching,
    Empty,
    /// Next Up and Favorites failed; the rest of Home stands.
    PartialError,
    Loading,
    /// Nothing in progress: the hero features a new movie.
    HeroFresh,
}

const BACKDROP: &[u8] = include_bytes!("../../assets/review/backdrop.jpg");
const BACKDROP_SERIES: &[u8] = include_bytes!("../../assets/review/backdrop-series.jpg");
const POSTER: &[u8] = include_bytes!("../../assets/review/poster.jpg");
const POSTER_SERIES: &[u8] = include_bytes!("../../assets/review/poster-series.jpg");
const THUMBS: [&[u8]; 3] = [
    include_bytes!("../../assets/review/thumb-1.jpg"),
    include_bytes!("../../assets/review/thumb-2.jpg"),
    include_bytes!("../../assets/review/thumb-3.jpg"),
];

impl HomeScreen {
    pub(crate) fn preview(
        runtime: Arc<ServiceRuntime>,
        loader: ArtworkLoader,
        scene: HomePreview,
        cx: &mut Context<Self>,
    ) -> Self {
        let mut model = preview_model(scene);
        if scene == HomePreview::ContinueWatching {
            model.note_opened(HomeShelf::ContinueWatching, id("northwind"));
            note_keyboard_navigation(cx);
        }
        let mut screen = Self::with_model(
            runtime,
            None,
            crate::model::review_session(),
            loader,
            model,
            cx,
        );
        if scene == HomePreview::ContinueWatching {
            screen.page.reveal_child(1);
        }
        for (index, request) in screen.wanted_artwork().into_iter().enumerate() {
            let url = request.url.as_str();
            let bytes = if url.contains("/Images/Backdrop") && url.contains("maxWidth=1920") {
                Some(if url.contains("series") || url.contains("harbor") {
                    BACKDROP_SERIES
                } else {
                    BACKDROP
                })
            } else if url.contains("maxWidth=480") {
                Some(THUMBS[index % THUMBS.len()])
            } else {
                // Every seventh poster has no art, to review the placeholder.
                match index % 7 {
                    6 => None,
                    n if n % 2 == 0 => Some(POSTER),
                    _ => Some(POSTER_SERIES),
                }
            };
            let art = bytes
                .and_then(|bytes| DecodedImage::decode(bytes, MAX_DECODED_SIDE).ok())
                .map(Artwork::Ready)
                .unwrap_or(Artwork::Missing);
            screen.art.insert(request.url, art);
        }
        screen
    }
}

fn id(value: &str) -> ItemId {
    ItemId::parse(value).expect("fixture id")
}

fn tag(value: &str) -> Option<ImageTag> {
    ImageTag::parse(value)
}

fn base(value: &str, name: &str, kind: ItemKind) -> MediaItem {
    MediaItem {
        identity: ItemIdentity {
            id: id(value),
            name: name.into(),
        },
        kind,
        metadata: ItemMetadata::default(),
        artwork: Default::default(),
        user: UserItemState::default(),
        hierarchy: ItemHierarchy::default(),
        media: TechnicalMedia::default(),
        people: Vec::new(),
        chapters: Vec::new(),
    }
}

fn title(value: &str, name: &str, kind: ItemKind, year: i32, minutes: u64) -> MediaItem {
    let movie = kind == ItemKind::Movie;
    let mut item = base(value, name, kind);
    item.metadata.year = Some(year);
    item.metadata.runtime = movie.then(|| Duration::from_secs(minutes * 60));
    item.artwork.primary = tag("poster");
    item.artwork.backdrops = tag("backdrop").into_iter().collect();
    item
}

fn resumed(mut item: MediaItem, minutes: u64, percent: f32) -> MediaItem {
    item.user = UserItemState::from_parts(
        Some(Duration::from_secs(minutes * 60)),
        Some(percent),
        false,
        false,
        0,
    );
    item
}

fn episode(series: &str, series_name: &str, season: u32, index: u32, name: &str) -> MediaItem {
    let mut episode = base(
        &format!("{series}-s{season}e{index}"),
        name,
        ItemKind::Episode,
    );
    episode.metadata.runtime = Some(Duration::from_secs(46 * 60));
    episode.hierarchy = ItemHierarchy {
        series_id: Some(id(series)),
        series_name: Some(series_name.into()),
        index: Some(index),
        parent_index: Some(season),
        ..ItemHierarchy::default()
    };
    episode.artwork.primary = tag("still");
    episode
}

fn northwind() -> MediaItem {
    let mut movie = title("northwind", "Northwind", ItemKind::Movie, 2019, 124);
    movie.metadata.overview = Some("A lighthouse keeper on a failing northern island finds a ship's log that predicts the next storm to the hour. As the village prepares, she has to decide how much of the rest of the log she is willing to believe.".into());
    movie.metadata.official_rating = Some("PG-13".into());
    movie.metadata.genres = vec!["Drama".into(), "Mystery".into()];
    resumed(movie, 42, 34.0)
}

fn continue_watching() -> Vec<MediaItem> {
    vec![
        northwind(),
        resumed(
            episode("harbor-lights", "Harbor Lights", 2, 5, "Keeper"),
            18,
            40.0,
        ),
        resumed(
            title("salt-road", "The Salt Road", ItemKind::Movie, 2020, 131),
            70,
            53.0,
        ),
        resumed(
            episode("quiet-hours", "Quiet Hours", 1, 3, "The Night Shift"),
            9,
            20.0,
        ),
    ]
}

fn next_up() -> Vec<MediaItem> {
    vec![
        episode("lantern-bay", "Lantern Bay", 3, 1, "Return Crossing"),
        episode("quiet-hours", "Quiet Hours", 1, 4, "Static"),
        episode("orchard", "The Orchard", 2, 2, "Frost Warning"),
        episode("cartographers", "Cartographers", 1, 7, "Blank Spaces"),
    ]
}

fn movies() -> Vec<MediaItem> {
    [
        ("tidewater", "Tidewater", 2017),
        ("long-winter", "Long Winter", 2015),
        ("signal", "Signal Fires", 2022),
        ("keeper", "The Keeper's Daughter", 2018),
        ("ferry", "Last Ferry", 2016),
        ("southwind", "Southwind", 2021),
        ("eastwind", "Eastwind", 2023),
        ("paper-moons", "Paper Moons", 2014),
        ("copper", "Copper Hill", 2020),
        ("glasshouse", "Glasshouse", 2024),
    ]
    .iter()
    .enumerate()
    .map(|(index, (value, name, year))| {
        let mut movie = title(value, name, ItemKind::Movie, *year, 98 + index as u64 * 7);
        if index == 3 {
            movie.user = UserItemState::from_parts(None, None, false, true, 1);
        }
        movie
    })
    .collect()
}

fn series() -> Vec<MediaItem> {
    [
        ("harbor-lights", "Harbor Lights", 2021),
        ("lantern-bay", "Lantern Bay", 2019),
        ("quiet-hours", "Quiet Hours", 2023),
        ("orchard", "The Orchard", 2022),
        ("cartographers", "Cartographers", 2024),
        ("night-market", "Night Market", 2020),
        ("bellwether", "Bellwether", 2018),
    ]
    .iter()
    .map(|(value, name, year)| title(value, name, ItemKind::Series, *year, 0))
    .collect()
}

fn favorites() -> Vec<MediaItem> {
    let mut all = movies();
    all.truncate(3);
    all.extend(series().into_iter().take(2));
    all
}

fn preview_model(scene: HomePreview) -> HomeModel {
    let (mut model, requests) = HomeModel::open();
    if scene == HomePreview::Loading {
        return model;
    }
    for request in requests {
        let response: Response = match (scene, request.shelf) {
            (HomePreview::Empty, _) => Ok(Vec::new()),
            (HomePreview::HeroFresh, HomeShelf::ContinueWatching | HomeShelf::NextUp) => {
                Ok(Vec::new())
            }
            (HomePreview::PartialError, HomeShelf::NextUp) => Err(HomeFailure::Unreachable),
            (HomePreview::PartialError, HomeShelf::Favorites) => Err(HomeFailure::Unreadable),
            (_, HomeShelf::ContinueWatching) => Ok(continue_watching()),
            (_, HomeShelf::NextUp) => Ok(next_up()),
            (_, HomeShelf::RecentMovies) => Ok(movies()),
            (_, HomeShelf::RecentSeries) => Ok(series()),
            (_, HomeShelf::Favorites) => Ok(favorites()),
        };
        model.apply(request, response);
    }
    model
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::home::model::{HeroState, PageState};

    #[test]
    fn review_scenes_cover_the_states_they_name() {
        assert!(matches!(
            preview_model(HomePreview::Home).hero(),
            HeroState::Ready(pick) if pick.item.id().as_str() == "northwind"
        ));
        let fresh = preview_model(HomePreview::HeroFresh);
        let HeroState::Ready(pick) = fresh.hero() else {
            panic!();
        };
        assert_eq!(pick.eyebrow(), "Tonight's feature");
        assert_eq!(preview_model(HomePreview::Empty).page(), PageState::Empty);
        assert_eq!(
            preview_model(HomePreview::Loading).hero(),
            HeroState::Loading
        );
        let partial = preview_model(HomePreview::PartialError);
        assert_eq!(partial.page(), PageState::Content);
        assert!(!partial.items(HomeShelf::ContinueWatching).is_empty());
    }
}
