//! The climate engine's push alerts (spec §9): safety actions, a stale reading and a failing
//! command. Raising and resolving are idempotent, so the passes can call them every time.

use oikos_climate::doc::{Decision, Source};
use serde_json::json;

use crate::hub::Hub;

/// How long a raised alert stays relevant, in seconds.
const TTL_S: u64 = 43_200;
/// The push source and channel of every climate alert.
const SOURCE: &str = "climate";

/// Why the safety row of the decision tree acted.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum Cause {
    Frost,
    Overheat,
    Overcool,
    Stale,
}

impl Cause {
    const ALL: [Cause; 4] = [Cause::Frost, Cause::Overheat, Cause::Overcool, Cause::Stale];

    fn slug(self) -> &'static str {
        match self {
            Cause::Frost => "frost",
            Cause::Overheat => "overheat",
            Cause::Overcool => "overcool",
            Cause::Stale => "stale",
        }
    }

    fn severity(self) -> &'static str {
        match self {
            Cause::Frost | Cause::Overheat => "critical",
            Cause::Overcool | Cause::Stale => "warning",
        }
    }

    fn title(self) -> &'static str {
        match self {
            Cause::Frost => "Frost protection",
            Cause::Overheat => "Overheat protection",
            Cause::Overcool => "Overcool protection",
            Cause::Stale => "No fresh temperature reading",
        }
    }
}

/// The safety cause behind `decision`, read from its reason (the decision tree's wording).
pub(crate) fn cause_of(decision: &Decision) -> Option<Cause> {
    if decision.source != Source::Safety {
        return None;
    }
    let reason = decision.reason.as_str();
    Some(if reason.starts_with("Frost") {
        Cause::Frost
    } else if reason.starts_with("Overheat") {
        Cause::Overheat
    } else if reason.starts_with("Overcool") {
        Cause::Overcool
    } else {
        Cause::Stale
    })
}

fn safety_id(zone: &str, cause: Cause) -> String {
    format!("climate-safety-{zone}-{}", cause.slug())
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

/// Raises the alert of `decision`'s safety cause and resolves the zone's other safety alerts.
pub(crate) fn sync_safety(hub: &Hub, zone: &str, zone_name: impl Fn() -> String, d: &Decision) {
    let current = cause_of(d);
    for cause in Cause::ALL {
        let id = safety_id(zone, cause);
        if current == Some(cause) {
            let title = format!("{} in {}", cause.title(), zone_name());
            raise(hub, &id, &title, &d.reason, cause.severity());
        } else {
            resolve(hub, &id);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::BTreeMap;

    fn decision(source: Source, reason: &str) -> Decision {
        Decision {
            zone: "z".into(),
            mode: None,
            setpoint: None,
            source,
            reason: reason.into(),
            reading: None,
            reading_age_s: None,
            outdoor_mean: None,
            home: true,
            applied: false,
            at: 0,
            entities: BTreeMap::new(),
        }
    }

    #[test]
    fn causes_follow_the_decision_reasons() {
        for (reason, want) in [
            ("Frost protection: 5.0 °C < 7.0 °C", Cause::Frost),
            ("Overheat protection: 31.0 °C > 30.0 °C", Cause::Overheat),
            ("Overcool protection: 15.0 °C < 16.0 °C", Cause::Overcool),
            ("Reading is stale (last 18.0 °C): holding", Cause::Stale),
            ("No temperature reading: not acting", Cause::Stale),
        ] {
            assert_eq!(cause_of(&decision(Source::Safety, reason)), Some(want));
        }
        assert_eq!(cause_of(&decision(Source::Schedule, "Frost")), None);
    }
}
