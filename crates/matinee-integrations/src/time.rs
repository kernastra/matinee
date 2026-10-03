//! Release instants.
//!
//! Radarr and Sonarr send ISO timestamps and, for Sonarr air dates, a calendar
//! day. This parser accepts those shapes. It does not accept the rest of
//! `Date.parse`.
//!
//! A zone-less date-time is UTC. A date-only value is UTC midnight. Both are
//! explicit so a filter does not depend on the machine zone. The shipping UI
//! still builds the request window from local midnight; see [`super::window`].

use chrono::{DateTime, NaiveDate, NaiveDateTime, Utc};

pub fn parse_instant(value: &str) -> Option<DateTime<Utc>> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }
    if let Ok(parsed) = DateTime::parse_from_rfc3339(value) {
        return Some(parsed.with_timezone(&Utc));
    }
    if let Ok(date) = NaiveDate::parse_from_str(value, "%Y-%m-%d") {
        return date.and_hms_opt(0, 0, 0).map(|naive| naive.and_utc());
    }
    for pattern in [
        "%Y-%m-%dT%H:%M:%S%.f",
        "%Y-%m-%dT%H:%M:%S",
        "%Y-%m-%d %H:%M:%S%.f",
        "%Y-%m-%d %H:%M:%S",
    ] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(value, pattern) {
            return Some(naive.and_utc());
        }
    }
    None
}

pub fn in_window(instant: DateTime<Utc>, start: DateTime<Utc>, end: DateTime<Utc>) -> bool {
    instant >= start && instant < end
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_server_shapes_and_rejects_garbage() {
        assert!(parse_instant("2026-08-20T00:00:00Z").is_some());
        assert!(parse_instant("2026-08-10T03:00:00.000Z").is_some());
        assert_eq!(
            parse_instant("2026-08-20").unwrap().to_rfc3339(),
            "2026-08-20T00:00:00+00:00"
        );
        assert!(parse_instant("2026-08-20T00:00:00").is_some());
        assert!(parse_instant("not-a-date").is_none());
        assert!(parse_instant("").is_none());
        assert!(parse_instant("2026-13-40").is_none());
    }
}
