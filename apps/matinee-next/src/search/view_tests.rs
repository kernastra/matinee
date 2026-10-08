//! Search on the headless GPUI platform: the rendered screen, real focus
//! handles, real keystrokes and clicks through GPUI's dispatch, and the fake
//! clock for the debounce. Requests are answered by the fixture server
//! through the screen's own `run`/`answer` path; no socket is opened (the
//! HTTP boundary is covered in `load.rs`).
//!
//! What this cannot show: pixels. Layout, opacity, and the focus ring are
//! asserted through state (`is_stale`, the grid's logical focus, the input
//! modality), not by looking at a frame.

use std::cell::RefCell;
use std::collections::HashSet;
use std::rc::Rc;
use std::sync::Arc;
use std::time::Duration;

use atelier_ui::gpui::{
    self, KeyBinding, Keystroke, Modifiers, TestAppContext, VisualTestContext, point, size,
};
use atelier_ui::prelude::*;
use atelier_ui::{
    ComponentKeymap, FocusNext, FocusPrevious, GridLayout, install_component_keybindings,
};
use matinee_core::SearchQuery;

use super::model::{DEBOUNCE_MILLIS, IdleReason, PAGE_SIZE, Request, SearchModel, SearchState};
use super::preview::{FixtureServer, fixture_art, search};
use super::screen::{SearchEvent, SearchScreen};
use crate::artwork::{ArtworkLoader, Client};
use crate::media_grid::{Layout, poster_request};
use crate::runtime::ServiceRuntime;

const WIDTH: f32 = 1200.0;
const HEIGHT: f32 = 760.0;
const POSTER: &[u8] = include_bytes!("../../assets/review/poster.jpg");

/// What the fixture server was asked, and which queries it leaves
/// unanswered (their first page hangs, as on a slow server).
#[derive(Default)]
struct Wire {
    sent: Vec<Request>,
    hold: HashSet<String>,
}

impl Wire {
    fn pages(&self) -> Vec<(String, usize)> {
        self.sent
            .iter()
            .filter_map(|request| match request {
                Request::Page { query, page, .. } => Some((query.term().to_string(), page.start)),
                Request::Item { .. } => None,
            })
            .collect()
    }
}

struct Opened<'a> {
    view: Entity<SearchScreen>,
    wire: Rc<RefCell<Wire>>,
    events: Rc<RefCell<Vec<String>>>,
    cx: &'a mut VisualTestContext,
}

fn keymap(cx: &mut App) {
    cx.bind_keys([
        KeyBinding::new("tab", FocusNext, None),
        KeyBinding::new("shift-tab", FocusPrevious, None),
    ]);
    install_component_keybindings(
        cx,
        &ComponentKeymap {
            primary: "ctrl",
            word: "ctrl",
            emacs_line_keys: false,
            character_palette: false,
        },
    );
}

fn describe(event: &SearchEvent) -> String {
    match event {
        SearchEvent::Open(id) => format!("open {}", id.as_str()),
        SearchEvent::Navigate(destination) => format!("go {}", destination.label()),
        SearchEvent::SignOut => "sign out".into(),
        SearchEvent::SessionExpired => "expired".into(),
    }
}

/// A Search screen over `model`, answered by `server`, at 1200 × 760.
fn open_with(
    cx: &mut TestAppContext,
    model: SearchModel,
    server: FixtureServer,
    client: Option<Client>,
    loader: Option<ArtworkLoader>,
) -> Opened<'_> {
    cx.update(keymap);
    let runtime = Arc::new(ServiceRuntime::new().unwrap());
    let loader = loader.unwrap_or_else(|| ArtworkLoader::new(Arc::clone(&runtime)));
    let wire = Rc::new(RefCell::new(Wire::default()));
    let answers = {
        let wire = Rc::clone(&wire);
        Rc::new(move |request: &Request| {
            let mut wire = wire.borrow_mut();
            wire.sent.push(request.clone());
            match request {
                Request::Page { query, .. } if wire.hold.contains(query.term()) => None,
                _ => Some(server.respond(request)),
            }
        })
    };
    let fixtures = client.is_none();
    let (view, cx) = cx.add_window_view(move |_, cx| {
        let mut screen = SearchScreen::with_model(
            runtime,
            client,
            crate::model::review_session(),
            loader,
            model,
            cx,
        );
        if fixtures {
            screen.fixture_art = Some(fixture_art());
            screen.fixture_answers = Some(answers);
        }
        screen
    });
    cx.simulate_resize(size(px(WIDTH), px(HEIGHT)));
    let events = Rc::new(RefCell::new(Vec::new()));
    let log = Rc::clone(&events);
    cx.update(|_, cx| {
        cx.subscribe(&view, move |_, event: &SearchEvent, _| {
            log.borrow_mut().push(describe(event));
        })
        .detach();
    });
    cx.run_until_parked();
    redraw(&view, cx);
    Opened {
        view,
        wire,
        events,
        cx,
    }
}

fn open(cx: &mut TestAppContext, model: SearchModel, server: FixtureServer) -> Opened<'_> {
    open_with(cx, model, server, None, None)
}

/// A model already showing the results of `text` on `server`.
fn showing(text: &str, server: FixtureServer) -> SearchModel {
    let mut model = SearchModel::new();
    search(&mut model, text, server);
    model
}

/// Every page of `text` loaded.
fn fully_loaded(text: &str, server: FixtureServer) -> SearchModel {
    let mut model = showing(text, server);
    while model.items().len() < server.total {
        let request = model
            .want_more(model.items().len() - 1)
            .expect("another page");
        let response = server.respond(&request);
        model.apply(&request, response);
    }
    model
}

fn redraw(view: &Entity<SearchScreen>, cx: &mut VisualTestContext) {
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
}

fn type_chars(cx: &mut VisualTestContext, text: &str) {
    cx.update(|window, cx| {
        for ch in text.chars() {
            window.dispatch_keystroke(Keystroke::parse(&ch.to_string()).unwrap(), cx);
        }
    });
    cx.run_until_parked();
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    cx.run_until_parked();
}

fn wait(cx: &mut VisualTestContext, millis: u64) {
    cx.executor().advance_clock(Duration::from_millis(millis));
    cx.run_until_parked();
}

fn field_focused(view: &Entity<SearchScreen>, cx: &mut VisualTestContext) -> bool {
    let field = view.read_with(cx, |screen, _| screen.field_focus());
    cx.update(|window, _| field.is_focused(window))
}

fn grid_focused(view: &Entity<SearchScreen>, cx: &mut VisualTestContext) -> bool {
    let grid = view.read_with(cx, |screen, _| screen.grid());
    cx.update(|window, _| grid.focus_handle().is_focused(window))
}

/// The middle of card `index` on screen, from the grid's own arithmetic.
fn card_point(
    view: &Entity<SearchScreen>,
    cx: &mut VisualTestContext,
    index: usize,
) -> gpui::Point<Pixels> {
    let (grid, count) = view.read_with(cx, |screen, _| (screen.grid(), screen.model.items().len()));
    let (width, height) = grid.measured().expect("the grid was painted");
    let layout = GridLayout::new(Layout::for_width(WIDTH).sizing(), width, count, 0.0);
    let (x, y) = layout.origin(index);
    let scroll = -f32::from(grid.scroll().offset().y);
    let top = HEIGHT - height;
    point(
        px(x + layout.cell_width / 2.0),
        px(top + y - scroll + layout.cell_width / 2.0),
    )
}

#[gpui::test]
fn search_opens_in_the_field_and_typing_needs_no_click(cx: &mut TestAppContext) {
    let Opened { view, wire, cx, .. } = open(cx, SearchModel::new(), FixtureServer::new(240));
    assert!(field_focused(&view, cx), "the field has focus on open");
    type_chars(cx, "harbor");
    view.read_with(cx, |screen, _| {
        assert_eq!(screen.model.input(), "harbor");
        assert_eq!(screen.model.state(), SearchState::Loading, "on its way");
    });
    assert!(
        wire.borrow().sent.is_empty(),
        "nothing before the wait ends"
    );
    wait(cx, DEBOUNCE_MILLIS - 1);
    assert!(wire.borrow().sent.is_empty(), "299 ms is not enough");
    wait(cx, 1);
    assert_eq!(wire.borrow().pages(), vec![("harbor".into(), 0)]);
    view.read_with(cx, |screen, _| {
        assert_eq!(screen.model.state(), SearchState::Ready);
        assert_eq!(screen.model.items().len(), PAGE_SIZE);
    });
    assert!(
        field_focused(&view, cx),
        "results do not take focus from typing"
    );
}

#[gpui::test]
fn each_keystroke_restarts_the_wait_and_only_the_last_text_is_sent(cx: &mut TestAppContext) {
    let Opened { wire, cx, .. } = open(cx, SearchModel::new(), FixtureServer::new(240));
    for ch in ["h", "a", "r", "b"] {
        type_chars(cx, ch);
        wait(cx, DEBOUNCE_MILLIS - 50);
    }
    assert!(wire.borrow().sent.is_empty(), "typing kept the wait open");
    wait(cx, 50);
    assert_eq!(wire.borrow().pages(), vec![("harb".into(), 0)]);
    wait(cx, 10 * DEBOUNCE_MILLIS);
    assert_eq!(wire.borrow().sent.len(), 1, "no stale timer fires later");
}

#[gpui::test]
fn enter_searches_now_and_never_twice(cx: &mut TestAppContext) {
    let Opened { view, wire, cx, .. } = open(cx, SearchModel::new(), FixtureServer::new(240));
    type_chars(cx, "harbor");
    press(cx, "enter");
    assert_eq!(wire.borrow().pages(), vec![("harbor".into(), 0)], "at once");
    press(cx, "enter");
    wait(cx, 2 * DEBOUNCE_MILLIS);
    assert_eq!(wire.borrow().sent.len(), 1, "Enter and the wait send once");
    assert_eq!(
        view.read_with(cx, |screen, _| screen.model.state()),
        SearchState::Ready
    );
}

#[gpui::test]
fn arrow_keys_in_the_field_edit_the_text(cx: &mut TestAppContext) {
    let server = FixtureServer::new(240);
    let Opened { view, cx, .. } = open(cx, showing("harbor", server), server);
    // The field holds "harbor"; put the caret two from the end and type.
    press(cx, "end");
    press(cx, "left");
    press(cx, "left");
    type_chars(cx, "x");
    assert_eq!(
        view.read_with(cx, |screen, _| screen.model.input().to_string()),
        "harbxor"
    );
    press(cx, "home");
    type_chars(cx, "a");
    assert_eq!(
        view.read_with(cx, |screen, _| screen.model.input().to_string()),
        "aharbxor"
    );
    press(cx, "up");
    assert!(
        field_focused(&view, cx),
        "Up in the field stays in the field"
    );
}

#[gpui::test]
fn down_enters_the_results_and_up_from_the_first_row_returns(cx: &mut TestAppContext) {
    let server = FixtureServer::new(240);
    let Opened {
        view, events, cx, ..
    } = open(cx, showing("harbor", server), server);
    let grid = view.read_with(cx, |screen, _| screen.grid());
    assert!(field_focused(&view, cx));
    press(cx, "down");
    assert!(grid_focused(&view, cx), "Down moves into the results");
    assert_eq!(grid.focused(), Some(0), "the first title");
    assert_eq!(
        cx.update(|_, cx| input_modality(cx)),
        InputModality::Keyboard,
        "so the card wears the focus ring"
    );
    press(cx, "right");
    press(cx, "down");
    let columns = GridLayout::new(Layout::for_width(WIDTH).sizing(), WIDTH, 60, 0.0).columns;
    assert_eq!(grid.focused(), Some(1 + columns), "the grid's own keys");
    press(cx, "up");
    assert_eq!(grid.focused(), Some(1), "Up inside the grid moves a row");
    assert!(grid_focused(&view, cx));
    press(cx, "up");
    assert!(field_focused(&view, cx), "Up from the first row returns");
    assert_eq!(grid.focused(), Some(1), "the grid keeps its place");
    press(cx, "down");
    assert!(grid_focused(&view, cx));
    assert_eq!(grid.focused(), Some(1), "back where it was");
    press(cx, "enter");
    let id = view.read_with(cx, |screen, _| screen.model.items()[1].id().clone());
    assert_eq!(*events.borrow(), vec![format!("open {}", id.as_str())]);
}

#[gpui::test]
fn down_does_nothing_without_results(cx: &mut TestAppContext) {
    let Opened { view, cx, .. } = open(cx, SearchModel::new(), FixtureServer::new(0));
    press(cx, "down");
    assert!(field_focused(&view, cx), "nothing to move into");
    type_chars(cx, "zzyzx");
    press(cx, "enter");
    assert_eq!(
        view.read_with(cx, |screen, _| screen.model.state()),
        SearchState::NoResults
    );
    press(cx, "down");
    assert!(field_focused(&view, cx), "no results to move into");
}

#[gpui::test]
fn escape_clears_the_field_then_leaves_for_home(cx: &mut TestAppContext) {
    let server = FixtureServer::new(240);
    let Opened {
        view, events, cx, ..
    } = open(cx, showing("harbor", server), server);
    let (slots, _) = view.read_with(cx, |screen, _| screen.art_counts());
    assert!(slots > 0, "posters for the built cards");
    press(cx, "escape");
    view.read_with(cx, |screen, _| {
        assert_eq!(screen.model.input(), "");
        assert_eq!(screen.model.state(), SearchState::Idle(IdleReason::Empty));
        assert!(
            screen.model.items().is_empty(),
            "no old titles under a blank field"
        );
        assert!(screen.wanted().is_empty(), "artwork wants are released");
        assert_eq!(screen.art_counts(), (0, 0));
    });
    assert!(events.borrow().is_empty(), "the first Escape only clears");
    assert!(field_focused(&view, cx));
    press(cx, "escape");
    assert_eq!(*events.borrow(), vec!["go Home".to_string()]);
}

#[gpui::test]
fn clearing_drops_the_request_in_flight(cx: &mut TestAppContext) {
    let Opened { view, wire, cx, .. } = open(cx, SearchModel::new(), FixtureServer::new(240));
    wire.borrow_mut().hold.insert("harbor".into());
    type_chars(cx, "harbor");
    wait(cx, DEBOUNCE_MILLIS);
    assert!(view.read_with(cx, |screen, _| screen.model.is_loading()));
    press(cx, "escape");
    view.read_with(cx, |screen, _| {
        assert!(!screen.model.is_loading(), "the model abandoned it");
        assert!(!screen.holds_page_task(), "and the screen stopped it");
        assert_eq!(screen.model.state(), SearchState::Idle(IdleReason::Empty));
    });
}

#[gpui::test]
fn stale_titles_are_dimmed_and_do_not_open(cx: &mut TestAppContext) {
    let server = FixtureServer::new(240);
    let Opened {
        view,
        wire,
        events,
        cx,
    } = open(cx, showing("harbor", server), server);
    // The new query's first page is slow.
    wire.borrow_mut().hold.insert("harbors".into());
    press(cx, "end");
    type_chars(cx, "s");
    wait(cx, DEBOUNCE_MILLIS);
    let held = view.read_with(cx, |screen, _| {
        assert!(
            screen.model.is_stale(),
            "harbor's titles while harbors loads"
        );
        assert_eq!(screen.model.state(), SearchState::Loading);
        screen.model.items()[0].id().clone()
    });
    // A click on a dimmed card is taken by the layer over them.
    let target = card_point(&view, cx, 0);
    cx.simulate_click(target, Modifiers::none());
    cx.run_until_parked();
    assert!(events.borrow().is_empty(), "a stale card does not open");
    // Down from the field does not enter them either.
    press(cx, "down");
    assert!(field_focused(&view, cx));
    // Even with keyboard focus moved onto the grid, Enter opens nothing.
    let grid = view.read_with(cx, |screen, _| screen.grid());
    cx.update(|window, _| window.focus(grid.focus_handle()));
    cx.run_until_parked();
    press(cx, "right");
    press(cx, "enter");
    assert!(
        events.borrow().is_empty(),
        "keyboard activation is inert too"
    );
    view.read_with(cx, |screen, _| {
        assert_eq!(
            screen.model.focused_index(),
            None,
            "stale focus is not noted"
        )
    });

    // The new page arrives: the grid starts at the top, and cards open.
    let request = wire
        .borrow()
        .sent
        .iter()
        .rev()
        .find(|request| matches!(request, Request::Page { query, .. } if query.term() == "harbors"))
        .cloned()
        .expect("harbors was asked");
    view.update(cx, |screen, cx| {
        let response = server.respond(&request);
        screen.answer(&request, response, cx);
    });
    cx.run_until_parked();
    redraw(&view, cx);
    let fresh = view.read_with(cx, |screen, _| {
        assert!(!screen.model.is_stale());
        assert_eq!(screen.model.state(), SearchState::Ready);
        screen.model.items()[0].id().clone()
    });
    assert_ne!(fresh, held, "harbors has its own titles");
    // The grid still has keyboard focus, so it holds a cell: the first.
    assert_eq!(
        grid.focused().unwrap_or(0),
        0,
        "a new query starts at the top"
    );
    assert!(f32::from(grid.scroll().offset().y) > -0.5);
    let target = card_point(&view, cx, 0);
    cx.simulate_click(target, Modifiers::none());
    cx.run_until_parked();
    assert_eq!(*events.borrow(), vec![format!("open {}", fresh.as_str())]);
}

#[gpui::test]
fn a_click_on_a_card_opens_it(cx: &mut TestAppContext) {
    let server = FixtureServer::new(240);
    let Opened {
        view, events, cx, ..
    } = open(cx, showing("harbor", server), server);
    let target = card_point(&view, cx, 2);
    cx.simulate_click(target, Modifiers::none());
    cx.run_until_parked();
    let id = view.read_with(cx, |screen, _| screen.model.items()[2].id().clone());
    assert_eq!(*events.borrow(), vec![format!("open {}", id.as_str())]);
    assert!(grid_focused(&view, cx), "the click focused the grid");
    assert_eq!(
        cx.update(|_, cx| input_modality(cx)),
        InputModality::Pointer,
        "no keyboard ring after a click"
    );
}

#[gpui::test]
fn resizing_keeps_the_logical_focus(cx: &mut TestAppContext) {
    let server = FixtureServer::new(240);
    let Opened { view, cx, .. } = open(cx, fully_loaded("harbor", server), server);
    let grid = view.read_with(cx, |screen, _| screen.grid());
    press(cx, "down");
    for _ in 0..6 {
        press(cx, "down");
    }
    let focused = grid.focused().expect("focused");
    assert!(focused > 30, "{focused}");
    cx.simulate_resize(size(px(960.0), px(620.0)));
    cx.run_until_parked();
    redraw(&view, cx);
    assert_eq!(grid.focused(), Some(focused), "the same title");
    let built = grid.rendered_cells();
    let visible = view.read_with(cx, |screen, _| screen.wanted().len());
    assert!(built > 0 && visible <= built);
    cx.simulate_resize(size(px(1920.0), px(1080.0)));
    cx.run_until_parked();
    redraw(&view, cx);
    assert_eq!(grid.focused(), Some(focused));
    assert!(grid_focused(&view, cx), "focus stayed in the grid");
}

#[gpui::test]
fn built_cards_follow_the_window_not_the_result_count(cx: &mut TestAppContext) {
    // 0, 1, 60, 61, 100, 1,000, and 10,000 titles, every page loaded.
    for total in [0, 1, 60, 61, 100, 1_000, 10_000] {
        let server = FixtureServer::new(total);
        let model = if total == 0 {
            showing("the", server)
        } else {
            fully_loaded("the", server)
        };
        let mut cx = cx.clone();
        let Opened { view, wire, cx, .. } = open(&mut cx, model, server);
        let grid = view.read_with(cx, |screen, _| screen.grid());
        let built = grid.rendered_cells();
        if total <= 30 {
            assert_eq!(built, total, "{total} titles: all built");
        } else {
            assert!(built < 60, "{total} titles: {built} built");
        }
        // Scroll to the very end: still a window's worth.
        grid.scroll().set_offset(point(px(0.0), px(-10_000_000.0)));
        redraw(&view, cx);
        assert!(
            grid.rendered_cells() <= total.min(59),
            "{total}: {} built at the end",
            grid.rendered_cells()
        );
        let (slots, _) = view.read_with(cx, |screen, _| screen.art_counts());
        assert!(slots <= grid.rendered_cells(), "{total}: {slots} posters");
        assert!(
            wire.borrow().sent.is_empty(),
            "{total}: all loaded, nothing asked"
        );
    }
}

#[gpui::test]
fn scrolling_ten_thousand_results_pages_one_at_a_time(cx: &mut TestAppContext) {
    let server = FixtureServer::new(10_000);
    let Opened { view, wire, cx, .. } = open(cx, showing("the", server), server);
    let grid = view.read_with(cx, |screen, _| screen.grid());
    for _ in 0..40 {
        grid.scroll().set_offset(point(px(0.0), px(-10_000_000.0)));
        redraw(&view, cx);
        assert!(grid.rendered_cells() < 60);
    }
    let pages = wire.borrow().pages();
    let loaded = view.read_with(cx, |screen, _| screen.model.items().len());
    assert_eq!(
        loaded,
        pages.len() * PAGE_SIZE + PAGE_SIZE,
        "every page asked was used"
    );
    let starts: Vec<usize> = pages.iter().map(|(_, start)| *start).collect();
    let expected: Vec<usize> = (1..=pages.len()).map(|page| page * PAGE_SIZE).collect();
    assert_eq!(starts, expected, "in order, never the same page twice");
    assert!(loaded < 10_000, "only what was scrolled to is held");
}

#[gpui::test]
fn query_replacement_moves_every_artwork_want(cx: &mut TestAppContext) {
    let server = FixtureServer::new(240);
    let Opened { view, cx, .. } = open(cx, showing("harbor", server), server);
    let before: Vec<String> = view.read_with(cx, |screen, _| screen.wanted().to_vec());
    assert!(!before.is_empty());
    press(cx, "end");
    type_chars(cx, "s");
    press(cx, "enter");
    redraw(&view, cx);
    let (after, slots) = view.read_with(cx, |screen, _| {
        (screen.wanted().to_vec(), screen.art_counts().0)
    });
    assert!(!after.is_empty());
    assert!(
        after.iter().all(|url| !before.contains(url)),
        "no poster of the old query is still wanted"
    );
    assert!(slots <= after.len(), "old slots are gone: {slots}");
    // No address carries the token.
    for url in &after {
        assert!(!url.contains(crate::test_support::FIXTURE_TOKEN));
        assert!(!url.contains("api_key"));
    }
}

#[gpui::test]
fn the_shared_cache_serves_search_posters_on_return(cx: &mut TestAppContext) {
    // 150 decoded fixture posters fit in the 96 MiB cache together.
    let server = FixtureServer::new(150);
    let model = fully_loaded("harbor", server);
    let runtime = Arc::new(ServiceRuntime::new().unwrap());
    let loader = ArtworkLoader::new(Arc::clone(&runtime));
    let session = crate::model::review_session();
    let image = DecodedImage::decode(POSTER, MAX_DECODED_SIDE).unwrap();
    cx.update(|cx| {
        let urls = session.artwork();
        for item in model.items() {
            if let Some(request) = poster_request(item, &urls) {
                loader.remember(&request.url, &image, cx);
            }
        }
    });
    // A client that is never asked for anything: every page is loaded and
    // every poster is cached.
    let client = crate::test_support::client(session.server_url());
    let Opened { view, cx, .. } = open_with(cx, model, server, Some(client), Some(loader.clone()));
    let grid = view.read_with(cx, |screen, _| screen.grid());
    let (slots, in_flight) = view.read_with(cx, |screen, _| screen.art_counts());
    assert!(slots > 0 && slots <= grid.rendered_cells());
    assert_eq!(in_flight, 0, "every poster came from the cache");
    let first = view.read_with(cx, |screen, _| screen.wanted()[0].clone());
    grid.scroll().set_offset(point(px(0.0), px(-1_000_000.0)));
    redraw(&view, cx);
    assert!(!view.read_with(cx, |screen, _| screen.wanted().contains(&first)));
    let before = loader.stats();
    grid.scroll().set_offset(point(px(0.0), px(0.0)));
    redraw(&view, cx);
    assert!(view.read_with(cx, |screen, _| screen.wanted().contains(&first)));
    let after = loader.stats();
    assert!(after.hits > before.hits, "{before:?} → {after:?}");
    assert_eq!(after.misses, 0, "no fetch for a poster seen before");
}

#[gpui::test]
fn a_failed_first_page_offers_try_again_and_recovers(cx: &mut TestAppContext) {
    let Opened { view, wire, cx, .. } = open(cx, SearchModel::new(), FixtureServer::new(240));
    // The fixture server is down for this query: answer with a failure.
    wire.borrow_mut().hold.insert("harbor".into());
    type_chars(cx, "harbor");
    press(cx, "enter");
    let request = wire.borrow().sent[0].clone();
    view.update(cx, |screen, cx| {
        let failure = super::model::Response::Page(Err(super::model::SearchFailure::Unreachable));
        screen.answer(&request, failure, cx);
    });
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |screen, _| screen.model.state()),
        SearchState::Failed(super::model::SearchFailure::Unreachable)
    );
    // Enter in the field is Try again; this time the server answers.
    wire.borrow_mut().hold.clear();
    assert!(field_focused(&view, cx));
    press(cx, "enter");
    assert_eq!(
        view.read_with(cx, |screen, _| screen.model.state()),
        SearchState::Ready
    );
    assert_eq!(wire.borrow().pages().len(), 2);
}

#[test]
fn the_fixture_answers_by_query() {
    // Guards the harness: two queries have disjoint titles.
    let server = FixtureServer::new(240);
    let a = SearchQuery::parse("harbor").unwrap();
    let b = SearchQuery::parse("harbors").unwrap();
    let ids: HashSet<_> = (0..240).map(|i| server.title(&a, i).id().clone()).collect();
    assert!((0..240).all(|i| !ids.contains(server.title(&b, i).id())));
}

#[gpui::test]
fn every_review_scene_opens_at_every_review_size(cx: &mut TestAppContext) {
    use super::preview::{SearchPreview, preview_model};
    let scenes = [
        SearchPreview::Empty,
        SearchPreview::TooShort,
        SearchPreview::Typing,
        SearchPreview::Loading,
        SearchPreview::Results,
        SearchPreview::ManyResults,
        SearchPreview::NoResults,
        SearchPreview::Error,
        SearchPreview::PartialPage,
        SearchPreview::Stale,
        SearchPreview::Episodes,
    ];
    let sizes = [
        (960.0, 620.0, 5),
        (1200.0, 760.0, 6),
        (1440.0, 900.0, 7),
        (1920.0, 1080.0, 10),
    ];
    cx.update(keymap);
    for scene in scenes {
        for (width, height, columns) in sizes {
            let runtime = Arc::new(ServiceRuntime::new().unwrap());
            let loader = ArtworkLoader::new(Arc::clone(&runtime));
            let (view, cx) =
                cx.add_window_view(move |_, cx| SearchScreen::preview(runtime, loader, scene, cx));
            cx.simulate_resize(size(px(width), px(height)));
            cx.run_until_parked();
            redraw(&view, cx);
            let label = format!("{scene:?} at {width}×{height}");
            let expected = preview_model(scene).state();
            let grid = view.read_with(cx, |screen, _| screen.grid());
            view.read_with(cx, |screen, _| {
                assert_eq!(screen.model.state(), expected, "{label}");
            });
            let shows_grid =
                matches!(expected, SearchState::Ready) || scene == SearchPreview::Stale;
            if shows_grid {
                let built = grid.rendered_cells();
                assert!(built > 0 && built <= 8 * columns, "{label}: {built} cards");
                let (measured, _) = grid.measured().expect("painted");
                assert_eq!(
                    GridLayout::new(Layout::for_width(width).sizing(), measured, 1, 0.0).columns,
                    columns,
                    "{label}"
                );
                let (slots, _) = view.read_with(cx, |screen, _| screen.art_counts());
                assert!(slots <= built, "{label}: {slots} posters");
            } else {
                let (slots, _) = view.read_with(cx, |screen, _| screen.art_counts());
                assert_eq!(slots, 0, "{label}: no grid, no posters");
            }
            if scene == SearchPreview::ManyResults {
                assert!(
                    grid_focused(&view, cx),
                    "{label}: keyboard focus on the results"
                );
                assert_eq!(grid.focused(), Some(251), "{label}");
            } else {
                assert!(field_focused(&view, cx), "{label}: the field has focus");
            }
        }
    }
}
