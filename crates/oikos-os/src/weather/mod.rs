//! Outdoor weather from Open-Meteo.

pub mod entities;
pub mod hub;
pub mod open_meteo;

/// Whether the last success is older than twice the refresh period.
pub fn is_stale(fetched_at: u64, now: u64, refresh_minutes: u64) -> bool {
    now.saturating_sub(fetched_at) > 2 * refresh_minutes * 60
}

/// Whether the last success is older than an hour: the entities go offline.
pub fn is_expired(fetched_at: u64, now: u64) -> bool {
    now.saturating_sub(fetched_at) > 3600
}
