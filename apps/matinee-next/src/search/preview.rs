//! Review scenes for Search. Generated titles and episodes and the abstract
//! review artwork; no server, no socket, no real media, no real queries.
//!
//! The fixture server answers any query: the titles it returns follow from
//! the query text, so two queries never share titles and a replaced query's
//! posters can be told apart from the new one's.

use std::rc::Rc;
use std::sync::Arc;

use atelier_ui::prelude::*;
use matinee_core::{
    ImageTag, ItemId, ItemKind, LibraryKind, LibraryPage, MediaItem, SearchQuery, UserItemState,
};

use super::model::{Request, Response, SearchFailure, SearchModel};
use super::screen::SearchScreen;
use crate::artwork::{Artwork, ArtworkLoader};
use crate::library::preview::fixture;
use crate::runtime::ServiceRuntime;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum SearchPreview {
    /// Nothing typed: the calm start.
    Empty,
    /// One letter: too short to search.
    TooShort,
    /// Results for "harbor" are held; the field reads "harbor li" and its
    /// debounce has not ended, so nothing new is sent yet.
    Typing,
    /// A first search whose first page has not arrived.
    Loading,
    /// A full first page of a 240-title result.
    Results,
    /// 10,000 matches, five pages loaded, keyboard focus on a card far down.
    ManyResults,
    /// A search that matched nothing.
    NoResults,
    /// The first page failed.
    Error,
    /// Two pages in, the third failed: Try again under the grid.
    PartialPage,
    /// "harbor" is held, dimmed and inert, while "harbor lights" loads.
    Stale,
    /// "pilot": episodes of several series, each labelled with its series
    /// and season and episode, among a few films.
    Episodes,
}

const POSTER: &[u8] = include_bytes!("../../assets/review/poster.jpg");
const POSTER_SERIES: &[u8] = include_bytes!("../../assets/review/poster-series.jpg");
const STILL: &[u8] = include_bytes!("../../assets/review/thumb-1.jpg");

const SERIES: [&str; 6] = [
    "Harbor Lights",
    "The Cartographer",
    "Night Ferry",
    "Glasshouse",
    "Salt Road",
    "Meridian",
];

/// Titles per query: each query's titles start at its own block of ids.
const BLOCK: usize = 100_000;

fn seed(query: &SearchQuery) -> usize {
    query
        .term()
        .bytes()
        .fold(7usize, |hash, byte| (hash * 31 + usize::from(byte)) % 997)
}

/// An episode named `name`, `index`-th of the fixture's series.
fn episode(number: usize, name: &str) -> MediaItem {
    let mut item = fixture(LibraryKind::Movies, number);
    item.identity.id = ItemId::parse(format!("episode-{number}")).expect("fixture id");
    item.identity.name = name.to_string();
    item.kind = ItemKind::Episode;
    item.metadata.runtime = None;
    item.hierarchy.series_id = ItemId::parse(format!("series-{}", number % SERIES.len())).ok();
    item.hierarchy.series_name = Some(SERIES[number % SERIES.len()].to_string());
    item.hierarchy.parent_index = Some(1 + (number / SERIES.len()) as u32 % 4);
    item.hierarchy.index = Some(1 + (number % 9) as u32);
    item.artwork.primary = ImageTag::parse(format!("still-{}", number % 2));
    if number % 4 == 1 {
        item.user = UserItemState::from_parts(
            Some(std::time::Duration::from_secs(900)),
            Some(40.0),
            false,
            false,
            0,
        );
    }
    item
}

/// The `index`-th result for a query seeded `seed`: mostly films, every
/// fifth a series, and, for an episode search, mostly episodes.
fn result(seed: usize, index: usize, episodes: bool) -> MediaItem {
    let number = seed * BLOCK + index;
    if episodes && index % 4 != 3 {
        return episode(number, "Pilot");
    }
    let kind = if index % 5 == 4 {
        LibraryKind::Series
    } else {
        LibraryKind::Movies
    };
    fixture(kind, number)
}

/// A fixture Jellyfin: how many titles each query matches.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct FixtureServer {
    pub total: usize,
    pub episodes: bool,
}

impl FixtureServer {
    pub(crate) const fn new(total: usize) -> Self {
        Self {
            total,
            episodes: false,
        }
    }

    /// The `index`-th title this server returns for `query`.
    pub(crate) fn title(self, query: &SearchQuery, index: usize) -> MediaItem {
        result(seed(query), index, self.episodes)
    }

    pub(crate) fn respond(self, request: &Request) -> Response {
        match request {
            Request::Page { query, page, .. } => {
                let end = (page.start + page.limit).min(self.total);
                Response::Page(Ok(LibraryPage {
                    items: (page.start.min(end)..end)
                        .map(|index| self.title(query, index))
                        .collect(),
                    start: page.start,
                    total: Some(self.total),
                }))
            }
            Request::Item { id, .. } => Response::Item(Ok(Box::new(refetch(id)))),
        }
    }
}

/// A title asked for again by id, as the fixture server holds it.
pub(crate) fn refetch(id: &ItemId) -> MediaItem {
    let (prefix, number) = id.as_str().rsplit_once('-').expect("fixture id");
    let number: usize = number.parse().expect("fixture id");
    match prefix {
        "episode" => episode(number, "Pilot"),
        "show" => fixture(LibraryKind::Series, number),
        _ => fixture(LibraryKind::Movies, number),
    }
}

/// Type `text`, let its wait end, and answer what it sends.
pub(crate) fn search(model: &mut SearchModel, text: &str, server: FixtureServer) {
    let wait = model.type_text(text).expect("a new query waits");
    for request in model.debounced(wait) {
        let response = server.respond(&request);
        model.apply(&request, response);
    }
}

/// Ask for the next page as if the last loaded card were built, and answer.
fn next_page(model: &mut SearchModel, answer: impl Fn(&Request) -> Response) {
    let last = model.items().len() - 1;
    if let Some(request) = model.want_more(last) {
        let response = answer(&request);
        model.apply(&request, response);
    }
}

fn server(scene: SearchPreview) -> FixtureServer {
    match scene {
        SearchPreview::ManyResults => FixtureServer::new(10_000),
        SearchPreview::NoResults => FixtureServer::new(0),
        SearchPreview::Episodes => FixtureServer {
            total: 36,
            episodes: true,
        },
        _ => FixtureServer::new(240),
    }
}

/// How the scene's server keeps answering once the screen is up: as the
/// scene was built. The loading and stale scenes never answer their query;
/// the error scenes keep failing.
fn scene_answers(scene: SearchPreview) -> impl Fn(&Request) -> Option<Response> {
    let server = server(scene);
    move |request| match (scene, request) {
        (SearchPreview::Loading, Request::Page { .. }) => None,
        (SearchPreview::Stale, Request::Page { query, .. }) if query.term() == "harbor lights" => {
            None
        }
        (SearchPreview::Error, Request::Page { page, .. }) if page.start == 0 => {
            Some(Response::Page(Err(SearchFailure::Unreachable)))
        }
        (SearchPreview::PartialPage, Request::Page { page, .. }) if page.start >= 120 => {
            Some(Response::Page(Err(SearchFailure::Unreachable)))
        }
        _ => Some(server.respond(request)),
    }
}

pub(crate) fn preview_model(scene: SearchPreview) -> SearchModel {
    let mut model = SearchModel::new();
    let server = server(scene);
    let answers = scene_answers(scene);
    let answer = |model: &mut SearchModel, requests: Vec<Request>| {
        for request in requests {
            if let Some(response) = answers(&request) {
                model.apply(&request, response);
            }
        }
    };
    match scene {
        SearchPreview::Empty => {}
        SearchPreview::TooShort => {
            model.type_text("h");
        }
        SearchPreview::Typing => {
            search(&mut model, "harbor", server);
            // The wait for "harbor li" has not ended: nothing is sent.
            let _ = model.type_text("harbor li");
        }
        SearchPreview::Loading | SearchPreview::Error => {
            let wait = model.type_text("harbor").expect("waits");
            let requests = model.debounced(wait);
            answer(&mut model, requests);
        }
        SearchPreview::Results => search(&mut model, "harbor", server),
        SearchPreview::ManyResults => {
            search(&mut model, "the", server);
            for _ in 0..4 {
                next_page(&mut model, |request| server.respond(request));
            }
        }
        SearchPreview::NoResults => search(&mut model, "zzyzx", server),
        SearchPreview::PartialPage => {
            search(&mut model, "harbor", server);
            for _ in 0..2 {
                next_page(&mut model, |request| {
                    answers(request).expect("the scene answers pages")
                });
            }
        }
        SearchPreview::Stale => {
            search(&mut model, "harbor", server);
            let wait = model.type_text("harbor lights").expect("waits");
            let requests = model.debounced(wait);
            answer(&mut model, requests);
        }
        SearchPreview::Episodes => search(&mut model, "pilot", server),
    }
    model
}

/// Fixture artwork for an address: posters for films and series, a still
/// for episodes.
pub(crate) fn fixture_art() -> Box<dyn Fn(&str) -> Artwork> {
    let decode = |bytes: &[u8]| {
        DecodedImage::decode(bytes, MAX_DECODED_SIDE)
            .map(Artwork::Ready)
            .unwrap_or(Artwork::Missing)
    };
    let (poster, series, still) = (decode(POSTER), decode(POSTER_SERIES), decode(STILL));
    Box::new(move |url: &str| {
        if url.contains("tag=still") {
            still.clone()
        } else if url.contains("tag=poster-1") {
            series.clone()
        } else {
            poster.clone()
        }
    })
}

impl SearchScreen {
    pub(crate) fn preview(
        runtime: Arc<ServiceRuntime>,
        loader: ArtworkLoader,
        scene: SearchPreview,
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
        screen.fixture_art = Some(fixture_art());
        screen.fixture_answers = Some(Rc::new(scene_answers(scene)));
        if scene == SearchPreview::ManyResults {
            note_keyboard_navigation(cx);
            screen.focus_results(Some(251));
        }
        screen
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::search::model::{Footer, IdleReason, SearchState};

    #[test]
    fn review_scenes_cover_the_states_they_name() {
        let state = |scene| preview_model(scene).state();
        assert_eq!(
            state(SearchPreview::Empty),
            SearchState::Idle(IdleReason::Empty)
        );
        assert_eq!(
            state(SearchPreview::TooShort),
            SearchState::Idle(IdleReason::TooShort)
        );
        let typing = preview_model(SearchPreview::Typing);
        assert_eq!(typing.state(), SearchState::Ready);
        assert_eq!(typing.input(), "harbor li");
        assert_eq!(typing.shown().map(SearchQuery::term), Some("harbor"));
        assert_eq!(state(SearchPreview::Loading), SearchState::Loading);
        let results = preview_model(SearchPreview::Results);
        assert_eq!(results.state(), SearchState::Ready);
        assert_eq!(results.items().len(), 60);
        assert_eq!(results.total(), Some(240));
        let many = preview_model(SearchPreview::ManyResults);
        assert_eq!(many.items().len(), 300);
        assert_eq!(many.total(), Some(10_000));
        assert_eq!(state(SearchPreview::NoResults), SearchState::NoResults);
        assert_eq!(
            state(SearchPreview::Error),
            SearchState::Failed(SearchFailure::Unreachable)
        );
        let partial = preview_model(SearchPreview::PartialPage);
        assert_eq!(partial.items().len(), 120);
        assert_eq!(partial.footer(), Footer::Failed(SearchFailure::Unreachable));
        let stale = preview_model(SearchPreview::Stale);
        assert!(stale.is_stale());
        assert_eq!(stale.state(), SearchState::Loading);
        let episodes = preview_model(SearchPreview::Episodes);
        assert!(
            episodes
                .items()
                .iter()
                .filter(|item| item.kind == ItemKind::Episode)
                .count()
                > 20
        );
    }

    #[test]
    fn each_query_has_its_own_titles() {
        let server = FixtureServer::new(240);
        let a = SearchQuery::parse("harbor").unwrap();
        let b = SearchQuery::parse("harbor lights").unwrap();
        assert_ne!(server.title(&a, 0).id(), server.title(&b, 0).id());
        assert_eq!(server.title(&a, 3), server.title(&a, 3), "deterministic");
        let id = server.title(&a, 7).id().clone();
        assert_eq!(refetch(&id).id(), &id);
        let pilot = FixtureServer {
            total: 4,
            episodes: true,
        }
        .title(&SearchQuery::parse("pilot").unwrap(), 0);
        assert_eq!(refetch(pilot.id()), pilot);
    }
}
