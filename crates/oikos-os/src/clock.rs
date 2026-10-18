//! Wall-clock time for state timestamps and snapshots.

use std::time::{SystemTime, UNIX_EPOCH};

/// Unix time in seconds; `0` if the clock is before the epoch.
pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}
