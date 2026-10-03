//! Home shelf selection.
//!
//! Downloaded releases are excluded. A Radarr movie appears once, preferring a
//! Digital or Physical milestone over Theatrical. The first of those home
//! milestones in chronological order wins, so Digital stays ahead of a later
//! Physical date. Sonarr keeps the first upcoming episode of each series.

use crate::model::{ReleaseKind, UpcomingRelease};
use crate::provider::IntegrationProvider;

pub fn home_upcoming(events: &[UpcomingRelease], limit: usize) -> Vec<UpcomingRelease> {
    let mut preferred: Vec<(i64, String)> = Vec::new();
    for event in events {
        if event.source != IntegrationProvider::Radarr || event.downloaded {
            continue;
        }
        let home = is_home(event.release_kind);
        match preferred.iter_mut().find(|(id, _)| *id == event.source_id) {
            Some((_, current)) => {
                let current_home = events
                    .iter()
                    .find(|candidate| candidate.id == *current)
                    .is_some_and(|candidate| is_home(candidate.release_kind));
                if home && !current_home {
                    *current = event.id.clone();
                }
            }
            None => preferred.push((event.source_id, event.id.clone())),
        }
    }

    let mut seen_series = Vec::new();
    let mut seen_movies = Vec::new();
    let mut selected = Vec::new();
    for event in events {
        if event.downloaded {
            continue;
        }
        if event.source == IntegrationProvider::Radarr {
            let preferred_id = preferred
                .iter()
                .find(|(id, _)| *id == event.source_id)
                .map(|(_, id)| id.as_str());
            if seen_movies.contains(&event.source_id) || preferred_id != Some(event.id.as_str()) {
                continue;
            }
            seen_movies.push(event.source_id);
        }
        if event.source == IntegrationProvider::Sonarr
            && let Some(series_id) = event.series_id
        {
            if seen_series.contains(&series_id) {
                continue;
            }
            seen_series.push(series_id);
        }
        selected.push(event.clone());
    }
    selected.sort_by(|left, right| left.instant.cmp(&right.instant));
    selected.truncate(limit);
    selected
}

fn is_home(kind: ReleaseKind) -> bool {
    matches!(kind, ReleaseKind::Digital | ReleaseKind::Physical)
}

#[cfg(test)]
mod tests {
    use chrono::{TimeZone, Utc};

    use super::*;
    use crate::normalize::normalize_calendar;

    fn bounds() -> (chrono::DateTime<Utc>, chrono::DateTime<Utc>) {
        (
            Utc.with_ymd_and_hms(2026, 8, 5, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 12, 3, 0, 0, 0).unwrap(),
        )
    }

    #[test]
    fn keeps_one_episode_per_series() {
        let (start, end) = bounds();
        let events = normalize_calendar(
            IntegrationProvider::Sonarr,
            &serde_json::json!([
                { "id": 1, "title": "One", "airDateUtc": "2026-08-10T03:00:00Z", "seriesId": 9, "series": { "id": 9, "title": "Same Show" } },
                { "id": 2, "title": "Two", "airDateUtc": "2026-08-17T03:00:00Z", "seriesId": 9, "series": { "id": 9, "title": "Same Show" } },
                { "id": 3, "title": "Pilot", "airDateUtc": "2026-08-11T03:00:00Z", "seriesId": 10, "series": { "id": 10, "title": "Another Show" } }
            ]),
            start,
            end,
        );
        let home = home_upcoming(&events, 12);
        assert_eq!(
            home.iter()
                .map(|event| event.id.as_str())
                .collect::<Vec<_>>(),
            vec!["sonarr-1", "sonarr-3"]
        );
    }

    #[test]
    fn prefers_digital_over_theatrical_and_physical() {
        let (start, end) = bounds();
        let events = normalize_calendar(
            IntegrationProvider::Radarr,
            &serde_json::json!([{
                "id": 12,
                "title": "The Long Wait",
                "inCinemas": "2026-08-10T00:00:00Z",
                "digitalRelease": "2026-09-08T00:00:00Z",
                "physicalRelease": "2026-10-01T00:00:00Z"
            }]),
            start,
            end,
        );
        let home = home_upcoming(&events, 12);
        assert_eq!(home.len(), 1);
        assert_eq!(home[0].id, "radarr-12-digital");
        assert_eq!(home[0].release_kind, ReleaseKind::Digital);
    }

    #[test]
    fn drops_downloaded_and_honors_the_limit() {
        let (start, end) = bounds();
        let events = normalize_calendar(
            IntegrationProvider::Radarr,
            &serde_json::json!([
                { "id": 1, "title": "Owned", "hasFile": true, "digitalRelease": "2026-08-10T00:00:00Z" },
                { "id": 2, "title": "Soon", "digitalRelease": "2026-08-11T00:00:00Z" },
                { "id": 3, "title": "Later", "digitalRelease": "2026-08-12T00:00:00Z" }
            ]),
            start,
            end,
        );
        let home = home_upcoming(&events, 1);
        assert_eq!(home.len(), 1);
        assert_eq!(home[0].title, "Soon");
    }
}
