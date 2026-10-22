//! The climate engine's push alerts (spec §9): safety actions, a stale reading and a failing
//! command. Raising and resolving are idempotent, so the passes can call them every time.

use oikos_climate::doc::{Decision, SafetyCause};
use serde_json::json;

use crate::hub::Hub;

/// How long a raised alert stays relevant, in seconds.
const TTL_S: u64 = 43_200;
/// The push source and channel of every climate alert.
const SOURCE: &str = "climate";

/// How long a safety cause must stay absent before its alert resolves, in seconds: a reading
/// hovering at a limit must not raise a push every time it crosses.
const RESOLVE_AFTER_S: u64 = 600;

fn slug(cause: SafetyCause) -> &'static str {
    match cause {
        SafetyCause::Frost => "frost",
        SafetyCause::Overheat => "overheat",
        SafetyCause::Overcool => "overcool",
        SafetyCause::Stale => "stale",
    }
}

fn severity(cause: SafetyCause) -> &'static str {
    match cause {
        SafetyCause::Frost | SafetyCause::Overheat => "critical",
        SafetyCause::Overcool | SafetyCause::Stale => "warning",
    }
}

fn title(cause: SafetyCause) -> &'static str {
    match cause {
        SafetyCause::Frost => "Frost protection",
        SafetyCause::Overheat => "Overheat protection",
        SafetyCause::Overcool => "Overcool protection",
        SafetyCause::Stale => "No fresh temperature reading",
    }
}

const ALL_CAUSES: [SafetyCause; 4] = [
    SafetyCause::Frost,
    SafetyCause::Overheat,
    SafetyCause::Overcool,
    SafetyCause::Stale,
];

fn safety_id(zone: &str, cause: SafetyCause) -> String {
    format!("climate-safety-{zone}-{}", slug(cause))
}

pub(crate) fn command_id(entity: &str) -> String {
    format!("climate-command-{entity}")
}

/// Raises alert `id` unless it is already raised.
pub(crate) fn raise(hub: &Hub, id: &str, title: &str, body: &str, severity: &str) {
    if hub.climate_runtime().active_alerts.contains(id) {
        return;
    }
    let payload = json!({
        "v": 1,
        "id": id,
        "title": title,
        "body": body,
        "channel": "climate",
        "severity": severity,
        "ttl": TTL_S,
    });
    match hub.notify(SOURCE, &payload) {
        Ok(_) => {
            hub.climate_runtime().active_alerts.insert(id.to_owned());
        }
        Err(rejected) => tracing::warn!(%id, reason = %rejected.reason, "climate alert refused"),
    }
}

/// Resolves alert `id` when it is raised.
pub(crate) fn resolve(hub: &Hub, id: &str) {
    if !hub.climate_runtime().active_alerts.remove(id) {
        return;
    }
    let payload = json!({ "v": 1, "id": id, "cancel": true });
    if let Err(rejected) = hub.notify(SOURCE, &payload) {
        tracing::warn!(%id, reason = %rejected.reason, "climate alert cancel refused");
    }
}

/// Raises the alert of `decision`'s safety cause. The zone's other safety alerts resolve once
/// their cause has been absent for ten minutes.
pub(crate) fn sync_safety(
    hub: &Hub,
    zone: &str,
    zone_name: impl Fn() -> String,
    d: &Decision,
    now: u64,
) {
    for cause in ALL_CAUSES {
        let id = safety_id(zone, cause);
        if d.cause == Some(cause) {
            hub.climate_runtime().absent_since.remove(&id);
            let name = title(cause);
            raise(
                hub,
                &id,
                &format!("{name} in {}", zone_name()),
                &d.reason,
                severity(cause),
            );
            continue;
        }
        let due = {
            let mut rt = hub.climate_runtime();
            if !rt.active_alerts.contains(&id) {
                rt.absent_since.remove(&id);
                continue;
            }
            let since = *rt.absent_since.entry(id.clone()).or_insert(now);
            now.saturating_sub(since) >= RESOLVE_AFTER_S
        };
        if due {
            hub.climate_runtime().absent_since.remove(&id);
            resolve(hub, &id);
        }
    }
}
