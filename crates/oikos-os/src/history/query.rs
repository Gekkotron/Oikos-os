//! The reader side of history: `series`, `daily`, `today` and `last_change`, plus the
//! shared `hours` helper that stitches stored hourly rows together with a live rollup for
//! the window past `done_until`.

use chrono::{DateTime, TimeZone, Utc};
use chrono_tz::Tz;

use oikos_api::{
    HistoryDaily, HistoryRange, HistorySeries, HistoryToday, LastChange, LastChangeItem,
};
use oikos_core::history::buckets::{Bucket, EnergyKind, Sample, fill, kwh, merge, rollup, round3};
use oikos_core::history::{HOUR, Micros};

use super::{History, HistoryError};

fn hour_floor(t: Micros) -> Micros {
    t.div_euclid(HOUR) * HOUR
}

fn align_down(t: Micros, window: Micros) -> Micros {
    t.div_euclid(window) * window
}

fn unavailable(e: sqlx::Error) -> HistoryError {
    HistoryError::Unavailable(e.to_string())
}

/// Returns `days + 1` boundaries: local midnight of (today − days + 1), …, local midnight
/// of today, then `now` as the closing boundary. DST-safe for whole-hour offsets.
pub fn day_starts(zone: Tz, now: Micros, days: u32) -> Vec<Micros> {
    let now_utc: DateTime<Utc> = DateTime::<Utc>::from_timestamp_micros(now)
        .unwrap_or(DateTime::<Utc>::UNIX_EPOCH);
    let now_local = now_utc.with_timezone(&zone);
    let today = now_local.date_naive();

    let mut out = Vec::with_capacity(days as usize + 1);
    for i in (0..days as i64).rev() {
        let date = today - chrono::Duration::days(i);
        let naive = date
            .and_hms_opt(0, 0, 0)
            .expect("00:00:00 is always a valid clock time");
        let local = zone
            .from_local_datetime(&naive)
            .earliest()
            .or_else(|| zone.from_local_datetime(&naive).latest())
            .unwrap_or_else(|| Utc.from_utc_datetime(&naive).with_timezone(&zone));
        out.push(local.timestamp_micros());
    }
    out.push(now);
    out
}

/// Carry at `start` for the 24h series: the last raw sample in `[hour_floor(start), start)`
/// for `entity`, falling back to `hourly_carry(hour_floor(start))`.
async fn series_carry_at(
    db: &super::db::Db,
    entity: &str,
    start: Micros,
) -> Result<Option<f64>, sqlx::Error> {
    let hour_start = hour_floor(start);
    if hour_start < start {
        let pre = db.samples(hour_start, start, Some(entity)).await?;
        if let Some((_, last)) = pre.last() {
            return Ok(Some(last.num));
        }
    }
    let carries = db
        .hourly_carry(hour_start, &[entity.to_string()])
        .await?;
    Ok(carries.get(entity).copied())
}

/// Hourly buckets from `from` to `now`: stored rows for `[from, done_until)`, a live
/// rollup from raw samples for `[done_until, now)`, then quiet-filled over the whole span
/// with the carry at `from`.
pub(crate) async fn hours(
    h: &History,
    entity: &str,
    from: Micros,
    now: Micros,
) -> Result<Vec<Bucket>, HistoryError> {
    let db = h.db()?;
    let done_until = db.done_until().await.map_err(unavailable)?;
    let live_start = done_until.unwrap_or(from).max(from);
    let stored_end = live_start.min(now);

    let stored = if from < stored_end {
        db.hourly(entity, from, stored_end)
            .await
            .map_err(unavailable)?
    } else {
        Vec::new()
    };

    let live = if live_start < now {
        let samples: Vec<Sample> = db
            .samples(live_start, now, Some(entity))
            .await
            .map_err(unavailable)?
            .into_iter()
            .map(|(_, s)| s)
            .collect();
        let carry = match stored.last() {
            Some(last) => Some(last.last),
            None => db
                .hourly_carry(live_start, &[entity.to_string()])
                .await
                .map_err(unavailable)?
                .get(entity)
                .copied(),
        };
        rollup(carry, &samples, live_start, now, HOUR)
    } else {
        Vec::new()
    };

    let mut all = stored;
    all.extend(live);

    let carry_at_from = db
        .hourly_carry(from, &[entity.to_string()])
        .await
        .map_err(unavailable)?
        .get(entity)
        .copied();
    Ok(fill(carry_at_from, &all, from, now, HOUR))
}

pub async fn series(
    h: &History,
    entity: &str,
    range: HistoryRange,
    now: Micros,
) -> Result<HistorySeries, HistoryError> {
    let db = h.db()?;
    if !db.is_recorded(entity).await.map_err(unavailable)? {
        return Err(HistoryError::NotRecorded);
    }

    let window_secs = range.window_secs();
    let window = window_secs * 1_000_000;
    let span = range.span_secs() * 1_000_000;
    let start = align_down(now - span, window);

    let buckets = match range {
        HistoryRange::Day => {
            let carry = series_carry_at(db, entity, start)
                .await
                .map_err(unavailable)?;
            let samples: Vec<Sample> = db
                .samples(start, now, Some(entity))
                .await
                .map_err(unavailable)?
                .into_iter()
                .map(|(_, s)| s)
                .collect();
            let raw = rollup(carry, &samples, start, now, window);
            fill(carry, &raw, start, now, window)
        }
        _ => {
            let bs = hours(h, entity, start, now).await?;
            merge(&bs, window)
        }
    };

    let points = buckets
        .iter()
        .map(|b| {
            (
                b.start / 1_000_000,
                round3(b.mean()),
                round3(b.min),
                round3(b.max),
            )
        })
        .collect();
    Ok(HistorySeries {
        window: window_secs,
        points,
    })
}

pub async fn daily(
    h: &History,
    entity: &str,
    days: u32,
    kind: EnergyKind,
    now: Micros,
) -> Result<HistoryDaily, HistoryError> {
    if !matches!(days, 7 | 30 | 365) {
        return Err(HistoryError::BadRequest("days must be 7, 30 or 365".into()));
    }
    let db = h.db()?;
    if !db.is_recorded(entity).await.map_err(unavailable)? {
        return Err(HistoryError::NotRecorded);
    }
    let boundaries = day_starts(h.zone(), now, days);
    let from = boundaries[0];
    let hourly = hours(h, entity, from, now).await?;

    let mut days_vec = Vec::new();
    for i in 0..days as usize {
        let day_from = boundaries[i];
        let day_to = boundaries[i + 1];
        if let Some(k) = kwh(&hourly, day_from, day_to, kind) {
            days_vec.push((day_from / 1_000_000, round3(k)));
        }
    }
    Ok(HistoryDaily { days: days_vec })
}

pub async fn today(
    h: &History,
    entity: &str,
    kind: EnergyKind,
    now: Micros,
) -> Result<HistoryToday, HistoryError> {
    let db = h.db()?;
    if !db.is_recorded(entity).await.map_err(unavailable)? {
        return Err(HistoryError::NotRecorded);
    }
    let boundaries = day_starts(h.zone(), now, 1);
    let midnight = boundaries[0];
    let hourly = hours(h, entity, midnight, now).await?;
    let kwh_val = kwh(&hourly, midnight, now, kind).map(round3);
    Ok(HistoryToday {
        kwh: kwh_val,
        since: midnight / 1_000_000,
    })
}

pub async fn last_change(h: &History, ids: &[String]) -> Result<LastChange, HistoryError> {
    if ids.is_empty() || ids.len() > 50 {
        return Err(HistoryError::BadRequest(
            "entity count must be 1..=50".into(),
        ));
    }
    let db = h.db()?;
    let changes = db.last_changes(ids).await.map_err(unavailable)?;
    let items = ids
        .iter()
        .map(|id| {
            let changed_at = changes.get(id).map(|m| m / 1_000_000);
            LastChangeItem {
                changed_at,
                recorded: changed_at.is_some(),
            }
        })
        .collect();
    Ok(LastChange { items })
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono_tz::Europe::Paris;

    fn micros_of_local(zone: Tz, s: &str) -> Micros {
        let naive = chrono::NaiveDateTime::parse_from_str(s, "%Y-%m-%d %H:%M:%S").unwrap();
        zone.from_local_datetime(&naive)
            .single()
            .unwrap()
            .timestamp_micros()
    }

    fn hours_between(a: Micros, b: Micros) -> i64 {
        (b - a) / HOUR
    }

    #[test]
    fn day_starts_cross_dst() {
        // Fall-back: 2026-10-25 is 25 h in Europe/Paris.
        let now_fall = micros_of_local(Paris, "2026-10-26 10:00:00");
        let starts = day_starts(Paris, now_fall, 2);
        assert_eq!(starts.len(), 3);
        assert_eq!(starts[0], micros_of_local(Paris, "2026-10-25 00:00:00"));
        assert_eq!(starts[1], micros_of_local(Paris, "2026-10-26 00:00:00"));
        assert_eq!(starts[2], now_fall);
        assert_eq!(hours_between(starts[0], starts[1]), 25);

        // Spring-forward: 2026-03-29 is 23 h.
        let now_spring = micros_of_local(Paris, "2026-03-30 10:00:00");
        let starts = day_starts(Paris, now_spring, 2);
        assert_eq!(starts[0], micros_of_local(Paris, "2026-03-29 00:00:00"));
        assert_eq!(starts[1], micros_of_local(Paris, "2026-03-30 00:00:00"));
        assert_eq!(hours_between(starts[0], starts[1]), 23);
    }
}
