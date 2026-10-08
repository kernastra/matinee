//! Library on the headless GPUI platform: the grid builds tens of cards,
//! artwork follows the built window, and the shared cache serves cards that
//! come back into view.

use std::sync::Arc;

use atelier_ui::gpui::{self, TestAppContext, VisualTestContext, point};
use atelier_ui::prelude::*;
use matinee_core::LibraryKind;

use super::preview::loaded_model;
use super::screen::LibraryScreen;
use crate::artwork::{ArtworkLoader, Client};
use crate::media_grid::poster_request;
use crate::runtime::ServiceRuntime;

const POSTER: &[u8] = include_bytes!("../../assets/review/poster.jpg");

/// A Library holding `total` titles, every page loaded. With `cached`,
/// every poster is already in the shared cache (so `total` must fit in it)
/// and the screen has a client that never needs the network; without it
/// the screen has no client at all.
fn open(
    cx: &mut TestAppContext,
    total: usize,
    cached: bool,
) -> (Entity<LibraryScreen>, ArtworkLoader, &mut VisualTestContext) {
    let runtime = Arc::new(ServiceRuntime::new().unwrap());
    let loader = ArtworkLoader::new(Arc::clone(&runtime));
    let session = crate::model::review_session();
    let client: Option<Client> = cached.then(|| {
        // Never asked for anything: every poster is in the cache.
        crate::test_support::client(session.server_url())
    });
    let model = loaded_model(LibraryKind::Movies, total);
    let image = DecodedImage::decode(POSTER, MAX_DECODED_SIDE).unwrap();
    cx.update(|cx| {
        if !cached {
            return;
        }
        let urls = session.artwork();
        for item in model.items() {
            if let Some(request) = poster_request(item, &urls) {
                loader.remember(&request.url, &image, cx);
            }
        }
    });
    let built = loader.clone();
    let (view, cx) = cx.add_window_view(move |_, cx| {
        LibraryScreen::with_model(runtime, client, session, built, model, cx)
    });
    cx.run_until_parked();
    (view, loader, cx)
}

fn redraw(view: &Entity<LibraryScreen>, cx: &mut VisualTestContext) {
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
}

#[gpui::test]
fn artwork_follows_the_built_cards_and_the_cache_serves_returns(cx: &mut TestAppContext) {
    // 150 decoded fixture posters fit in the 96 MiB cache together.
    let (view, loader, cx) = open(cx, 150, true);
    let grid = view.read_with(cx, |screen, _| screen.grid(LibraryKind::Movies));
    let built = grid.rendered_cells();
    assert!(built > 0 && built < 100, "built {built} of 150");
    let (slots, in_flight) = view.read_with(cx, |screen, _| screen.art_counts());
    assert!(
        slots > 0 && slots <= built,
        "{slots} slots for {built} cards"
    );
    assert_eq!(in_flight, 0, "every poster came from the cache");
    let first = view.read_with(cx, |screen, _| screen.wanted().first().cloned().unwrap());

    // To the end: the wanted set moves with the window.
    grid.scroll().set_offset(point(px(0.0), px(-1_000_000.0)));
    redraw(&view, cx);
    let wanted = view.read_with(cx, |screen, _| screen.wanted().to_vec());
    assert!(!wanted.contains(&first), "the first poster left the window");
    assert!(wanted.iter().any(|url| url.contains("film-149")));
    let (slots, _) = view.read_with(cx, |screen, _| screen.art_counts());
    assert!(
        slots <= grid.rendered_cells(),
        "the screen holds only the window"
    );

    // Back to the top: the first poster is served by the shared cache.
    let before = loader.stats();
    grid.scroll().set_offset(point(px(0.0), px(0.0)));
    redraw(&view, cx);
    let wanted = view.read_with(cx, |screen, _| screen.wanted().to_vec());
    assert!(wanted.contains(&first));
    let after = loader.stats();
    assert!(
        after.hits > before.hits,
        "cache hits on return: {before:?} {after:?}"
    );
    assert_eq!(after.misses, 0, "no fetch for a poster seen before");
    let (_, in_flight) = view.read_with(cx, |screen, _| screen.art_counts());
    assert_eq!(in_flight, 0);
}

#[gpui::test]
fn ten_thousand_titles_still_build_tens_of_cards(cx: &mut TestAppContext) {
    let (view, _, cx) = open(cx, 10_000, false);
    let grid = view.read_with(cx, |screen, _| screen.grid(LibraryKind::Movies));
    assert!(grid.rendered_cells() < 100, "{}", grid.rendered_cells());
    grid.scroll().set_offset(point(px(0.0), px(-400_000.0)));
    redraw(&view, cx);
    let built = grid.rendered_cells();
    assert!(built < 100, "{built}");
    let first = view.read_with(cx, |screen, _| {
        screen.wanted().first().cloned().unwrap_or_default()
    });
    assert!(
        first.contains("film-"),
        "posters wanted deep in the library"
    );
    let wanted = view.read_with(cx, |screen, _| screen.wanted().len());
    assert!(wanted <= built, "{wanted} posters wanted for {built} cards");
}

#[gpui::test]
fn keyboard_activation_opens_the_focused_title(cx: &mut TestAppContext) {
    cx.update(|cx| {
        install_component_keybindings(
            cx,
            &ComponentKeymap {
                primary: "ctrl",
                word: "ctrl",
                emacs_line_keys: false,
                character_palette: false,
            },
        )
    });
    let (view, _, cx) = open(cx, 300, false);
    let grid = view.read_with(cx, |screen, _| screen.grid(LibraryKind::Movies));
    cx.update(|window, cx| {
        note_keyboard_navigation(cx);
        window.focus(grid.focus_handle());
    });
    cx.run_until_parked();
    for key in ["right", "down", "enter"] {
        cx.simulate_keystrokes(key);
        cx.run_until_parked();
    }
    let focused = grid.focused().unwrap();
    assert!(focused > 1, "one right and one row down: {focused}");
    view.read_with(cx, |screen, _| {
        let id = screen.model.items()[focused].id().clone();
        assert_eq!(screen.model.focused(), Some(&id), "the opened title");
    });
}
