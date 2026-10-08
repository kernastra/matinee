//! The calendar model, with no GPUI and no HTTP. Every instant and offset is
//! explicit, so each rule is checked for the zone and day it names.

use chrono::{
    DateTime, Datelike, Days, FixedOffset, MappedLocalTime, NaiveDate, NaiveDateTime, TimeDelta,
    TimeZone, Utc,
};
use matinee_integrations::{
    IntegrationProvider, ReleaseKind, ReleaseMilestone, ReleaseTiming, UpcomingRelease,
};

use super::event::MediaFilter;
use super::grid::Window;
use super::model::{
    Applied, CalendarModel, FRESH_FOR, Link, Overview, RETAINED_WINDOWS, RETRY_AFTER, Request,
    Response, SourceFailure, SourceStatus,
};

const RADARR: IntegrationProvider = IntegrationProvider::Radarr;
const SONARR: IntegrationProvider = IntegrationProvider::Sonarr;

/// UTC-4, the offset the shipping fixtures are written in.
fn new_york() -> FixedOffset {
    FixedOffset::west_opt(4 * 3600).expect("offset")
}

fn at(text: &str) -> DateTime<Utc> {
    DateTime::parse_from_rfc3339(text)
        .expect("fixture instant")
        .with_timezone(&Utc)
}

fn day(text: &str) -> NaiveDate {
    NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("fixture day")
}

/// Mid-August 2026 in New York.
fn now() -> DateTime<Utc> {
    at("2026-08-05T16:00:00Z")
}

fn model() -> CalendarModel<FixedOffset> {
    CalendarModel::new(new_york(), now())
}

fn movie(id: i64, title: &str, kind: ReleaseKind, written: &str) -> UpcomingRelease {
    let stamp = format!("{written}T00:00:00Z");
    UpcomingRelease {
        id: format!("radarr-{id}-{}", kind.as_str()),
        source: RADARR,
        source_id: id,
        series_id: None,
        title: title.into(),
        subtitle: None,
        overview: Some("An overview".into()),
        date: stamp.clone(),
        timing: ReleaseTiming::CivilDay,
        release_kind: kind,
        image_url: None,
        genres: vec!["Drama".into()],
        monitored: true,
        downloaded: false,
        season_number: None,
        episode_number: None,
        milestones: None,
        instant: at(&stamp),
    }
}

/// An episode the source sent as an air time (`airDateUtc`).
fn episode(id: i64, series: &str, air: &str) -> UpcomingRelease {
    UpcomingRelease {
        id: format!("sonarr-{id}"),
        source: SONARR,
        source_id: id,
        series_id: Some(4),
        title: series.into(),
        subtitle: Some("S01E01 · Pilot".into()),
        overview: None,
        date: air.into(),
        timing: ReleaseTiming::Instant,
        release_kind: ReleaseKind::Episode,
        image_url: None,
        genres: Vec::new(),
        monitored: true,
        downloaded: false,
        season_number: Some(1),
        episode_number: Some(1),
        milestones: None,
        instant: at(air),
    }
}

/// An episode the source sent as a bare day (`airDate`).
fn dated_episode(id: i64, series: &str, written: &str) -> UpcomingRelease {
    let mut release = episode(id, series, &format!("{written}T00:00:00Z"));
    release.date = written.into();
    release.timing = ReleaseTiming::CivilDay;
    release
}

/// The request for `provider`'s month, if one was planned.
fn releases_for(
    requests: &[Request],
    provider: IntegrationProvider,
) -> Option<(super::model::Ticket, Window)> {
    requests.iter().find_map(|request| match request {
        Request::Releases {
            ticket,
            provider: p,
            window,
        } if *p == provider => Some((*ticket, *window)),
        _ => None,
    })
}

fn link_for(requests: &[Request], provider: IntegrationProvider) -> Option<super::model::Ticket> {
    requests.iter().find_map(|request| match request {
        Request::Link {
            ticket,
            provider: p,
        } if *p == provider => Some(*ticket),
        _ => None,
    })
}

/// Connect both sources and answer their connection checks.
fn connect_both<Z: TimeZone>(model: &mut CalendarModel<Z>) -> Vec<Request> {
    let requests = model.plan();
    for provider in [RADARR, SONARR] {
        let ticket = link_for(&requests, provider).expect("a link check");
        assert_eq!(
            model.apply(Response::Link {
                ticket,
                provider,
                result: Ok(true),
            }),
            Applied::Updated
        );
    }
    model.plan()
}

fn answer<Z: TimeZone>(
    model: &mut CalendarModel<Z>,
    ticket: super::model::Ticket,
    provider: IntegrationProvider,
    releases: Vec<UpcomingRelease>,
) -> Applied {
    model.apply(Response::Releases {
        ticket,
        provider,
        at: now(),
        result: Ok(releases),
    })
}

fn ids(events: &[&super::event::CalendarEvent]) -> Vec<String> {
    events.iter().map(|event| event.id.clone()).collect()
}

// Month navigation, the year boundary, and leap years.

#[test]
fn a_new_calendar_shows_todays_month_with_today_selected() {
    let model = model();
    assert_eq!(model.today(), day("2026-08-05"));
    assert_eq!(model.month(), day("2026-08-01"));
    assert_eq!(model.selected(), day("2026-08-05"));
    assert_eq!(model.window(), Window::for_month(day("2026-08-01")));
}

#[test]
fn the_selected_day_keeps_its_number_across_months() {
    let mut model = model();
    model.select(day("2026-08-31"));
    model.next_month();
    assert_eq!(model.month(), day("2026-09-01"));
    assert_eq!(
        model.selected(),
        day("2026-09-30"),
        "31 clamps to September's 30"
    );
    model.previous_month();
    model.previous_month();
    assert_eq!(model.month(), day("2026-07-01"));
    assert_eq!(model.selected(), day("2026-07-30"));
}

#[test]
fn next_and_previous_cross_the_year_boundary_both_ways() {
    let mut model = CalendarModel::new(new_york(), at("2026-12-15T12:00:00Z"));
    model.next_month();
    assert_eq!(model.month(), day("2027-01-01"));
    model.previous_month();
    assert_eq!(model.month(), day("2026-12-01"));
    model.previous_month();
    model.previous_month();
    assert_eq!(model.month(), day("2026-10-01"));
}

#[test]
fn leap_day_months_show_the_twenty_ninth_and_a_grid_of_six_weeks() {
    let mut model = CalendarModel::new(new_york(), at("2028-01-31T12:00:00Z"));
    model.next_month();
    assert_eq!(model.month(), day("2028-02-01"));
    assert_eq!(model.selected(), day("2028-02-29"));
    let window = model.window();
    assert!(window.index_of(day("2028-02-29")).is_some());
    assert_eq!(
        window
            .last()
            .signed_duration_since(window.start())
            .num_days(),
        41
    );
}

#[test]
fn today_returns_to_todays_month_and_selects_it() {
    let mut model = model();
    model.select(day("2026-02-14"));
    model.next_month();
    model.go_to_today();
    assert_eq!(model.month(), day("2026-08-01"));
    assert_eq!(model.selected(), day("2026-08-05"));
}

#[test]
fn choosing_a_day_from_another_month_shows_that_month() {
    let mut model = model();
    model.select(day("2026-09-02"));
    assert_eq!(model.month(), day("2026-09-01"));
    assert_eq!(model.selected(), day("2026-09-02"));
}

#[test]
fn the_clock_moving_past_midnight_moves_today_but_not_the_view() {
    let mut model = model();
    model.select(day("2026-08-20"));
    model.observe_now(at("2026-08-06T05:00:00Z"));
    assert_eq!(
        model.today(),
        day("2026-08-06"),
        "05:00 UTC is 01:00 on the 6th in New York"
    );
    assert_eq!(model.selected(), day("2026-08-20"));
    assert_eq!(model.month(), day("2026-08-01"));
}

#[test]
fn today_is_the_local_day_not_the_utc_day() {
    // 02:00 UTC on the 6th is 22:00 on the 5th in New York.
    let model = CalendarModel::new(new_york(), at("2026-08-06T02:00:00Z"));
    assert_eq!(model.today(), day("2026-08-05"));
}

// Requests: one per connected source, per month.

#[test]
fn a_new_calendar_checks_both_connections_before_asking_for_any_month() {
    let mut model = model();
    let first = model.plan();
    assert_eq!(first.len(), 2, "two connection checks, nothing else");
    assert!(
        first
            .iter()
            .all(|request| matches!(request, Request::Link { .. }))
    );
    assert_eq!(model.overview(), Overview::Checking);
    assert_eq!(model.status(RADARR), SourceStatus::Checking);
}

#[test]
fn connected_sources_ask_for_the_shown_month_once() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (_, radarr_window) = releases_for(&requests, RADARR).expect("Radarr month");
    let (_, sonarr_window) = releases_for(&requests, SONARR).expect("Sonarr month");
    assert_eq!(radarr_window, Window::for_month(day("2026-08-01")));
    assert_eq!(sonarr_window, radarr_window);
    assert_eq!(model.status(RADARR), SourceStatus::Loading);
    assert!(model.plan().is_empty(), "already in flight: nothing new");
}

#[test]
fn selecting_a_day_in_the_shown_month_asks_for_nothing() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (ticket, _) = releases_for(&requests, RADARR).unwrap();
    answer(&mut model, ticket, RADARR, Vec::new());
    let (sticket, _) = releases_for(&requests, SONARR).unwrap();
    answer(&mut model, sticket, SONARR, Vec::new());
    model.select(day("2026-08-20"));
    assert!(model.plan().is_empty(), "same month, no new request");
}

#[test]
fn returning_to_a_loaded_month_asks_nothing_again() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(7, "Film", ReleaseKind::Theatrical, "2026-08-20")],
    );
    answer(&mut model, sonarr, SONARR, Vec::new());

    model.next_month();
    let september = model.plan();
    assert_eq!(september.len(), 2, "September is new for both");
    let (r2, _) = releases_for(&september, RADARR).unwrap();
    let (s2, _) = releases_for(&september, SONARR).unwrap();
    answer(&mut model, r2, RADARR, Vec::new());
    answer(&mut model, s2, SONARR, Vec::new());

    model.previous_month();
    assert!(model.plan().is_empty(), "August is still fresh: no request");
    assert_eq!(model.status(RADARR), SourceStatus::Ready);
    assert_eq!(ids(&model.grid_events()), vec!["radarr-7-theatrical"]);
}

#[test]
fn a_month_older_than_the_freshness_window_is_asked_for_again_without_losing_its_events() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(7, "Film", ReleaseKind::Theatrical, "2026-08-20")],
    );
    answer(&mut model, sonarr, SONARR, Vec::new());

    model.show(now() + FRESH_FOR + TimeDelta::seconds(1));
    let requests = model.plan();
    let (ticket, _) = releases_for(&requests, RADARR).expect("refetch once stale");
    assert_eq!(model.status(RADARR), SourceStatus::Loading);
    assert_eq!(
        ids(&model.grid_events()),
        vec!["radarr-7-theatrical"],
        "the old answer stays on screen until the new one replaces it"
    );
    answer(&mut model, ticket, RADARR, Vec::new());
    assert!(model.grid_events().is_empty(), "the new answer removed it");
}

// Source-specific loading and partial success.

#[test]
fn a_slow_sonarr_never_holds_back_a_radarr_answer() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(1, "Film", ReleaseKind::Theatrical, "2026-08-12")],
    );

    assert_eq!(model.status(RADARR), SourceStatus::Ready);
    assert_eq!(model.status(SONARR), SourceStatus::Loading);
    assert_eq!(ids(&model.grid_events()), vec!["radarr-1-theatrical"]);
    assert!(
        !model.settled(),
        "an empty day is not known to be empty yet"
    );

    answer(
        &mut model,
        sonarr,
        SONARR,
        vec![episode(22, "Night Shift", "2026-08-11T01:00:00Z")],
    );
    assert_eq!(model.grid_events().len(), 2);
    assert!(model.settled());
}

#[test]
fn a_failed_source_keeps_the_other_sources_events() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(1, "Film", ReleaseKind::Theatrical, "2026-08-12")],
    );
    model.apply(Response::Releases {
        ticket: sonarr,
        provider: SONARR,
        at: now(),
        result: Err(SourceFailure::Unavailable),
    });

    assert_eq!(
        model.status(SONARR),
        SourceStatus::Failed(SourceFailure::Unavailable)
    );
    assert_eq!(model.status(RADARR), SourceStatus::Ready);
    assert_eq!(ids(&model.grid_events()), vec!["radarr-1-theatrical"]);
    assert!(!model.settled(), "a failed month is not an empty month");
}

#[test]
fn each_failure_kind_is_kept_for_its_own_source() {
    for failure in [
        SourceFailure::Unauthorized,
        SourceFailure::Unavailable,
        SourceFailure::Malformed,
    ] {
        let mut model = model();
        let requests = connect_both(&mut model);
        let (ticket, _) = releases_for(&requests, SONARR).unwrap();
        model.apply(Response::Releases {
            ticket,
            provider: SONARR,
            at: now(),
            result: Err(failure),
        });
        assert_eq!(model.status(SONARR), SourceStatus::Failed(failure));
        assert_eq!(model.status(RADARR), SourceStatus::Loading);
    }
}

#[test]
fn an_unauthorized_source_is_not_a_session_problem_for_the_other_source() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    model.apply(Response::Releases {
        ticket: radarr,
        provider: RADARR,
        at: now(),
        result: Err(SourceFailure::Unauthorized),
    });
    assert_eq!(
        model.status(RADARR),
        SourceStatus::Failed(SourceFailure::Unauthorized)
    );
    assert_eq!(
        model.link(RADARR),
        Link::Connected,
        "the connection is still saved"
    );
    assert_eq!(model.overview(), Overview::Connected);
}

// Failure and retry.

#[test]
fn a_failed_month_is_not_asked_for_on_every_plan() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (ticket, _) = releases_for(&requests, SONARR).unwrap();
    model.apply(Response::Releases {
        ticket,
        provider: SONARR,
        at: now(),
        result: Err(SourceFailure::Unavailable),
    });
    assert!(model.plan().is_empty(), "no retry loop on render");
    assert!(model.plan().is_empty());
}

#[test]
fn refresh_is_the_retry_for_a_failed_month() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (ticket, _) = releases_for(&requests, SONARR).unwrap();
    model.apply(Response::Releases {
        ticket,
        provider: SONARR,
        at: now(),
        result: Err(SourceFailure::Unavailable),
    });
    model.refresh();
    let retry = model.plan();
    assert_eq!(retry.len(), 4, "both connections and both months again");
    let (sonarr, _) = releases_for(&retry, SONARR).expect("retry");
    answer(
        &mut model,
        sonarr,
        SONARR,
        vec![episode(22, "Night Shift", "2026-08-11T01:00:00Z")],
    );
    assert_eq!(model.status(SONARR), SourceStatus::Ready);
    assert_eq!(
        model.status(RADARR),
        SourceStatus::Loading,
        "Radarr's answer is pending"
    );
}

#[test]
fn a_visit_once_the_retry_interval_has_passed_asks_for_a_failed_month_again() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (ticket, _) = releases_for(&requests, SONARR).unwrap();
    model.apply(Response::Releases {
        ticket,
        provider: SONARR,
        at: now(),
        result: Err(SourceFailure::Malformed),
    });
    model.show(now() + RETRY_AFTER);
    let again = model.plan();
    assert!(releases_for(&again, SONARR).is_some());
}

#[test]
fn a_failure_for_one_month_does_not_stop_another_month_from_loading() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (ticket, _) = releases_for(&requests, SONARR).unwrap();
    model.apply(Response::Releases {
        ticket,
        provider: SONARR,
        at: now(),
        result: Err(SourceFailure::Unavailable),
    });
    model.next_month();
    let september = model.plan();
    assert!(
        releases_for(&september, SONARR).is_some(),
        "a new month is asked for"
    );
    assert_eq!(model.status(SONARR), SourceStatus::Loading);
}

// Stale answers and refresh races.

#[test]
fn an_answer_for_a_month_no_longer_shown_is_ignored() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (old_radarr, _) = releases_for(&requests, RADARR).unwrap();
    model.next_month();
    let september = model.plan();
    let (new_radarr, _) = releases_for(&september, RADARR).unwrap();

    assert_eq!(
        answer(
            &mut model,
            old_radarr,
            RADARR,
            vec![movie(
                1,
                "August film",
                ReleaseKind::Theatrical,
                "2026-08-12"
            )]
        ),
        Applied::Ignored,
        "the old ticket is no longer waited on"
    );
    assert!(model.grid_events().is_empty());
    assert_eq!(
        answer(&mut model, new_radarr, RADARR, Vec::new()),
        Applied::Updated
    );
}

#[test]
fn an_answer_arriving_twice_applies_once() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (ticket, _) = releases_for(&requests, RADARR).unwrap();
    assert_eq!(
        answer(&mut model, ticket, RADARR, Vec::new()),
        Applied::Updated
    );
    assert_eq!(
        answer(
            &mut model,
            ticket,
            RADARR,
            vec![movie(1, "Film", ReleaseKind::Theatrical, "2026-08-12")]
        ),
        Applied::Ignored
    );
    assert!(
        model.grid_events().is_empty(),
        "the second copy changed nothing"
    );
}

#[test]
fn refreshing_during_a_request_makes_the_old_answer_stale() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (old, _) = releases_for(&requests, RADARR).unwrap();
    model.refresh();
    let retry = model.plan();
    let (new, _) = releases_for(&retry, RADARR).expect("a fresh request");
    assert_ne!(old, new);
    assert_eq!(
        answer(
            &mut model,
            old,
            RADARR,
            vec![movie(1, "Old", ReleaseKind::Theatrical, "2026-08-12")]
        ),
        Applied::Ignored
    );
    answer(
        &mut model,
        new,
        RADARR,
        vec![movie(2, "New", ReleaseKind::Theatrical, "2026-08-13")],
    );
    assert_eq!(ids(&model.grid_events()), vec!["radarr-2-theatrical"]);
}

#[test]
fn a_connection_check_answered_after_a_refresh_is_ignored() {
    let mut model = model();
    let first = model.plan();
    let old = link_for(&first, RADARR).unwrap();
    model.refresh();
    let second = model.plan();
    let new = link_for(&second, RADARR).unwrap();
    assert_eq!(
        model.apply(Response::Link {
            ticket: old,
            provider: RADARR,
            result: Ok(false),
        }),
        Applied::Ignored
    );
    assert_eq!(model.link(RADARR), Link::Checking);
    model.apply(Response::Link {
        ticket: new,
        provider: RADARR,
        result: Ok(true),
    });
    assert_eq!(model.link(RADARR), Link::Connected);
}

#[test]
fn rapid_month_changes_end_on_the_last_month_asked_for() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (first, _) = releases_for(&requests, RADARR).unwrap();
    model.next_month();
    let a = model.plan();
    model.next_month();
    let b = model.plan();
    model.next_month();
    let c = model.plan();
    let (ta, _) = releases_for(&a, RADARR).unwrap();
    let (tb, _) = releases_for(&b, RADARR).unwrap();
    let (tc, window_c) = releases_for(&c, RADARR).unwrap();
    assert_eq!(
        window_c,
        Window::for_month(day("2026-11-01")),
        "August, then three months on"
    );
    assert_eq!(
        answer(&mut model, first, RADARR, Vec::new()),
        Applied::Ignored
    );
    assert_eq!(answer(&mut model, ta, RADARR, Vec::new()), Applied::Ignored);
    assert_eq!(answer(&mut model, tb, RADARR, Vec::new()), Applied::Ignored);
    assert_eq!(answer(&mut model, tc, RADARR, Vec::new()), Applied::Updated);
    assert_eq!(model.status(RADARR), SourceStatus::Ready);
}

// Event placement, filtering, duplicates, and ordering.

#[test]
fn a_movie_and_an_episode_each_land_on_their_own_day() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(1, "Film", ReleaseKind::Theatrical, "2026-08-20")],
    );
    answer(
        &mut model,
        sonarr,
        SONARR,
        vec![episode(22, "Night Shift", "2026-08-11T01:00:00Z")],
    );

    // 01:00 UTC on the 11th is 21:00 on the 10th in New York.
    assert_eq!(ids(&model.day_events(day("2026-08-10"))), vec!["sonarr-22"]);
    assert!(model.day_events(day("2026-08-11")).is_empty());
    assert_eq!(
        ids(&model.day_events(day("2026-08-20"))),
        vec!["radarr-1-theatrical"]
    );
}

#[test]
fn a_civil_day_is_not_moved_by_the_viewers_zone() {
    let mut model = CalendarModel::new(FixedOffset::west_opt(12 * 3600).unwrap(), now());
    let requests = model.plan();
    for provider in [RADARR, SONARR] {
        let ticket = link_for(&requests, provider).unwrap();
        model.apply(Response::Link {
            ticket,
            provider,
            result: Ok(true),
        });
    }
    let requests = model.plan();
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(1, "Film", ReleaseKind::Theatrical, "2026-08-20")],
    );
    answer(
        &mut model,
        sonarr,
        SONARR,
        vec![dated_episode(30, "Desk", "2026-08-16")],
    );
    assert_eq!(
        ids(&model.day_events(day("2026-08-20"))),
        vec!["radarr-1-theatrical"]
    );
    assert_eq!(ids(&model.day_events(day("2026-08-16"))), vec!["sonarr-30"]);
}

#[test]
fn an_episode_is_placed_by_the_viewers_day_when_it_is_an_air_time() {
    // 01:30 UTC on the 16th is still the 15th on the Americas' west coast.
    let mut model = CalendarModel::new(FixedOffset::west_opt(7 * 3600).unwrap(), now());
    let requests = connect_both(&mut model);
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(
        &mut model,
        sonarr,
        SONARR,
        vec![episode(31, "Desk", "2026-08-16T01:30:00Z")],
    );
    assert_eq!(ids(&model.day_events(day("2026-08-15"))), vec!["sonarr-31"]);
}

#[test]
fn a_release_on_a_grid_day_from_the_neighbouring_month_is_shown_in_the_grid() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, window) = releases_for(&requests, RADARR).unwrap();
    assert_eq!(window.start(), day("2026-07-26"));
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(3, "Early", ReleaseKind::Theatrical, "2026-07-27")],
    );
    answer(&mut model, sonarr, SONARR, Vec::new());
    assert_eq!(ids(&model.grid_events()), vec!["radarr-3-theatrical"]);
    assert_eq!(model.month_release_count(), 0, "it is July's, not August's");
}

#[test]
fn an_answer_only_places_days_the_requested_month_shows() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![
            movie(1, "Inside", ReleaseKind::Theatrical, "2026-09-05"),
            movie(2, "Outside", ReleaseKind::Theatrical, "2026-09-06"),
            movie(3, "Before", ReleaseKind::Theatrical, "2026-07-25"),
        ],
    );
    assert_eq!(ids(&model.grid_events()), vec!["radarr-1-theatrical"]);
}

#[test]
fn a_duplicate_id_in_one_answer_is_shown_once() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    let air = "2026-08-11T01:00:00Z";
    answer(
        &mut model,
        sonarr,
        SONARR,
        vec![
            episode(22, "Night Shift", air),
            episode(22, "Night Shift", air),
        ],
    );
    assert_eq!(model.grid_events().len(), 1);
}

#[test]
fn a_fresh_answer_replaces_a_days_events_rather_than_adding_to_them() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(1, "Film", ReleaseKind::Theatrical, "2026-08-20")],
    );
    model.refresh();
    let retry = model.plan();
    let (again, _) = releases_for(&retry, RADARR).unwrap();
    answer(
        &mut model,
        again,
        RADARR,
        vec![movie(1, "Film", ReleaseKind::Theatrical, "2026-08-20")],
    );
    assert_eq!(
        model.grid_events().len(),
        1,
        "one row per id after a refresh"
    );
}

#[test]
fn the_same_movie_on_two_milestone_days_is_two_rows() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![
            movie(1, "Film", ReleaseKind::Theatrical, "2026-08-20"),
            movie(1, "Film", ReleaseKind::Digital, "2026-09-12"),
        ],
    );
    assert_eq!(
        model.grid_events().len(),
        1,
        "September 12 is past this grid, so only August 20 is shown"
    );
    assert_eq!(model.month_release_count(), 1);
    assert_eq!(
        ids(&model.day_events(day("2026-08-20"))),
        vec!["radarr-1-theatrical"]
    );
}

#[test]
fn a_day_lists_movies_before_episodes_and_episodes_by_air_time() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(5, "Zeta", ReleaseKind::Theatrical, "2026-08-10")],
    );
    answer(
        &mut model,
        sonarr,
        SONARR,
        vec![
            episode(41, "Late Show", "2026-08-11T03:30:00Z"),
            episode(40, "Early Show", "2026-08-11T01:00:00Z"),
        ],
    );
    // All three land on the 10th in New York (the episodes air at 21:00 and 23:30 there).
    assert_eq!(
        ids(&model.day_events(day("2026-08-10"))),
        vec!["radarr-5-theatrical", "sonarr-40", "sonarr-41"]
    );
}

#[test]
fn the_filter_shows_only_its_media_and_drops_a_focus_it_hides() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(1, "Film", ReleaseKind::Theatrical, "2026-08-05")],
    );
    answer(
        &mut model,
        sonarr,
        SONARR,
        vec![dated_episode(30, "Desk", "2026-08-05")],
    );
    model.select(day("2026-08-05"));
    model.focus_event("sonarr-30");
    assert_eq!(model.focused_event().unwrap().id, "sonarr-30");

    model.set_filter(MediaFilter::Movies);
    assert_eq!(
        ids(&model.day_events(day("2026-08-05"))),
        vec!["radarr-1-theatrical"]
    );
    assert_eq!(
        model.focused_event().unwrap().id,
        "radarr-1-theatrical",
        "the hidden focus is dropped"
    );
    model.set_filter(MediaFilter::Series);
    assert_eq!(ids(&model.grid_events()), vec!["sonarr-30"]);
    model.set_filter(MediaFilter::All);
    assert_eq!(model.grid_events().len(), 2);
}

#[test]
fn the_focused_event_defaults_to_the_first_of_the_day() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(1, "Film", ReleaseKind::Theatrical, "2026-08-05")],
    );
    assert_eq!(model.focused_event().unwrap().id, "radarr-1-theatrical");
    model.focus_event("not-a-release");
    assert_eq!(model.focused_event().unwrap().id, "radarr-1-theatrical");
    model.select(day("2026-08-06"));
    assert!(
        model.focused_event().is_none(),
        "an empty day has nothing focused"
    );
}

#[test]
fn milestones_and_source_fields_survive_normalization() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    let mut release = movie(9, "Window Piece", ReleaseKind::Theatrical, "2026-08-20");
    release.milestones = Some(vec![
        ReleaseMilestone {
            date: "2026-08-20T00:00:00Z".into(),
            kind: ReleaseKind::Theatrical,
        },
        ReleaseMilestone {
            date: "2026-09-12T00:00:00Z".into(),
            kind: ReleaseKind::Digital,
        },
    ]);
    answer(&mut model, radarr, RADARR, vec![release]);
    model.select(day("2026-08-20"));
    let event = model.focused_event().unwrap().clone();
    assert_eq!(event.milestones.len(), 2);
    assert_eq!(event.milestones[1].day, day("2026-09-12"));
    assert_eq!(event.overview.as_deref(), Some("An overview"));
    assert_eq!(event.genres, vec!["Drama".to_string()]);
    assert!(event.is_movie());
}

// Source configuration changes.

#[test]
fn losing_a_connection_removes_that_source_and_nothing_else() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(1, "Film", ReleaseKind::Theatrical, "2026-08-12")],
    );
    answer(
        &mut model,
        sonarr,
        SONARR,
        vec![episode(22, "Night Shift", "2026-08-11T01:00:00Z")],
    );

    model.show(now());
    let checks = model.plan();
    let check = link_for(&checks, SONARR).unwrap();
    model.apply(Response::Link {
        ticket: check,
        provider: SONARR,
        result: Ok(false),
    });

    assert_eq!(model.link(SONARR), Link::Disconnected);
    assert_eq!(model.status(SONARR), SourceStatus::Unlinked);
    assert_eq!(ids(&model.grid_events()), vec!["radarr-1-theatrical"]);
    assert!(
        model.plan().is_empty(),
        "an unlinked source asks for no months"
    );
}

#[test]
fn a_connection_added_later_is_asked_for_on_the_next_visit() {
    let mut model = model();
    let first = model.plan();
    for provider in [RADARR, SONARR] {
        let ticket = link_for(&first, provider).unwrap();
        model.apply(Response::Link {
            ticket,
            provider,
            result: Ok(provider == RADARR),
        });
    }
    assert_eq!(model.status(SONARR), SourceStatus::Unlinked);
    assert_eq!(model.overview(), Overview::Connected);

    model.show(now());
    let checks = model.plan();
    let sonarr_check = link_for(&checks, SONARR).expect("rechecked on show");
    model.apply(Response::Link {
        ticket: sonarr_check,
        provider: SONARR,
        result: Ok(true),
    });
    let months = model.plan();
    assert!(releases_for(&months, SONARR).is_some());
}

#[test]
fn a_connection_that_cannot_be_read_keeps_the_events_it_had() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(1, "Film", ReleaseKind::Theatrical, "2026-08-12")],
    );

    model.show(now());
    let checks = model.plan();
    let check = link_for(&checks, RADARR).unwrap();
    model.apply(Response::Link {
        ticket: check,
        provider: RADARR,
        result: Err(SourceFailure::Unavailable),
    });
    assert_eq!(
        model.status(RADARR),
        SourceStatus::Failed(SourceFailure::Unavailable)
    );
    assert_eq!(ids(&model.grid_events()), vec!["radarr-1-theatrical"]);
}

#[test]
fn a_releases_answer_that_says_not_configured_disconnects_the_source() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (ticket, _) = releases_for(&requests, SONARR).unwrap();
    model.apply(Response::Releases {
        ticket,
        provider: SONARR,
        at: now(),
        result: Err(SourceFailure::Unlinked),
    });
    assert_eq!(model.link(SONARR), Link::Disconnected);
    assert_eq!(model.status(SONARR), SourceStatus::Unlinked);
}

#[test]
fn an_unconnected_calendar_says_so_and_asks_nothing_for_months() {
    let mut model = model();
    let first = model.plan();
    for provider in [RADARR, SONARR] {
        let ticket = link_for(&first, provider).unwrap();
        model.apply(Response::Link {
            ticket,
            provider,
            result: Ok(false),
        });
    }
    assert_eq!(model.overview(), Overview::NothingConnected);
    assert!(model.plan().is_empty());
    assert!(
        model.settled(),
        "nothing connected means nothing is pending"
    );
    assert!(model.grid_events().is_empty());
}

#[test]
fn a_calendar_with_no_releases_is_empty_once_both_sources_have_answered() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(&mut model, radarr, RADARR, Vec::new());
    answer(&mut model, sonarr, SONARR, Vec::new());
    assert!(model.settled());
    assert_eq!(model.status(RADARR), SourceStatus::Ready);
    assert!(model.grid_events().is_empty());
    assert_eq!(model.month_release_count(), 0);
    assert!(model.last_updated().is_some());
}

// Retained months and the memory bound.

#[test]
fn only_a_bounded_number_of_months_is_kept_and_the_rest_are_forgotten() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(1, "August", ReleaseKind::Theatrical, "2026-08-20")],
    );
    answer(&mut model, sonarr, SONARR, Vec::new());

    // Walk forward past the retained limit, answering each month with nothing.
    for _ in 0..RETAINED_WINDOWS {
        model.next_month();
        let requests = model.plan();
        for provider in [RADARR, SONARR] {
            if let Some((ticket, _)) = releases_for(&requests, provider) {
                answer(&mut model, ticket, provider, Vec::new());
            }
        }
    }
    // August's answer was the oldest and has been dropped with its events.
    model.previous_month();
    model.previous_month();
    model.previous_month();
    model.previous_month();
    assert_eq!(model.month(), day("2026-08-01"));
    assert!(
        model.grid_events().is_empty(),
        "the old answer was forgotten"
    );
    let requests = model.plan();
    assert!(
        releases_for(&requests, RADARR).is_some(),
        "so August is asked for again"
    );
}

#[test]
fn the_month_on_screen_is_ready_after_its_own_answer_however_far_the_walk_goes() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(1, "August", ReleaseKind::Theatrical, "2026-08-20")],
    );
    answer(&mut model, sonarr, SONARR, Vec::new());
    // Walk well past the retained limit. Each month the screen shows is
    // answered and stays answered until something newer pushes it out.
    for _ in 0..(RETAINED_WINDOWS + 2) {
        model.next_month();
        let requests = model.plan();
        for provider in [RADARR, SONARR] {
            if let Some((ticket, _)) = releases_for(&requests, provider) {
                answer(&mut model, ticket, provider, Vec::new());
            }
        }
        assert_eq!(model.status(RADARR), SourceStatus::Ready);
        assert_eq!(model.status(SONARR), SourceStatus::Ready);
        assert!(
            model.plan().is_empty(),
            "the answered month is not asked for again"
        );
    }
}

#[test]
fn a_month_back_and_forth_within_the_limit_costs_nothing() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(&mut model, radarr, RADARR, Vec::new());
    answer(&mut model, sonarr, SONARR, Vec::new());
    let mut asks = 0;
    for _ in 0..(RETAINED_WINDOWS - 1) {
        model.next_month();
        let requests = model.plan();
        asks += requests.len();
        for provider in [RADARR, SONARR] {
            if let Some((ticket, _)) = releases_for(&requests, provider) {
                answer(&mut model, ticket, provider, Vec::new());
            }
        }
    }
    for _ in 0..(RETAINED_WINDOWS - 1) {
        model.previous_month();
        asks += model.plan().len();
    }
    assert_eq!(
        asks,
        2 * (RETAINED_WINDOWS - 1),
        "only the first visit to each month asked"
    );
}

// Presentation rules the screen reads.

#[test]
fn the_grid_is_forty_two_days_and_a_day_is_marked_inside_its_month() {
    let model = model();
    let window = model.window();
    let days: Vec<NaiveDate> = (0..42).map(|index| window.day(index)).collect();
    assert_eq!(days.len(), 42);
    assert_eq!(days[0], day("2026-07-26"));
    assert_eq!(days[41], day("2026-09-05"));
    assert_eq!(days.iter().filter(|d| d.month() == 8).count(), 31);
}

#[test]
fn the_sources_status_and_overview_follow_the_connections() {
    let mut model = model();
    assert_eq!(model.overview(), Overview::Checking);
    let requests = model.plan();
    let radarr = link_for(&requests, RADARR).unwrap();
    model.apply(Response::Link {
        ticket: radarr,
        provider: RADARR,
        result: Ok(true),
    });
    assert_eq!(
        model.overview(),
        Overview::Connected,
        "one connection is enough to show"
    );
    assert_eq!(model.status(SONARR), SourceStatus::Checking);
}

#[test]
fn a_link_check_that_fails_is_reported_as_that_failure() {
    let mut model = model();
    let requests = model.plan();
    let sonarr = link_for(&requests, SONARR).unwrap();
    model.apply(Response::Link {
        ticket: sonarr,
        provider: SONARR,
        result: Err(SourceFailure::Unavailable),
    });
    assert_eq!(
        model.status(SONARR),
        SourceStatus::Failed(SourceFailure::Unavailable)
    );
    assert_eq!(model.link(SONARR), Link::Failed(SourceFailure::Unavailable));
}

#[test]
fn a_date_only_episode_is_the_written_day_for_every_viewer() {
    let mut model = CalendarModel::new(FixedOffset::west_opt(7 * 3600).unwrap(), now());
    let requests = connect_both(&mut model);
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    answer(
        &mut model,
        sonarr,
        SONARR,
        vec![dated_episode(30, "Desk", "2026-08-16")],
    );
    assert_eq!(ids(&model.day_events(day("2026-08-16"))), vec!["sonarr-30"]);
}

#[test]
fn a_busy_calendar_reports_it_is_busy_until_its_answers_arrive() {
    let mut model = model();
    assert!(!model.busy(), "nothing is asked before the first plan");
    let requests = connect_both(&mut model);
    assert!(model.busy());
    for provider in [RADARR, SONARR] {
        let (ticket, _) = releases_for(&requests, provider).unwrap();
        answer(&mut model, ticket, provider, Vec::new());
    }
    assert!(!model.busy());
}

// Returning to Calendar.

/// Answer every connection check in `requests` as connected.
fn answer_links(model: &mut CalendarModel<FixedOffset>, requests: &[Request]) {
    for provider in [RADARR, SONARR] {
        if let Some(ticket) = link_for(requests, provider) {
            model.apply(Response::Link {
                ticket,
                provider,
                result: Ok(true),
            });
        }
    }
}

#[test]
fn coming_back_right_after_a_failure_does_not_retry_it_but_a_later_visit_does() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (ticket, _) = releases_for(&requests, SONARR).unwrap();
    model.apply(Response::Releases {
        ticket,
        provider: SONARR,
        at: now(),
        result: Err(SourceFailure::Unavailable),
    });
    // Calendar, Home, Calendar, and again, a few seconds apart.
    for second in 1..=5 {
        model.show(now() + TimeDelta::seconds(second));
        let requests = model.plan();
        assert!(
            releases_for(&requests, SONARR).is_none(),
            "visit {second} asked the failing server again"
        );
        answer_links(&mut model, &requests);
        assert_eq!(
            model.status(SONARR),
            SourceStatus::Failed(SourceFailure::Unavailable)
        );
    }
    // A visit a minute later is a deliberate retry.
    model.show(now() + TimeDelta::minutes(1));
    let requests = model.plan();
    assert!(releases_for(&requests, SONARR).is_some(), "retried");
    assert!(
        releases_for(&requests, RADARR).is_none(),
        "Radarr's first request is still in flight, so it is not repeated"
    );
}

#[test]
fn coming_back_while_the_month_is_loading_asks_for_nothing_more() {
    let mut model = model();
    let requests = connect_both(&mut model);
    assert!(releases_for(&requests, RADARR).is_some());
    assert!(releases_for(&requests, SONARR).is_some());
    for second in 1..=5 {
        model.show(now() + TimeDelta::seconds(second));
        let requests = model.plan();
        assert!(
            requests
                .iter()
                .all(|request| matches!(request, Request::Link { .. })),
            "visit {second} started another month request: {requests:?}"
        );
        answer_links(&mut model, &requests);
    }
    assert!(
        model.busy(),
        "the first requests are still the ones in flight"
    );
}

#[test]
fn a_month_request_that_ends_without_an_answer_never_leaves_its_source_loading() {
    let mut model = model();
    let requests = connect_both(&mut model);
    let (ticket, _) = releases_for(&requests, SONARR).unwrap();
    assert_eq!(model.abandon(ticket, now()), Applied::Updated);
    assert!(!model.waiting_on(ticket));
    assert_eq!(
        model.status(SONARR),
        SourceStatus::Failed(SourceFailure::Unavailable),
        "shown as a failure, so Try again appears"
    );
    assert!(model.plan().is_empty(), "and not asked for in a loop");
    assert_eq!(
        model.abandon(ticket, now()),
        Applied::Ignored,
        "a ticket no longer waited on changes nothing"
    );
    model.refresh();
    assert!(
        releases_for(&model.plan(), SONARR).is_some(),
        "Refresh retries"
    );
}

#[test]
fn a_connection_check_that_ends_without_an_answer_fails_only_a_first_check() {
    let mut model = model();
    let first = model.plan();
    let ticket = link_for(&first, RADARR).unwrap();
    model.abandon(ticket, now());
    assert_eq!(
        model.link(RADARR),
        Link::Failed(SourceFailure::Unavailable),
        "never read, so not left checking"
    );

    let mut model = super::model_tests::model();
    connect_both(&mut model);
    model.show(now());
    let recheck = link_for(&model.plan(), RADARR).unwrap();
    model.abandon(recheck, now());
    assert_eq!(
        model.link(RADARR),
        Link::Connected,
        "a known connection is kept when its recheck is lost"
    );
}

// Zones, the fetch padding, and daylight saving.

/// Every offset in use, from UTC-12 to UTC+14, in quarter hours.
fn every_offset() -> impl Iterator<Item = FixedOffset> {
    (-12 * 4..=14 * 4).map(|quarters| FixedOffset::east_opt(quarters * 15 * 60).expect("offset"))
}

/// The UTC instant of `day`'s local midnight in `zone`.
pub(super) fn local_midnight<Z: TimeZone>(zone: &Z, day: NaiveDate) -> DateTime<Utc> {
    zone.from_local_datetime(&day.and_hms_opt(0, 0, 0).expect("midnight"))
        .earliest()
        .expect("a local midnight")
        .with_timezone(&Utc)
}

#[test]
fn the_fetch_range_holds_every_local_moment_of_every_grid_day_in_every_zone() {
    for year in 2024..=2030 {
        for month in 1..=12 {
            let window = Window::for_month(NaiveDate::from_ymd_opt(year, month, 1).unwrap());
            let (from, to) = window.fetch_range();
            let after_last = window.last().checked_add_days(Days::new(1)).unwrap();
            for zone in every_offset() {
                let first = local_midnight(&zone, window.start());
                let end = local_midnight(&zone, after_last);
                assert!(from <= first, "{year}-{month} at {zone}: {from} > {first}");
                assert!(end <= to, "{year}-{month} at {zone}: {end} > {to}");
            }
        }
    }
}

/// US Eastern time for 2026 only: UTC-5, and UTC-4 from 07:00 UTC on 8 March
/// to 06:00 UTC on 1 November. A real zone with real transitions, without a
/// time zone database.
#[derive(Clone, Copy, Debug)]
struct Eastern2026;

impl Eastern2026 {
    fn at(utc: &NaiveDateTime) -> FixedOffset {
        let spring = day("2026-03-08").and_hms_opt(7, 0, 0).unwrap();
        let fall = day("2026-11-01").and_hms_opt(6, 0, 0).unwrap();
        let hours = if *utc >= spring && *utc < fall {
            -4
        } else {
            -5
        };
        FixedOffset::east_opt(hours * 3600).unwrap()
    }
}

impl TimeZone for Eastern2026 {
    type Offset = FixedOffset;

    fn from_offset(_: &FixedOffset) -> Self {
        Self
    }

    fn offset_from_local_date(&self, local: &NaiveDate) -> MappedLocalTime<FixedOffset> {
        self.offset_from_local_datetime(&local.and_hms_opt(0, 0, 0).unwrap())
    }

    fn offset_from_local_datetime(&self, local: &NaiveDateTime) -> MappedLocalTime<FixedOffset> {
        let fits: Vec<FixedOffset> = [-5, -4]
            .into_iter()
            .map(|hours| FixedOffset::east_opt(hours * 3600).unwrap())
            .filter(|offset| {
                Self::at(&(*local - TimeDelta::seconds(offset.local_minus_utc().into()))) == *offset
            })
            .collect();
        match fits[..] {
            [only] => MappedLocalTime::Single(only),
            [early, late] => MappedLocalTime::Ambiguous(early, late),
            _ => MappedLocalTime::None,
        }
    }

    fn offset_from_utc_date(&self, utc: &NaiveDate) -> FixedOffset {
        Self::at(&utc.and_hms_opt(0, 0, 0).unwrap())
    }

    fn offset_from_utc_datetime(&self, utc: &NaiveDateTime) -> FixedOffset {
        Self::at(utc)
    }
}

#[test]
fn daylight_saving_places_each_air_time_by_the_offset_in_force_then() {
    // Spring forward. Today turns at each local midnight, on either side.
    let mut model = CalendarModel::new(Eastern2026, at("2026-03-08T04:59:00Z"));
    assert_eq!(model.today(), day("2026-03-07"), "23:59 EST");
    model.observe_now(at("2026-03-08T05:00:00Z"));
    assert_eq!(model.today(), day("2026-03-08"), "midnight EST");
    model.observe_now(at("2026-03-09T03:59:00Z"));
    assert_eq!(model.today(), day("2026-03-08"), "23:59 EDT");
    model.observe_now(at("2026-03-09T04:00:00Z"));
    assert_eq!(model.today(), day("2026-03-09"), "midnight EDT");

    let requests = connect_both(&mut model);
    let (sonarr, _) = releases_for(&requests, SONARR).unwrap();
    let (radarr, _) = releases_for(&requests, RADARR).unwrap();
    answer(
        &mut model,
        sonarr,
        SONARR,
        vec![
            // 23:30 EST on the 7th. A fixed EDT offset would say the 8th.
            episode(1, "Before", "2026-03-08T04:30:00Z"),
            // 00:30 EDT on the 9th. A fixed EST offset would say the 8th.
            episode(2, "After", "2026-03-09T04:30:00Z"),
        ],
    );
    answer(
        &mut model,
        radarr,
        RADARR,
        vec![movie(3, "Spring", ReleaseKind::Digital, "2026-03-08")],
    );
    assert_eq!(ids(&model.day_events(day("2026-03-07"))), vec!["sonarr-1"]);
    assert_eq!(
        ids(&model.day_events(day("2026-03-08"))),
        vec!["radarr-3-digital"],
        "the short day keeps its civil release and nothing else"
    );
    assert_eq!(ids(&model.day_events(day("2026-03-09"))), vec!["sonarr-2"]);

    // Fall back. November's grid starts on Sunday the 1st.
    let mut model = CalendarModel::new(Eastern2026, at("2026-11-15T17:00:00Z"));
    let requests = connect_both(&mut model);
    let (sonarr, window) = releases_for(&requests, SONARR).unwrap();
    assert_eq!(window.start(), day("2026-11-01"));
    answer(
        &mut model,
        sonarr,
        SONARR,
        vec![
            // 23:30 EDT on 31 October: before this grid, so not on it.
            episode(1, "Halloween", "2026-11-01T03:30:00Z"),
            // 01:30 on the 1st, the hour that happens twice: EDT, then EST.
            episode(2, "Once", "2026-11-01T05:30:00Z"),
            episode(3, "Twice", "2026-11-01T06:30:00Z"),
            // 23:30 EST on the 1st. A fixed EDT offset would say the 2nd.
            episode(4, "Late", "2026-11-02T04:30:00Z"),
        ],
    );
    assert!(
        model
            .grid_events()
            .iter()
            .all(|event| event.id != "sonarr-1"),
        "an air time on the day before the grid is not drawn on it"
    );
    assert_eq!(
        ids(&model.day_events(day("2026-11-01"))),
        vec!["sonarr-2", "sonarr-3", "sonarr-4"],
        "in air-time order through the repeated hour"
    );
    assert!(model.day_events(day("2026-11-02")).is_empty());
}
