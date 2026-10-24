//! The alarm's settings, published panel entity and read view, driven without a broker.

use std::collections::BTreeSet;
use std::sync::Arc;

use oikos_alarm::doc::{
    AlarmSettings, ArmMode, Entry, Remote, RemoteAction, SensorKind, SensorRule, default_signals,
};
use oikos_api::{ConnectionStatus, WsEvent};
use oikos_core::command::Publish;
use oikos_core::model::{Kind, StateValue};
use oikos_os::config::Config;
use oikos_os::hub::{Hub, HubError};
use serde_json::json;
use tokio::sync::mpsc;

const CONFIG_TOPIC: &str = "oikos/alarm_control_panel/alarm/house/config";

fn hub() -> (Arc<Hub>, mpsc::Receiver<Publish>) {
    let mut cfg = Config::default();
    cfg.topics.discovery_prefixes = vec!["oikos".to_owned()];
    let (tx, rx) = mpsc::channel(64);
    let hub = Hub::new(&cfg, tx);
    hub.set_connection(ConnectionStatus::Online);
    (hub, rx)
}

fn drain(rx: &mut mpsc::Receiver<Publish>) -> Vec<Publish> {
    let mut out = Vec::new();
    while let Ok(p) = rx.try_recv() {
        out.push(p);
    }
    out
}

fn contact(id: &str) -> serde_json::Value {
    json!({
        "name": id, "unique_id": id, "device_class": "door",
        "state_topic": format!("fx/{id}"),
        "payload_on": "open", "payload_off": "closed",
        "device": {"identifiers": [format!("dev_{id}")], "name": id}
    })
}

fn feed(hub: &Hub, component: &str, id: &str, config: serde_json::Value) {
    hub.ingest(
        &format!("oikos/{component}/{id}/{id}/config"),
        config.to_string().as_bytes(),
    );
}

/// Two door contacts, a motion sensor, a siren switch, a remote and a tracker.
fn house(hub: &Hub) {
    feed(hub, "binary_sensor", "front_door", contact("front_door"));
    feed(hub, "binary_sensor", "back_door", contact("back_door"));
    let mut motion = contact("hall_motion");
    motion["device_class"] = json!("motion");
    feed(hub, "binary_sensor", "hall_motion", motion);
    feed(
        hub,
        "switch",
        "siren_plug",
        json!({"name": "Siren", "unique_id": "siren_plug", "state_topic": "fx/siren",
               "command_topic": "fx/siren/set"}),
    );
    feed(
        hub,
        "sensor",
        "hall_temp",
        json!({"name": "Temp", "unique_id": "hall_temp", "state_topic": "fx/temp"}),
    );
    feed(
        hub,
        "device_tracker",
        "phone_a",
        json!({"name": "Phone", "unique_id": "phone_a", "state_topic": "fx/phone_a",
               "payload_home": "home", "payload_not_home": "not_home"}),
    );
    feed(
        hub,
        "device_automation",
        "keyfob",
        json!({"automation_type": "trigger", "type": "action", "subtype": "arm",
               "payload": "arm", "topic": "fx/keyfob/action",
               "device": {"identifiers": ["dev_keyfob"], "name": "Keyfob"}}),
    );
}

fn entity_id(hub: &Hub, fragment: &str) -> String {
    hub.snapshot()
        .entities
        .into_iter()
        .find(|e| e.id.contains(fragment))
        .unwrap_or_else(|| panic!("no entity {fragment}"))
        .id
}

fn enabled_with(sensor: &str) -> AlarmSettings {
    AlarmSettings {
        enabled: true,
        sensors: vec![SensorRule {
            entity: sensor.to_owned(),
            kind: SensorKind::Opening,
            modes: BTreeSet::from([ArmMode::Away]),
            entry: Entry::Instant,
        }],
        ..AlarmSettings::default()
    }
}

#[tokio::test]
async fn disabled_by_default_publishes_nothing() {
    let (hub, mut rx) = hub();
    assert!(!hub.alarm_view().enabled);
    assert_eq!(hub.alarm_view().state, "disarmed");
    assert!(
        !drain(&mut rx)
            .iter()
            .any(|p| p.topic.contains("alarm_control_panel"))
    );
    assert!(
        hub.extra_filters()
            .contains(&"oikos/alarm/settings".to_owned())
    );
    assert!(
        hub.extra_filters()
            .contains(&"oikos/alarm/state".to_owned())
    );
    assert!(
        hub.extra_filters()
            .contains(&"oikos/alarm/panel/set".to_owned())
    );
}

#[tokio::test]
async fn enable_publishes_entity() {
    let (hub, mut rx) = hub();
    house(&hub);
    let mut events = hub.subscribe();
    let sensor = entity_id(&hub, "front_door");
    hub.set_alarm_settings(enabled_with(&sensor)).await.unwrap();

    let published = drain(&mut rx);
    let config = published
        .iter()
        .find(|p| p.topic == CONFIG_TOPIC)
        .expect("config published");
    assert!(config.retain);
    let doc: serde_json::Value = serde_json::from_str(&config.payload).unwrap();
    assert_eq!(
        doc["supported_features"],
        json!(["arm_away", "arm_home", "arm_night"])
    );
    assert_eq!(doc["code_arm_required"], json!(false));
    assert_eq!(doc["unique_id"], json!("oikos_alarm_house"));
    assert!(
        published
            .iter()
            .any(|p| p.topic == "oikos/alarm/settings" && p.retain)
    );
    // The runtime is persisted before the panel state is published (R6).
    let at = |topic: &str| published.iter().position(|p| p.topic == topic).unwrap();
    assert!(at("oikos/alarm/state") < at("oikos/alarm/panel"));
    assert_eq!(
        published
            .iter()
            .find(|p| p.topic == "oikos/alarm/panel")
            .unwrap()
            .payload,
        "disarmed"
    );

    let view = hub.entity_view("oikos_alarm_house").expect("panel entity");
    assert_eq!(view.kind, Kind::AlarmPanel);
    assert_eq!(view.state.value, StateValue::Alarm("disarmed".to_owned()));
    assert!(hub.alarm_view().enabled);

    let mut saw = false;
    while let Ok(event) = events.try_recv() {
        saw |= matches!(event, WsEvent::AlarmChanged);
    }
    assert!(saw, "AlarmChanged was broadcast");
}

#[tokio::test]
async fn disable_removes_entity_keeps_settings() {
    let (hub, mut rx) = hub();
    house(&hub);
    let sensor = entity_id(&hub, "front_door");
    let mut settings = enabled_with(&sensor);
    hub.set_alarm_settings(settings.clone()).await.unwrap();
    assert!(hub.entity_view("oikos_alarm_house").is_some());
    drain(&mut rx);

    settings.enabled = false;
    hub.set_alarm_settings(settings).await.unwrap();
    let published = drain(&mut rx);
    let removal = published.iter().find(|p| p.topic == CONFIG_TOPIC).unwrap();
    assert!(removal.retain && removal.payload.is_empty());
    assert!(hub.entity_view("oikos_alarm_house").is_none());
    assert_eq!(hub.alarm_settings().sensors.len(), 1);
    assert!(!hub.alarm_view().enabled);
}

#[tokio::test]
async fn enable_seeds_runtime_from_live_trackers() {
    let (hub, mut rx) = hub();
    house(&hub);
    hub.ingest("fx/phone_a", b"home");
    let sensor = entity_id(&hub, "front_door");
    let tracker = entity_id(&hub, "phone_a");
    let mut settings = enabled_with(&sensor);
    settings.presence.trackers = vec![tracker.clone()];
    hub.set_alarm_settings(settings).await.unwrap();
    let state = drain(&mut rx)
        .into_iter()
        .find(|p| p.topic == "oikos/alarm/state")
        .unwrap();
    let doc: serde_json::Value = serde_json::from_str(&state.payload).unwrap();
    assert_eq!(doc["trackers"][&tracker], json!("home"));
    assert_eq!(doc["auto_arm_ready"], json!(true));
}

#[tokio::test]
async fn rejects_unknown_or_wrong_kind_entities() {
    let (hub, _rx) = hub();
    house(&hub);
    let unknown = enabled_with("no_such_entity");
    assert!(matches!(
        hub.set_alarm_settings(unknown).await,
        Err(HubError::Rejected(_))
    ));
    let switch = entity_id(&hub, "siren_plug");
    assert!(matches!(
        hub.set_alarm_settings(enabled_with(&switch)).await,
        Err(HubError::Rejected(_))
    ));
    let sensor = entity_id(&hub, "front_door");
    let mut bad_remote = enabled_with(&sensor);
    bad_remote.remotes = vec![Remote {
        entity: entity_id(&hub, "hall_temp"),
        signals: default_signals(),
    }];
    assert!(matches!(
        hub.set_alarm_settings(bad_remote).await,
        Err(HubError::Rejected(_))
    ));
    let mut bad_tracker = enabled_with(&sensor);
    bad_tracker.presence.trackers = vec![sensor.clone()];
    assert!(matches!(
        hub.set_alarm_settings(bad_tracker).await,
        Err(HubError::Rejected(_))
    ));
    assert!(!hub.alarm_view().enabled);
}

#[tokio::test]
async fn accepts_siren_switch_remote_and_tracker() {
    let (hub, _rx) = hub();
    house(&hub);
    let mut settings = enabled_with(&entity_id(&hub, "front_door"));
    settings.sirens = vec![oikos_alarm::doc::Siren::Entity {
        entity: entity_id(&hub, "siren_plug"),
        chirp_ms: None,
    }];
    settings.remotes = vec![Remote {
        entity: entity_id(&hub, "keyfob"),
        signals: [("arm".to_owned(), RemoteAction::Away)].into(),
    }];
    settings.presence.trackers = vec![entity_id(&hub, "phone_a")];
    hub.set_alarm_settings(settings).await.unwrap();
}

#[tokio::test]
async fn invalid_retained_settings_keep_previous() {
    let (hub, _rx) = hub();
    let good = json!({"enabled": false, "siren_s": 30}).to_string();
    hub.ingest_message("oikos/alarm/settings", good.as_bytes(), true);
    assert_eq!(hub.alarm_settings().siren_s, 30);
    assert!(hub.alarm_view().settings_invalid.is_none());
    hub.ingest_message("oikos/alarm/settings", br#"{"siren_s": 5}"#, true);
    assert!(hub.alarm_view().settings_invalid.is_some());
    assert_eq!(hub.alarm_settings().siren_s, 30);
}

#[tokio::test]
async fn view_warns_about_missing_entities() {
    let (hub, _rx) = hub();
    let doc = serde_json::to_string(&enabled_with("ghost_door")).unwrap();
    hub.ingest_message("oikos/alarm/settings", doc.as_bytes(), true);
    let warnings = hub.alarm_view().warnings;
    assert!(
        warnings.iter().any(|w| w.contains("ghost_door")),
        "{warnings:?}"
    );
}

#[tokio::test]
async fn alarm_documents_are_not_devices() {
    let (hub, _rx) = hub();
    let changes = hub.ingest_message("oikos/alarm/panel/set", b"ARM_AWAY", false);
    assert!(changes.is_empty());
    hub.ingest_message("oikos/alarm/state", b"not json", true);
    assert!(hub.snapshot().entities.is_empty());
}
