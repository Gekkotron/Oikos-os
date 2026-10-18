//! Wire types against the recorded house.

use std::path::PathBuf;

use oikos_api::{ConnectionStatus, Snapshot, WsEvent, snapshot};
use oikos_core::registry::{Registry, RegistryConfig};

const AT: u64 = 1_700_000_000;

fn fixtures() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../oikos-core/tests/fixtures/"
    ))
}

fn strings(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_owned()).collect()
}

fn files(sub: &str) -> Vec<(String, Vec<u8>)> {
    let mut out: Vec<(String, Vec<u8>)> = std::fs::read_dir(fixtures().join(sub))
        .expect("fixture dir")
        .map(|entry| {
            let path = entry.expect("dir entry").path();
            let stem = path
                .file_stem()
                .and_then(|s| s.to_str())
                .expect("fixture stem")
                .to_owned();
            (stem, std::fs::read(&path).expect("fixture"))
        })
        .collect();
    out.sort();
    out
}

fn feed(registry: &mut Registry, topic: &str, payload: Vec<u8>) {
    let inbound = registry.route(topic, payload, AT);
    registry.apply(inbound);
}

fn load_house() -> Registry {
    let mut registry = Registry::new(RegistryConfig {
        discovery_prefixes: strings(&["oikos"]),
        ui_prefix: "oikos/ui".to_owned(),
        z2m_base: "zigbee2mqtt".to_owned(),
        reserved: strings(&["ui", "automation"]),
    });
    for (file, topic) in [
        ("bridge_info.json", "zigbee2mqtt/bridge/info"),
        ("bridge_devices.json", "zigbee2mqtt/bridge/devices"),
        ("bridge_state.json", "zigbee2mqtt/bridge/state"),
    ] {
        let payload = std::fs::read(fixtures().join(file)).expect("fixture");
        feed(&mut registry, topic, payload);
    }
    for (stem, payload) in files("discovery") {
        feed(
            &mut registry,
            &format!("oikos/{}/config", stem.replace("__", "/")),
            payload,
        );
    }
    for (stem, payload) in files("ui") {
        feed(&mut registry, &format!("oikos/ui/{stem}"), payload);
    }
    for (stem, payload) in files("states") {
        feed(
            &mut registry,
            &format!("zigbee2mqtt/{}", stem.replace("__", "/")),
            payload,
        );
    }
    registry
}

#[test]
fn snapshot_of_recorded_house_serializes() {
    let registry = load_house();
    let snap = snapshot(&registry, ConnectionStatus::Online, AT);
    assert_eq!(snap.generated_at, AT);
    assert!(
        snap.entities.len() >= 400,
        "entities: {}",
        snap.entities.len()
    );
    assert_eq!(snap.devices.len(), 66);
    let json = serde_json::to_string(&snap).expect("serialize");
    let back: Snapshot = serde_json::from_str(&json).expect("deserialize");
    assert_eq!(back, snap);
    let with_battery = snap.devices.iter().filter(|d| d.battery.is_some()).count();
    assert!(with_battery > 0, "no device has a battery");
    let thermometer = snap
        .devices
        .iter()
        .find(|d| d.friendly_name == "rooms/bedroom2/thermometer")
        .expect("thermometer");
    assert!(thermometer.battery.is_some());
    assert!(thermometer.linkquality.is_some());
}

#[test]
fn ws_event_tag_shape() {
    let event = WsEvent::EntityRemoved { id: "x".to_owned() };
    assert_eq!(
        serde_json::to_string(&event).expect("serialize"),
        r#"{"type":"entity_removed","id":"x"}"#
    );
}

#[test]
fn snapshot_has_a_json_schema() {
    let schema = schemars::schema_for!(Snapshot);
    assert!(schema.as_value().is_object());
    let _ = schemars::schema_for!(WsEvent);
}
