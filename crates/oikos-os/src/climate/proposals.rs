//! Proposals and their approval (spec §6.4, §8.3): publishing a pending proposal with its push,
//! approving or rejecting it from the web, MCP or a push button, and the override writers.

use std::sync::Arc;

use oikos_climate::doc::{
    Override, OverrideOrigin, Proposal, ProposalKind, Rejection, Rules, ZoneState,
};
use oikos_climate::topics::{self, ZoneDoc};
use oikos_climate::view::LearnStatus;
use serde::Deserialize;
use serde_json::json;

use super::learning;
use crate::clock;
use crate::hub::{Hub, HubError};

/// The push alert of a proposal is `<ALERT_PREFIX><proposal id>`.
const ALERT_PREFIX: &str = "climate-proposal-";
/// A zone gets at most one proposal push in this many seconds.
const PUSH_EVERY_S: u64 = 24 * 3600;
/// How long a rejected proposal is remembered, in seconds.
const REJECT_FOR_S: u64 = 7 * 24 * 3600;
const SOURCE: &str = "climate";

fn alert_id(proposal_id: &str) -> String {
    format!("{ALERT_PREFIX}{proposal_id}")
}

/// Resolves the push of a proposal. Sent whether or not this run raised it: the alert may
/// have been raised before a restart, or the push history may not have loaded yet.
fn cancel_alert(hub: &Hub, proposal_id: &str) {
    let payload = json!({ "v": 1, "id": alert_id(proposal_id), "cancel": true });
    if let Err(rejected) = hub.notify(SOURCE, &payload) {
        tracing::warn!(reason = %rejected.reason, "climate proposal alert cancel refused");
    }
}

/// Publishes `proposal` retained as the zone's one pending proposal, and tells the user unless
/// they were told less than a day ago. A pending proposal it replaces has its alert resolved.
pub(crate) fn publish(hub: &Hub, zone: &str, proposal: Proposal, now: u64) {
    if !hub.climate_enabled() {
        return;
    }
    let replaced = hub
        .climate_store()
        .zones
        .get(zone)
        .and_then(|docs| docs.proposal.as_ref().map(|old| old.id.clone()))
        .filter(|old| *old != proposal.id);
    hub.write_zone_doc(zone, ZoneDoc::Proposal, Some(&proposal));
    if let Some(old) = replaced {
        cancel_alert(hub, &old);
    }
    let due = hub
        .climate_runtime()
        .last_proposal_push
        .get(zone)
        .is_none_or(|at| now.saturating_sub(*at) >= PUSH_EVERY_S);
    if !due {
        return;
    }
    let name = hub.zone_display_name(zone);
    let title = match proposal.kind {
        ProposalKind::Initial => format!("Climate: new rules for {name}"),
        ProposalKind::Change => format!("Climate: rule changes for {name}"),
    };
    let mut lines: Vec<&str> = proposal
        .changes
        .iter()
        .take(2)
        .map(String::as_str)
        .collect();
    lines.push("Review on the Climate screen.");
    let payload = json!({
        "v": 1,
        "id": alert_id(&proposal.id),
        "title": title,
        "body": lines.join("\n"),
        "channel": "climate",
        "severity": "info",
        "actions": [
            { "id": "approve", "label": "Approve" },
            { "id": "reject", "label": "Reject" },
        ],
    });
    match hub.notify(SOURCE, &payload) {
        Ok(_) => {
            hub.climate_runtime()
                .last_proposal_push
                .insert(zone.to_owned(), now);
        }
        Err(rejected) => {
            tracing::warn!(reason = %rejected.reason, "climate proposal alert refused");
        }
    }
}

/// A push response: the alert id and the button the user pressed.
#[derive(Deserialize)]
struct Reply {
    id: String,
    action: String,
}

impl Hub {
    /// Learns `zone` now and returns where the learner stands.
    pub async fn learn_climate_zone(self: &Arc<Self>, zone: &str) -> Result<LearnStatus, HubError> {
        self.require_climate()?;
        self.require_zone(zone)?;
        Ok(learning::learn_zone(self, zone, clock::now()).await)
    }

    /// Makes the pending proposal of `zone` its rules: `edited` (validated) when given, else the
    /// proposal's. A pending zone becomes active, and the zone is evaluated at once.
    pub async fn approve_climate_proposal(
        self: &Arc<Self>,
        zone: &str,
        edited: Option<Rules>,
    ) -> Result<(), HubError> {
        let climate = self.require_climate()?;
        let now = clock::now();
        {
            let _pass = climate.pass.lock().await;
            let proposal = self.pending_proposal(zone)?;
            let rules = edited.unwrap_or_else(|| proposal.rules.clone());
            rules.validate().map_err(HubError::Rejected)?;
            let mut settings = self.zone_settings(zone);
            settings.rules = rules;
            if settings.state == ZoneState::Pending {
                settings.state = ZoneState::Active;
            }
            settings.approved_at = Some(now);
            settings.approved_proposal = Some(proposal.id.clone());
            settings.validate().map_err(HubError::Rejected)?;
            self.publish_zone_doc(zone, ZoneDoc::Settings, Some(&settings))?;
            self.publish_zone_doc(zone, ZoneDoc::Proposal, None::<&Proposal>)?;
            cancel_alert(self, &proposal.id);
            self.climate_runtime().learn_status.remove(zone);
        }
        self.evaluate_climate_zone(zone, now).await;
        Ok(())
    }

    /// Drops the pending proposal of `zone` and remembers its rules for a week, so the same
    /// proposal is not made again.
    pub async fn reject_climate_proposal(self: &Arc<Self>, zone: &str) -> Result<(), HubError> {
        let climate = self.require_climate()?;
        let now = clock::now();
        let _pass = climate.pass.lock().await;
        let proposal = self.pending_proposal(zone)?;
        let mut settings = self.zone_settings(zone);
        settings.rejected.retain(|r| r.until > now);
        settings.rejected.push(Rejection {
            fingerprint: proposal.fingerprint.clone(),
            until: now + REJECT_FOR_S,
        });
        self.publish_zone_doc(zone, ZoneDoc::Settings, Some(&settings))?;
        self.publish_zone_doc(zone, ZoneDoc::Proposal, None::<&Proposal>)?;
        cancel_alert(self, &proposal.id);
        self.climate_runtime()
            .learn_status
            .insert(zone.to_owned(), LearnStatus::Rejected);
        Ok(())
    }

    /// Adds `ov` to the zone's overrides, replacing the one already held for the same device
    /// (or for the whole zone).
    pub async fn set_climate_override(
        self: &Arc<Self>,
        zone: &str,
        ov: Override,
    ) -> Result<(), HubError> {
        self.require_climate()?;
        self.require_zone(zone)?;
        let now = clock::now();
        if ov.until <= now {
            return Err(HubError::Rejected("the override already ended".to_owned()));
        }
        if let Some(entity) = &ov.entity {
            let settings = self.zone_settings(zone);
            if !super::inputs::zone_entity_ids(self, zone, &settings).contains(entity) {
                return Err(HubError::Rejected(format!(
                    "`{entity}` is not a climate device of zone `{zone}`"
                )));
            }
        }
        if let Some(oikos_climate::doc::Setpoint::Temp(t)) = ov.setpoint
            && !(0.0..=40.0).contains(&t)
        {
            return Err(HubError::Rejected(format!(
                "setpoint {t} is outside 0..=40"
            )));
        }
        let ov = Override {
            origin: OverrideOrigin::User,
            ..ov
        };
        self.update_overrides(zone, |list| {
            list.retain(|o| o.until > now && o.entity != ov.entity);
            list.push(ov);
        });
        Ok(())
    }

    /// Removes every override of the zone, so the schedule applies again.
    pub async fn clear_climate_override(self: &Arc<Self>, zone: &str) -> Result<(), HubError> {
        self.require_climate()?;
        self.require_zone(zone)?;
        self.update_overrides(zone, Vec::clear);
        Ok(())
    }

    /// A push response on `<push prefix>/responses`. Only a button of a climate proposal that is
    /// still pending does anything; a stale or repeated one is ignored.
    pub(crate) fn on_climate_reply(self: &Arc<Self>, payload: &[u8]) {
        let Ok(reply) = serde_json::from_slice::<Reply>(payload) else {
            return;
        };
        let Some(proposal_id) = reply.id.strip_prefix(ALERT_PREFIX) else {
            return;
        };
        let zone = self
            .climate_store()
            .zones
            .iter()
            .find(|(_, docs)| docs.proposal.as_ref().is_some_and(|p| p.id == proposal_id))
            .map(|(zone, _)| zone.clone());
        let Some(zone) = zone else {
            tracing::debug!(id = %reply.id, "climate reply for a proposal that is not pending");
            return;
        };
        let approve = match reply.action.as_str() {
            "approve" => true,
            "reject" => false,
            _ => return,
        };
        if tokio::runtime::Handle::try_current().is_err() {
            return;
        }
        let hub = Arc::clone(self);
        tokio::spawn(async move {
            let done = if approve {
                hub.approve_climate_proposal(&zone, None).await
            } else {
                hub.reject_climate_proposal(&zone).await
            };
            if let Err(err) = done {
                tracing::info!(%zone, error = %err, "climate proposal reply not applied");
            }
        });
    }

    fn pending_proposal(&self, zone: &str) -> Result<Proposal, HubError> {
        self.climate_store()
            .zones
            .get(zone)
            .and_then(|docs| docs.proposal.clone())
            .ok_or_else(|| HubError::Rejected("no pending proposal".to_owned()))
    }

    /// `zone` is a zone the engine knows: an area with climate devices or one with documents.
    fn require_zone(&self, zone: &str) -> Result<(), HubError> {
        if self.climate_zone_ids().iter().any(|z| z == zone) {
            Ok(())
        } else {
            Err(HubError::UnknownArea(zone.to_owned()))
        }
    }

    /// Publishes a zone document retained, failing when the broker link is down: an approval
    /// must not look done while nothing was sent.
    fn publish_zone_doc<T: serde::Serialize>(
        &self,
        zone: &str,
        doc: ZoneDoc,
        value: Option<&T>,
    ) -> Result<(), HubError> {
        let prefix = self.require_climate()?.prefix.clone();
        self.publish_climate(topics::zone_topic(&prefix, zone, doc), value)
    }
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use oikos_climate::doc::{Window, ZoneSettings};
    use oikos_core::command::Publish;
    use tokio::sync::mpsc;

    use super::*;
    use crate::config::Config;

    fn hub() -> (Arc<Hub>, mpsc::Receiver<Publish>) {
        let mut cfg = Config::default();
        cfg.climate.enabled = true;
        let (tx, rx) = mpsc::channel(64);
        (Hub::new(&cfg, tx), rx)
    }

    fn proposal(id: &str, kind: ProposalKind, changes: &[&str]) -> Proposal {
        Proposal {
            id: id.to_owned(),
            zone: "living".to_owned(),
            kind,
            created: 0,
            window: Window {
                from: 0,
                to: 1,
                days: 28,
            },
            rules: Rules::default(),
            changes: changes.iter().map(|c| (*c).to_owned()).collect(),
            fingerprint: id.to_owned(),
        }
    }

    /// Lets the push service accept alerts and returns the alerts as the hub sees them after
    /// the broker echoes the history back.
    async fn settle(hub: &Hub) {
        hub.on_connected();
        hub.on_suback();
        tokio::time::sleep(Duration::from_secs(5)).await;
    }

    fn echo(hub: &Hub, rx: &mut mpsc::Receiver<Publish>) {
        while let Ok(p) = rx.try_recv() {
            if p.topic == "oikos/push/history" {
                hub.ingest_message(&p.topic, p.payload.as_bytes(), false);
            }
        }
    }

    fn alerts_of(hub: &Hub) -> Vec<oikos_api::AlertView> {
        hub.alerts()
            .into_iter()
            .filter(|a| a.id.starts_with(ALERT_PREFIX))
            .collect()
    }

    #[tokio::test(start_paused = true)]
    async fn proposal_push_has_buttons_and_daily_limit() {
        let (hub, mut rx) = hub();
        settle(&hub).await;
        let t = 1_000_000;

        let first = proposal(
            "p-1",
            ProposalKind::Initial,
            &["Base: 17.0 °C", "New: a", "New: b"],
        );
        publish(&hub, "living", first.clone(), t);
        echo(&hub, &mut rx);
        let alerts = alerts_of(&hub);
        assert_eq!(alerts.len(), 1, "{alerts:?}");
        let alert = &alerts[0];
        assert_eq!(alert.id, "climate-proposal-p-1");
        assert_eq!(alert.title, "Climate: new rules for living");
        assert_eq!(
            alert.body, "Base: 17.0 °C\nNew: a\nReview on the Climate screen.",
            "the first two changes, then the pointer to the screen"
        );
        let buttons: Vec<(&str, &str)> = alert
            .actions
            .iter()
            .map(|a| (a.id.as_str(), a.label.as_str()))
            .collect();
        assert_eq!(buttons, [("approve", "Approve"), ("reject", "Reject")]);
        assert_eq!(serde_json::to_value(alert.severity).unwrap(), "info");
        assert!(!alert.resolved);

        // The same proposal again, and then a newer one, within a day: the document is
        // replaced, nobody is pinged, and the old alert is resolved by the replacement.
        publish(&hub, "living", first, t + 600);
        let second = proposal(
            "p-2",
            ProposalKind::Change,
            &["Changed: weekdays start 07:00 → 07:30"],
        );
        publish(&hub, "living", second.clone(), t + 3_600);
        echo(&hub, &mut rx);
        let alerts = alerts_of(&hub);
        assert_eq!(alerts.len(), 1, "one push per zone per day: {alerts:?}");
        assert!(
            alerts[0].resolved,
            "the replaced proposal's alert is resolved"
        );
        assert_eq!(
            hub.climate_store().zones["living"]
                .proposal
                .as_ref()
                .map(|p| p.id.as_str()),
            Some("p-2")
        );

        // A day after the first push, a new proposal pings again.
        let third = proposal(
            "p-3",
            ProposalKind::Change,
            &["Changed: weekdays end 22:00 → 22:30"],
        );
        publish(&hub, "living", third, t + PUSH_EVERY_S);
        echo(&hub, &mut rx);
        let alerts = alerts_of(&hub);
        let fresh = alerts
            .iter()
            .find(|a| a.id == "climate-proposal-p-3")
            .unwrap();
        assert_eq!(fresh.title, "Climate: rule changes for living");
        assert!(!fresh.resolved);
    }

    #[tokio::test]
    async fn learn_status_from_the_run_reaches_the_view() {
        let (hub, _rx) = hub();
        let (history, _in) = crate::history::History::detached(8, chrono_tz::UTC);
        hub.set_history(history);
        hub.ingest_message(
            "oikos/climate/zone/attic/settings",
            serde_json::to_string(&ZoneSettings {
                state: oikos_climate::doc::ZoneState::Active,
                ..ZoneSettings::default()
            })
            .unwrap()
            .as_bytes(),
            true,
        );
        let learn = |hub: &Hub| hub.climate_view().unwrap().zones[0].learn.clone();
        assert_eq!(learn(&hub), LearnStatus::UpToDate);
        for status in [
            LearnStatus::NotEnough {
                days: 6,
                needed: 10,
            },
            LearnStatus::Rejected,
        ] {
            hub.climate_runtime()
                .learn_status
                .insert("attic".to_owned(), status.clone());
            assert_eq!(learn(&hub), status);
        }
        // A pending proposal wins over whatever the last run said.
        hub.ingest_message(
            "oikos/climate/zone/attic/proposal",
            serde_json::to_string(&proposal("p-1", ProposalKind::Initial, &[]))
                .unwrap()
                .as_bytes(),
            true,
        );
        assert_eq!(learn(&hub), LearnStatus::Proposed);
    }
}
