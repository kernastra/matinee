//! Month arithmetic for the calendar grid. Pure: chrono's calendar types only,
//! no clock, no zone.
//!
//! The grid is six rows of seven days starting on Sunday, which is what the
//! shipping calendar shows. Its first day is the Sunday on or before the 1st
//! of the month, so the grid can show days from the months on either side.

use chrono::{Datelike, Days, Months, NaiveDate};

/// Cells in the month grid: six weeks, always, so the grid never changes
/// height from month to month.
pub(crate) const GRID_DAYS: u64 = 42;

/// The 42 days a month's grid shows, starting on a Sunday.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub(crate) struct Window {
    start: NaiveDate,
}

impl Window {
    /// The grid for the month that contains `day`.
    pub(crate) fn for_month(day: NaiveDate) -> Self {
        let first = month_start(day);
        let lead = u64::from(first.weekday().num_days_from_sunday());
        let start = first
            .checked_sub_days(Days::new(lead))
            .expect("a grid start is always a representable date");
        Self { start }
    }

    /// The first day shown: a Sunday, possibly in the previous month.
    #[cfg(test)]
    pub(crate) fn start(self) -> NaiveDate {
        self.start
    }

    /// The last day shown.
    pub(crate) fn last(self) -> NaiveDate {
        self.day(GRID_DAYS - 1)
    }

    /// The `index`th day of the grid, `0..42`.
    pub(crate) fn day(self, index: u64) -> NaiveDate {
        self.start
            .checked_add_days(Days::new(index))
            .expect("grid days are within the representable range")
    }

    /// Whether `day` is one of the 42 shown.
    pub(crate) fn contains(self, day: NaiveDate) -> bool {
        day >= self.start && day <= self.last()
    }

    /// The position of `day` in the grid, if it is shown.
    pub(crate) fn index_of(self, day: NaiveDate) -> Option<usize> {
        if !self.contains(day) {
            return None;
        }
        usize::try_from((day - self.start).num_days()).ok()
    }

    /// The days to ask a source for, as `[from, to)` in UTC midnights.
    ///
    /// The grid is padded by one day on each side. A local day starts no
    /// earlier than 14 hours before its UTC midnight and ends no later than 12
    /// hours after the next one, because offsets run from UTC−12 to UTC+14. So
    /// this range holds every instant whose local day is in the grid. The app
    /// drops what lands outside the grid.
    pub(crate) fn fetch_range(
        self,
    ) -> (chrono::DateTime<chrono::Utc>, chrono::DateTime<chrono::Utc>) {
        let from = self
            .start
            .checked_sub_days(Days::new(1))
            .expect("representable");
        let to = self
            .last()
            .checked_add_days(Days::new(2))
            .expect("representable");
        (utc_midnight(from), utc_midnight(to))
    }
}

/// The first day of the month that contains `day`.
pub(crate) fn month_start(day: NaiveDate) -> NaiveDate {
    day.with_day(1).expect("every month has a first day")
}

/// The first day of the month `months` after `month` (negative: before).
/// Clamped to the representable range, which no real calendar reaches.
pub(crate) fn shift_month(month: NaiveDate, months: i32) -> NaiveDate {
    let shifted = if months >= 0 {
        month.checked_add_months(Months::new(months.unsigned_abs()))
    } else {
        month.checked_sub_months(Months::new(months.unsigned_abs()))
    };
    shifted.map(month_start).unwrap_or(month)
}

/// Days in the month that contains `day`. Leap years included.
pub(crate) fn days_in_month(day: NaiveDate) -> u32 {
    let first = month_start(day);
    let next = first
        .checked_add_months(Months::new(1))
        .expect("the next month is representable");
    (next - first).num_days() as u32
}

/// The same day number in another month, clamped to that month's length:
/// January 31 moves to February 28 (29 in a leap year).
pub(crate) fn same_day_in_month(day: NaiveDate, month: NaiveDate) -> NaiveDate {
    let first = month_start(month);
    let number = day.day().min(days_in_month(first));
    first.with_day(number).expect("clamped to the month")
}

fn utc_midnight(day: NaiveDate) -> chrono::DateTime<chrono::Utc> {
    day.and_hms_opt(0, 0, 0).expect("midnight exists").and_utc()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn date(text: &str) -> NaiveDate {
        NaiveDate::parse_from_str(text, "%Y-%m-%d").expect("fixture date")
    }

    #[test]
    fn the_grid_starts_on_the_sunday_on_or_before_the_first() {
        // 1 Aug 2026 is a Saturday, so the grid starts on Sunday 26 July.
        let august = Window::for_month(date("2026-08-01"));
        assert_eq!(august.start(), date("2026-07-26"));
        assert_eq!(august.last(), date("2026-09-05"));
        // 1 Mar 2026 is a Sunday: the grid starts on the 1st itself.
        assert_eq!(
            Window::for_month(date("2026-03-01")).start(),
            date("2026-03-01")
        );
    }

    #[test]
    fn every_grid_is_six_weeks_of_seven_days() {
        for month in ["2026-01-01", "2026-02-01", "2026-06-01", "2028-02-01"] {
            let window = Window::for_month(date(month));
            assert_eq!((window.last() - window.start()).num_days(), 41, "{month}");
            assert_eq!(window.start().weekday().num_days_from_sunday(), 0);
        }
    }

    #[test]
    fn a_day_is_found_only_inside_its_grid() {
        let august = Window::for_month(date("2026-08-15"));
        assert_eq!(august.index_of(date("2026-08-01")), Some(6));
        assert_eq!(august.index_of(date("2026-07-26")), Some(0));
        assert_eq!(august.index_of(date("2026-09-05")), Some(41));
        assert_eq!(august.index_of(date("2026-07-25")), None);
        assert_eq!(august.index_of(date("2026-09-06")), None);
        for index in 0..GRID_DAYS {
            let day = august.day(index);
            assert_eq!(august.index_of(day), Some(index as usize));
        }
    }

    #[test]
    fn months_move_across_year_boundaries() {
        assert_eq!(shift_month(date("2026-12-01"), 1), date("2027-01-01"));
        assert_eq!(shift_month(date("2027-01-01"), -1), date("2026-12-01"));
        assert_eq!(shift_month(date("2026-03-18"), 0), date("2026-03-01"));
        assert_eq!(shift_month(date("2026-01-15"), -13), date("2024-12-01"));
    }

    #[test]
    fn month_lengths_follow_leap_years_and_centuries() {
        assert_eq!(days_in_month(date("2024-02-10")), 29);
        assert_eq!(days_in_month(date("2026-02-10")), 28);
        assert_eq!(days_in_month(date("2000-02-10")), 29, "divisible by 400");
        assert_eq!(days_in_month(date("2100-02-10")), 28, "century, not by 400");
        assert_eq!(days_in_month(date("2026-04-30")), 30);
        assert_eq!(days_in_month(date("2026-12-31")), 31);
    }

    #[test]
    fn a_selected_day_keeps_its_number_where_the_month_allows() {
        assert_eq!(
            same_day_in_month(date("2026-01-31"), date("2026-02-01")),
            date("2026-02-28")
        );
        assert_eq!(
            same_day_in_month(date("2028-01-31"), date("2028-02-01")),
            date("2028-02-29")
        );
        assert_eq!(
            same_day_in_month(date("2026-08-18"), date("2026-09-01")),
            date("2026-09-18")
        );
    }

    #[test]
    fn the_fetch_range_pads_the_grid_by_a_day_each_side() {
        let window = Window::for_month(date("2026-08-01"));
        let (from, to) = window.fetch_range();
        assert_eq!(from.to_rfc3339(), "2026-07-25T00:00:00+00:00");
        assert_eq!(to.to_rfc3339(), "2026-09-07T00:00:00+00:00");
    }
}
