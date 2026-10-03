//! A calendar day, without a date library.
//!
//! Server timestamps often carry a time and a zone. Callers need the day.
//! A value that is not a real calendar day is absent, not an error.

/// A year, month, and day in the Gregorian calendar.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct CalendarDate {
    year: i32,
    month: u8,
    day: u8,
}

impl CalendarDate {
    /// Parse `YYYY-MM-DD`, ignoring a time that follows a `T` or a space.
    ///
    /// Trailing text that is not a time is absent.
    pub fn parse(value: &str) -> Option<Self> {
        let value = value.trim();
        let (date, rest) = value
            .split_once(['T', ' '])
            .map(|(date, rest)| (date, Some(rest)))
            .unwrap_or((value, None));
        if let Some(rest) = rest
            && !rest.as_bytes().first().is_some_and(u8::is_ascii_digit)
        {
            return None;
        }
        let mut parts = date.split('-');
        let year: i32 = parts.next()?.parse().ok()?;
        let month: u8 = parts.next()?.parse().ok()?;
        let day: u8 = parts.next()?.parse().ok()?;
        if parts.next().is_some() || !(1..=9999).contains(&year) {
            return None;
        }
        let max = days_in_month(year, month)?;
        if day == 0 || day > max {
            return None;
        }
        Some(Self { year, month, day })
    }

    pub fn year(self) -> i32 {
        self.year
    }

    pub fn month(self) -> u8 {
        self.month
    }

    pub fn day(self) -> u8 {
        self.day
    }
}

fn days_in_month(year: i32, month: u8) -> Option<u8> {
    Some(match month {
        1 | 3 | 5 | 7 | 8 | 10 | 12 => 31,
        4 | 6 | 9 | 11 => 30,
        2 if is_leap(year) => 29,
        2 => 28,
        _ => return None,
    })
}

fn is_leap(year: i32) -> bool {
    year % 4 == 0 && (year % 100 != 0 || year % 400 == 0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_day_and_ignores_a_time() {
        let date = CalendarDate::parse("2024-05-01T00:00:00.0000000Z").unwrap();
        assert_eq!((date.year(), date.month(), date.day()), (2024, 5, 1));
        assert_eq!(
            CalendarDate::parse("2020-02-29"),
            CalendarDate::parse("2020-02-29T12:00:00Z")
        );
    }

    #[test]
    fn malformed_days_are_absent() {
        assert!(CalendarDate::parse("").is_none());
        assert!(CalendarDate::parse("not-a-date").is_none());
        assert!(CalendarDate::parse("2024-13-01").is_none());
        assert!(CalendarDate::parse("2023-02-29").is_none());
        assert!(CalendarDate::parse("2024-04-31").is_none());
        assert!(CalendarDate::parse("2024-05-01 extra").is_none());
    }
}
