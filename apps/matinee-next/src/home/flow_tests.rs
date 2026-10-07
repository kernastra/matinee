//! The signed-in flow around Home, without GPUI or a real server: startup
//! and Login land on Home, Home loads over the real HTTP client against a
//! loopback stand-in, and navigation above Home brings fresh progress back.

use std::sync::Arc;
use std::time::Duration;

use matinee_core::{HomeShelf, ItemKind};
use matinee_secrets::MemoryStore;

use super::load;
use super::model::tests::{answer_all, item, resumed, with_backdrop};
use super::model::{Applied, FocusTarget, HeroState, HomeFailure, HomeModel, ShelfState};
use crate::model::{AppModel, SESSION_ENDED};
use crate::nav::Navigation;
use crate::runtime::ServiceRuntime;
use crate::session::{Startup, accept_authentication, restore_session};
use crate::test_support::{FIXTURE_TOKEN, Reply, client, fake_jellyfin};

fn items_body(ids: &[&str]) -> Vec<u8> {
    let items: Vec<String> = ids
        .iter()
        .map(|id| format!(r#"{{"Id":"{id}","Name":"{id}","Type":"Movie"}}"#))
        .collect();
    format!(r#"{{"Items":[{}]}}"#, items.join(",")).into_bytes()
}

#[test]
fn a_restored_session_opens_home_and_asks_every_shelf() {
    let store = MemoryStore::new();
    let session = crate::model::review_session();
    matinee_jellyfin::save_session(&store, &session).unwrap();
    let mut app = AppModel::starting();
    app.apply_startup(restore_session(&store));
    assert!(app.shows_home(), "startup → restored session → Home");
    let (model, requests) = HomeModel::open();
    let shelves: Vec<HomeShelf> = requests.iter().map(|request| request.shelf).collect();
    assert_eq!(
        shelves,
        HomeShelf::ALL.to_vec(),
        "all at once, none waiting"
    );
    assert_eq!(model.hero(), HeroState::Loading);
}

#[test]
fn a_fresh_sign_in_opens_home() {
    let store = MemoryStore::new();
    let mut app = AppModel::starting();
    app.apply_startup(Startup::Unauthenticated);
    app.set_server("http://jellyfin.local:8096".into());
    app.set_username("alex".into());
    assert!(app.begin_sign_in().is_some());
    let saved = accept_authentication(&store, Ok(crate::model::review_session())).unwrap();
    app.apply_sign_in(Ok(saved));
    assert!(app.shows_home(), "Login → Home");
    assert!(!app.shows_login());
}

#[test]
fn shelves_load_over_http_with_the_session_header_and_fail_alone() {
    let (address, seen) = fake_jellyfin(vec![
        Reply::Status(200, items_body(&["resume-1"])),
        Reply::Status(500, b"server text that must not be painted".to_vec()),
        Reply::Status(200, items_body(&["movie-1", "movie-2"])),
        Reply::Status(200, b"{not json".to_vec()),
        Reply::Status(200, items_body(&[])),
    ]);
    let client = client(&address);
    let runtime = ServiceRuntime::new().unwrap();
    let (mut model, requests) = HomeModel::open();
    for request in requests {
        let client = Arc::clone(&client);
        let (_task, rx) = runtime.spawn(async move { load::run(client.as_ref(), request).await });
        model.apply(request, rx.blocking_recv().unwrap());
    }
    assert_eq!(model.items(HomeShelf::ContinueWatching).len(), 1);
    assert_eq!(
        model.shelf(HomeShelf::NextUp),
        &ShelfState::Failed(HomeFailure::Unreadable)
    );
    assert_eq!(model.items(HomeShelf::RecentMovies).len(), 2);
    assert_eq!(
        model.shelf(HomeShelf::RecentSeries),
        &ShelfState::Failed(HomeFailure::Unreadable),
        "malformed data is a shelf failure"
    );
    assert_eq!(
        model.shelf(HomeShelf::Favorites),
        &ShelfState::Ready(Vec::new())
    );
    assert!(
        matches!(model.hero(), HeroState::Ready(_)),
        "Home stays usable"
    );
    for _ in 0..5 {
        let request = seen.recv().unwrap();
        let line = request.lines().next().unwrap().to_string();
        assert!(
            !line.contains("api_key") && !line.contains(FIXTURE_TOKEN),
            "{line}"
        );
        assert!(
            request.lines().any(
                |header| header.to_ascii_lowercase().starts_with("authorization:")
                    && header.contains(FIXTURE_TOKEN)
            ),
            "{line}"
        );
    }
}

#[test]
fn an_unauthorized_home_returns_to_login() {
    let (address, _seen) = fake_jellyfin(vec![Reply::Status(401, Vec::new())]);
    let client = client(&address);
    let runtime = ServiceRuntime::new().unwrap();
    let mut app = AppModel::starting();
    app.apply_startup(Startup::Authenticated(crate::model::review_session()));
    let (mut model, requests) = HomeModel::open();
    let request = requests[0];
    let (_task, rx) = runtime.spawn(async move { load::run(client.as_ref(), request).await });
    let outcome = model.apply(request, rx.blocking_recv().unwrap());
    assert_eq!(outcome, Applied::SessionExpired);
    // The shell's response to Home's report.
    assert!(app.expire_session());
    assert!(app.shows_login());
    assert_eq!(app.notice(), Some(SESSION_ENDED));
}

/// Pages above Home in the shell's stack.
#[derive(Debug, PartialEq)]
enum Page {
    Details,
    Player,
}

#[test]
fn home_details_player_and_back_shows_new_progress() {
    let movie = || with_backdrop(item("northwind", ItemKind::Movie));
    let (mut home, requests) = HomeModel::open();
    answer_all(&mut home, &requests, |shelf| match shelf {
        HomeShelf::ContinueWatching => Ok(vec![resumed(movie(), 20)]),
        _ => Ok(vec![movie()]),
    });
    let mut nav: Navigation<Page> = Navigation::default();

    // Home → Continue Watching card → Details → Resume → Player.
    home.note_opened(HomeShelf::ContinueWatching, movie().id().clone());
    nav.push(Page::Details);
    nav.push(Page::Player);
    nav.mark_root_stale();
    // Twenty minutes later: Back to Details, then Back to Home.
    assert_eq!(nav.pop_if(|page| *page == Page::Player), Some(Page::Player));
    assert!(!nav.take_root_stale(), "Details refreshes itself first");
    assert_eq!(
        nav.pop_if(|page| *page == Page::Details),
        Some(Page::Details)
    );
    assert!(nav.take_root_stale(), "Home refreshes on return");

    let refresh = home.refresh();
    let HeroState::Ready(before) = home.hero() else {
        panic!();
    };
    assert_eq!(before.play().unwrap().label(), "Resume · 20:00", "no flash");
    answer_all(&mut home, &refresh, |shelf| match shelf {
        HomeShelf::ContinueWatching => Ok(vec![resumed(movie(), 40)]),
        _ => Ok(vec![movie()]),
    });
    let HeroState::Ready(after) = home.hero() else {
        panic!();
    };
    assert_eq!(after.play().unwrap().label(), "Resume · 40:00");
    assert_eq!(
        home.items(HomeShelf::ContinueWatching)[0]
            .user
            .resume_position(),
        Some(Duration::from_secs(40 * 60))
    );
    assert_eq!(
        home.return_focus(),
        FocusTarget::Card(HomeShelf::ContinueWatching, movie().id().clone()),
        "focus goes back to the card that was opened"
    );

    // Details and Back without playback: no refresh.
    nav.push(Page::Details);
    nav.pop_if(|_| true);
    assert!(!nav.take_root_stale());
}
