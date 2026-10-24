//! What the machine asks the hub to do: ring and silence the sirens, chirp, raise and resolve
//! the push alerts, and warn about a watched sensor that went offline.

use std::sync::Arc;
use std::sync::atomic::Ordering;
use std::time::Duration;

use futures_util::future::join_all;
use oikos_alarm::doc::{ArmMode, MqttSiren, SensorKind, Siren};
use oikos_alarm::machine::Effect;
use oikos_core::command::Publish;
use oikos_core::model::Command;
use serde_json::{Value, json};

use super::{ALARM_ENTITY_ID, lock};
use crate::hub::Hub;

/// The push source of every alarm alert.
const SOURCE: &str = "alarm";

/// Every alert the alarm raises has an id that starts with this; replies are routed by it.
pub(crate) const ALERT_PREFIX: &str = "alarm-";

/// How long an entity siren has to confirm a command before it counts as failed, so one
/// unresponsive siren delays neither the alert nor a disarm.
const ALARM_SIREN_CONFIRM: Duration = Duration::from_secs(2);

/// The id of the offline warning of a watched sensor.
fn offline_id(entity: &str) -> String {
    format!("{ALERT_PREFIX}offline-{entity}")
}

/// A push response: the alert id and the button the user pressed.
#[derive(serde::Deserialize)]
struct Reply {
    id: String,
    action: String,
}

fn mode_word(mode: ArmMode) -> &'static str {
    match mode {
        ArmMode::Away => "away",
        ArmMode::Home => "home",
        ArmMode::Night => "night",
    }
}

impl Hub {
    /// What the machine asked for, in order. A failing siren is collected, never fatal: the
    /// others still ring and the failure goes into the trigger alert of the same batch.
    pub(crate) async fn run_alarm_effects(self: &Arc<Self>, effects: Vec<Effect>) {
        let mut failures: Vec<String> = Vec::new();
        for effect in effects {
            match effect {
                Effect::SirensOn => failures = self.alarm_sirens(true).await,
                Effect::SirensOff => {
                    for failure in self.alarm_sirens(false).await {
                        tracing::warn!(%failure, "alarm siren could not be switched off");
                    }
                }
                Effect::Chirp => self.alarm_chirp(),
                Effect::Alert { cause } => {
                    let body = self.alarm_alert_body(&cause, &failures);
                    self.alarm_raise(&body);
                }
                Effect::ResolveAlerts => self.alarm_resolve_alerts(),
                Effect::OpenAtArm(open) => self.alarm_open_at_arm(&open),
            }
        }
    }

    /// The display name of an entity, or its id when the registry does not know it.
    fn alarm_name(&self, id: &str) -> String {
        self.entity_view(id)
            .map_or_else(|| id.to_owned(), |v| v.name)
    }

    fn alarm_names(&self, ids: &[String]) -> String {
        ids.iter()
            .map(|id| self.alarm_name(id))
            .collect::<Vec<_>>()
            .join(", ")
    }

    /// Switches every siren on or off at once; the failures come back as readable lines.
    async fn alarm_sirens(self: &Arc<Self>, on: bool) -> Vec<String> {
        let sirens = self.alarm_settings().sirens;
        let results = join_all(sirens.iter().map(|siren| async move {
            let (label, outcome) = match siren {
                Siren::Entity { entity, .. } => (
                    self.alarm_name(entity),
                    self.alarm_entity_siren(entity, on).await,
                ),
                Siren::Mqtt { mqtt } => (mqtt.topic.clone(), self.alarm_mqtt_siren(mqtt, on)),
            };
            outcome.err().map(|reason| format!("{label} ({reason})"))
        }))
        .await;
        results.into_iter().flatten().collect()
    }

    /// Commands an entity siren: a switch turns on or off, a generic siren takes `power`.
    async fn alarm_entity_siren(&self, entity: &str, on: bool) -> Result<(), String> {
        let is_switch = self
            .entity_view(entity)
            .is_some_and(|v| v.kind == oikos_core::model::Kind::Switch);
        let cmd = match (is_switch, on) {
            (true, true) => Command::TurnOn {
                brightness: None,
                color: None,
                effect: None,
            },
            (true, false) => Command::TurnOff,
            (false, on) => Command::Control {
                control: "power".to_owned(),
                value: Some(Value::Bool(on)),
            },
        };
        let sent = tokio::time::timeout(ALARM_SIREN_CONFIRM, self.send_command(entity, cmd)).await;
        match sent.unwrap_or_else(|_| {
            Ok(oikos_api::CommandResponse {
                accepted: false,
                reason: Some("no confirmation".to_owned()),
                state: None,
            })
        }) {
            Ok(reply) if reply.accepted => Ok(()),
            Ok(reply) => Err(reply.reason.unwrap_or_else(|| "refused".to_owned())),
            Err(err) => Err(err.to_string()),
        }
    }

    fn alarm_mqtt_siren(&self, siren: &MqttSiren, on: bool) -> Result<(), String> {
        self.alarm_publish(&siren.topic, if on { &siren.on } else { &siren.off })
    }

    fn alarm_publish(&self, topic: &str, payload: &str) -> Result<(), String> {
        self.publish(Publish {
            topic: topic.to_owned(),
            payload: payload.to_owned(),
            retain: false,
        })
        .map_err(|err| err.to_string())
    }

    /// Every siren with a chirp sounds once. An entity siren's chirp is on, a pause, off; it
    /// runs on its own task so the step does not wait for it.
    fn alarm_chirp(self: &Arc<Self>) {
        for siren in self.alarm_settings().sirens {
            match siren {
                Siren::Mqtt { mqtt } => {
                    if let Some(chirp) = &mqtt.chirp
                        && let Err(err) = self.alarm_publish(&mqtt.topic, chirp)
                    {
                        tracing::warn!(topic = %mqtt.topic, %err, "alarm chirp failed");
                    }
                }
                Siren::Entity {
                    entity,
                    chirp_ms: Some(ms),
                } if tokio::runtime::Handle::try_current().is_ok() => {
                    let hub = Arc::clone(self);
                    tokio::spawn(async move {
                        let outcome = hub.alarm_entity_siren(&entity, true).await;
                        tokio::time::sleep(Duration::from_millis(u64::from(ms))).await;
                        let off = hub.alarm_entity_siren(&entity, false).await;
                        if let Some(err) = outcome.err().or(off.err()) {
                            tracing::warn!(%entity, %err, "alarm chirp failed");
                        }
                    });
                }
                Siren::Entity { .. } => {}
            }
        }
    }

    /// The text of the trigger alert: what tripped it, then any siren that failed to ring.
    fn alarm_alert_body(&self, cause: &[String], failures: &[String]) -> String {
        let settings = self.alarm_settings();
        let is_motion = |id: &String| {
            settings
                .sensors
                .iter()
                .any(|r| &r.entity == id && r.kind == SensorKind::Motion)
        };
        let (sos, rest): (Vec<&String>, Vec<&String>) =
            cause.iter().partition(|c| c.starts_with("sos:"));
        let (motion, openings): (Vec<String>, Vec<String>) =
            rest.into_iter().cloned().partition(is_motion);
        let mut lines: Vec<String> = Vec::new();
        if !openings.is_empty() {
            lines.push(format!("{} opened", self.alarm_names(&openings)));
        }
        if !motion.is_empty() {
            lines.push(format!(
                "{} motions ({})",
                motion.len(),
                self.alarm_names(&motion)
            ));
        }
        for remote in sos {
            let remote = remote.trim_start_matches("sos:");
            let name = self
                .entity_view(remote)
                .map_or_else(|| remote.to_owned(), |v| v.device_name.unwrap_or(v.name));
            lines.push(format!("SOS ({name})"));
        }
        lines.extend(failures.iter().map(|f| format!("Siren failed: {f}")));
        lines.join("\n")
    }

    /// Raises the critical trigger alert, with a Disarm button, and remembers its id.
    fn alarm_raise(&self, body: &str) {
        let now = self.alarm_now();
        let n = self.alarm.alert_seq.fetch_add(1, Ordering::SeqCst);
        let id = format!("{ALERT_PREFIX}{now}-{n}");
        let payload = json!({
            "v": 1,
            "id": id,
            "title": "Alarm",
            "body": body,
            "channel": "alarm",
            "severity": "critical",
            "actions": [{"id": "disarm", "label": "Disarm"}],
            "entity_id": ALARM_ENTITY_ID,
        });
        match self.notify(SOURCE, &payload) {
            Ok(_) => lock(&self.alarm.runtime).alert_ids.push(id),
            Err(rejected) => tracing::warn!(reason = %rejected.reason, "alarm alert refused"),
        }
    }

    fn alarm_cancel(&self, id: &str) {
        let payload = json!({"v": 1, "id": id, "cancel": true, "channel": "alarm"});
        if let Err(rejected) = self.notify(SOURCE, &payload) {
            tracing::warn!(reason = %rejected.reason, "alarm alert cancel refused");
        }
    }

    /// Resolves the trigger alerts and the offline warnings still open.
    fn alarm_resolve_alerts(&self) {
        let ids = std::mem::take(&mut lock(&self.alarm.runtime).alert_ids);
        for id in &ids {
            self.alarm_cancel(id);
        }
        self.alarm_cancel_offline(None);
    }

    /// Cancels the offline warning of `entity`, or of every sensor when `None`.
    pub(crate) fn alarm_cancel_offline(&self, entity: Option<&str>) {
        let raised: Vec<String> = {
            let mut set = lock(&self.alarm.offline_raised);
            match entity {
                Some(id) => set.remove(id).then(|| id.to_owned()).into_iter().collect(),
                None => std::mem::take(&mut *set).into_iter().collect(),
            }
        };
        for id in raised {
            self.alarm_cancel(&offline_id(&id));
        }
    }

    /// A quiet note that the alarm armed with sensors open.
    fn alarm_open_at_arm(&self, open: &[String]) {
        let mode = lock(&self.alarm.runtime).mode().map_or("", mode_word);
        let now = self.alarm_now();
        let payload = json!({
            "v": 1,
            "id": format!("{ALERT_PREFIX}open-{now}"),
            "title": "Alarm",
            "body": format!("Armed {mode}. {} open, watched for closing", self.alarm_names(open)),
            "channel": "alarm",
            "severity": "info",
            "silent": true,
            "entity_id": ALARM_ENTITY_ID,
        });
        if let Err(rejected) = self.notify(SOURCE, &payload) {
            tracing::warn!(reason = %rejected.reason, "alarm note refused");
        }
    }

    /// A watched sensor went offline: one warning per outage, while the alarm is on.
    pub(crate) fn alarm_sensor_offline(&self, entity: &str) {
        if !lock(&self.alarm.offline_raised).insert(entity.to_owned()) {
            return;
        }
        let payload = json!({
            "v": 1,
            "id": offline_id(entity),
            "title": "Alarm",
            "body": format!("Alarm sensor offline: {}", self.alarm_name(entity)),
            "channel": "alarm",
            "severity": "warning",
            "entity_id": ALARM_ENTITY_ID,
        });
        if let Err(rejected) = self.notify(SOURCE, &payload) {
            tracing::warn!(reason = %rejected.reason, "alarm offline warning refused");
        }
    }

    /// A push response whose alert id starts with `alarm-`. Only the Disarm button does
    /// anything, and only while the alarm is on and something is armed or ringing.
    pub(crate) fn on_alarm_reply(self: &Arc<Self>, payload: &[u8]) {
        use oikos_alarm::machine::{Event, Source};
        let Ok(reply) = serde_json::from_slice::<Reply>(payload) else {
            return;
        };
        if reply.action != "disarm" || !self.alarm_settings().enabled {
            return;
        }
        let known = lock(&self.alarm.runtime).alert_ids.contains(&reply.id);
        let armed = lock(&self.alarm.runtime).state_name() != "disarmed";
        if !(known || (reply.id.starts_with(ALERT_PREFIX) && armed))
            || tokio::runtime::Handle::try_current().is_err()
        {
            return;
        }
        let hub = Arc::clone(self);
        tokio::spawn(async move {
            let ev = Event::Disarm {
                source: Source::Command,
            };
            hub.alarm_event(ev, hub.alarm_now()).await;
        });
    }
}
