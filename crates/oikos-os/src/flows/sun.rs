//! Wall-clock bookkeeping for `trigger.sun`: the armed events and the search for the next one.
//! `oikos-core` computes the sun's position; every clock and zone lives here.

use chrono::{DateTime, Duration, Utc};
use chrono_tz::Tz;
use oikos_core::flow::SunWhich;
use oikos_core::flow::sun::sun_events;

/// How many days after the current one are searched for the next event.
const SEARCH_DAYS: i64 = 7;

/// A sun node armed for its next event.
pub(super) struct ArmedSun {
    pub node: String,
    /// The instant the node fires at next, offset included.
    pub at: DateTime<Tz>,
    pub which: SunWhich,
    pub offset_minutes: i32,
}

/// The first fire of `which` (shifted by `offset_minutes`) strictly after `now`, searching the
/// UTC day of `now` and the seven that follow; `None` when none falls in that window (polar
/// night, midnight sun).
pub(super) fn next_sun_fire(
    lat: f64,
    lon: f64,
    zone: Tz,
    which: SunWhich,
    offset_minutes: i32,
    now: DateTime<Tz>,
) -> Option<DateTime<Tz>> {
    let first = now.with_timezone(&Utc).date_naive();
    (0..=SEARCH_DAYS)
        .filter_map(|days| {
            let events = sun_events(lat, lon, first + Duration::days(days));
            let event = match which {
                SunWhich::Sunrise => events.sunrise,
                SunWhich::Sunset => events.sunset,
                SunWhich::Dusk => events.dusk,
                SunWhich::Dawn => events.dawn,
            }?;
            Some((event + Duration::minutes(i64::from(offset_minutes))).with_timezone(&zone))
        })
        .find(|at| *at > now)
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn next_sun_fire_skips_past_events_and_arms_tomorrow() {
        let zone = chrono_tz::Europe::Paris;
        let (lat, lon) = (48.8566, 2.3522);
        // 2026-01-01 20:00 Paris: today's sunset at ~17:03 has already passed.
        let now = zone
            .with_ymd_and_hms(2026, 1, 1, 20, 0, 0)
            .single()
            .unwrap();
        let at = next_sun_fire(lat, lon, zone, SunWhich::Sunset, 0, now).unwrap();
        assert_eq!(
            at.date_naive(),
            chrono::NaiveDate::from_ymd_opt(2026, 1, 2).unwrap()
        );
    }

    #[test]
    fn next_sun_fire_applies_signed_offset() {
        let zone = chrono_tz::Europe::Paris;
        let (lat, lon) = (48.8566, 2.3522);
        let now = zone.with_ymd_and_hms(2026, 1, 1, 6, 0, 0).single().unwrap();
        let base = next_sun_fire(lat, lon, zone, SunWhich::Sunset, 0, now).unwrap();
        let minus30 = next_sun_fire(lat, lon, zone, SunWhich::Sunset, -30, now).unwrap();
        assert_eq!((base - minus30).num_minutes(), 30);
    }

    #[test]
    fn next_sun_fire_returns_none_in_polar_night() {
        let zone = chrono_tz::Europe::Oslo;
        let now = zone
            .with_ymd_and_hms(2026, 1, 1, 12, 0, 0)
            .single()
            .unwrap();
        assert_eq!(
            next_sun_fire(78.22, 15.65, zone, SunWhich::Sunrise, 0, now),
            None
        );
    }
}
