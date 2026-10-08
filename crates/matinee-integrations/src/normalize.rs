//! Turn Radarr and Sonarr calendar JSON into [`UpcomingRelease`].
//!
//! One malformed date is skipped. A document that is not a JSON array yields
//! no events. The caller reports a transport or JSON failure separately.

use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::Value;

use crate::model::{ReleaseKind, ReleaseMilestone, ReleaseTiming, UpcomingRelease};
use crate::provider::IntegrationProvider;
use crate::time::{in_window, parse_instant};

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RawImage {
    cover_type: Option<String>,
    remote_url: Option<String>,
    url: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RadarrMovie {
    id: Option<i64>,
    title: Option<String>,
    overview: Option<String>,
    monitored: Option<bool>,
    has_file: Option<bool>,
    in_cinemas: Option<String>,
    digital_release: Option<String>,
    physical_release: Option<String>,
    images: Option<Vec<RawImage>>,
    genres: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SonarrSeries {
    id: Option<i64>,
    title: Option<String>,
    overview: Option<String>,
    monitored: Option<bool>,
    images: Option<Vec<RawImage>>,
    genres: Option<Vec<String>>,
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
struct SonarrEpisode {
    id: Option<i64>,
    title: Option<String>,
    air_date_utc: Option<String>,
    air_date: Option<String>,
    monitored: Option<bool>,
    has_file: Option<bool>,
    season_number: Option<i64>,
    episode_number: Option<i64>,
    series_id: Option<i64>,
    series: Option<SonarrSeries>,
    images: Option<Vec<RawImage>>,
}

pub fn normalize_calendar(
    provider: IntegrationProvider,
    body: &Value,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<UpcomingRelease> {
    let Value::Array(items) = body else {
        return Vec::new();
    };
    match provider {
        IntegrationProvider::Radarr => items
            .iter()
            .filter_map(|item| serde_json::from_value::<RadarrMovie>(item.clone()).ok())
            .flat_map(|movie| radarr_events(movie, start, end))
            .collect(),
        IntegrationProvider::Sonarr => items
            .iter()
            .filter_map(|item| serde_json::from_value::<SonarrEpisode>(item.clone()).ok())
            .filter_map(|episode| sonarr_event(episode, start, end))
            .collect(),
    }
}

fn radarr_events(
    movie: RadarrMovie,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Vec<UpcomingRelease> {
    let (Some(id), Some(title)) = (movie.id, movie.title) else {
        return Vec::new();
    };
    if id == 0 || title.is_empty() || movie.monitored == Some(false) {
        return Vec::new();
    }
    let mut milestones = Vec::new();
    push_milestone(&mut milestones, movie.in_cinemas, ReleaseKind::Theatrical);
    push_milestone(&mut milestones, movie.digital_release, ReleaseKind::Digital);
    push_milestone(
        &mut milestones,
        movie.physical_release,
        ReleaseKind::Physical,
    );
    milestones.sort_by_key(|milestone| milestone.1);
    let release_milestones: Vec<ReleaseMilestone> = milestones
        .iter()
        .map(|(date, _, kind)| ReleaseMilestone {
            date: date.clone(),
            kind: *kind,
        })
        .collect();
    let image = image_url(movie.images.as_deref(), &["poster", "fanart"]);
    let genres = movie.genres.unwrap_or_default();
    let downloaded = movie.has_file.unwrap_or(false);
    let overview = blank_to_none(movie.overview);
    milestones
        .into_iter()
        .filter(|(_, instant, _)| in_window(*instant, start, end))
        .map(|(date, instant, kind)| UpcomingRelease {
            id: format!("radarr-{id}-{}", kind.as_str()),
            source: IntegrationProvider::Radarr,
            source_id: id,
            series_id: None,
            title: title.clone(),
            subtitle: None,
            overview: overview.clone(),
            date,
            timing: ReleaseTiming::CivilDay,
            release_kind: kind,
            image_url: image.clone(),
            genres: genres.clone(),
            monitored: true,
            downloaded,
            season_number: None,
            episode_number: None,
            milestones: Some(release_milestones.clone()),
            instant,
        })
        .collect()
}

fn push_milestone(
    milestones: &mut Vec<(String, DateTime<Utc>, ReleaseKind)>,
    value: Option<String>,
    kind: ReleaseKind,
) {
    let Some(value) = value.filter(|value| !value.is_empty()) else {
        return;
    };
    let Some(instant) = parse_instant(&value) else {
        return;
    };
    milestones.push((value, instant, kind));
}

fn sonarr_event(
    episode: SonarrEpisode,
    start: DateTime<Utc>,
    end: DateTime<Utc>,
) -> Option<UpcomingRelease> {
    let id = episode.id.filter(|id| *id != 0)?;
    // `airDateUtc` wins whenever it is present, even when it is malformed.
    // Only an absent or empty value falls back to the network-local day.
    let (raw_date, timing) = match non_empty(episode.air_date_utc) {
        Some(utc) => (utc, ReleaseTiming::Instant),
        None => (non_empty(episode.air_date)?, ReleaseTiming::CivilDay),
    };
    let instant = parse_instant(&raw_date)?;
    if !in_window(instant, start, end) {
        return None;
    }
    if episode.monitored == Some(false)
        || episode.series.as_ref().and_then(|series| series.monitored) == Some(false)
    {
        return None;
    }
    let series = episode.series;
    let series_id = nonzero(episode.series_id)
        .or_else(|| series.as_ref().and_then(|series| nonzero(series.id)));
    let title = series
        .as_ref()
        .and_then(|series| series.title.clone())
        .filter(|title| !title.is_empty())
        .unwrap_or_else(|| "Upcoming series".to_string());
    let code = match (episode.season_number, episode.episode_number) {
        (Some(season), Some(number)) => Some(format!("S{}E{}", pad2(season), pad2(number))),
        _ => None,
    };
    let subtitle = join_subtitle(code.as_deref(), episode.title.as_deref());
    let image = image_url(episode.images.as_deref(), &["screenshot"]).or_else(|| {
        image_url(
            series.as_ref().and_then(|series| series.images.as_deref()),
            &["poster", "fanart"],
        )
    });
    Some(UpcomingRelease {
        id: format!("sonarr-{id}"),
        source: IntegrationProvider::Sonarr,
        source_id: id,
        series_id,
        title,
        subtitle,
        overview: series
            .as_ref()
            .and_then(|series| blank_to_none(series.overview.clone())),
        date: raw_date,
        timing,
        release_kind: ReleaseKind::Episode,
        image_url: image,
        genres: series.and_then(|series| series.genres).unwrap_or_default(),
        monitored: true,
        downloaded: episode.has_file.unwrap_or(false),
        season_number: episode.season_number,
        episode_number: episode.episode_number,
        milestones: None,
        instant,
    })
}

fn non_empty(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn nonzero(value: Option<i64>) -> Option<i64> {
    value.filter(|value| *value != 0)
}

fn blank_to_none(value: Option<String>) -> Option<String> {
    value.filter(|value| !value.is_empty())
}

fn join_subtitle(code: Option<&str>, title: Option<&str>) -> Option<String> {
    let joined = [code, title]
        .into_iter()
        .flatten()
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>()
        .join(" · ");
    (!joined.is_empty()).then_some(joined)
}

fn pad2(value: i64) -> String {
    let text = value.to_string();
    if text.len() >= 2 {
        text
    } else {
        format!("{text:0>2}")
    }
}

fn image_url(images: Option<&[RawImage]>, preferred: &[&str]) -> Option<String> {
    let images = images?;
    for cover in preferred {
        let Some(image) = images.iter().find(|candidate| {
            candidate
                .cover_type
                .as_deref()
                .is_some_and(|cover_type| cover_type.eq_ignore_ascii_case(cover))
        }) else {
            continue;
        };
        let url = image
            .remote_url
            .clone()
            .filter(|url| !url.is_empty())
            .or_else(|| image.url.clone());
        if let Some(url) = url
            && (url.starts_with("http://") || url.starts_with("https://"))
        {
            return Some(url);
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    fn bounds() -> (DateTime<Utc>, DateTime<Utc>) {
        (
            Utc.with_ymd_and_hms(2026, 8, 5, 0, 0, 0).unwrap(),
            Utc.with_ymd_and_hms(2026, 12, 3, 0, 0, 0).unwrap(),
        )
    }

    #[test]
    fn radarr_milestones_match_the_shipping_fixture() {
        let (start, end) = bounds();
        let body = serde_json::json!([{
            "id": 7,
            "title": "Future Feature",
            "monitored": true,
            "hasFile": false,
            "inCinemas": "2026-08-20T00:00:00Z",
            "digitalRelease": "2026-09-12T00:00:00Z",
            "images": [{ "coverType": "poster", "remoteUrl": "https://image.example/poster.jpg" }]
        }]);
        let events = normalize_calendar(IntegrationProvider::Radarr, &body, start, end);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].id, "radarr-7-theatrical");
        assert_eq!(events[0].release_kind, ReleaseKind::Theatrical);
        assert_eq!(
            events[0].image_url.as_deref(),
            Some("https://image.example/poster.jpg")
        );
        assert_eq!(events[1].id, "radarr-7-digital");
        assert_eq!(events[0].milestones.as_ref().unwrap().len(), 2);
        assert!(!events[0].downloaded);
    }

    #[test]
    fn sonarr_episode_identity_matches_the_shipping_fixture() {
        let (start, end) = bounds();
        let body = serde_json::json!([{
            "id": 22,
            "title": "The Return",
            "airDateUtc": "2026-08-10T03:00:00Z",
            "monitored": true,
            "seriesId": 4,
            "seasonNumber": 2,
            "episodeNumber": 3,
            "series": { "id": 4, "title": "Night Shift", "monitored": true, "genres": ["Drama"] }
        }]);
        let events = normalize_calendar(IntegrationProvider::Sonarr, &body, start, end);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].id, "sonarr-22");
        assert_eq!(events[0].title, "Night Shift");
        assert_eq!(events[0].subtitle.as_deref(), Some("S02E03 · The Return"));
        assert_eq!(events[0].genres, vec!["Drama".to_string()]);
        assert_eq!(events[0].series_id, Some(4));
    }

    #[test]
    fn skips_unmonitored_downloaded_flags_malformed_dates_and_outside_window() {
        let (start, end) = bounds();
        let body = serde_json::json!([
            { "id": 1, "title": "Hidden", "monitored": false, "inCinemas": "2026-08-20T00:00:00Z" },
            { "id": 2, "title": "Broken", "inCinemas": "not-a-date", "digitalRelease": "2026-09-01T00:00:00Z" },
            { "id": 3, "title": "Early", "inCinemas": "2026-01-01T00:00:00Z" },
            { "id": 4, "title": "Owned", "hasFile": true, "digitalRelease": "2026-08-21T00:00:00Z" }
        ]);
        let events = normalize_calendar(IntegrationProvider::Radarr, &body, start, end);
        assert_eq!(events.len(), 2);
        assert_eq!(events[0].title, "Broken");
        assert_eq!(events[0].release_kind, ReleaseKind::Digital);
        assert!(events[1].downloaded);
        assert!(
            normalize_calendar(
                IntegrationProvider::Radarr,
                &serde_json::json!({}),
                start,
                end
            )
            .is_empty()
        );
    }

    #[test]
    fn sonarr_filters_and_falls_back_images() {
        let (start, end) = bounds();
        let body = serde_json::json!([
            {
                "id": 1,
                "airDateUtc": "not-a-date",
                "airDate": "2026-08-11",
                "series": { "id": 9, "title": "Ignored", "monitored": true }
            },
            {
                "id": 6,
                "airDate": "2026-08-16",
                "series": { "id": 6, "title": "Date only", "monitored": true }
            },
            {
                "id": 2,
                "airDateUtc": "2026-08-12T00:00:00Z",
                "monitored": false,
                "series": { "id": 9, "title": "Fallback" }
            },
            {
                "id": 3,
                "airDateUtc": "2026-08-13T00:00:00Z",
                "series": { "id": 9, "title": "Fallback", "monitored": false }
            },
            {
                "id": 4,
                "title": "Shot",
                "airDateUtc": "2026-08-14T00:00:00Z",
                "hasFile": true,
                "seasonNumber": 1,
                "episodeNumber": 1,
                "images": [{ "coverType": "screenshot", "url": "https://img.example/shot.jpg" }],
                "series": {
                    "id": 8,
                    "title": "Pictures",
                    "images": [{ "coverType": "fanart", "remoteUrl": "https://img.example/fan.jpg" }]
                }
            },
            {
                "id": 5,
                "title": "Poster only",
                "airDateUtc": "2026-08-15T00:00:00Z",
                "series": {
                    "id": 7,
                    "title": "Art",
                    "images": [{ "coverType": "poster", "remoteUrl": "note-a-url" }, { "coverType": "fanart", "remoteUrl": "https://img.example/fan.jpg" }]
                }
            }
        ]);
        let events = normalize_calendar(IntegrationProvider::Sonarr, &body, start, end);
        assert_eq!(
            events
                .iter()
                .map(|event| event.id.as_str())
                .collect::<Vec<_>>(),
            vec!["sonarr-6", "sonarr-4", "sonarr-5"]
        );
        assert_eq!(events[0].date, "2026-08-16");
        assert!(events[1].downloaded);
        assert_eq!(
            events[1].image_url.as_deref(),
            Some("https://img.example/shot.jpg")
        );
        assert_eq!(events[1].subtitle.as_deref(), Some("S01E01 · Shot"));
        assert_eq!(
            events[2].image_url.as_deref(),
            Some("https://img.example/fan.jpg")
        );
    }

    #[test]
    fn parity_fixture_covers_milestones_text_and_image_priority() {
        let (start, end) = bounds();
        let movies = normalize_calendar(
            IntegrationProvider::Radarr,
            &serde_json::json!([{
                "id": 9,
                "title": "Window Piece",
                "overview": "A quiet overview",
                "monitored": true,
                "hasFile": false,
                "genres": ["Drama", "Mystery"],
                "inCinemas": "2026-08-20T00:00:00Z",
                "digitalRelease": "2026-09-12T00:00:00Z",
                "physicalRelease": "2026-10-02T00:00:00Z",
                "images": [
                    { "coverType": "fanart", "remoteUrl": "https://image.example/fan.jpg" },
                    { "coverType": "poster", "remoteUrl": "https://image.example/poster.jpg" }
                ]
            }, {
                "id": 10,
                "title": "On the end",
                "inCinemas": "2026-12-03T00:00:00Z"
            }]),
            start,
            end,
        );
        assert_eq!(
            movies
                .iter()
                .map(|event| event.id.as_str())
                .collect::<Vec<_>>(),
            vec![
                "radarr-9-theatrical",
                "radarr-9-digital",
                "radarr-9-physical"
            ]
        );
        assert_eq!(movies[0].title, "Window Piece");
        assert_eq!(movies[0].overview.as_deref(), Some("A quiet overview"));
        assert_eq!(
            movies[0].genres,
            vec!["Drama".to_string(), "Mystery".to_string()]
        );
        assert!(!movies[0].downloaded);
        assert_eq!(movies[2].release_kind, ReleaseKind::Physical);
        assert_eq!(
            movies[0].image_url.as_deref(),
            Some("https://image.example/poster.jpg")
        );
        let episodes = normalize_calendar(
            IntegrationProvider::Sonarr,
            &serde_json::json!([{
                "id": 30,
                "title": "Cold Open",
                "overview": "Episode text",
                "airDateUtc": "2026-08-18T01:00:00Z",
                "monitored": true,
                "hasFile": false,
                "seriesId": 11,
                "seasonNumber": 1,
                "episodeNumber": 4,
                "images": [{ "coverType": "screenshot", "url": "https://img.example/shot.jpg" }],
                "series": {
                    "id": 11,
                    "title": "Night Desk",
                    "overview": "Series text",
                    "monitored": true,
                    "genres": ["Crime"],
                    "images": [
                        { "coverType": "poster", "remoteUrl": "https://img.example/poster.jpg" },
                        { "coverType": "fanart", "remoteUrl": "https://img.example/fan.jpg" }
                    ]
                }
            }]),
            start,
            end,
        );
        assert_eq!(episodes.len(), 1);
        assert_eq!(episodes[0].title, "Night Desk");
        assert_eq!(episodes[0].subtitle.as_deref(), Some("S01E04 · Cold Open"));
        assert_eq!(episodes[0].overview.as_deref(), Some("Series text"));
        assert_eq!(episodes[0].genres, vec!["Crime".to_string()]);
        assert_eq!(episodes[0].series_id, Some(11));
        assert_eq!(episodes[0].season_number, Some(1));
        assert_eq!(episodes[0].episode_number, Some(4));
        assert!(!episodes[0].downloaded);
        assert_eq!(
            episodes[0].image_url.as_deref(),
            Some("https://img.example/shot.jpg")
        );
    }
}
