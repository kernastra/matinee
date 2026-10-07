//! Library model: pagination, tickets, query changes, and scale.

use matinee_core::{
    ItemId, ItemKind, LibraryContent, LibraryGenre, LibraryId, LibraryKind, LibraryPage,
    LibrarySort, LibraryView, MediaItem, WatchFilter,
};

use super::model::{
    Applied, CatalogState, EmptyReason, Fetch, Footer, LibraryFailure, LibraryModel, PAGE_SIZE,
    PREFETCH_ITEMS, Request, Response, count_label,
};

pub(crate) fn title(id: &str, kind: ItemKind) -> MediaItem {
    MediaItem {
        identity: matinee_core::ItemIdentity {
            id: ItemId::parse(id).unwrap(),
            name: id.to_string(),
        },
        kind,
        metadata: Default::default(),
        artwork: Default::default(),
        user: Default::default(),
        hierarchy: Default::default(),
        media: Default::default(),
        people: Vec::new(),
        chapters: Vec::new(),
    }
}

pub(crate) fn movie(index: usize) -> MediaItem {
    title(&format!("movie-{index}"), ItemKind::Movie)
}

/// A server holding `total` movies, answering any page.
fn page_of(request: &Request, total: usize) -> Response {
    let Request::Page { page, .. } = request else {
        panic!("not a page: {request:?}");
    };
    let end = (page.start + page.limit).min(total);
    Response::Page(Ok(LibraryPage {
        items: (page.start.min(end)..end).map(movie).collect(),
        start: page.start,
        total: Some(total),
    }))
}

fn pages(requests: &[Request]) -> Vec<&Request> {
    requests
        .iter()
        .filter(|request| matches!(request, Request::Page { .. }))
        .collect()
}

fn only_page(requests: &[Request]) -> Request {
    let pages = pages(requests);
    assert_eq!(pages.len(), 1, "{requests:?}");
    pages[0].clone()
}

fn start_of(request: &Request) -> usize {
    match request {
        Request::Page { page, .. } => page.start,
        _ => panic!("not a page"),
    }
}

/// A model showing Movies with its first page of a `total`-title library.
fn loaded(total: usize) -> LibraryModel {
    let (mut model, requests) = LibraryModel::open(LibraryKind::Movies);
    let first = only_page(&requests);
    assert_eq!(
        model.apply(&first, page_of(&first, total)),
        Applied::Replaced
    );
    model
}

#[test]
fn opening_asks_for_the_first_page_the_libraries_and_the_genres() {
    let (model, requests) = LibraryModel::open(LibraryKind::Movies);
    assert_eq!(requests.len(), 3);
    assert!(matches!(requests[0], Request::Views { .. }));
    let first = only_page(&requests);
    match &first {
        Request::Page {
            kind, query, page, ..
        } => {
            assert_eq!(*kind, LibraryKind::Movies);
            assert_eq!(query.sort, LibrarySort::Name, "shipping default");
            assert_eq!(page.start, 0);
            assert_eq!(page.limit, PAGE_SIZE);
        }
        _ => unreachable!(),
    }
    assert!(requests.iter().any(|r| matches!(r, Request::Genres { .. })));
    assert_eq!(model.state(), CatalogState::Loading);
    assert_eq!(model.footer(), Footer::Idle);
}

#[test]
fn pages_append_until_the_server_has_no_more() {
    let mut model = loaded(250);
    assert_eq!(model.items().len(), 100);
    assert_eq!(model.total(), Some(250));
    assert_eq!(model.footer(), Footer::Idle);

    // Far from the end: nothing.
    assert!(model.want_more(10).is_none());
    // Near the end: page two.
    let second = model.want_more(100 - PREFETCH_ITEMS).expect("page two");
    assert_eq!(start_of(&second), 100);
    assert_eq!(model.footer(), Footer::Loading);
    // The same trigger again while it loads: no second request.
    assert!(model.want_more(99).is_none());
    assert_eq!(
        model.apply(&second, page_of(&second, 250)),
        Applied::Appended
    );
    assert_eq!(model.items().len(), 200);

    let third = model.want_more(199).expect("page three");
    assert_eq!(start_of(&third), 200);
    model.apply(&third, page_of(&third, 250));
    assert_eq!(model.items().len(), 250);
    assert_eq!(model.footer(), Footer::End, "no endless spinner");
    assert!(model.want_more(249).is_none(), "nothing past the end");
    assert_eq!(model.stats().pages_requested, 3);
}

#[test]
fn a_failed_page_keeps_earlier_pages_and_retries_the_same_page() {
    let mut model = loaded(400);
    let second = model.want_more(99).unwrap();
    model.apply(&second, page_of(&second, 400));
    let third = model.want_more(199).unwrap();
    assert_eq!(
        model.apply(&third, Response::Page(Err(LibraryFailure::Unreachable))),
        Applied::Updated
    );
    assert_eq!(model.items().len(), 200, "pages one and two stay");
    assert_eq!(model.state(), CatalogState::Ready);
    assert_eq!(model.footer(), Footer::Failed(LibraryFailure::Unreachable));
    assert!(model.want_more(199).is_none(), "scrolling does not retry");

    let retry = model.retry();
    let again = only_page(&retry);
    assert_eq!(start_of(&again), 200, "the page that failed");
    model.apply(&again, page_of(&again, 400));
    assert_eq!(model.items().len(), 300);
    assert_eq!(model.footer(), Footer::Idle);
}

#[test]
fn a_failed_first_page_offers_try_again() {
    let (mut model, requests) = LibraryModel::open(LibraryKind::Movies);
    let first = only_page(&requests);
    model.apply(&first, Response::Page(Err(LibraryFailure::Unreadable)));
    assert_eq!(
        model.state(),
        CatalogState::Failed(LibraryFailure::Unreadable)
    );
    let retry = model.retry();
    let again = only_page(&retry);
    assert_eq!(model.state(), CatalogState::Loading);
    model.apply(&again, page_of(&again, 3));
    assert_eq!(model.state(), CatalogState::Ready);
}

#[test]
fn a_query_change_ignores_the_old_query_s_late_pages() {
    let mut model = loaded(300);
    let second = model.want_more(99).unwrap();
    // The sort changes while page two is in flight.
    let requests = model.set_sort(LibrarySort::DateCreated);
    let fresh = only_page(&requests);
    assert_eq!(start_of(&fresh), 0);
    assert!(
        model.items().is_empty(),
        "old titles are not shown as new ones"
    );
    assert_eq!(model.state(), CatalogState::Loading);
    // The old page two arrives: ignored, nothing appended.
    assert_eq!(
        model.apply(&second, page_of(&second, 300)),
        Applied::Ignored
    );
    assert!(model.items().is_empty());
    assert_eq!(model.apply(&fresh, page_of(&fresh, 300)), Applied::Replaced);
    assert_eq!(model.items().len(), 100);
    match &fresh {
        Request::Page { query, .. } => assert_eq!(query.sort, LibrarySort::DateCreated),
        _ => unreachable!(),
    }
    // Choosing the same sort again changes nothing.
    assert!(model.set_sort(LibrarySort::DateCreated).is_empty());
}

#[test]
fn rapid_query_changes_apply_only_the_last() {
    let (mut model, requests) = LibraryModel::open(LibraryKind::Movies);
    let first = only_page(&requests);
    let by_year = only_page(&model.set_sort(LibrarySort::ProductionYear));
    let unwatched = only_page(&model.set_watch(WatchFilter::Unwatched));
    let genre = only_page(&model.set_genre(Some(ItemId::parse("g-1").unwrap())));
    // Answers arrive in the worst order.
    assert_eq!(model.apply(&genre, page_of(&genre, 5)), Applied::Replaced);
    assert_eq!(model.apply(&first, page_of(&first, 500)), Applied::Ignored);
    assert_eq!(
        model.apply(&unwatched, page_of(&unwatched, 50)),
        Applied::Ignored
    );
    assert_eq!(
        model.apply(&by_year, page_of(&by_year, 500)),
        Applied::Ignored
    );
    assert_eq!(model.items().len(), 5);
    assert_eq!(model.total(), Some(5));
    let query = model.query();
    assert_eq!(query.sort, LibrarySort::ProductionYear);
    assert_eq!(query.filter.watch, WatchFilter::Unwatched);
}

#[test]
fn refresh_keeps_titles_until_the_new_first_page_and_ignores_stale_answers() {
    let mut model = loaded(250);
    let second = model.want_more(99).unwrap();
    model.apply(&second, page_of(&second, 250));
    let selections = model.query().clone();

    let refresh_a = only_page(&model.refresh());
    let refresh_b = only_page(&model.refresh());
    assert_eq!(model.items().len(), 200, "kept while refreshing");
    assert_eq!(model.state(), CatalogState::Ready);
    match &refresh_b {
        Request::Page { query, page, .. } => {
            assert_eq!(query, &selections, "same selections");
            assert_eq!(page.start, 0, "from page one");
        }
        _ => unreachable!(),
    }
    assert_eq!(
        model.apply(&refresh_b, page_of(&refresh_b, 260)),
        Applied::Replaced
    );
    assert_eq!(model.items().len(), 100, "page one replaces every page");
    assert_eq!(model.total(), Some(260));
    assert_eq!(
        model.apply(&refresh_a, page_of(&refresh_a, 250)),
        Applied::Ignored
    );
    assert_eq!(model.total(), Some(260));

    // A failed refresh keeps what is shown.
    let refresh_c = only_page(&model.refresh());
    model.apply(&refresh_c, Response::Page(Err(LibraryFailure::Unreachable)));
    assert_eq!(model.items().len(), 100);
    assert_eq!(model.state(), CatalogState::Ready);
}

#[test]
fn an_ended_session_is_reported_not_shown() {
    let mut model = loaded(250);
    let second = model.want_more(99).unwrap();
    assert_eq!(
        model.apply(&second, Response::Page(Err(LibraryFailure::SignedOut))),
        Applied::SessionExpired
    );
    assert_eq!(model.items().len(), 100);
    assert_eq!(model.footer(), Footer::Loading, "no failure painted");

    let (mut model, requests) = LibraryModel::open(LibraryKind::Movies);
    for request in &requests {
        let response = match request {
            Request::Page { .. } => Response::Page(Err(LibraryFailure::SignedOut)),
            Request::Views { .. } => Response::Views(Err(LibraryFailure::SignedOut)),
            Request::Genres { .. } => Response::Genres(Err(LibraryFailure::SignedOut)),
            Request::Item { .. } => Response::Item(Err(LibraryFailure::SignedOut)),
        };
        assert_eq!(model.apply(request, response), Applied::SessionExpired);
    }
    assert_eq!(model.state(), CatalogState::Loading);
}

#[test]
fn an_empty_library_differs_from_filters_that_match_nothing() {
    let model = loaded(0);
    assert_eq!(model.state(), CatalogState::Empty(EmptyReason::Library));
    assert_eq!(model.footer(), Footer::Idle);
    assert_eq!(
        EmptyReason::Library.message(),
        "Nothing is in this library yet."
    );

    let mut model = loaded(40);
    let filtered = only_page(&model.set_watch(WatchFilter::Favorites));
    model.apply(&filtered, page_of(&filtered, 0));
    assert_eq!(model.state(), CatalogState::Empty(EmptyReason::Filtered));
    assert_eq!(
        EmptyReason::Filtered.message(),
        "Nothing matches these filters."
    );
    let cleared = only_page(&model.clear_filters());
    model.apply(&cleared, page_of(&cleared, 40));
    assert_eq!(model.state(), CatalogState::Ready);
    assert!(model.clear_filters().is_empty(), "nothing left to clear");
}

#[test]
fn movies_and_series_keep_their_own_state() {
    let mut model = loaded(300);
    let second = model.want_more(99).unwrap();
    model.apply(&second, page_of(&second, 300));
    model.note_focused(ItemId::parse("movie-150").unwrap());
    model.set_sort(LibrarySort::Name);

    let requests = model.show(LibraryKind::Series);
    let series_first = only_page(&requests);
    assert!(
        !requests.iter().any(|r| matches!(r, Request::Views { .. })),
        "libraries are asked for once"
    );
    assert_eq!(model.state(), CatalogState::Loading);
    // A late Movies answer while Series shows is still applied to Movies.
    let requests = model.show(LibraryKind::Movies);
    assert!(requests.is_empty(), "Movies is shown as it was");
    assert_eq!(model.items().len(), 200);
    assert_eq!(model.focused_index(), Some(150));
    model.show(LibraryKind::Series);
    match &series_first {
        Request::Page { kind, .. } => assert_eq!(*kind, LibraryKind::Series),
        _ => unreachable!(),
    }
    let Response::Page(Ok(mut page)) = page_of(&series_first, 2) else {
        unreachable!()
    };
    for title in &mut page.items {
        title.kind = ItemKind::Series;
    }
    assert_eq!(
        model.apply(&series_first, Response::Page(Ok(page))),
        Applied::Replaced
    );
    assert_eq!(model.items().len(), 2);
    model.show(LibraryKind::Movies);
    assert_eq!(model.items().len(), 200, "Movies untouched");
}

#[test]
fn library_menus_follow_the_kind_and_fail_quietly() {
    let (mut model, requests) = LibraryModel::open(LibraryKind::Movies);
    let views = requests
        .iter()
        .find(|r| matches!(r, Request::Views { .. }))
        .unwrap()
        .clone();
    let genres = requests
        .iter()
        .find(|r| matches!(r, Request::Genres { .. }))
        .unwrap()
        .clone();
    let view = |id: &str, name: &str, content| LibraryView {
        id: LibraryId::parse(id).unwrap(),
        name: name.into(),
        content,
    };
    model.apply(
        &views,
        Response::Views(Ok(vec![
            view("films", "Films", LibraryContent::Movies),
            view("kids", "Kids", LibraryContent::Movies),
            view("tv", "Television", LibraryContent::Series),
            view("home", "Home Media", LibraryContent::Mixed),
        ])),
    );
    let names: Vec<&str> = model.views().iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, vec!["Films", "Kids", "Home Media"]);
    model.apply(&genres, Response::Genres(Err(LibraryFailure::Unreachable)));
    assert_eq!(model.genres(), &Fetch::Failed, "the genre menu is left out");
    assert_eq!(model.state(), CatalogState::Loading, "browsing goes on");

    // Choosing a library clears the genre and asks for its genres.
    model.set_genre(Some(ItemId::parse("g-1").unwrap()));
    let requests = model.set_view(Some(LibraryId::parse("kids").unwrap()));
    assert_eq!(pages(&requests).len(), 1);
    let genres = requests
        .iter()
        .find(|r| matches!(r, Request::Genres { .. }))
        .expect("genres for the library")
        .clone();
    assert_eq!(model.query().filter.genre, None);
    assert_eq!(model.view_name(), Some("Kids"));
    match &genres {
        Request::Genres { view, .. } => assert_eq!(view.as_ref().unwrap().as_str(), "kids"),
        _ => unreachable!(),
    }
    model.apply(
        &genres,
        Response::Genres(Ok(vec![LibraryGenre {
            id: ItemId::parse("g-2").unwrap(),
            name: "Animation".into(),
        }])),
    );
    model.set_genre(Some(ItemId::parse("g-2").unwrap()));
    assert_eq!(model.genre_name(), Some("Animation"));

    model.show(LibraryKind::Series);
    let names: Vec<&str> = model.views().iter().map(|v| v.name.as_str()).collect();
    assert_eq!(names, vec!["Television", "Home Media"]);
}

#[test]
fn playback_reconciles_only_the_opened_title() {
    let mut model = loaded(150);
    let target = ItemId::parse("movie-42").unwrap();
    model.note_opened(target.clone());
    let request = model.reconcile().expect("one request");
    match &request {
        Request::Item { id, .. } => assert_eq!(id, &target),
        _ => panic!("not an item request"),
    }
    let mut updated = movie(42);
    updated.user = matinee_core::UserItemState::from_parts(None, None, false, true, 1);
    assert_eq!(
        model.apply(&request, Response::Item(Ok(Box::new(updated)))),
        Applied::Updated
    );
    assert_eq!(model.items().len(), 100, "nothing reloaded");
    assert!(model.items()[42].user.is_played(), "in place");
    assert_eq!(model.focused_index(), Some(42));
    // A reconcile answer after a query change is ignored.
    let request = model.reconcile().unwrap();
    model.set_sort(LibrarySort::CommunityRating);
    assert_eq!(
        model.apply(&request, Response::Item(Ok(Box::new(movie(42))))),
        Applied::Ignored
    );
}

#[test]
fn duplicates_across_pages_are_dropped() {
    let mut model = loaded(250);
    let second = model.want_more(99).unwrap();
    // The library shifted: page two repeats the last five of page one.
    let Response::Page(Ok(mut page)) = page_of(&second, 250) else {
        unreachable!()
    };
    page.items.splice(0..5, (95..100).map(movie));
    model.apply(&second, Response::Page(Ok(page)));
    assert_eq!(model.items().len(), 195);
    let next = model.want_more(194).unwrap();
    assert_eq!(start_of(&next), 200, "paging follows the server's offsets");
}

/// Scroll through a whole library the way the grid does, a viewport at a
/// time, answering every page. Returns pages requested and the most items
/// held.
fn scroll_through(total: usize) -> (usize, usize) {
    let mut model = loaded(total);
    let columns = 6;
    let built_rows = 6;
    let mut first_row = 0;
    loop {
        let last_built = ((first_row + built_rows) * columns).min(model.items().len()) - 1;
        if let Some(request) = model.want_more(last_built) {
            assert!(model.want_more(last_built).is_none(), "one at a time");
            model.apply(&request, page_of(&request, total));
        }
        if last_built + 1 >= total {
            break;
        }
        first_row += 1;
    }
    assert_eq!(model.items().len(), total);
    assert_eq!(model.footer(), Footer::End);
    (model.stats().pages_requested, model.items().len())
}

#[test]
fn scrolling_through_large_libraries_requests_each_page_once() {
    assert_eq!(scroll_through(100), (1, 100));
    assert_eq!(scroll_through(1_000), (10, 1_000));
    assert_eq!(scroll_through(10_000), (100, 10_000));
}

#[test]
fn opening_a_large_library_fetches_one_page() {
    let model = loaded(10_000);
    assert_eq!(model.items().len(), PAGE_SIZE);
    assert_eq!(model.total(), Some(10_000));
    assert_eq!(model.stats().pages_requested, 1);
}

#[test]
fn counts_read_naturally() {
    assert_eq!(count_label(1), "1 title");
    assert_eq!(count_label(0), "0 titles");
    assert_eq!(count_label(999), "999 titles");
    assert_eq!(count_label(1_284), "1,284 titles");
    assert_eq!(count_label(10_000), "10,000 titles");
    assert_eq!(count_label(1_234_567), "1,234,567 titles");
}
