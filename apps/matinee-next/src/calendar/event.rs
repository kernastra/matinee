//! One release as the calendar shows it, and the rule that places it on a day.
//!
//! The rule is the presentation policy, and it lives here so the integration
//! crate can stay a description of what each source sent:
//!
//! - A [`ReleaseTiming::CivilDay`] value is the day the source wrote. Radarr's
//!   stamps are UTC midnight on that day, and Sonarr's `airDate` is the
//!   network's day. Neither moves when the viewer's zone changes, so the day
//!   is read from the text, never converted.
//! - A [`ReleaseTiming::Instant`] value (Sonarr's `airDateUtc`) is a moment.
//!   It belongs to the viewer's local day of that moment, so an episode that
//!   airs at 9 pm in New York shows on the next day in Tokyo, as it does for
//!   a person there who watches it then.
//!
//! The zone is a parameter, so the rule is testable for any offset.

use chrono::{DateTime, NaiveDate, TimeZone, Utc};
use matinee_integrations::{IntegrationProvider, ReleaseKind, ReleaseTiming, UpcomingRelease};

/// The day a release falls on for a viewer in `zone`. `None` only for a
/// civil day the source wrote in a form that is not a date.
pub(crate) fn presentation_day<Z: TimeZone>(
    release: &UpcomingRelease,
    zone: &Z,
) -> Option<NaiveDate> {
    match release.timing {
        ReleaseTiming::CivilDay => release.civil_day(),
        ReleaseTiming::Instant => Some(release.instant.with_timezone(zone).date_naive()),
    }
}

/// Which media a person is looking at. Movies come from Radarr, series
/// episodes from Sonarr.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) enum MediaFilter {
    All,
    Movies,
    Series,
}

impl MediaFilter {
    pub(crate) const ALL: [MediaFilter; 3] = [Self::All, Self::Movies, Self::Series];

    pub(crate) fn label(self) -> &'static str {
        match self {
            Self::All => "All",
            Self::Movies => "Movies",
            Self::Series => "Series",
        }
    }

    pub(crate) fn admits(self, source: IntegrationProvider) -> bool {
        match self {
            Self::All => true,
            Self::Movies => source == IntegrationProvider::Radarr,
            Self::Series => source == IntegrationProvider::Sonarr,
        }
    }
}

/// A Radarr release milestone on its own day. Only movies have these.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct Milestone {
    pub day: NaiveDate,
    pub kind: ReleaseKind,
}

/// A release on one calendar day. Source-specific fields stay optional: a
/// movie has milestones and no series, an episode has a code and a series.
#[derive(Clone, Debug, PartialEq, Eq)]
pub(crate) struct CalendarEvent {
    /// Stable across refreshes: `radarr-7-digital`, `sonarr-22`.
    pub id: String,
    pub source: IntegrationProvider,
    pub source_id: i64,
    pub series_id: Option<i64>,
    pub day: NaiveDate,
    /// The air time, for an episode the source sent as a moment.
    pub instant: Option<DateTime<Utc>>,
    /// The movie's name, or the series' name for an episode.
    pub title: String,
    /// An episode's `S02E03 · Title`. Absent for a movie.
    pub subtitle: Option<String>,
    pub overview: Option<String>,
    pub genres: Vec<String>,
    /// Theatrical, digital, or physical for a movie; episode for a series.
    pub kind: ReleaseKind,
    /// All of a movie's milestones, in day order. Empty for an episode.
    pub milestones: Vec<Milestone>,
    pub image_url: Option<String>,
    /// The file is already in the library.
    pub downloaded: bool,
}

impl CalendarEvent {
    /// Normalize one release. `None` when its day cannot be placed.
    pub(crate) fn from_release<Z: TimeZone>(release: &UpcomingRelease, zone: &Z) -> Option<Self> {
        let day = presentation_day(release, zone)?;
        let mut milestones: Vec<Milestone> = release
            .milestones
            .iter()
            .flatten()
            .filter_map(|milestone| {
                Some(Milestone {
                    day: milestone.civil_day()?,
                    kind: milestone.kind,
                })
            })
            .collect();
        milestones.sort_by_key(|milestone| milestone.day);
        Some(Self {
            id: release.id.clone(),
            source: release.source,
            source_id: release.source_id,
            series_id: release.series_id,
            day,
            instant: match release.timing {
                ReleaseTiming::Instant => Some(release.instant),
                ReleaseTiming::CivilDay => None,
            },
            title: release.title.clone(),
            subtitle: release.subtitle.clone(),
            overview: release.overview.clone(),
            genres: release.genres.clone(),
            kind: release.release_kind,
            milestones,
            image_url: release.image_url.clone(),
            downloaded: release.downloaded,
        })
    }

    pub(crate) fn is_movie(&self) -> bool {
        self.source == IntegrationProvider::Radarr
    }

    /// The order inside one day: movies first (they have no time of day),
    /// then episodes by air time, then the title and id so equal rows never
    /// trade places between refreshes.
    pub(crate) fn day_order(&self, other: &Self) -> std::cmp::Ordering {
        self.instant
            .is_some()
            .cmp(&other.instant.is_some())
            .then_with(|| self.instant.cmp(&other.instant))
            .then_with(|| self.source_rank().cmp(&other.source_rank()))
            .then_with(|| self.title.to_lowercase().cmp(&other.title.to_lowercase()))
            .then_with(|| self.id.cmp(&other.id))
    }

    fn source_rank(&self) -> u8 {
        match self.source {
            IntegrationProvider::Radarr => 0,
            IntegrationProvider::Sonarr => 1,
        }
    }
}

#[cfg(test)]
mod tests {
    use chrono::{FixedOffset, Local};
    use matinee_integrations::{ReleaseMilestone, UpcomingRelease};

    use super::*;

    fn offset(hours: i32) -> FixedOffset {
        FixedOffset::east_opt(hours * 3600).expect("offset")
    }

    fn release(
        source: IntegrationProvider,
        timing: ReleaseTiming,
        date: &str,
        instant: DateTime<Utc>,
    ) -> UpcomingRelease {
        UpcomingRelease {
            id: format!("{source}-1"),
            source,
            source_id: 1,
            series_id: None,
            title: "Title".into(),
            subtitle: None,
            overview: None,
            date: date.into(),
            timing,
            release_kind: ReleaseKind::Theatrical,
            image_url: None,
            genres: Vec::new(),
            monitored: true,
            downloaded: false,
            season_number: None,
            episode_number: None,
            milestones: None,
            instant,
        }
    }

    fn utc(text: &str) -> DateTime<Utc> {
        DateTime::parse_from_rfc3339(text)
            .expect("fixture")
            .with_timezone(&Utc)
    }

    #[test]
    fn a_radarr_stamp_is_its_written_day_in_every_zone() {
        // The shipping calendar converts this stamp to local time, so it shows
        // on the 19th for a viewer west of UTC. The written day is the 20th.
        let movie = release(
            IntegrationProvider::Radarr,
            ReleaseTiming::CivilDay,
            "2026-08-20T00:00:00Z",
            utc("2026-08-20T00:00:00Z"),
        );
        for zone in [offset(-12), offset(-4), offset(0), offset(9), offset(14)] {
            assert_eq!(
                presentation_day(&movie, &zone),
                NaiveDate::from_ymd_opt(2026, 8, 20),
                "{zone}"
            );
        }
    }

    #[test]
    fn a_sonarr_date_only_value_is_the_written_day_not_utc_midnight() {
        // Shipping parses "2026-08-16" as UTC midnight and shows the 15th in
        // the Americas. The network's day is the 16th.
        let episode = release(
            IntegrationProvider::Sonarr,
            ReleaseTiming::CivilDay,
            "2026-08-16",
            utc("2026-08-16T00:00:00Z"),
        );
        assert_eq!(
            presentation_day(&episode, &offset(-7)),
            NaiveDate::from_ymd_opt(2026, 8, 16)
        );
        assert_eq!(
            presentation_day(&episode, &offset(12)),
            NaiveDate::from_ymd_opt(2026, 8, 16)
        );
    }

    #[test]
    fn an_air_time_belongs_to_the_viewers_local_day() {
        // 9 pm in New York on 10 Aug is 01:00 UTC on 11 Aug.
        let aired = utc("2026-08-11T01:00:00Z");
        let episode = release(
            IntegrationProvider::Sonarr,
            ReleaseTiming::Instant,
            "2026-08-11T01:00:00Z",
            aired,
        );
        assert_eq!(
            presentation_day(&episode, &offset(-4)),
            NaiveDate::from_ymd_opt(2026, 8, 10),
            "still the 10th in New York"
        );
        assert_eq!(
            presentation_day(&episode, &offset(9)),
            NaiveDate::from_ymd_opt(2026, 8, 11),
            "the 11th in Tokyo"
        );
    }

    #[test]
    fn the_local_day_boundary_is_exact_at_midnight() {
        let zone = offset(-4);
        let last_minute = release(
            IntegrationProvider::Sonarr,
            ReleaseTiming::Instant,
            "",
            utc("2026-08-11T03:59:59Z"),
        );
        let first_minute = release(
            IntegrationProvider::Sonarr,
            ReleaseTiming::Instant,
            "",
            utc("2026-08-11T04:00:00Z"),
        );
        assert_eq!(
            presentation_day(&last_minute, &zone),
            NaiveDate::from_ymd_opt(2026, 8, 10)
        );
        assert_eq!(
            presentation_day(&first_minute, &zone),
            NaiveDate::from_ymd_opt(2026, 8, 11)
        );
    }

    #[test]
    fn a_civil_day_that_is_not_a_date_places_nothing() {
        let broken = release(
            IntegrationProvider::Radarr,
            ReleaseTiming::CivilDay,
            "soon",
            utc("2026-08-20T00:00:00Z"),
        );
        assert_eq!(presentation_day(&broken, &offset(0)), None);
        assert!(CalendarEvent::from_release(&broken, &offset(0)).is_none());
    }

    #[test]
    fn a_movie_keeps_its_milestones_in_day_order() {
        let mut movie = release(
            IntegrationProvider::Radarr,
            ReleaseTiming::CivilDay,
            "2026-09-12T00:00:00Z",
            utc("2026-09-12T00:00:00Z"),
        );
        movie.milestones = Some(vec![
            ReleaseMilestone {
                date: "2026-10-02T00:00:00Z".into(),
                kind: ReleaseKind::Physical,
            },
            ReleaseMilestone {
                date: "2026-08-20T00:00:00Z".into(),
                kind: ReleaseKind::Theatrical,
            },
            ReleaseMilestone {
                date: "not a date".into(),
                kind: ReleaseKind::Digital,
            },
        ]);
        let event = CalendarEvent::from_release(&movie, &offset(0)).expect("event");
        assert!(event.is_movie());
        assert_eq!(event.milestones.len(), 2, "the unreadable one is skipped");
        assert_eq!(event.milestones[0].kind, ReleaseKind::Theatrical);
        assert_eq!(
            event.milestones[1].day,
            NaiveDate::from_ymd_opt(2026, 10, 2).unwrap()
        );
        assert_eq!(event.instant, None);
    }

    #[test]
    fn an_episode_reads_as_a_series_with_its_air_time() {
        let mut episode = release(
            IntegrationProvider::Sonarr,
            ReleaseTiming::Instant,
            "2026-08-11T01:00:00Z",
            utc("2026-08-11T01:00:00Z"),
        );
        episode.subtitle = Some("S02E03 · The Return".into());
        episode.series_id = Some(4);
        let event = CalendarEvent::from_release(&episode, &offset(-4)).expect("event");
        assert!(!event.is_movie());
        assert_eq!(event.series_id, Some(4));
        assert_eq!(event.instant, Some(utc("2026-08-11T01:00:00Z")));
        assert!(event.milestones.is_empty());
    }

    #[test]
    fn the_day_order_puts_movies_first_then_air_times() {
        let movie = CalendarEvent::from_release(
            &release(
                IntegrationProvider::Radarr,
                ReleaseTiming::CivilDay,
                "2026-08-20T00:00:00Z",
                utc("2026-08-20T00:00:00Z"),
            ),
            &offset(0),
        )
        .unwrap();
        let early = CalendarEvent::from_release(
            &release(
                IntegrationProvider::Sonarr,
                ReleaseTiming::Instant,
                "2026-08-20T01:00:00Z",
                utc("2026-08-20T01:00:00Z"),
            ),
            &offset(0),
        )
        .unwrap();
        let late = CalendarEvent::from_release(
            &release(
                IntegrationProvider::Sonarr,
                ReleaseTiming::Instant,
                "2026-08-20T05:00:00Z",
                utc("2026-08-20T05:00:00Z"),
            ),
            &offset(0),
        )
        .unwrap();
        assert_eq!(movie.day_order(&early), std::cmp::Ordering::Less);
        assert_eq!(early.day_order(&late), std::cmp::Ordering::Less);
        assert_eq!(late.day_order(&movie), std::cmp::Ordering::Greater);
    }

    #[test]
    fn filters_admit_only_their_media() {
        assert!(MediaFilter::All.admits(IntegrationProvider::Radarr));
        assert!(MediaFilter::All.admits(IntegrationProvider::Sonarr));
        assert!(MediaFilter::Movies.admits(IntegrationProvider::Radarr));
        assert!(!MediaFilter::Movies.admits(IntegrationProvider::Sonarr));
        assert!(MediaFilter::Series.admits(IntegrationProvider::Sonarr));
        assert!(!MediaFilter::Series.admits(IntegrationProvider::Radarr));
    }

    #[test]
    fn the_local_zone_is_usable_for_the_production_policy() {
        // The app passes `Local`. Only the type is checked here; its offset
        // depends on the machine running the test.
        let movie = release(
            IntegrationProvider::Radarr,
            ReleaseTiming::CivilDay,
            "2026-08-20T00:00:00Z",
            utc("2026-08-20T00:00:00Z"),
        );
        assert_eq!(
            presentation_day(&movie, &Local),
            NaiveDate::from_ymd_opt(2026, 8, 20)
        );
    }
}
