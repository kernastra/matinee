//! Review scenes. Generated fixture titles and the abstract review artwork;
//! no server, no socket, no real film frames.

use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use atelier_ui::prelude::*;
use matinee_core::{
    ImageTag, ItemId, ItemIdentity, ItemKind, LibraryContent, LibraryGenre, LibraryId, LibraryKind,
    LibraryPage, LibraryView, MediaItem, UserItemState, WatchFilter,
};

use super::model::{LibraryFailure, LibraryModel, Request, Response};
use super::screen::LibraryScreen;
use crate::artwork::{Artwork, ArtworkLoader};
use crate::runtime::ServiceRuntime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum LibraryPreview {
    /// Movies across two movie libraries, a genre menu, a full first page.
    Movies,
    Series,
    /// The header in place, placeholders where the first page will go.
    Loading,
    /// The library has no titles.
    Empty,
    /// Unwatched titles in one genre: none.
    FilteredEmpty,
    /// Three pages in, the fourth failed: Try again under the grid.
    PartialPage,
    /// The first page failed.
    Error,
    /// A 10,000-title library, six pages loaded, scrolled deep, keyboard
    /// focus on a card.
    ManyItems,
}

const POSTER: &[u8] = include_bytes!("../../assets/review/poster.jpg");
const POSTER_SERIES: &[u8] = include_bytes!("../../assets/review/poster-series.jpg");

const WORDS: [&str; 24] = [
    "Northwind",
    "Tidewater",
    "Signal",
    "Lantern",
    "Harbor",
    "Orchard",
    "Copper",
    "Glasshouse",
    "Paper",
    "Moon",
    "Ferry",
    "Winter",
    "Keeper",
    "Salt",
    "Road",
    "Night",
    "Market",
    "Bellwether",
    "Quiet",
    "Hours",
    "Cartographer",
    "Ember",
    "Meridian",
    "Hollow",
];

fn id(value: &str) -> ItemId {
    ItemId::parse(value).expect("fixture id")
}

/// The `index`-th fixture title of `kind`. Deterministic: names, years,
/// ratings, progress, and watched marks follow the index.
pub(crate) fn fixture(kind: LibraryKind, index: usize) -> MediaItem {
    let first = WORDS[index % WORDS.len()];
    let second = WORDS[(index * 7 + 3) % WORDS.len()];
    let name = match index {
        // One title long enough to truncate.
        4 => "The Extraordinarily Long and Winding Account of the Cartographer's Last Northern Voyage".to_string(),
        _ if index.is_multiple_of(3) => format!("{first} {second}"),
        _ => format!("The {first}"),
    };
    let movie = kind == LibraryKind::Movies;
    let prefix = if movie { "film" } else { "show" };
    let mut item = MediaItem {
        identity: ItemIdentity {
            id: id(&format!("{prefix}-{index}")),
            name,
        },
        kind: if movie {
            ItemKind::Movie
        } else {
            ItemKind::Series
        },
        metadata: Default::default(),
        artwork: Default::default(),
        user: UserItemState::default(),
        hierarchy: Default::default(),
        media: Default::default(),
        people: Vec::new(),
        chapters: Vec::new(),
    };
    item.metadata.year = Some(1968 + (index * 13 % 57) as i32);
    item.metadata.community_rating = Some(5.5 + (index * 17 % 40) as f64 / 10.0);
    item.metadata.runtime = movie.then(|| Duration::from_secs((88 + index as u64 % 60) * 60));
    // Every ninth title has no poster, to review the placeholder.
    if index % 9 != 8 {
        item.artwork.primary = ImageTag::parse(format!("poster-{}", index % 2));
    }
    item.user = match index % 11 {
        2 => UserItemState::from_parts(
            Some(Duration::from_secs(40 * 60)),
            Some(35.0),
            false,
            false,
            0,
        ),
        5 | 7 => UserItemState::from_parts(None, None, false, true, 1),
        _ => UserItemState::default(),
    };
    item
}

fn page(kind: LibraryKind, request: &Request, total: usize) -> Response {
    let Request::Page { page, .. } = request else {
        unreachable!("not a page")
    };
    let end = (page.start + page.limit).min(total);
    Response::Page(Ok(LibraryPage {
        items: (page.start.min(end)..end)
            .map(|index| fixture(kind, index))
            .collect(),
        start: page.start,
        total: Some(total),
    }))
}

fn views() -> Vec<LibraryView> {
    let view = |value: &str, name: &str, content| LibraryView {
        id: LibraryId::parse(value).expect("fixture id"),
        name: name.into(),
        content,
    };
    vec![
        view("films", "Films", LibraryContent::Movies),
        view("family", "Family Films", LibraryContent::Movies),
        view("television", "Television", LibraryContent::Series),
    ]
}

fn genres() -> Vec<LibraryGenre> {
    [
        "Adventure",
        "Animation",
        "Comedy",
        "Documentary",
        "Drama",
        "Fantasy",
        "History",
        "Mystery",
        "Romance",
        "Science Fiction",
        "Thriller",
        "Western",
    ]
    .iter()
    .enumerate()
    .map(|(index, name)| LibraryGenre {
        id: id(&format!("genre-{index}")),
        name: (*name).into(),
    })
    .collect()
}

/// How a fixture server holding `total` titles answers one request. The
/// page's kind comes from the request.
fn respond(request: &Request, total: usize) -> Response {
    match request {
        Request::Page { kind, .. } => page(*kind, request, total),
        Request::Views { .. } => Response::Views(Ok(views())),
        Request::Genres { .. } => Response::Genres(Ok(genres())),
        Request::Item { kind, id, .. } => {
            let index = id
                .as_str()
                .rsplit('-')
                .next()
                .and_then(|n| n.parse().ok())
                .unwrap_or(0);
            Response::Item(Ok(Box::new(fixture(*kind, index))))
        }
    }
}

/// Answer `requests` the way a server holding `total` titles would.
fn answer(model: &mut LibraryModel, _kind: LibraryKind, requests: Vec<Request>, total: usize) {
    for request in requests {
        let response = respond(&request, total);
        model.apply(&request, response);
    }
}

/// How the scene's server keeps answering once the screen is up: as the
/// scene was built (the loading scene never answers a page; the error
/// scene keeps failing).
fn scene_answers(scene: LibraryPreview) -> impl Fn(&Request) -> Option<Response> {
    move |request| match (scene, request) {
        (LibraryPreview::Loading, Request::Page { .. }) => None,
        (LibraryPreview::Error, Request::Page { page, .. }) if page.start == 0 => {
            Some(Response::Page(Err(LibraryFailure::Unreachable)))
        }
        (LibraryPreview::Empty, _) => Some(respond(request, 0)),
        (LibraryPreview::ManyItems, _) => Some(respond(request, 10_000)),
        _ => Some(respond(request, 1_284)),
    }
}

pub(crate) fn preview_model(scene: LibraryPreview) -> LibraryModel {
    let kind = match scene {
        LibraryPreview::Series => LibraryKind::Series,
        _ => LibraryKind::Movies,
    };
    let (mut model, requests) = LibraryModel::open(kind);
    match scene {
        LibraryPreview::Loading => {
            let (meta, _): (Vec<Request>, Vec<Request>) = requests
                .into_iter()
                .partition(|request| !matches!(request, Request::Page { .. }));
            answer(&mut model, kind, meta, 0);
        }
        LibraryPreview::Empty => answer(&mut model, kind, requests, 0),
        LibraryPreview::FilteredEmpty => {
            answer(&mut model, kind, requests, 240);
            let genre = model.set_genre(Some(id("genre-11")));
            answer(&mut model, kind, genre, 240);
            let watch = model.set_watch(WatchFilter::Unwatched);
            answer(&mut model, kind, watch, 0);
        }
        LibraryPreview::Error => {
            for request in requests {
                let response = match &request {
                    Request::Page { .. } => Response::Page(Err(LibraryFailure::Unreachable)),
                    Request::Views { .. } => Response::Views(Ok(views())),
                    _ => Response::Genres(Ok(genres())),
                };
                model.apply(&request, response);
            }
        }
        LibraryPreview::PartialPage => {
            answer(&mut model, kind, requests, 1_284);
            for _ in 0..2 {
                let last = model.items().len() - 1;
                let next = model.want_more(last).into_iter().collect();
                answer(&mut model, kind, next, 1_284);
            }
            let last = model.items().len() - 1;
            if let Some(next) = model.want_more(last) {
                model.apply(&next, Response::Page(Err(LibraryFailure::Unreachable)));
            }
        }
        LibraryPreview::ManyItems => {
            answer(&mut model, kind, requests, 10_000);
            for _ in 0..5 {
                let last = model.items().len() - 1;
                let next = model.want_more(last).into_iter().collect();
                answer(&mut model, kind, next, 10_000);
            }
        }
        LibraryPreview::Movies | LibraryPreview::Series => {
            answer(&mut model, kind, requests, 1_284);
        }
    }
    model
}

impl LibraryScreen {
    pub(crate) fn preview(
        runtime: Arc<ServiceRuntime>,
        loader: ArtworkLoader,
        scene: LibraryPreview,
        cx: &mut Context<Self>,
    ) -> Self {
        let model = preview_model(scene);
        let mut screen = Self::with_model(
            runtime,
            None,
            crate::model::review_session(),
            loader,
            model,
            cx,
        );
        let posters: Vec<Artwork> = [POSTER, POSTER_SERIES]
            .iter()
            .map(|bytes| {
                DecodedImage::decode(bytes, MAX_DECODED_SIDE)
                    .map(Artwork::Ready)
                    .unwrap_or(Artwork::Missing)
            })
            .collect();
        screen.fixture_art = Some(Box::new(move |url: &str| {
            if url.contains("tag=poster-1") {
                posters[1].clone()
            } else {
                posters[0].clone()
            }
        }));
        screen.fixture_answers = Some(Rc::new(scene_answers(scene)));
        if scene == LibraryPreview::ManyItems {
            note_keyboard_navigation(cx);
            let grid = screen.grid(LibraryKind::Movies);
            grid.focus_index(Some(431));
        }
        screen
    }
}

/// A model holding every page of a `total`-title library, for tests.
#[cfg(test)]
pub(crate) fn loaded_model(kind: LibraryKind, total: usize) -> LibraryModel {
    let (mut model, requests) = LibraryModel::open(kind);
    answer(&mut model, kind, requests, total);
    while model.items().len() < total {
        let last = model.items().len() - 1;
        let next: Vec<Request> = model.want_more(last).into_iter().collect();
        assert!(!next.is_empty());
        answer(&mut model, kind, next, total);
    }
    model
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::library::model::{CatalogState, EmptyReason, Footer};

    #[test]
    fn review_scenes_cover_the_states_they_name() {
        let movies = preview_model(LibraryPreview::Movies);
        assert_eq!(movies.state(), CatalogState::Ready);
        assert_eq!(movies.items().len(), 100);
        assert_eq!(movies.views().len(), 2, "two movie libraries: a menu");
        assert_eq!(
            preview_model(LibraryPreview::Series).kind(),
            LibraryKind::Series
        );
        assert_eq!(
            preview_model(LibraryPreview::Loading).state(),
            CatalogState::Loading
        );
        assert_eq!(
            preview_model(LibraryPreview::Empty).state(),
            CatalogState::Empty(EmptyReason::Library)
        );
        assert_eq!(
            preview_model(LibraryPreview::FilteredEmpty).state(),
            CatalogState::Empty(EmptyReason::Filtered)
        );
        let partial = preview_model(LibraryPreview::PartialPage);
        assert_eq!(partial.items().len(), 300);
        assert_eq!(
            partial.footer(),
            Footer::Failed(LibraryFailure::Unreachable)
        );
        assert_eq!(
            preview_model(LibraryPreview::Error).state(),
            CatalogState::Failed(LibraryFailure::Unreachable)
        );
        let many = preview_model(LibraryPreview::ManyItems);
        assert_eq!(many.items().len(), 600);
        assert_eq!(many.total(), Some(10_000));
    }

    #[test]
    fn fixtures_are_deterministic_and_include_the_awkward_cases() {
        assert_eq!(
            fixture(LibraryKind::Movies, 3),
            fixture(LibraryKind::Movies, 3)
        );
        assert!(
            fixture(LibraryKind::Movies, 4).name().len() > 60,
            "a long title"
        );
        assert!(fixture(LibraryKind::Movies, 8).artwork.primary.is_none());
        assert!(fixture(LibraryKind::Movies, 2).is_resumable());
        assert!(fixture(LibraryKind::Movies, 5).user.is_played());
    }
}
