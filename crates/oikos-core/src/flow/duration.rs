//! Parsing of the delay node's `for` text (`90s`, `5m`, `1h30m`).

use std::time::Duration;

/// Why a duration text was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq, thiserror::Error)]
pub enum DurationError {
    /// The text is empty.
    #[error("duration is empty")]
    Empty,
    /// A number is not followed by a unit (`5`).
    #[error("duration number is missing a unit (use s, m or h)")]
    MissingUnit,
    /// A unit letter other than `s`, `m` or `h`.
    #[error("unknown duration unit `{0}` (use s, m or h)")]
    BadUnit(char),
    /// A unit with no number before it, or a number too large.
    #[error("duration has a missing or invalid number")]
    BadNumber,
    /// The total is zero.
    #[error("duration must be longer than zero")]
    Zero,
}

/// Parses whole seconds, minutes and hours, alone or combined (`1h30m`).
///
/// Every number needs a unit, so `5` is refused rather than guessed; the total must not be zero.
pub fn parse_duration(text: &str) -> Result<Duration, DurationError> {
    if text.is_empty() {
        return Err(DurationError::Empty);
    }
    let mut total: u64 = 0;
    let mut digits = String::new();
    for c in text.chars() {
        if c.is_ascii_digit() {
            digits.push(c);
            continue;
        }
        let factor: u64 = match c {
            's' => 1,
            'm' => 60,
            'h' => 3600,
            other => return Err(DurationError::BadUnit(other)),
        };
        let number: u64 = digits.parse().map_err(|_| DurationError::BadNumber)?;
        digits.clear();
        total = number
            .checked_mul(factor)
            .and_then(|part| total.checked_add(part))
            .ok_or(DurationError::BadNumber)?;
    }
    if !digits.is_empty() {
        return Err(DurationError::MissingUnit);
    }
    if total == 0 {
        return Err(DurationError::Zero);
    }
    Ok(Duration::from_secs(total))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_units_and_combinations() {
        assert_eq!(parse_duration("90s"), Ok(Duration::from_secs(90)));
        assert_eq!(parse_duration("5m"), Ok(Duration::from_secs(300)));
        assert_eq!(parse_duration("1h30m"), Ok(Duration::from_secs(5400)));
        assert_eq!(parse_duration("2h"), Ok(Duration::from_secs(7200)));
    }

    #[test]
    fn rejects_missing_unit_zero_and_garbage() {
        assert_eq!(parse_duration("5"), Err(DurationError::MissingUnit));
        assert_eq!(parse_duration("0m"), Err(DurationError::Zero));
        assert_eq!(parse_duration(""), Err(DurationError::Empty));
        assert_eq!(parse_duration("5x"), Err(DurationError::BadUnit('x')));
        assert_eq!(parse_duration("m"), Err(DurationError::BadNumber));
        assert_eq!(
            parse_duration("99999999999999999999s"),
            Err(DurationError::BadNumber)
        );
    }
}
