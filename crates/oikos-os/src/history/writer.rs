//! Writer task: drains the writer channel into a bounded [`Pending`] buffer, diffs each
//! batch against the dedupe cache, and inserts the resulting rows. On a database error the
//! batch is re-queued at the front and the task backs off exponentially up to 30 s.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use tokio::sync::mpsc;
use tokio::time::{self, MissedTickBehavior};

use oikos_core::history::buckets::EnergyKind;
use oikos_core::history::{HOUR, Incoming, LastWritten, Pending, Rows};

use super::History;
use super::db::Db;

const _: EnergyKind = EnergyKind::Counter; // keep oikos_core::history::buckets reachable

/// How many pending incoming states trigger an early flush. The 1 s tick covers slow
/// streams; this covers busy ones.
const BATCH_THRESHOLD: usize = 500;
const FLUSH_INTERVAL: Duration = Duration::from_secs(1);
const INITIAL_BACKOFF: Duration = Duration::from_secs(1);
const MAX_BACKOFF: Duration = Duration::from_secs(30);

pub async fn run(
    db: Db,
    history: Arc<History>,
    mut rx: mpsc::Receiver<Incoming>,
    buffer_rows: usize,
) {
    if let Err(e) = db.migrate().await {
        tracing::error!(error = %e, "history migrate failed; writer exiting");
        return;
    }
    let mut last_written = match db.load_latest().await {
        Ok(lw) => lw,
        Err(e) => {
            tracing::error!(error = %e, "history load_latest failed; writer exiting");
            return;
        }
    };
    let mut pending = Pending::new(buffer_rows.max(1));
    let mut backoff = INITIAL_BACKOFF;
    let mut tick = time::interval(FLUSH_INTERVAL);
    tick.set_missed_tick_behavior(MissedTickBehavior::Delay);

    loop {
        tokio::select! {
            maybe = rx.recv() => {
                let Some(inc) = maybe else {
                    // Channel closed; try one final flush and exit.
                    let _ = flush(&db, &mut pending, &mut last_written, &history, &mut backoff).await;
                    return;
                };
                if !pending.push(inc) {
                    tracing::warn!(pending = pending.len(), "history pending buffer full; dropped oldest");
                }
                if pending.len() >= BATCH_THRESHOLD {
                    flush(&db, &mut pending, &mut last_written, &history, &mut backoff).await;
                }
            }
            _ = tick.tick() => {
                if !pending.is_empty() {
                    flush(&db, &mut pending, &mut last_written, &history, &mut backoff).await;
                }
            }
        }
    }
}

/// Flush as much of `pending` as the batch size allows, in order. Returns after one
/// batch's attempt — the caller re-enters the loop and tries again next tick.
async fn flush(
    db: &Db,
    pending: &mut Pending,
    last_written: &mut LastWritten,
    history: &Arc<History>,
    backoff: &mut Duration,
) {
    let batch = pending.take(BATCH_THRESHOLD);
    if batch.is_empty() {
        return;
    }
    let mut tentative = last_written.clone();
    let mut rows = Rows::default();
    for inc in &batch {
        tentative.diff(inc, &mut rows);
    }
    let oldest_at = batch.iter().map(|i| i.at).min();

    if rows.is_empty() {
        // Nothing to insert but we still consumed the batch (all duplicates).
        *last_written = tentative;
        *backoff = INITIAL_BACKOFF;
        return;
    }
    match db.insert(&rows).await {
        Ok(()) => {
            *last_written = tentative;
            *backoff = INITIAL_BACKOFF;
            if let Some(oldest) = oldest_at {
                let oldest_hour = (oldest.div_euclid(HOUR)) * HOUR;
                let now = crate::clock::now_micros();
                let current_hour = (now.div_euclid(HOUR)) * HOUR;
                if oldest_hour < current_hour {
                    history.dirty_from.fetch_min(oldest_hour, Ordering::Relaxed);
                }
            }
        }
        Err(e) => {
            tracing::error!(error = %e, batch = batch.len(), "history insert failed; backing off");
            pending.requeue_front(batch);
            time::sleep(*backoff).await;
            *backoff = (*backoff * 2).min(MAX_BACKOFF);
        }
    }
}
