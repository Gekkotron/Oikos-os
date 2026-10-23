//! The task that keeps the solar forecast fresh within the daily request budget: it asks
//! [`oikos_solcast::decide`] when to call Solcast, fetches every site, and hands the merged
//! result to the hub.

use std::sync::Arc;
use std::time::Duration;

use chrono::{DateTime, Duration as ChronoDuration, NaiveDate, TimeZone, Utc};
use oikos_api::ConnectionStatus;
use oikos_core::flow::sun::sun_events;
use oikos_solcast::{
    ApiKey, BASE_URL, Client, Decision, SolcastError, decide, merge, refreshes_per_day, slots,
};
use tokio::task::JoinHandle;

use crate::clock;
use crate::config::read_key_file;
use crate::hub::Hub;

/// How long the poller waits after the entities are published, so the broker can replay the
/// retained cache before the first decision. Without the cache a restart would look like a
/// first start.
pub const CACHE_GRACE: Duration = Duration::from_secs(3);

/// The shortest sleep between two decisions.
const MIN_SLEEP: Duration = Duration::from_secs(1);

/// Sleep used once the key was refused: nothing more is called until a restart.
const FAR_FUTURE: Duration = Duration::from_secs(24 * 3600);

/// Days of forecast kept after today.
const KEEP_DAYS: i64 = 8;

impl Hub {
    /// Starts the Solcast poller; `None` while `[solcast] enabled` is off, in which case
    /// nothing ever leaves the LAN.
    pub fn start_solar(self: &Arc<Self>) -> Option<JoinHandle<()>> {
        self.solar_enabled()
            .then(|| spawn_with(self, BASE_URL.to_owned(), CACHE_GRACE))
    }
}

/// Spawns [`run`] against `base_url` (tests point it at a stub), waiting `cache_grace` after
/// the broker is online before the first decision.
pub fn spawn_with(hub: &Arc<Hub>, base_url: String, cache_grace: Duration) -> JoinHandle<()> {
    tokio::spawn(run(Arc::clone(hub), base_url, cache_grace))
}

/// How one round of calls ended.
enum Round {
    Fetched,
    Failed,
    Unauthorized,
}

/// The poller. Loops until the task is aborted; only a missing key file or a refused key
/// stops the calls.
pub async fn run(hub: Arc<Hub>, base_url: String, cache_grace: Duration) {
    let key = match hub.solar_key_file() {
        None => {
            tracing::error!("solar: no API key file is configured");
            return;
        }
        Some(path) => match read_key_file(&path) {
            Ok(key) if !key.is_empty() => key,
            Ok(_) => {
                tracing::error!(path = %path.display(), "solar: the API key file is empty");
                return;
            }
            Err(e) => {
                tracing::error!(path = %path.display(), error = %e, "solar: cannot read the API key file");
                return;
            }
        },
    };
    let client = match Client::new(base_url, ApiKey::new(key)) {
        Ok(client) => client,
        Err(e) => {
            tracing::error!(error = %e, "solar: cannot build the HTTP client");
            return;
        }
    };
    let mut connection = hub.connection();
    if connection
        .wait_for(|status| matches!(status, ConnectionStatus::Online))
        .await
        .is_err()
    {
        return;
    }
    // Once, from this task only, so it never interleaves with `apply_solar_forecast`.
    hub.publish_solar_configs();
    tokio::time::sleep(cache_grace).await;

    let sites = hub.solar_sites();
    let limit = hub.solar_daily_limit();
    let site_count = u32::try_from(sites.len()).unwrap_or(u32::MAX);
    let mut unauthorized = false;
    let mut warned = false;
    loop {
        let now = Utc::now();
        let day_slots = planned_slots(&hub, now, limit, site_count);
        let decision = if unauthorized {
            Decision::SleepUntil(now + ChronoDuration::seconds(FAR_FUTURE.as_secs() as i64))
        } else {
            decide(
                now,
                &hub.solar_ledger(),
                hub.solar_has_forecast(),
                &day_slots,
                limit,
                site_count,
            )
        };
        let mut wake = match decision {
            Decision::SleepUntil(at) => Some(at),
            Decision::CallNow => None,
        };
        if wake.is_none() {
            match fetch_round(&hub, &client, &sites, &mut warned).await {
                Round::Fetched => warned = false,
                Round::Failed => {}
                Round::Unauthorized => {
                    unauthorized = true;
                    wake = Some(now + ChronoDuration::seconds(FAR_FUTURE.as_secs() as i64));
                }
            }
        }
        let now = Utc::now();
        let next = if unauthorized {
            None
        } else {
            day_slots.iter().filter(|s| **s > now).min().copied()
        };
        hub.set_solar_next_refresh(next.and_then(|t| u64::try_from(t.timestamp()).ok()));
        hub.refresh_solar_states();
        tokio::time::sleep(sleep_for(now, wake)).await;
    }
}

/// Refresh slots of local yesterday, today and tomorrow (a day's slots can straddle UTC
/// midnight).
fn planned_slots(hub: &Hub, now: DateTime<Utc>, limit: u32, sites: u32) -> Vec<DateTime<Utc>> {
    let zone = hub.zone();
    let (lat, lon) = hub.house_point();
    let refreshes = refreshes_per_day(limit, sites);
    let today = now.with_timezone(&zone).date_naive();
    let mut out = Vec::new();
    for offset in -1..=1 {
        let date: NaiveDate = today + ChronoDuration::days(offset);
        let sun = sun_events(lat, lon, date);
        out.extend(slots(sun.sunrise, sun.sunset, date, zone, refreshes));
    }
    out
}

/// Fetches every site in order. All must succeed for the forecast to be replaced: one failure
/// keeps the previous forecast whole.
async fn fetch_round(hub: &Hub, client: &Client, sites: &[String], warned: &mut bool) -> Round {
    let mut results = Vec::with_capacity(sites.len());
    for site in sites {
        hub.record_solar_attempt();
        match client.site_forecast(site).await {
            Ok(periods) => results.push(periods),
            Err(SolcastError::Unauthorized) => {
                tracing::error!(
                    "solar: Solcast rejected the API key; no further calls until restart"
                );
                hub.record_solar_failure();
                return Round::Unauthorized;
            }
            Err(SolcastError::QuotaExceeded) => {
                tracing::warn!(
                    "solar: Solcast daily request limit reached; waiting for the next UTC day"
                );
                hub.record_solar_quota_exceeded();
                hub.record_solar_failure();
                return Round::Failed;
            }
            Err(e) => {
                if !*warned {
                    tracing::warn!(error = %e, "solar: fetching the forecast failed; keeping the last one");
                    *warned = true;
                }
                hub.record_solar_failure();
                return Round::Failed;
            }
        }
    }
    let now = Utc::now();
    let mut forecast = merge(results, now.timestamp());
    let zone = hub.zone();
    let midnight = zone
        .from_local_datetime(
            &now.with_timezone(&zone)
                .date_naive()
                .and_hms_opt(0, 0, 0)
                .expect("midnight is valid"),
        )
        .earliest()
        .map_or(now, |t| t.with_timezone(&Utc));
    forecast.trim(midnight, midnight + ChronoDuration::days(KEEP_DAYS));
    hub.apply_solar_forecast(forecast);
    let calls_today = hub.solar_ledger().calls_on(now);
    tracing::info!(calls_today, "solar: forecast applied");
    Round::Fetched
}

/// Time to sleep: until `wake` or the next :00/:30 wall-clock boundary, whichever is first,
/// and at least [`MIN_SLEEP`].
fn sleep_for(now: DateTime<Utc>, wake: Option<DateTime<Utc>>) -> Duration {
    let boundary = (clock::now() / 1800 + 1) * 1800;
    let to_boundary = Duration::from_secs(boundary.saturating_sub(clock::now()));
    let to_wake = wake.map_or(to_boundary, |at| {
        (at - now).to_std().unwrap_or(Duration::ZERO)
    });
    to_boundary.min(to_wake).max(MIN_SLEEP)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sleep_is_at_least_a_second() {
        let now = Utc::now();
        assert_eq!(sleep_for(now, Some(now)), MIN_SLEEP);
        assert_eq!(
            sleep_for(now, Some(now - ChronoDuration::hours(1))),
            MIN_SLEEP
        );
    }

    #[test]
    fn sleep_never_passes_the_half_hour() {
        let now = Utc::now();
        let far = now + ChronoDuration::hours(10);
        assert!(sleep_for(now, Some(far)) <= Duration::from_secs(1800));
        assert!(sleep_for(now, None) <= Duration::from_secs(1800));
    }
}
