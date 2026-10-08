//! Calendar on the headless GPUI platform: the rendered screen, real focus,
//! real keystrokes through GPUI's dispatch, and the state a person sees.
//!
//! These screens are built from fixtures (`CalendarPreview`), so they never
//! open a socket and their answers are fixed. The HTTP boundary and the
//! request lifecycle are covered in `load_tests.rs` and the model tests.
//!
//! What this cannot show: pixels. Layout is exercised by drawing every scene
//! at the standard window sizes, and the rest is asserted through state
//! (the model, the grid's focused cell, and what focus handles report).

use std::sync::Arc;

use atelier_ui::gpui::{self, Entity, KeyBinding, TestAppContext, VisualTestContext, size};
use atelier_ui::prelude::*;
use atelier_ui::{ComponentKeymap, FocusNext, FocusPrevious, install_component_keybindings};
use chrono::{Datelike, NaiveDate};
use matinee_integrations::IntegrationProvider;

use super::grid::{month_start, shift_month};
use super::model::{Link, Overview, SourceFailure, SourceStatus};
use super::preview::CalendarPreview;
use super::screen::{
    CalendarScreen, Step, day_number_color, empty_day_line, grid_sizing, source_line,
};
use crate::artwork::ArtworkLoader;
use crate::runtime::ServiceRuntime;

const WIDTH: f32 = 1200.0;
const HEIGHT: f32 = 760.0;
const RADARR: IntegrationProvider = IntegrationProvider::Radarr;
const SONARR: IntegrationProvider = IntegrationProvider::Sonarr;

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

/// A Calendar on `scene`, shown the way the shell shows a root, at 1200 × 760.
fn open(
    cx: &mut TestAppContext,
    scene: CalendarPreview,
) -> (Entity<CalendarScreen>, &mut VisualTestContext) {
    cx.update(keymap);
    let runtime = Arc::new(ServiceRuntime::new().unwrap());
    let loader = ArtworkLoader::new(Arc::clone(&runtime));
    let (view, cx) = cx.add_window_view(move |_, cx| scene.screen(runtime, loader, cx));
    cx.simulate_resize(size(px(WIDTH), px(HEIGHT)));
    // What the shell does when the root is first shown, without a visit: the
    // scene's fixture answers are its state, and a visit would clear failures.
    view.update(cx, |screen, _| screen.settle_on_selected_day());
    cx.run_until_parked();
    redraw(&view, cx);
    (view, cx)
}

fn redraw(view: &Entity<CalendarScreen>, cx: &mut VisualTestContext) {
    view.update(cx, |_, cx| cx.notify());
    cx.run_until_parked();
}

fn press(cx: &mut VisualTestContext, keys: &str) {
    cx.simulate_keystrokes(keys);
    cx.run_until_parked();
}

fn grid_focused(view: &Entity<CalendarScreen>, cx: &mut VisualTestContext) -> bool {
    let handle = view.read_with(cx, |screen, _| screen.grid_state().focus_handle().clone());
    cx.update(|window, _| handle.is_focused(window))
}

fn event_ids(
    view: &Entity<CalendarScreen>,
    cx: &mut VisualTestContext,
    when: NaiveDate,
) -> Vec<String> {
    view.read_with(cx, |screen, _| {
        screen
            .model
            .day_events(when)
            .iter()
            .map(|event| event.id.clone())
            .collect()
    })
}

#[gpui::test]
fn the_month_opens_on_todays_day_with_the_grid_focused(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Populated);
    let (today, selected, month) = view.read_with(cx, |screen, _| {
        (
            screen.model.today(),
            screen.model.selected(),
            screen.model.month(),
        )
    });
    assert_eq!(today, selected, "today is selected on open");
    assert_eq!(month.day(), 1);
    assert_eq!(month.month(), today.month());
    assert!(
        grid_focused(&view, cx),
        "the month takes focus, so arrows work at once"
    );
    let focused = view.read_with(cx, |screen, _| screen.grid_state().focused());
    let index = view.read_with(cx, |screen, _| screen.model.window().index_of(today));
    assert_eq!(focused, index, "the grid's focused cell is today");
}

#[gpui::test]
fn arrow_keys_move_the_focused_day_and_enter_selects_it(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Populated);
    let before = view.read_with(cx, |screen, _| screen.grid_state().focused());
    press(cx, "right");
    let after = view.read_with(cx, |screen, _| screen.grid_state().focused());
    assert_eq!(after, before.map(|index| index + 1), "right moves one day");
    let before_selection = view.read_with(cx, |screen, _| screen.model.selected());
    press(cx, "enter");
    let selected = view.read_with(cx, |screen, _| screen.model.selected());
    assert_ne!(selected, before_selection, "Enter selects the focused day");
    assert_eq!(
        selected,
        view.read_with(cx, |screen, _| screen
            .model
            .window()
            .day(after.unwrap() as u64)),
        "and it is the day the grid had focused"
    );
}

#[gpui::test]
fn down_moves_a_week_and_home_returns_to_the_first_cell(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Populated);
    let start = view
        .read_with(cx, |screen, _| screen.grid_state().focused())
        .unwrap();
    press(cx, "down");
    assert_eq!(
        view.read_with(cx, |screen, _| screen.grid_state().focused()),
        Some(start + 7)
    );
    press(cx, "home");
    assert_eq!(
        view.read_with(cx, |screen, _| screen.grid_state().focused()),
        Some(0)
    );
}

#[gpui::test]
fn next_and_previous_move_the_month_and_keep_the_selection_valid(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Populated);
    let first = view.read_with(cx, |screen, _| screen.model.month());
    view.update(cx, |screen, cx| screen.step(Step::Next, cx));
    cx.run_until_parked();
    let (month, selected) = view.read_with(cx, |screen, _| {
        (screen.model.month(), screen.model.selected())
    });
    assert_eq!(month, shift_month(first, 1), "the next calendar month");
    assert_eq!(
        month_start(selected),
        month,
        "the selection is in the month shown"
    );
    view.update(cx, |screen, cx| screen.step(Step::Previous, cx));
    view.update(cx, |screen, cx| screen.step(Step::Previous, cx));
    assert_eq!(
        view.read_with(cx, |screen, _| screen.model.month()),
        shift_month(first, -1),
        "two steps back from next is one month before the start"
    );
}

#[gpui::test]
fn today_returns_to_the_current_month_from_anywhere(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Populated);
    let today = view.read_with(cx, |screen, _| screen.model.today());
    view.update(cx, |screen, cx| screen.step(Step::Next, cx));
    view.update(cx, |screen, cx| screen.step(Step::Next, cx));
    view.update(cx, |screen, cx| screen.step(Step::Today, cx));
    let (month, selected) = view.read_with(cx, |screen, _| {
        (screen.model.month(), screen.model.selected())
    });
    assert_eq!(selected, today);
    assert_eq!(month, today.with_day(1).unwrap());
}

#[gpui::test]
fn a_selected_day_lists_its_releases_movies_first(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::SelectedDay);
    let selected = view.read_with(cx, |screen, _| screen.model.selected());
    assert_eq!(selected.day(), 14, "the fixture's day");
    let ids = event_ids(&view, cx, selected);
    assert_eq!(ids.len(), 3, "a movie and two episodes");
    assert_eq!(
        ids[0],
        format!("radarr-104-theatrical"),
        "movies come first"
    );
    let focused = view.read_with(cx, |screen, _| {
        screen.model.focused_event().map(|event| event.id.clone())
    });
    assert_eq!(
        focused.as_deref(),
        Some("sonarr-204"),
        "the chosen episode is in the panel"
    );
}

#[gpui::test]
fn covers_are_kept_for_the_selected_day_only_and_follow_the_selection(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::SelectedDay);
    let covers = |view: &Entity<CalendarScreen>, cx: &mut VisualTestContext| {
        view.read_with(cx, |screen, _| {
            let mut urls: Vec<String> = screen.art.keys().cloned().collect();
            urls.sort();
            urls
        })
    };
    assert_eq!(
        covers(&view, cx),
        vec![
            "https://covers.example/radarr/104.jpg",
            "https://covers.example/sonarr/403.jpg",
            "https://covers.example/sonarr/405.jpg",
        ],
        "the 14th's three covers, before any action"
    );
    view.update(cx, |screen, cx| {
        screen.set_filter(super::event::MediaFilter::Movies, cx)
    });
    assert_eq!(
        covers(&view, cx),
        vec!["https://covers.example/radarr/104.jpg"],
        "a filtered-out release lets its cover go"
    );
    view.update(cx, |screen, cx| {
        let index = screen
            .model
            .window()
            .index_of(day_of_month(&screen.model, 2));
        screen.select_index(index.expect("the 2nd"), cx);
    });
    assert!(
        covers(&view, cx).is_empty(),
        "a day without releases holds none"
    );
}

#[gpui::test]
fn choosing_a_day_outside_the_shown_month_shows_its_month(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Populated);
    // Cell 0 is the leading day of the grid, which belongs to the month before.
    let leading = view.read_with(cx, |screen, _| screen.model.window().start());
    view.update(cx, |screen, cx| screen.select_index(0, cx));
    let (month, selected) = view.read_with(cx, |screen, _| {
        (screen.model.month(), screen.model.selected())
    });
    assert_eq!(selected, leading, "the leading day is the one chosen");
    assert_eq!(
        month,
        month_start(leading),
        "and its month is the one shown"
    );
    let (focused, index) = view.read_with(cx, |screen, _| {
        (
            screen.grid_state().focused(),
            screen.model.window().index_of(selected),
        )
    });
    assert_eq!(focused, index, "focus is on the chosen day in its month");
}

#[gpui::test]
fn enter_on_a_day_of_the_next_month_keeps_focus_on_that_day(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Populated);
    // The last cell always belongs to the following month.
    let last = view.read_with(cx, |screen, _| screen.model.window().last());
    press(cx, "end");
    press(cx, "enter");
    let (selected, month, focused, index) = view.read_with(cx, |screen, _| {
        (
            screen.model.selected(),
            screen.model.month(),
            screen.grid_state().focused(),
            screen.model.window().index_of(screen.model.selected()),
        )
    });
    assert_eq!(selected, last);
    assert_eq!(month, month_start(last), "the next month is shown");
    assert_eq!(focused, index, "the focus ring is on the chosen day");
    assert!(grid_focused(&view, cx));
}

#[gpui::test]
fn arrows_move_day_by_day_across_weeks_and_months(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Populated);
    let focused = |view: &Entity<CalendarScreen>, cx: &mut VisualTestContext| {
        view.read_with(cx, |screen, _| {
            let index = screen.grid_state().focused().expect("a focused cell");
            screen.model.window().day(index as u64)
        })
    };
    // Saturday of the first week: Right is the Sunday after it.
    view.update(cx, |screen, cx| {
        screen.grid_state().focus_index(Some(6));
        cx.notify();
    });
    cx.run_until_parked();
    let saturday = focused(&view, cx);
    press(cx, "right");
    assert_eq!(focused(&view, cx), saturday.succ_opt().unwrap());
    press(cx, "left");
    assert_eq!(focused(&view, cx), saturday, "and Left goes back");

    // The last cell: Right shows the month of the next day, focused on it.
    press(cx, "end");
    let last = focused(&view, cx);
    press(cx, "right");
    let next = last.succ_opt().unwrap();
    assert_eq!(focused(&view, cx), next);
    assert_eq!(
        view.read_with(cx, |screen, _| screen.model.month()),
        month_start(next)
    );

    // The first row: Up shows the week before, in the month before.
    press(cx, "home");
    let first = focused(&view, cx);
    press(cx, "up");
    let week_before = first - chrono::TimeDelta::days(7);
    assert_eq!(focused(&view, cx), week_before);
    assert_eq!(
        view.read_with(cx, |screen, _| screen.model.month()),
        month_start(week_before)
    );
    assert!(grid_focused(&view, cx), "the grid keeps the keyboard");
}

#[gpui::test]
fn the_populated_month_places_movies_and_episodes_on_their_days(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Populated);
    let month = view.read_with(cx, |screen, _| screen.model.month());
    let counts = view.read_with(cx, |screen, _| {
        (
            screen.model.grid_events().len(),
            screen.model.month_release_count(),
            screen.model.settled(),
        )
    });
    assert!(
        counts.0 >= 8,
        "fixture releases are on the grid: {}",
        counts.0
    );
    assert_eq!(counts.1, counts.0, "every fixture release is in this month");
    assert!(counts.2, "both sources answered");
    assert_eq!(
        event_ids(&view, cx, month.with_day(4).unwrap()),
        vec!["radarr-101-theatrical"]
    );
}

#[gpui::test]
fn the_next_month_draws_releases_on_the_days_its_grid_borrows(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::NextMonth);
    let (month, today, window) = view.read_with(cx, |screen, _| {
        (
            screen.model.month(),
            screen.model.today(),
            screen.model.window(),
        )
    });
    assert_eq!(
        month,
        shift_month(month_start(today), 1),
        "the month after this one"
    );
    assert_eq!(
        event_ids(&view, cx, window.start()),
        vec!["radarr-105-digital"]
    );
    assert_eq!(event_ids(&view, cx, window.last()), vec!["sonarr-208"]);
    let (on_grid, in_month) = view.read_with(cx, |screen, _| {
        (
            screen.model.grid_events().len(),
            screen.model.month_release_count(),
        )
    });
    // The trailing day always belongs to the next month; the leading one
    // does unless the month starts on a Sunday.
    let borrowed = [window.start(), window.last()]
        .into_iter()
        .filter(|day| month_start(*day) != month)
        .count();
    assert!(borrowed >= 1);
    assert_eq!(
        on_grid,
        in_month + borrowed,
        "borrowed days are drawn but not counted in the month"
    );
}

#[gpui::test]
fn an_empty_month_is_known_to_be_empty(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Empty);
    let (grid, settled, overview) = view.read_with(cx, |screen, _| {
        (
            screen.model.grid_events().len(),
            screen.model.settled(),
            screen.model.overview(),
        )
    });
    assert_eq!(grid, 0);
    assert!(settled, "both sources answered with nothing");
    assert_eq!(
        overview,
        Overview::Connected,
        "connected and empty is not disconnected"
    );
}

#[gpui::test]
fn a_partial_failure_is_shown_for_its_source_and_the_other_releases_stay(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::PartialFailure);
    let (radarr, sonarr, grid, settled) = view.read_with(cx, |screen, _| {
        (
            screen.model.status(RADARR),
            screen.model.status(SONARR),
            screen.model.grid_events().len(),
            screen.model.settled(),
        )
    });
    assert_eq!(radarr, SourceStatus::Ready);
    assert_eq!(sonarr, SourceStatus::Failed(SourceFailure::Unavailable));
    assert!(grid > 0, "Radarr's movies are still shown");
    assert!(!settled, "a failed month is not an empty month");
    assert_eq!(
        source_line(SONARR, sonarr),
        "Sonarr couldn't be reached",
        "the line names the cause without server text"
    );
    assert!(
        view.read_with(cx, |screen, _| screen
            .model
            .grid_events()
            .iter()
            .all(|event| event.is_movie())),
        "no episodes from the failed source"
    );
}

#[test]
fn each_failure_has_its_own_plain_sentence() {
    assert_eq!(
        source_line(RADARR, SourceStatus::Failed(SourceFailure::Unauthorized)),
        "Radarr rejected the saved key"
    );
    assert_eq!(
        source_line(SONARR, SourceStatus::Failed(SourceFailure::Malformed)),
        "Sonarr sent a calendar Matinee could not read"
    );
    assert_eq!(
        source_line(RADARR, SourceStatus::Unlinked),
        "Radarr isn't connected"
    );
    assert_eq!(
        source_line(SONARR, SourceStatus::Loading),
        "Loading Sonarr…"
    );
    assert_eq!(source_line(SONARR, SourceStatus::Ready), "Sonarr connected");
    assert_eq!(
        source_line(SONARR, SourceStatus::Checking),
        "Checking Sonarr…"
    );
}

#[gpui::test]
fn a_day_without_releases_names_the_source_that_failed_instead_of_waiting(cx: &mut TestAppContext) {
    let line = |scene, cx: &mut TestAppContext| {
        let (view, cx) = open(cx, scene);
        view.read_with(cx, |screen, _| empty_day_line(&screen.model))
    };
    // The 2nd has no fixture release in either scene.
    let (view, vcx) = open(cx, CalendarPreview::PartialFailure);
    view.update(vcx, |screen, cx| {
        let index = screen
            .model
            .window()
            .index_of(day_of_month(&screen.model, 2));
        screen.select_index(index.expect("the 2nd is in the grid"), cx);
    });
    assert_eq!(
        view.read_with(vcx, |screen, _| empty_day_line(&screen.model)),
        Some("Releases from Sonarr could not be loaded for this month.".to_string()),
        "Sonarr failed: the day is not known to be empty, and nothing is still loading"
    );
    assert_eq!(
        line(CalendarPreview::Error, cx),
        Some("Releases from Radarr and Sonarr could not be loaded for this month.".to_string())
    );
    assert_eq!(
        line(CalendarPreview::Loading, cx),
        Some("Waiting for every connected source to answer this month.".to_string())
    );
    assert_eq!(
        line(CalendarPreview::Empty, cx),
        Some("Nothing is scheduled for this day.".to_string())
    );
    assert_eq!(line(CalendarPreview::Disconnected, cx), None);
}

#[gpui::test]
fn radarr_only_shows_movies_and_no_episodes(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::RadarrOnly);
    let (sonarr, episodes) = view.read_with(cx, |screen, _| {
        (
            screen.model.status(SONARR),
            screen
                .model
                .grid_events()
                .iter()
                .filter(|event| !event.is_movie())
                .count(),
        )
    });
    assert_eq!(sonarr, SourceStatus::Unlinked);
    assert_eq!(episodes, 0);
}

#[gpui::test]
fn sonarr_only_shows_episodes_and_no_movies(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::SonarrOnly);
    let (radarr, movies, episodes) = view.read_with(cx, |screen, _| {
        (
            screen.model.status(RADARR),
            screen
                .model
                .grid_events()
                .iter()
                .filter(|event| event.is_movie())
                .count(),
            screen
                .model
                .grid_events()
                .iter()
                .filter(|event| !event.is_movie())
                .count(),
        )
    });
    assert_eq!(radarr, SourceStatus::Unlinked);
    assert_eq!(movies, 0);
    assert!(episodes > 0);
}

#[gpui::test]
fn a_disconnected_calendar_says_so_and_asks_nothing(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Disconnected);
    let (overview, radarr, sonarr) = view.read_with(cx, |screen, _| {
        (
            screen.model.overview(),
            screen.model.link(RADARR),
            screen.model.link(SONARR),
        )
    });
    assert_eq!(overview, Overview::NothingConnected);
    assert_eq!(radarr, Link::Disconnected);
    assert_eq!(sonarr, Link::Disconnected);
    let busy = view.read_with(cx, |screen, _| screen.model.busy());
    assert!(
        !busy,
        "nothing is in flight for a calendar with no connections"
    );
}

#[gpui::test]
fn loading_has_no_answer_yet_and_no_day_is_known_empty(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Loading);
    let (radarr, sonarr, settled) = view.read_with(cx, |screen, _| {
        (
            screen.model.status(RADARR),
            screen.model.status(SONARR),
            screen.model.settled(),
        )
    });
    assert_eq!(radarr, SourceStatus::Loading);
    assert_eq!(sonarr, SourceStatus::Loading);
    assert!(!settled);
}

#[gpui::test]
fn both_sources_unreachable_is_an_error_for_each(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Error);
    let (radarr, sonarr) = view.read_with(cx, |screen, _| {
        (screen.model.status(RADARR), screen.model.status(SONARR))
    });
    assert_eq!(radarr, SourceStatus::Failed(SourceFailure::Unavailable));
    assert_eq!(sonarr, SourceStatus::Failed(SourceFailure::Unavailable));
    let connected = view.read_with(cx, |screen, _| screen.model.overview());
    assert_eq!(
        connected,
        Overview::Connected,
        "they are configured, only unreachable"
    );
}

#[gpui::test]
fn refresh_keeps_the_shown_releases_until_new_answers_replace_them(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::PartialFailure);
    let before = view.read_with(cx, |screen, _| screen.model.grid_events().len());
    view.update(cx, |screen, cx| screen.refresh(cx));
    cx.run_until_parked();
    let (after, busy, radarr) = view.read_with(cx, |screen, _| {
        (
            screen.model.grid_events().len(),
            screen.model.busy(),
            screen.model.status(RADARR),
        )
    });
    assert_eq!(
        after, before,
        "no blank flash while the answer is asked for"
    );
    assert_eq!(radarr, SourceStatus::Loading);
    assert!(
        !busy,
        "no service in a review scene, so nothing is asked for"
    );
}

#[gpui::test]
fn the_grid_holds_a_single_focus_stop(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Populated);
    // Choose the 6th, which has releases, so the panel has rows to Tab into.
    view.update(cx, |screen, cx| {
        let index = screen
            .model
            .window()
            .index_of(day_of_month(&screen.model, 6));
        screen.select_index(index.expect("the 6th is in the grid"), cx);
    });
    cx.run_until_parked();
    redraw(&view, cx);
    assert!(grid_focused(&view, cx));
    // Atelier's app root turns Tab into `focus_next`. This window has no app
    // root, so the same call stands in for the keystroke.
    cx.update(|window, _| window.focus_next());
    cx.run_until_parked();
    assert!(
        !grid_focused(&view, cx),
        "Tab leaves the grid for the next control"
    );
    cx.update(|window, _| window.focus_prev());
    cx.run_until_parked();
    assert!(
        grid_focused(&view, cx),
        "and shift-Tab returns to the one stop"
    );
}

/// The `n`th day of the month the screen shows.
fn day_of_month(model: &super::model::CalendarModel<chrono::Local>, n: u32) -> NaiveDate {
    month_start(model.month())
        .with_day(n)
        .expect("a day in the month")
}

#[gpui::test]
fn every_review_scene_draws_at_each_standard_window_size(cx: &mut TestAppContext) {
    let scenes = [
        CalendarPreview::Populated,
        CalendarPreview::Empty,
        CalendarPreview::SelectedDay,
        CalendarPreview::MovieHeavy,
        CalendarPreview::EpisodeHeavy,
        CalendarPreview::Mixed,
        CalendarPreview::RadarrOnly,
        CalendarPreview::SonarrOnly,
        CalendarPreview::Disconnected,
        CalendarPreview::PartialFailure,
        CalendarPreview::Loading,
        CalendarPreview::Error,
        CalendarPreview::NextMonth,
    ];
    let sizes = [
        (960.0, 620.0),
        (1200.0, 760.0),
        (1440.0, 900.0),
        (1920.0, 1080.0),
    ];
    cx.update(keymap);
    for scene in scenes {
        for (width, height) in sizes {
            let runtime = Arc::new(ServiceRuntime::new().unwrap());
            let loader = ArtworkLoader::new(Arc::clone(&runtime));
            let (view, cx) = cx.add_window_view(move |_, cx| scene.screen(runtime, loader, cx));
            cx.simulate_resize(size(px(width), px(height)));
            view.update(cx, |screen, _| screen.settle_on_selected_day());
            cx.run_until_parked();
            redraw(&view, cx);
            // Drawing is the check: a layout that panics fails the test.
            cx.update(|window, _| window.refresh());
            cx.run_until_parked();
        }
    }
}

/// The month is seven columns at every standard width, with the release panel
/// beside it. Columns follow the width the grid is given.
#[test]
fn the_month_is_seven_columns_at_every_standard_width() {
    let sizing = grid_sizing();
    for width in [560.0, 760.0, 1100.0, 1500.0] {
        assert_eq!(sizing.columns(width), 7, "width {width}");
    }
}

#[gpui::test]
fn a_calendar_remembers_its_month_and_selection_while_it_is_not_shown(cx: &mut TestAppContext) {
    let (view, cx) = open(cx, CalendarPreview::Populated);
    view.update(cx, |screen, cx| screen.step(Step::Next, cx));
    view.update(cx, |screen, cx| screen.select_index(10, cx));
    let kept = view.read_with(cx, |screen, _| {
        (screen.model.month(), screen.model.selected())
    });
    // The shell keeps the entity while another root shows; nothing is reset.
    cx.run_until_parked();
    assert_eq!(
        view.read_with(cx, |screen, _| (
            screen.model.month(),
            screen.model.selected()
        )),
        kept
    );
}

#[gpui::test]
fn tabbing_to_a_release_below_the_fold_scrolls_the_panel_to_it(cx: &mut TestAppContext) {
    // The narrow standard window, on a day with six episodes: the last rows
    // are below the panel's fold.
    let (view, cx) = open(cx, CalendarPreview::EpisodeHeavy);
    cx.simulate_resize(size(px(960.0), px(620.0)));
    view.update(cx, |screen, cx| {
        let index = screen
            .model
            .window()
            .index_of(day_of_month(&screen.model, 15));
        screen.select_index(index.expect("the 15th is in the grid"), cx);
    });
    redraw(&view, cx);
    let rows = event_ids(&view, cx, view.read_with(cx, |s, _| s.model.selected())).len();
    assert_eq!(rows, 6);
    assert!(grid_focused(&view, cx));
    let top = view.read_with(cx, |screen, _| f32::from(screen.panel_scroll().offset().y));
    assert_eq!(top, 0.0, "the panel starts at its top");
    for _ in 0..rows {
        cx.update(|window, _| window.focus_next());
        cx.run_until_parked();
        redraw(&view, cx);
    }
    let offset = view.read_with(cx, |screen, _| f32::from(screen.panel_scroll().offset().y));
    assert!(
        offset < 0.0,
        "the panel scrolled to the focused last row (offset {offset})"
    );
}

#[gpui::test]
fn the_header_status_and_month_stack_without_overlap_at_every_standard_size(
    cx: &mut TestAppContext,
) {
    // Drawing alone cannot catch this: at 960 × 620 the grid area used to
    // take its full content height and was centred over the header and the
    // status line, hiding each source's state.
    for (width, height) in [
        (960.0, 620.0),
        (1200.0, 760.0),
        (1440.0, 900.0),
        (1920.0, 1080.0),
    ] {
        let (view, vcx) = open(cx, CalendarPreview::Populated);
        vcx.simulate_resize(size(px(width), px(height)));
        redraw(&view, vcx);
        redraw(&view, vcx);
        let bounds = |name: &'static str, vcx: &mut VisualTestContext| {
            vcx.debug_bounds(name)
                .unwrap_or_else(|| panic!("{name} at {width}x{height}"))
        };
        let header = bounds("calendar-header", vcx);
        let status = bounds("calendar-status", vcx);
        let body = bounds("calendar-body", vcx);
        let grid = bounds("calendar-grid", vcx);
        let at = format!("{width}x{height}");
        assert!(header.bottom() <= status.top(), "{at}: header over status");
        assert!(status.bottom() <= body.top(), "{at}: status over the month");
        assert!(
            grid.top() >= body.top(),
            "{at}: grid above its area {grid:?}"
        );
        assert!(
            grid.bottom() <= body.bottom(),
            "{at}: grid below its area {grid:?}"
        );
        assert!(
            body.bottom() <= px(height),
            "{at}: the month leaves the window"
        );
    }
}

#[gpui::test]
fn days_borrowed_from_a_neighbouring_month_read_as_muted(cx: &mut TestAppContext) {
    // The number used to be painted primary over its muted tone, so the
    // borrowed days read like the month's own.
    let theme = cx.update(|cx| cx.theme().clone());
    assert_eq!(
        day_number_color(&theme, false, false),
        theme.colors.text.muted
    );
    assert_eq!(
        day_number_color(&theme, true, false),
        theme.colors.text.primary
    );
    assert_eq!(
        day_number_color(&theme, true, true),
        theme.colors.control.accent
    );
    assert_ne!(theme.colors.text.muted, theme.colors.text.primary);
}

#[gpui::test]
fn a_release_row_spans_the_panel_whether_or_not_the_panel_scrolls(cx: &mut TestAppContext) {
    // One row (no scrolling) and three rows with the detail (scrolling) at
    // the narrow size. Flattened into a flex scroll view, a lone row shrank
    // to its text, so its highlight and click target stopped short.
    for scene in [CalendarPreview::NextMonth, CalendarPreview::SelectedDay] {
        let (view, vcx) = open(cx, scene);
        vcx.simulate_resize(size(px(960.0), px(620.0)));
        redraw(&view, vcx);
        redraw(&view, vcx);
        let panel = vcx.debug_bounds("calendar-panel").expect("panel");
        let row = vcx.debug_bounds("calendar-row-0").expect("a row");
        // The scroll bar may take its width at the right edge.
        assert!(
            row.size.width >= panel.size.width - px(12.0),
            "{scene:?}: row {row:?} in panel {panel:?}"
        );
    }
}
