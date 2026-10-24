//! A Solcast forecast that covers today, for the hub and poller tests.
#![allow(dead_code)]

use chrono::{DateTime, Duration, TimeZone, Utc};
use chrono_tz::Europe::Paris;
use oikos_solcast::{Forecast, merge, parse_forecasts};

/// The Task 1 fixture: 48 half-hour periods of 2026-10-23, Europe/Paris.
const FIXTURE: &str = include_str!("../../../oikos-solcast/tests/fixtures/forecasts.json");

/// The fixture shifted so its first period starts at local midnight (Europe/Paris) of the day
/// of `at`, merged into a one-site forecast fetched at `at`.
pub fn forecast_covering(at: DateTime<Utc>) -> Forecast {
    let mut periods = parse_forecasts(FIXTURE).expect("fixture parses");
    let first_start = periods[0].start();
    let today = at.with_timezone(&Paris).date_naive();
    let midnight = Paris
        .from_local_datetime(&today.and_hms_opt(0, 0, 0).unwrap())
        .earliest()
        .expect("midnight exists")
        .with_timezone(&Utc);
    let shift: Duration = midnight - first_start;
    for period in &mut periods {
        period.end += shift;
    }
    merge(vec![periods], at.timestamp())
}

/// [`forecast_covering`] now.
pub fn forecast_today() -> Forecast {
    forecast_covering(Utc::now())
}
