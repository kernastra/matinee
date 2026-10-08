//! Review scenes for Calendar. Each scene is a model prepared with fixture
//! answers through the same `plan` and `apply` the live screen uses. No
//! request is started, so no socket opens and the scene is deterministic.
//!
//! Releases are placed relative to the first of the shown month, so a scene
//! reads the same whatever the day it is built on.

use std::sync::Arc;

use atelier_ui::gpui::Context;
use atelier_ui::prelude::DecodedImage;
use chrono::{DateTime, Local, NaiveDate, TimeDelta, Utc};
use matinee_integrations::{
    IntegrationProvider, ReleaseKind, ReleaseMilestone, ReleaseTiming, UpcomingRelease,
};

use super::grid::month_start;
use super::model::{CalendarModel, Request, Response, SourceFailure};
use super::screen::CalendarScreen;
use crate::artwork::ArtworkLoader;
use crate::runtime::ServiceRuntime;

const RADARR: IntegrationProvider = IntegrationProvider::Radarr;
const SONARR: IntegrationProvider = IntegrationProvider::Sonarr;
const COVER: &[u8] = include_bytes!("../../assets/review/poster.jpg");

/// Which calendar review scene to build.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CalendarPreview {
    /// Both sources, releases across the month.
    Populated,
    /// Both sources, nothing scheduled this month.
    Empty,
    /// A day with a movie and episodes, and an episode chosen.
    SelectedDay,
    /// Mostly movies.
    MovieHeavy,
    /// Mostly episodes, several on one day.
    EpisodeHeavy,
    /// Movies and episodes on the same days.
    Mixed,
    RadarrOnly,
    SonarrOnly,
    /// Neither source is connected.
    Disconnected,
    /// Radarr answered, Sonarr could not be reached.
    PartialFailure,
    /// Connected, and nothing has answered yet.
    Loading,
    /// Both sources could not be reached.
    Error,
}

impl CalendarPreview {
    /// Build the screen for this scene. Reads no vault and opens no socket.
    pub(crate) fn screen(
        self,
        runtime: Arc<ServiceRuntime>,
        loader: ArtworkLoader,
        cx: &mut Context<CalendarScreen>,
    ) -> CalendarScreen {
        let now = Utc::now();
        let mut model = CalendarModel::new(Local, now);
        if self == Self::SelectedDay {
            model.select(day_of(model.month(), 14));
        }
        self.prepare(&mut model, now);
        if self == Self::SelectedDay {
            model.focus_event("sonarr-204");
        }
        let mut screen =
            CalendarScreen::with_model(runtime, None, loader, "Fixture".to_string(), model, cx);
        screen.set_fixture_cover(DecodedImage::decode(COVER, 480).ok());
        screen
    }

    fn prepare(self, model: &mut CalendarModel<Local>, now: DateTime<Utc>) {
        let requests = model.plan();
        let (radarr_link, sonarr_link) = match self {
            Self::Disconnected => (false, false),
            Self::RadarrOnly => (true, false),
            Self::SonarrOnly => (false, true),
            _ => (true, true),
        };
        for (provider, connected) in [(RADARR, radarr_link), (SONARR, sonarr_link)] {
            if let Some(ticket) = link_ticket(&requests, provider) {
                model.apply(Response::Link {
                    ticket,
                    provider,
                    result: Ok(connected),
                });
            }
        }
        let requests = model.plan();
        if self == Self::Loading {
            return;
        }
        for provider in [RADARR, SONARR] {
            let Some(ticket) = releases_ticket(&requests, provider) else {
                continue;
            };
            let result = match (self, provider) {
                (Self::Error, _) | (Self::PartialFailure, SONARR) => {
                    Err(SourceFailure::Unavailable)
                }
                _ => Ok(self.releases(provider, model.month())),
            };
            model.apply(Response::Releases {
                ticket,
                provider,
                at: now,
                result,
            });
        }
    }

    fn releases(self, provider: IntegrationProvider, month: NaiveDate) -> Vec<UpcomingRelease> {
        match (self, provider) {
            (Self::Empty, _) => Vec::new(),
            (Self::MovieHeavy, RADARR) => movie_heavy(month),
            (Self::MovieHeavy, _) => vec![episode(
                901,
                301,
                "Field Notes",
                1,
                5,
                "Quiet Hours",
                month,
                12,
                0,
            )],
            (Self::EpisodeHeavy, SONARR) => episode_heavy(month),
            (Self::EpisodeHeavy, _) => Vec::new(),
            (Self::Mixed | Self::SelectedDay, RADARR) => populated_movies_for_mixed(month),
            (Self::Mixed | Self::SelectedDay, _) => mixed_episodes(month),
            (_, RADARR) => populated_movies(month),
            (_, _) => populated_episodes(month),
        }
    }
}

fn link_ticket(
    requests: &[Request],
    provider: IntegrationProvider,
) -> Option<super::model::Ticket> {
    requests.iter().find_map(|request| match request {
        Request::Link {
            ticket,
            provider: p,
        } if *p == provider => Some(*ticket),
        _ => None,
    })
}

fn releases_ticket(
    requests: &[Request],
    provider: IntegrationProvider,
) -> Option<super::model::Ticket> {
    requests.iter().find_map(|request| match request {
        Request::Releases {
            ticket,
            provider: p,
            ..
        } if *p == provider => Some(*ticket),
        _ => None,
    })
}

/// The `n`th day of `month`, in the shown month.
fn day_of(month: NaiveDate, n: u32) -> NaiveDate {
    month_start(month)
        .checked_add_signed(TimeDelta::days(i64::from(n) - 1))
        .expect("a day in the month")
}

fn movie(
    id: i64,
    title: &str,
    kind: ReleaseKind,
    day: NaiveDate,
    milestones: &[(ReleaseKind, NaiveDate)],
    downloaded: bool,
) -> UpcomingRelease {
    let stamp = format!("{}T00:00:00Z", day.format("%Y-%m-%d"));
    let instant = DateTime::parse_from_rfc3339(&stamp)
        .expect("fixture stamp")
        .with_timezone(&Utc);
    UpcomingRelease {
        id: format!("radarr-{id}-{}", kind.as_str()),
        source: RADARR,
        source_id: id,
        series_id: None,
        title: title.to_string(),
        subtitle: None,
        overview: Some(
            "A quiet town, a missing letter, and the one person who kept the post.".into(),
        ),
        date: stamp,
        timing: ReleaseTiming::CivilDay,
        release_kind: kind,
        image_url: Some(format!("https://covers.example/radarr/{id}.jpg")),
        genres: vec!["Drama".into(), "Mystery".into()],
        monitored: true,
        downloaded,
        season_number: None,
        episode_number: None,
        milestones: (!milestones.is_empty()).then(|| {
            milestones
                .iter()
                .map(|(kind, day)| ReleaseMilestone {
                    date: format!("{}T00:00:00Z", day.format("%Y-%m-%d")),
                    kind: *kind,
                })
                .collect()
        }),
        instant,
    }
}

/// An episode that airs at 12:00 UTC on `day`, which is the same local day in
/// every zone from UTC-11 to UTC+11.
#[allow(clippy::too_many_arguments)]
fn episode(
    id: i64,
    series_id: i64,
    series: &str,
    season: i64,
    number: i64,
    title: &str,
    month: NaiveDate,
    day: u32,
    hour_offset: i64,
) -> UpcomingRelease {
    let air = day_of(month, day)
        .and_hms_opt(12, 0, 0)
        .expect("noon")
        .and_utc()
        + TimeDelta::hours(hour_offset);
    let stamp = air.to_rfc3339_opts(chrono::SecondsFormat::Secs, true);
    UpcomingRelease {
        id: format!("sonarr-{id}"),
        source: SONARR,
        source_id: id,
        series_id: Some(series_id),
        title: series.to_string(),
        subtitle: Some(format!("S{season:02}E{number:02} · {title}")),
        overview: Some(
            "A network desk, a long night, and a story that will not stay filed.".into(),
        ),
        date: stamp.clone(),
        timing: ReleaseTiming::Instant,
        release_kind: ReleaseKind::Episode,
        image_url: Some(format!("https://covers.example/sonarr/{series_id}.jpg")),
        genres: vec!["Drama".into()],
        monitored: true,
        downloaded: false,
        season_number: Some(season),
        episode_number: Some(number),
        milestones: None,
        instant: air,
    }
}

fn populated_movies(month: NaiveDate) -> Vec<UpcomingRelease> {
    vec![
        movie(
            101,
            "The Long Corridor",
            ReleaseKind::Theatrical,
            day_of(month, 4),
            &[
                (ReleaseKind::Theatrical, day_of(month, 4)),
                (ReleaseKind::Digital, day_of(month, 19)),
            ],
            false,
        ),
        movie(
            102,
            "Harbour Lights",
            ReleaseKind::Theatrical,
            day_of(month, 12),
            &[],
            false,
        ),
        movie(
            103,
            "Paper Orchard",
            ReleaseKind::Physical,
            day_of(month, 26),
            &[(ReleaseKind::Physical, day_of(month, 26))],
            true,
        ),
    ]
}

fn populated_episodes(month: NaiveDate) -> Vec<UpcomingRelease> {
    vec![
        episode(201, 401, "Night Desk", 2, 3, "The Quiet Desk", month, 6, 0),
        episode(202, 401, "Night Desk", 2, 4, "Ferry", month, 6, 0),
        episode(203, 402, "Low Tide", 1, 1, "Pilot", month, 9, 0),
        episode(204, 403, "Ash Garden", 3, 7, "Tuesday", month, 14, 0),
        episode(205, 404, "Field Notes", 1, 5, "Quiet Hours", month, 20, 0),
    ]
}

fn mixed_episodes(month: NaiveDate) -> Vec<UpcomingRelease> {
    let mut releases = populated_episodes(month);
    releases.push(episode(
        206,
        405,
        "Glasshouse",
        1,
        2,
        "Cold Open",
        month,
        14,
        0,
    ));
    releases.push(episode(
        207,
        406,
        "Lamp Street",
        4,
        1,
        "Return",
        month,
        26,
        0,
    ));
    releases
}

fn populated_movies_for_mixed(month: NaiveDate) -> Vec<UpcomingRelease> {
    let mut releases = populated_movies(month);
    releases.push(movie(
        104,
        "Glasshouse",
        ReleaseKind::Theatrical,
        day_of(month, 14),
        &[(ReleaseKind::Theatrical, day_of(month, 14))],
        false,
    ));
    releases
}

fn movie_heavy(month: NaiveDate) -> Vec<UpcomingRelease> {
    let titles = [
        "The Long Corridor",
        "Harbour Lights",
        "Paper Orchard",
        "Iron Sea",
        "Mercy Road",
        "Slow Engine",
        "Blue Almanac",
        "Winter Hymn",
        "Second Harvest",
    ];
    titles
        .iter()
        .enumerate()
        .map(|(index, title)| {
            let day = 2 + (index as u32) * 3;
            movie(
                110 + index as i64,
                title,
                ReleaseKind::Theatrical,
                day_of(month, day),
                &[(ReleaseKind::Theatrical, day_of(month, day))],
                index % 4 == 0,
            )
        })
        .collect()
}

fn episode_heavy(month: NaiveDate) -> Vec<UpcomingRelease> {
    let shows = [
        "Night Desk",
        "Low Tide",
        "Ash Garden",
        "Field Notes",
        "Glasshouse",
        "Lamp Street",
    ];
    let mut releases = Vec::new();
    let mut id = 500;
    for day in [3, 5, 8, 12, 15, 19, 22, 27] {
        for (offset, show) in shows.iter().take(4).enumerate() {
            id += 1;
            releases.push(episode(
                id,
                600 + offset as i64,
                show,
                1,
                i64::from(day),
                "Episode",
                month,
                day,
                offset as i64,
            ));
        }
    }
    releases
}
