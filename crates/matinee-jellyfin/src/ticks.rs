//! Jellyfin tick conversion.
//!
//! One tick is 100 nanoseconds. Domain code sees [`std::time::Duration`].

use std::time::Duration;

use serde_json::Number;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum TickError {
    Malformed,
    Overflow,
}

pub(crate) fn duration_from_ticks(ticks: i64) -> Result<Duration, TickError> {
    if ticks < 0 {
        return Err(TickError::Malformed);
    }
    let ticks = u64::try_from(ticks).map_err(|_| TickError::Overflow)?;
    let seconds = ticks / 10_000_000;
    let nanos = (ticks % 10_000_000) * 100;
    let nanos = u32::try_from(nanos).map_err(|_| TickError::Overflow)?;
    Ok(Duration::new(seconds, nanos))
}

pub(crate) fn duration_from_json_number(number: &Number) -> Result<Duration, TickError> {
    if let Some(ticks) = number.as_i64() {
        return duration_from_ticks(ticks);
    }
    if number.as_u64().is_some() {
        return Err(TickError::Overflow);
    }
    Err(TickError::Malformed)
}

pub(crate) fn ticks_from_duration(duration: Duration) -> Result<i64, TickError> {
    let seconds = i64::try_from(duration.as_secs()).map_err(|_| TickError::Overflow)?;
    let from_seconds = seconds.checked_mul(10_000_000).ok_or(TickError::Overflow)?;
    let from_nanos = i64::from(duration.subsec_nanos() / 100);
    from_seconds
        .checked_add(from_nanos)
        .ok_or(TickError::Overflow)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn converts_whole_seconds_and_single_ticks() {
        assert_eq!(duration_from_ticks(0).unwrap(), Duration::ZERO);
        assert_eq!(
            duration_from_ticks(10_000_000).unwrap(),
            Duration::from_secs(1)
        );
        assert_eq!(
            duration_from_ticks(90_000_000).unwrap(),
            Duration::from_secs(9)
        );
        assert_eq!(duration_from_ticks(1).unwrap(), Duration::from_nanos(100));
        assert_eq!(
            ticks_from_duration(Duration::from_secs(1)).unwrap(),
            10_000_000
        );
        assert_eq!(ticks_from_duration(Duration::from_nanos(150)).unwrap(), 1);
    }

    #[test]
    fn rejects_negative_and_fractional_ticks() {
        assert_eq!(duration_from_ticks(-1), Err(TickError::Malformed));
        let fraction = serde_json::from_str::<serde_json::Value>("1.5").unwrap();
        assert_eq!(
            duration_from_json_number(fraction.as_number().unwrap()),
            Err(TickError::Malformed)
        );
        let text = serde_json::json!("100");
        assert!(text.as_number().is_none());
    }

    #[test]
    fn largest_i64_tick_count_fits_and_a_wider_integer_does_not() {
        let duration = duration_from_ticks(i64::MAX).unwrap();
        assert_eq!(duration.as_secs(), (i64::MAX as u64) / 10_000_000);
        let huge = serde_json::from_str::<serde_json::Value>("18446744073709551615").unwrap();
        assert_eq!(
            duration_from_json_number(huge.as_number().unwrap()),
            Err(TickError::Overflow)
        );
        assert_eq!(
            ticks_from_duration(Duration::new(u64::MAX, 0)),
            Err(TickError::Overflow)
        );
    }
}
