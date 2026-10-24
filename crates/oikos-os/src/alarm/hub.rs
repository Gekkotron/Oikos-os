//! The alarm side of the hub: the read view, the settings writer and the retained documents.

use std::collections::{BTreeMap, BTreeSet};
use std::sync::Arc;
use std::sync::atomic::Ordering;

use oikos_alarm::doc::{AlarmSettings, Siren};
use oikos_alarm::machine::{Presence, Runtime, initial_ready};
use oikos_alarm::topics::{self, AlarmTopic};
use oikos_alarm::view::AlarmView;
use oikos_api::WsEvent;
use oikos_core::command::Publish;
use oikos_core::model::{Entity, Kind, StateValue};
use oikos_core::types::{Placement, type_def};

use super::{ALARM_ENTITY_ID, entity, lock};
use crate::climate::store::{Applied, apply_validated};
use crate::clock;
use crate::hub::{Hub, HubError};

impl Hub {
    /// The settings in force (the default, switched off, until a document arrives).
    pub fn alarm_settings(&self) -> AlarmSettings {
        lock(&self.alarm.settings).clone()
    }

    /// The whole alarm as one document: settings, state and run-time warnings.
    pub fn alarm_view(&self) -> AlarmView {
        let settings = self.alarm_settings();
        let runtime = lock(&self.alarm.runtime).clone();
        let warnings = self.alarm_warnings(&settings);
        AlarmView {
            enabled: settings.enabled,
            entity_id: ALARM_ENTITY_ID.to_owned(),
            state: runtime.state_name().to_owned(),
            mode: runtime.mode(),
            source: runtime.source,
            since: runtime.since,
            deadline: runtime.deadline(),
            cause: runtime.cause.clone(),
            open_at_arm: runtime.open_at_arm.iter().cloned().collect(),
            settings,
            settings_invalid: lock(&self.alarm.settings_invalid).clone(),
            warnings,
        }
    }

    /// Replaces the settings document: validated, checked against the house, published retained
    /// and applied here. Switching the alarm on publishes the panel entity (starting from
    /// `disarmed`); switching it off removes the entity and resets the runtime.
    pub async fn set_alarm_settings(
        self: &Arc<Self>,
        settings: AlarmSettings,
    ) -> Result<(), HubError> {
        settings.validate().map_err(HubError::Rejected)?;
        self.check_alarm_entities(&settings)
            .map_err(HubError::Rejected)?;
        let was_enabled = self.alarm_settings().enabled;
        let payload = serde_json::to_string(&settings)
            .map_err(|err| HubError::Rejected(format!("serialize: {err}")))?;
        let topic = topics::settings_topic(&self.alarm.prefix);
        self.publish(Publish {
            topic: topic.clone(),
            payload: payload.clone(),
            retain: true,
        })?;
        self.ingest_alarm(&topic, payload.as_bytes(), true);
        let enabled = settings.enabled;
        // The first panel state the house sees is `disarmed`, so the runtime is reset first.
        if enabled && !was_enabled {
            self.alarm_reset_runtime(&settings)?;
        }
        self.sync_alarm_entity()?;
        if was_enabled && !enabled {
            self.alarm_disable()?;
        }
        self.broadcast(WsEvent::AlarmChanged);
        Ok(())
    }

    /// Each configured entity must exist and be of the right shape for its role.
    pub(crate) fn check_alarm_entities(&self, s: &AlarmSettings) -> Result<(), String> {
        let reg = self.read();
        let find = |role: &str, id: &str| {
            reg.entity(id)
                .ok_or_else(|| format!("{role} `{id}` is not a known entity"))
        };
        for rule in &s.sensors {
            let e = find("sensor", &rule.entity)?;
            if e.kind != Kind::BinarySensor {
                return Err(format!("sensor `{}` is not a binary sensor", rule.entity));
            }
        }
        for siren in &s.sirens {
            if let Siren::Entity { entity: id, .. } = siren {
                let e = find("siren", id)?;
                let has_power = e
                    .capabilities
                    .generic
                    .as_ref()
                    .is_some_and(|g| g.controls.iter().any(|c| c == "power"));
                if e.kind != Kind::Switch && !(e.kind == Kind::Generic && has_power) {
                    return Err(format!(
                        "siren `{id}` is neither a switch nor a siren with a power control"
                    ));
                }
            }
        }
        for remote in &s.remotes {
            let e = find("remote", &remote.entity)?;
            if !type_def(&e.component).is_some_and(|d| d.placement == Placement::Signal) {
                return Err(format!(
                    "remote `{}` does not emit signals (a device trigger or an event entity)",
                    remote.entity
                ));
            }
        }
        for id in &s.presence.trackers {
            let e = find("tracker", id)?;
            if !is_tracker(e) {
                return Err(format!("tracker `{id}` does not report home / not_home"));
            }
        }
        Ok(())
    }

    /// Publishes the panel entity while the alarm is on (config, then state and attributes);
    /// while it is off, empties the retained config so the registry drops the entity.
    pub(crate) fn sync_alarm_entity(&self) -> Result<(), HubError> {
        let topic = entity::config_topic(&self.discovery_prefix);
        if self.alarm_settings().enabled {
            let config = entity::build_config(&self.alarm.prefix).to_string();
            self.publish(Publish {
                topic: topic.clone(),
                payload: config.clone(),
                retain: true,
            })?;
            self.ingest(&topic, config.as_bytes());
            let runtime = lock(&self.alarm.runtime).clone();
            self.publish_alarm_panel(&runtime)
        } else {
            self.publish(Publish {
                topic: topic.clone(),
                payload: String::new(),
                retain: true,
            })?;
            self.ingest(&topic, b"");
            for stale in [
                topics::panel_state_topic(&self.alarm.prefix),
                topics::panel_attributes_topic(&self.alarm.prefix),
            ] {
                self.publish(Publish {
                    topic: stale,
                    payload: String::new(),
                    retain: true,
                })?;
            }
            Ok(())
        }
    }

    /// Publishes the panel's state and attributes retained, and applies them here.
    pub(crate) fn publish_alarm_panel(&self, rt: &Runtime) -> Result<(), HubError> {
        let state = topics::panel_state_topic(&self.alarm.prefix);
        let attributes = topics::panel_attributes_topic(&self.alarm.prefix);
        let attrs = serde_json::json!({
            "source": rt.source,
            "since": rt.since,
            "deadline": rt.deadline(),
            "cause": rt.cause,
            "open_at_arm": rt.open_at_arm,
        })
        .to_string();
        for (topic, payload) in [(state, rt.state_name().to_owned()), (attributes, attrs)] {
            self.publish(Publish {
                topic: topic.clone(),
                payload: payload.clone(),
                retain: true,
            })?;
            self.ingest(&topic, payload.as_bytes());
        }
        Ok(())
    }

    /// Stores `rt` and persists it retained at `<p>/state`.
    pub(crate) fn persist_alarm_runtime(&self, rt: &Runtime) -> Result<(), HubError> {
        let payload = serde_json::to_string(rt)
            .map_err(|err| HubError::Rejected(format!("serialize: {err}")))?;
        self.publish(Publish {
            topic: topics::runtime_topic(&self.alarm.prefix),
            payload,
            retain: true,
        })?;
        *lock(&self.alarm.runtime) = rt.clone();
        self.alarm.runtime_loaded.store(true, Ordering::SeqCst);
        Ok(())
    }

    /// A fresh runtime for an alarm just switched on: disarmed, with the live presence of each
    /// configured tracker, and presence allowed to arm only if someone is home now.
    fn alarm_reset_runtime(&self, settings: &AlarmSettings) -> Result<(), HubError> {
        let trackers = self.alarm_live_presence(&settings.presence.trackers);
        let rt = Runtime {
            since: clock::now(),
            auto_arm_ready: initial_ready(&trackers),
            trackers,
            ..Runtime::default()
        };
        self.persist_alarm_runtime(&rt)
    }

    /// Switching the alarm off: the runtime goes back to its default. (Sirens and alerts are
    /// handled by the engine's effects.)
    pub(crate) fn alarm_disable(&self) -> Result<(), HubError> {
        self.persist_alarm_runtime(&Runtime {
            since: clock::now(),
            ..Runtime::default()
        })
    }

    /// What each tracker reports now: `home`, `not_home`, or unknown.
    fn alarm_live_presence(&self, trackers: &[String]) -> BTreeMap<String, Presence> {
        let reg = self.read();
        trackers
            .iter()
            .map(|id| {
                let seen = match reg.state(id).map(|s| &s.value) {
                    Some(StateValue::Generic(g)) => match g.fields.get("state") {
                        Some(oikos_core::model::GenericValue::Text(t)) => presence_of(t),
                        _ => Presence::Unknown,
                    },
                    Some(StateValue::Text(t)) => presence_of(t),
                    _ => Presence::Unknown,
                };
                (id.clone(), seen)
            })
            .collect()
    }

    /// The configured entity ids the house does not have (§7.2, at run time).
    fn alarm_warnings(&self, s: &AlarmSettings) -> Vec<String> {
        let mut ids: BTreeSet<&str> = BTreeSet::new();
        ids.extend(s.sensors.iter().map(|r| r.entity.as_str()));
        ids.extend(s.sirens.iter().filter_map(|siren| match siren {
            Siren::Entity { entity, .. } => Some(entity.as_str()),
            Siren::Mqtt { .. } => None,
        }));
        ids.extend(s.remotes.iter().map(|r| r.entity.as_str()));
        ids.extend(s.presence.trackers.iter().map(String::as_str));
        let reg = self.read();
        ids.into_iter()
            .filter(|id| reg.entity(id).is_none())
            .map(|id| format!("`{id}` is not a known entity"))
            .collect()
    }

    /// Feeds a message to the alarm when `topic` is one of its documents; false when it is not,
    /// so the message goes on to the registry.
    pub(crate) fn ingest_alarm(&self, topic: &str, payload: &[u8], _retain: bool) -> bool {
        let Some(parsed) = topics::parse(&self.alarm.prefix, topic) else {
            return false;
        };
        let changed = match parsed {
            AlarmTopic::Settings => {
                let applied = {
                    let mut settings = lock(&self.alarm.settings);
                    let mut invalid = lock(&self.alarm.settings_invalid);
                    apply_validated(
                        &mut *settings,
                        &mut invalid,
                        payload,
                        AlarmSettings::validate,
                    )
                };
                match applied {
                    Applied::Changed => true,
                    Applied::Unchanged => false,
                    Applied::Invalid(reason) => {
                        tracing::warn!(%topic, %reason, "alarm settings refused, keeping the previous ones");
                        true
                    }
                }
            }
            AlarmTopic::Runtime => match serde_json::from_slice::<Runtime>(payload) {
                Ok(rt) => {
                    self.alarm.runtime_loaded.store(true, Ordering::SeqCst);
                    let mut current = lock(&self.alarm.runtime);
                    let changed = *current != rt;
                    *current = rt;
                    changed
                }
                Err(err) => {
                    if !payload.is_empty() {
                        tracing::warn!(%topic, %err, "alarm runtime document ignored");
                    }
                    false
                }
            },
            // The engine acts on a non-retained command; a retained one is a replay, never a press.
            AlarmTopic::PanelCommand => false,
        };
        if changed {
            self.broadcast(WsEvent::AlarmChanged);
        }
        true
    }
}

fn presence_of(value: &str) -> Presence {
    match value {
        "home" => Presence::Home,
        "not_home" => Presence::Away,
        _ => Presence::Unknown,
    }
}

/// A presence tracker: a generic choice state that offers `home` and `not_home`.
fn is_tracker(e: &Entity) -> bool {
    e.capabilities
        .generic
        .as_ref()
        .and_then(|g| g.options.get("state"))
        .is_some_and(|o| o.iter().any(|v| v == "home") && o.iter().any(|v| v == "not_home"))
}
