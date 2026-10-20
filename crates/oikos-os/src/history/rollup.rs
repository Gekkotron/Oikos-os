//! Hourly rollup task. Every 5 min (plus one run 30 s after start) it fills
//! [`history_hourly`] for the window `[from, hour_floor(now))`, where `from` is
//! `min(done_until, dirty_from, hour_floor(now) − HOUR)` and `done_until` falls back to
//! `hour_floor(first_time)` on first run. The window is walked in 24 h slices.

use std::collections::HashMap;
use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::time;

use oikos_core::history::buckets::{Bucket, Sample, rollup};
use oikos_core::history::{HOUR, Micros};

use super::History;
use super::db::Db;

const START_DELAY: Duration = Duration::from_secs(30);
const INTERVAL: Duration = Duration::from_secs(5 * 60);
const DAY_STEP: Micros = 24 * HOUR;

pub async fn run(db: Db, history: Arc<History>) {
    time::sleep(START_DELAY).await;
    loop {
        let now = crate::clock::now_micros();
        let dirty_from = history.dirty_from.load(Ordering::Relaxed);
        match rollup_once(&db, now, dirty_from).await {
            Ok(()) => history.dirty_from.store(i64::MAX, Ordering::Relaxed),
            Err(e) => tracing::warn!(error = %e, "history rollup failed"),
        }
        time::sleep(INTERVAL).await;
    }
}

/// Public so tests can drive the rollup synchronously.
pub async fn rollup_once(db: &Db, now: Micros, dirty_from: Micros) -> Result<(), sqlx::Error> {
    let to = hour_floor(now);

    let done_until = db.done_until().await?;
    let starting_point = match done_until {
        Some(d) => d,
        None => match db.first_time().await? {
            Some(first) => hour_floor(first),
            None => return Ok(()),
        },
    };

    // from = min(done_until | hour_floor(first_time), dirty_from, hour_floor(now) − HOUR)
    let last_closed = to.saturating_sub(HOUR);
    let from = hour_floor(starting_point.min(dirty_from).min(last_closed));

    if from >= to {
        db.set_done_until(to).await?;
        return Ok(());
    }

    let mut cursor = from;
    while cursor < to {
        let step_end = (cursor + DAY_STEP).min(to);
        let samples = db.samples(cursor, step_end, None).await?;

        let mut by_entity: HashMap<String, Vec<Sample>> = HashMap::new();
        for (id, sample) in samples {
            by_entity.entry(id).or_default().push(sample);
        }

        let entities: Vec<String> = by_entity.keys().cloned().collect();
        let carries = db.hourly_carry(cursor, &entities).await?;

        let mut upserts: Vec<(String, Bucket)> = Vec::new();
        for (id, samples) in by_entity {
            let carry = carries.get(&id).copied();
            for b in rollup(carry, &samples, cursor, step_end, HOUR) {
                upserts.push((id.clone(), b));
            }
        }
        db.upsert_hourly(&upserts).await?;
        cursor = step_end;
    }

    db.set_done_until(to).await?;
    Ok(())
}

fn hour_floor(t: Micros) -> Micros {
    t.div_euclid(HOUR) * HOUR
}
