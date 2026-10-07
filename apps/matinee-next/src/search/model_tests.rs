//! Search model behavior, with no clock and no network. Debounce waits are
//! tokens the test fires by hand, so timing is deterministic.

use matinee_core::{LibraryKind, LibraryPage, LibraryPageRequest, MediaItem};
use matinee_jellyfin::JellyfinError;

use super::model::{
    Applied, Footer, IdleReason, PAGE_SIZE, Request, Response, SearchFailure, SearchModel,
    SearchState,
};
use crate::library::preview::fixture;

fn titles(start: usize, count: usize, total: usize) -> Vec<MediaItem> {
    (start..(start + count).min(total))
        .map(|index| fixture(LibraryKind::Movies, index))
        .collect()
}

/// What Jellyfin answers for `request` when a query matches `total` titles.
fn serve(request: &Request, total: usize) -> Response {
    match request {
        Request::Page { page, .. } => Response::Page(Ok(LibraryPage {
            items: titles(page.start, page.limit, total),
            start: page.start,
            total: Some(total),
        })),
        Request::Item { id, .. } => {
            let index = id
                .as_str()
                .trim_start_matches("film-")
                .parse()
                .expect("fixture id");
            Response::Item(Ok(Box::new(fixture(LibraryKind::Movies, index))))
        }
    }
}

/// Type `text`, fire its debounce, and return what was sent.
fn search(model: &mut SearchModel, text: &str, total: usize) -> Vec<Request> {
    let debounce = model.type_text(text).expect("a search waits for typing");
    let requests = model.debounced(debounce);
    for request in &requests {
        let response = serve(request, total);
        model.apply(request, response);
    }
    requests
}

fn page(start: usize) -> LibraryPageRequest {
    LibraryPageRequest {
        start,
        limit: PAGE_SIZE,
    }
}

#[test]
fn empty_and_blank_input_sends_nothing_and_shows_the_calm_start() {
    let mut model = SearchModel::new();
    assert_eq!(model.state(), SearchState::Idle(IdleReason::Empty));
    assert_eq!(model.type_text("   "), None, "blank is not searched");
    assert_eq!(
        model.submit(),
        Vec::new(),
        "Enter on a blank field sends nothing"
    );
    assert_eq!(model.state(), SearchState::Idle(IdleReason::Empty));
    assert!(model.effective().is_none());
}

#[test]
fn one_letter_is_not_sent_and_clears_the_held_titles() {
    let mut model = SearchModel::new();
    search(&mut model, "alien", 30);
    assert_eq!(model.state(), SearchState::Ready);
    assert_eq!(model.type_text("a"), None, "one letter waits for two");
    assert!(model.items().is_empty(), "no query, so no titles");
    assert_eq!(model.state(), SearchState::Idle(IdleReason::TooShort));
    assert_eq!(model.submit(), Vec::new(), "still too short to send");
}

#[test]
fn typing_waits_for_debounce_then_sends_one_first_page() {
    let mut model = SearchModel::new();
    let debounce = model.type_text("alien").expect("waits");
    assert_eq!(
        model.state(),
        SearchState::Loading,
        "on its way, not a blank field"
    );
    assert!(
        model.effective().is_none(),
        "nothing sent before the wait ends"
    );
    let sent = model.debounced(debounce);
    assert_eq!(sent.len(), 1);
    match &sent[0] {
        Request::Page { query, page: p, .. } => {
            assert_eq!(query.term(), "alien");
            assert_eq!(*p, page(0));
        }
        other => panic!("expected a first page, got {other:?}"),
    }
    assert_eq!(model.state(), SearchState::Loading);
    assert_eq!(model.input(), "alien");
}

#[test]
fn typing_again_replaces_the_pending_debounce() {
    let mut model = SearchModel::new();
    let first = model.type_text("ali").expect("waits");
    let second = model.type_text("alie").expect("waits");
    let third = model.type_text("alien").expect("waits");
    assert_eq!(
        model.debounced(first),
        Vec::new(),
        "an old wait sends nothing"
    );
    assert_eq!(
        model.debounced(second),
        Vec::new(),
        "nor does a replaced one"
    );
    let sent = model.debounced(third);
    assert_eq!(sent.len(), 1, "only the latest wait sends");
    match &sent[0] {
        Request::Page { query, .. } => assert_eq!(query.term(), "alien"),
        other => panic!("{other:?}"),
    }
}

#[test]
fn enter_bypasses_the_debounce_and_cancels_it() {
    let mut model = SearchModel::new();
    let pending = model.type_text("solaris").expect("waits");
    let sent = model.submit();
    assert_eq!(sent.len(), 1, "Enter sends at once");
    assert_eq!(
        model.debounced(pending),
        Vec::new(),
        "the wait Enter bypassed does not send a second time"
    );
}

#[test]
fn the_same_query_is_never_sent_twice() {
    let mut model = SearchModel::new();
    search(&mut model, "alien", 30);
    assert_eq!(model.submit(), Vec::new(), "Enter on held results");
    assert_eq!(
        model.type_text("alien "),
        None,
        "trimmed to the same query: no wait"
    );
    assert_eq!(model.submit(), Vec::new());
    assert_eq!(model.pages_requested(), 1);
}

#[test]
fn typing_back_to_the_query_in_flight_keeps_its_request() {
    let mut model = SearchModel::new();
    let wait = model.type_text("alien").expect("waits");
    let sent = model.debounced(wait);
    let in_flight = sent[0].clone();
    // The same text again while the first page is on its way.
    assert_eq!(model.type_text("alien"), None);
    let response = serve(&in_flight, 30);
    assert_eq!(model.apply(&in_flight, response), Applied::Replaced);
    assert_eq!(
        model.state(),
        SearchState::Ready,
        "the answer still applies"
    );
}

#[test]
fn a_new_query_gets_a_new_request_and_the_old_answer_is_ignored() {
    let mut model = SearchModel::new();
    let wait = model.type_text("alien").expect("waits");
    let old = model.debounced(wait).remove(0);
    let wait = model.type_text("aliens").expect("waits");
    let new = model.debounced(wait).remove(0);
    assert_ne!(old, new);
    assert_eq!(
        model.apply(&old, serve(&old, 5)),
        Applied::Ignored,
        "the old query's answer arrives late"
    );
    assert!(model.items().is_empty(), "nothing from the old query shown");
    assert_eq!(model.apply(&new, serve(&new, 9)), Applied::Replaced);
    assert_eq!(model.shown().map(|q| q.term()), Some("aliens"));
    assert_eq!(model.total(), Some(9));
}

#[test]
fn a_late_answer_never_overwrites_newer_results() {
    let mut model = SearchModel::new();
    let wait = model.type_text("blade").expect("waits");
    let old = model.debounced(wait).remove(0);
    let wait = model.type_text("blade runner").expect("waits");
    let new = model.debounced(wait).remove(0);
    model.apply(&new, serve(&new, 3));
    assert_eq!(model.apply(&old, serve(&old, 800)), Applied::Ignored);
    assert_eq!(model.total(), Some(3), "the newer answer stands");
    assert_eq!(model.items().len(), 3);
}

#[test]
fn old_titles_stay_marked_stale_until_the_new_first_page_replaces_them() {
    let mut model = SearchModel::new();
    search(&mut model, "alien", 4);
    assert!(!model.is_stale());
    let wait = model.type_text("aliens").expect("waits");
    let next = model.debounced(wait).remove(0);
    assert_eq!(model.items().len(), 4, "held while the new query loads");
    assert!(model.is_stale(), "dimmed: they are not the new query's");
    assert_eq!(model.state(), SearchState::Loading);
    model.apply(&next, serve(&next, 2));
    assert!(!model.is_stale());
    assert_eq!(model.items().len(), 2, "replaced, not appended");
}

#[test]
fn first_page_then_next_page_appends_without_duplicates() {
    let mut model = SearchModel::new();
    search(&mut model, "the", 100);
    assert_eq!(model.items().len(), PAGE_SIZE);
    let more = model.want_more(model.items().len() - 1).expect("next page");
    assert_eq!(model.apply(&more, serve(&more, 100)), Applied::Appended);
    assert_eq!(model.items().len(), 100, "the last page is short");
    let ids: std::collections::HashSet<_> = model.items().iter().map(|i| i.id().clone()).collect();
    assert_eq!(ids.len(), 100, "every title once");
}

#[test]
fn end_of_results_asks_for_nothing_more() {
    let mut model = SearchModel::new();
    search(&mut model, "alien", 12);
    assert_eq!(model.footer(), Footer::End);
    assert_eq!(model.want_more(11), None);
}

#[test]
fn only_one_next_page_is_in_flight_and_renders_do_not_ask_again() {
    let mut model = SearchModel::new();
    search(&mut model, "the", 500);
    let last = model.items().len() - 1;
    let first = model.want_more(last).expect("asked once");
    assert_eq!(model.footer(), Footer::Loading);
    for _ in 0..50 {
        assert_eq!(model.want_more(last), None, "repeated renders send nothing");
    }
    assert_eq!(model.apply(&first, serve(&first, 500)), Applied::Appended);
}

#[test]
fn a_next_page_asks_only_near_the_end_of_what_is_loaded() {
    let mut model = SearchModel::new();
    search(&mut model, "the", 500);
    assert_eq!(
        model.want_more(0),
        None,
        "the top of the list is not the end"
    );
    assert!(model.want_more(model.items().len() - 1).is_some());
}

#[test]
fn a_failed_next_page_keeps_page_one_and_retry_asks_again() {
    let mut model = SearchModel::new();
    search(&mut model, "the", 500);
    let more = model.want_more(model.items().len() - 1).expect("next page");
    let applied = model.apply(&more, Response::Page(Err(SearchFailure::Unreachable)));
    assert_eq!(applied, Applied::Updated);
    assert_eq!(model.items().len(), PAGE_SIZE, "page one stays");
    assert_eq!(model.state(), SearchState::Ready, "not a failed search");
    assert_eq!(model.footer(), Footer::Failed(SearchFailure::Unreachable));
    assert_eq!(
        model.want_more(model.items().len() - 1),
        None,
        "waits for Try again"
    );
    let retry = model.retry();
    assert_eq!(retry.len(), 1);
    assert_eq!(
        model.apply(&retry[0], serve(&retry[0], 500)),
        Applied::Appended
    );
    assert_eq!(model.items().len(), 2 * PAGE_SIZE);
}

#[test]
fn a_first_page_failure_offers_retry_and_shows_no_old_titles() {
    let mut model = SearchModel::new();
    search(&mut model, "alien", 4);
    let wait = model.type_text("aliens").expect("waits");
    let next = model.debounced(wait).remove(0);
    let applied = model.apply(&next, Response::Page(Err(SearchFailure::Unreachable)));
    assert_eq!(applied, Applied::Updated);
    assert_eq!(
        model.state(),
        SearchState::Failed(SearchFailure::Unreachable)
    );
    assert!(
        model.items().is_empty(),
        "old titles are not under a failure"
    );
    assert!(!model.is_stale());
    let retry = model.retry();
    assert_eq!(retry.len(), 1, "Try again sends the first page");
    model.apply(&retry[0], serve(&retry[0], 2));
    assert_eq!(model.state(), SearchState::Ready);
}

#[test]
fn submitting_after_a_failure_retries_the_same_query() {
    let mut model = SearchModel::new();
    let wait = model.type_text("alien").expect("waits");
    let request = model.debounced(wait).remove(0);
    model.apply(&request, Response::Page(Err(SearchFailure::Unreadable)));
    assert_eq!(model.submit().len(), 1, "Enter retries a failed first page");
    assert_eq!(
        model.submit(),
        Vec::new(),
        "while that request is in flight"
    );
}

#[test]
fn changing_the_query_during_a_next_page_drops_the_late_page() {
    let mut model = SearchModel::new();
    search(&mut model, "the", 500);
    let late = model.want_more(model.items().len() - 1).expect("next page");
    let wait = model.type_text("alien").expect("waits");
    let fresh = model.debounced(wait).remove(0);
    assert_eq!(
        model.apply(&late, serve(&late, 500)),
        Applied::Ignored,
        "page two of the old query never joins the new one"
    );
    assert_eq!(
        model.items().len(),
        PAGE_SIZE,
        "old titles stay stale, not extended"
    );
    model.apply(&fresh, serve(&fresh, 7));
    assert_eq!(model.items().len(), 7);
}

#[test]
fn zero_matches_is_a_result_not_an_idle_start() {
    let mut model = SearchModel::new();
    search(&mut model, "solaris", 0);
    assert_eq!(model.state(), SearchState::NoResults);
    assert_eq!(model.shown().map(|q| q.term()), Some("solaris"));
    assert_eq!(model.footer(), Footer::End);
    let blank = SearchModel::new();
    assert_eq!(blank.state(), SearchState::Idle(IdleReason::Empty));
}

#[test]
fn an_expired_session_is_reported_once_and_late_answers_are_ignored() {
    let mut model = SearchModel::new();
    let wait = model.type_text("alien").expect("waits");
    let request = model.debounced(wait).remove(0);
    let failure = Response::Page(Err(SearchFailure::from_error(&JellyfinError::Cancelled)));
    assert_eq!(model.apply(&request, failure), Applied::Updated);
    let expired = Response::Page(Err(SearchFailure::SignedOut));
    let first = model.submit();
    let request = first.first().cloned().expect("a fresh request");
    assert_eq!(
        model.apply(&request, expired.clone()),
        Applied::SessionExpired
    );
    assert_eq!(
        model.apply(&request, expired),
        Applied::Ignored,
        "concurrent auth failures do nothing more"
    );
}

#[test]
fn playback_reconciles_only_the_opened_title() {
    let mut model = SearchModel::new();
    search(&mut model, "the", 500);
    assert!(model.reconcile().is_none(), "nothing opened yet");
    let opened = model.items()[3].id().clone();
    model.note_opened(opened.clone());
    let request = model.reconcile().expect("one title");
    assert!(matches!(request, Request::Item { ref id, .. } if *id == opened));
    let before = model.pages_requested();
    model.apply(&request, serve(&request, 500));
    assert_eq!(model.pages_requested(), before, "no search is run again");
    assert_eq!(model.items().len(), PAGE_SIZE, "the list is not reloaded");
}

#[test]
fn focus_survives_a_return_but_not_a_different_query() {
    let mut model = SearchModel::new();
    search(&mut model, "the", 500);
    let focused = model.items()[7].id().clone();
    model.note_focused(focused.clone());
    assert_eq!(model.focused_index(), Some(7));
    // Re-showing the same query keeps everything.
    assert_eq!(model.submit(), Vec::new());
    assert_eq!(model.focused_index(), Some(7));
    let wait = model.type_text("alien").expect("waits");
    let request = model.debounced(wait).remove(0);
    model.apply(&request, serve(&request, 4));
    assert_eq!(
        model.focused_index(),
        None,
        "a title not in the new results loses focus"
    );
}

#[test]
fn returning_to_the_held_query_shows_its_titles_without_a_request() {
    let mut model = SearchModel::new();
    search(&mut model, "alien", 30);
    let wait = model.type_text("aliens").expect("waits");
    let _ = model.debounced(wait);
    let back = model.type_text("alien").expect("typing back waits");
    assert_eq!(
        model.debounced(back),
        Vec::new(),
        "held titles, nothing sent"
    );
    assert_eq!(model.state(), SearchState::Ready);
    assert_eq!(model.items().len(), 30, "the same titles");
    assert_eq!(model.pages_requested(), 2, "only the two queries asked");
}

/// Scroll a window of `window` cards down a result of `total` titles, one
/// screen at a time. Each render may ask for the next page, and answers
/// come back in order. Returns (requests, most titles held at any time).
fn scroll_through(total: usize, window: usize) -> (usize, usize) {
    let mut model = SearchModel::new();
    search(&mut model, "the", total);
    let mut most = model.items().len();
    let mut built_end = window.min(total);
    loop {
        // A render builds `window` cards and reports the last one built.
        if let Some(request) = model.want_more(built_end.saturating_sub(1)) {
            assert_eq!(model.footer(), Footer::Loading);
            assert_eq!(model.want_more(built_end - 1), None, "one in flight");
            model.apply(&request, serve(&request, total));
            most = most.max(model.items().len());
        }
        if built_end >= total {
            break;
        }
        built_end = (built_end + window / 2).min(total);
    }
    (model.pages_requested(), most)
}

#[test]
fn scale_100_results_fetch_at_most_two_pages() {
    let (requests, held) = scroll_through(100, 30);
    assert!(requests <= 2, "{requests} pages for 100 titles");
    assert_eq!(held, 100);
}

#[test]
fn scale_1000_results_fetch_only_as_the_window_moves() {
    let (requests, held) = scroll_through(1_000, 30);
    assert_eq!(
        requests,
        1_000usize.div_ceil(PAGE_SIZE),
        "pages follow scrolling"
    );
    assert_eq!(held, 1_000);
}

#[test]
fn scale_10000_results_hold_only_loaded_pages_and_one_request_at_a_time() {
    // The window is what the grid builds: thirty cards. Only the first screen
    // is asked for when the person does not scroll, so most of 10,000 is not
    // fetched at all.
    let mut model = SearchModel::new();
    search(&mut model, "the", 10_000);
    assert_eq!(model.total(), Some(10_000));
    assert_eq!(
        model.items().len(),
        PAGE_SIZE,
        "one page held, not ten thousand"
    );
    assert_eq!(model.pages_requested(), 1, "a short look asks for one page");
    let (requests, held) = scroll_through(10_000, 30);
    assert!(requests <= 10_000usize.div_ceil(PAGE_SIZE));
    assert_eq!(
        held, 10_000,
        "scrolling to the end loads all of it, page by page"
    );
}

#[test]
fn refresh_asks_the_first_page_again_and_keeps_the_titles_until_it_arrives() {
    let mut model = SearchModel::new();
    assert_eq!(
        model.refresh(),
        Vec::new(),
        "nothing to refresh before a query"
    );
    search(&mut model, "alien", 30);
    let again = model.refresh();
    assert_eq!(again.len(), 1);
    assert_eq!(model.items().len(), 30, "held until the new first page");
    assert_eq!(
        model.apply(&again[0], serve(&again[0], 31)),
        Applied::Replaced
    );
    assert_eq!(model.total(), Some(31));
}
