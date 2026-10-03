//! The 120-day upcoming window.
//!
//! The shipping UI computes this in local time: start is local midnight of
//! the given instant, and end is that civil date plus `days` midnights. The
//! duration is therefore not always `days * 24h` across a DST change. Instants
//! are returned in UTC.
//!
//! Ambiguous local midnights (a fall-back overlap) use the earlier offset. A
//! local midnight that does not exist is `None`. Midnight is outside the usual
//! DST gap, which is later in the morning.

use chrono::{DateTime, Duration, FixedOffset, NaiveTime, TimeZone, Utc};

pub const DEFAULT_WINDOW_DAYS: i64 = 120;

pub fn upcoming_window(
    from: DateTime<FixedOffset>,
    days: i64,
) -> Option<(DateTime<Utc>, DateTime<Utc>)> {
    if days < 0 {
        return None;
    }
    let zone = from.timezone();
    let start_date = from.date_naive();
    let midnight = NaiveTime::from_hms_opt(0, 0, 0)?;
    let start = zone
        .from_local_datetime(&start_date.and_time(midnight))
        .earliest()?;
    let end_date = start_date.checked_add_signed(Duration::days(days))?;
    let end = zone
        .from_local_datetime(&end_date.and_time(midnight))
        .earliest()?;
    Some((start.with_timezone(&Utc), end.with_timezone(&Utc)))
}

#[cfg(test)]
mod tests {
    use chrono::TimeZone;

    use super::*;

    #[test]
    fn uses_local_midnights_including_a_non_utc_offset() {
        let zone = FixedOffset::west_opt(4 * 3600).unwrap();
        let from = zone.with_ymd_and_hms(2026, 8, 5, 15, 30, 0).unwrap();
        let (start, end) = upcoming_window(from, 120).unwrap();
        assert_eq!(start.to_rfc3339(), "2026-08-05T04:00:00+00:00");
        assert_eq!(end.to_rfc3339(), "2026-12-03T04:00:00+00:00");
    }

    #[test]
    fn default_span_is_one_hundred_twenty_local_days() {
        let zone = FixedOffset::east_opt(0).unwrap();
        let from = zone.with_ymd_and_hms(2026, 1, 1, 0, 30, 0).unwrap();
        let (start, end) = upcoming_window(from, DEFAULT_WINDOW_DAYS).unwrap();
        let days = (end - start).num_days();
        assert_eq!(days, 120);
    }
}
