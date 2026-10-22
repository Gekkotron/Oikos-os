//! The climate switch, its retained documents and the read view, driven without a broker.

use std::path::PathBuf;
use std::sync::Arc;

use oikos_api::{ConnectionStatus, WsEvent};
use oikos_climate::doc::{Block, Day, HhMm, ModeRules, Rules, Setpoint, ZoneSettings, ZoneState};
use oikos_core::command::Publish;
use oikos_os::config::Config;
use oikos_os::hub::{Hub, HubError};
use tokio::sync::mpsc;

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(
        PathBuf::from(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../oikos-core/tests/fixtures/discovery/"
        ))
        .join(name),
    )
    .expect("fixture")
}

fn hub(enabled: bool) -> (Arc<Hub>, mpsc::Receiver<Publish>) {
    let mut cfg = Config::default();
    cfg.topics.discovery_prefixes = vec!["oikos".to_owned()];
    cfg.climate.enabled = enabled;
    let (tx, rx) = mpsc::channel(64);
    let hub = Hub::new(&cfg, tx);
    hub.set_connection(ConnectionStatus::Online);
    (hub, rx)
}

/// The recorded Daikin `n` (0..=3), placed in `area` by override; returns its entity id.
async fn feed_daikin(hub: &Hub, n: u8, area: &str) -> String {
    let name = format!("climate__daikin_192_168_1_15{n}__climate.json");
    let topic = format!("oikos/climate/daikin_192_168_1_15{n}/climate/config");
    hub.ingest(&topic, &fixture(&name));
    place(hub, &format!("daikin_192_168_1_15{n}"), area).await
}

/// Puts the entity whose id starts with `prefix` in `area`.
async fn place(hub: &Hub, prefix: &str, area: &str) -> String {
    let id = hub
        .snapshot()
        .entities
        .into_iter()
        .find(|e| e.id.starts_with(prefix))
        .unwrap_or_else(|| panic!("no entity {prefix}"))
        .id;
    let mut patch = serde_json::Map::new();
    patch.insert("area".to_owned(), serde_json::Value::from(area));
    hub.set_override(&id, &patch).await.expect("area override");
    id
}

fn drain(rx: &mut mpsc::Receiver<Publish>) -> Vec<Publish> {
    let mut out = Vec::new();
    while let Ok(p) = rx.try_recv() {
        out.push(p);
    }
    out
}

fn active() -> ZoneSettings {
    ZoneSettings {
        state: ZoneState::Active,
        ..ZoneSettings::default()
    }
}

#[tokio::test]
async fn disabled_has_no_filters_and_refuses() {
    let (hub, _rx) = hub(false);
    assert!(!hub.extra_filters().iter().any(|f| f.contains("climate")));
    assert!(!hub.climate_enabled());
    assert!(matches!(hub.climate_view(), Err(HubError::Publish(m)) if m == "climate disabled"));
    assert!(matches!(
        hub.set_climate_zone("living", active()),
        Err(HubError::Publish(m)) if m == "climate disabled"
    ));
    // A climate-looking topic is just an unknown topic while the engine is off.
    hub.ingest_message("oikos/climate/house", b"{}", true);
}

#[tokio::test]
async fn enabled_subscribes_house_and_zones_only() {
    let (hub, _rx) = hub(true);
    let f = hub.extra_filters();
    assert!(
        f.contains(&"oikos/climate/house".to_owned())
            && f.contains(&"oikos/climate/zone/+/+".to_owned())
    );
    assert!(!f.iter().any(|t| t.ends_with('#')));
}

#[tokio::test]
async fn zone_settings_doc_is_not_a_device() {
    let (hub, _rx) = hub(true);
    let changes = hub.ingest_message(
        "oikos/climate/zone/living/settings",
        br#"{"state":"active"}"#,
        true,
    );
    assert!(changes.is_empty());
    assert!(
        hub.snapshot()
            .entities
            .iter()
            .all(|e| !e.id.contains("living"))
    );
    assert!(hub.snapshot().entities.is_empty());
}

#[tokio::test]
async fn climate_group_discovery_still_reaches_registry() {
    let (hub, _rx) = hub(true);
    let text = String::from_utf8(fixture("climate__daikin_192_168_1_150__climate.json"))
        .unwrap()
        .replace("daikin_192_168_1_150", "group_g1");
    hub.ingest_message("oikos/climate/group/g1/config", text.as_bytes(), true);
    assert!(
        hub.snapshot()
            .entities
            .iter()
            .any(|e| e.id.starts_with("group_g1")),
        "a group's discovery config under the climate prefix is still a device"
    );
}

#[tokio::test]
async fn store_keeps_invalid_with_reason() {
    let (hub, _rx) = hub(true);
    let mut events = hub.subscribe();
    hub.ingest_message(
        "oikos/climate/zone/attic/settings",
        br#"{"state":"active"}"#,
        true,
    );
    assert!(matches!(
        events.recv().await.unwrap(),
        WsEvent::ClimateChanged { zone: Some(z) } if z == "attic"
    ));
    hub.ingest_message(
        "oikos/climate/zone/attic/settings",
        br#"{"state":"bogus"}"#,
        true,
    );
    let view = hub.climate_view().unwrap();
    let zone = view.zones.iter().find(|z| z.zone == "attic").unwrap();
    assert_eq!(zone.settings.state, ZoneState::Active);
    assert!(zone.settings_invalid.is_some());

    hub.ingest_message("oikos/climate/house", br#"{"frost":40.0}"#, true);
    let view = hub.climate_view().unwrap();
    assert!(view.house_invalid.is_some());
}

#[tokio::test]
async fn view_lists_area_zones_and_skips_groups() {
    let (hub, _rx) = hub(true);
    let a = feed_daikin(&hub, 0, "living").await;
    let b = feed_daikin(&hub, 1, "living").await;
    let text = String::from_utf8(fixture("climate__daikin_192_168_1_152__climate.json"))
        .unwrap()
        .replace("daikin_192_168_1_152", "group_g1");
    hub.ingest("oikos/climate/group/g1/config", text.as_bytes());
    place(&hub, "group_g1", "living").await;

    let view = hub.climate_view().unwrap();
    assert!(view.enabled && !view.history);
    assert_eq!(view.zones.len(), 1, "{:?}", view.zones);
    let zone = &view.zones[0];
    assert_eq!(zone.zone, "living");
    assert!(!zone.orphan);
    let mut want = vec![a, b];
    want.sort();
    assert_eq!(zone.entities, want);

    // `settings.entities` wins over the area's devices.
    let settings = ZoneSettings {
        entities: Some(vec![want[0].clone()]),
        ..active()
    };
    hub.ingest_message(
        "oikos/climate/zone/living/settings",
        serde_json::to_string(&settings).unwrap().as_bytes(),
        true,
    );
    let view = hub.climate_view().unwrap();
    assert_eq!(view.zones[0].entities, vec![want[0].clone()]);
}

#[tokio::test]
async fn zone_without_area_is_listed_orphan() {
    let (hub, _rx) = hub(true);
    hub.ingest_message(
        "oikos/climate/zone/attic/settings",
        br#"{"state":"active"}"#,
        true,
    );
    let view = hub.climate_view().unwrap();
    assert_eq!(view.zones.len(), 1);
    assert!(view.zones[0].orphan);
    assert_eq!(view.zones[0].zone, "attic");
}

#[tokio::test]
async fn set_zone_publishes_retained_and_applies() {
    let (hub, mut rx) = hub(true);
    feed_daikin(&hub, 0, "living").await;
    drain(&mut rx);
    let mut events = hub.subscribe();
    hub.set_climate_zone("living", active()).unwrap();
    let published = drain(&mut rx);
    assert_eq!(published.len(), 1);
    assert_eq!(published[0].topic, "oikos/climate/zone/living/settings");
    assert!(published[0].retain);
    let view = hub.climate_view().unwrap();
    assert_eq!(view.zones[0].settings.state, ZoneState::Active);
    assert!(matches!(
        events.recv().await.unwrap(),
        WsEvent::ClimateChanged { zone: Some(z) } if z == "living"
    ));
}

#[tokio::test]
async fn set_house_publishes_retained_and_applies() {
    let (hub, mut rx) = hub(true);
    let house = oikos_climate::doc::HouseSettings {
        heat_below: 17.0,
        ..Default::default()
    };
    hub.set_climate_house(house).unwrap();
    let published = drain(&mut rx);
    assert_eq!(published[0].topic, "oikos/climate/house");
    assert!(published[0].retain);
    assert_eq!(hub.climate_view().unwrap().house.heat_below, 17.0);
    let bad = oikos_climate::doc::HouseSettings {
        frost: 40.0,
        ..Default::default()
    };
    assert!(matches!(
        hub.set_climate_house(bad),
        Err(HubError::Rejected(_))
    ));
}

#[tokio::test]
async fn set_zone_rejects_invalid_rules() {
    let (hub, mut rx) = hub(true);
    feed_daikin(&hub, 0, "living").await;
    drain(&mut rx);
    let block = |from: u16, to: u16| Block {
        days: vec![Day::Mon],
        from: HhMm(from),
        to: HhMm(to),
        setpoint: Setpoint::Temp(20.0),
        evidence: None,
    };
    let overlapping = ZoneSettings {
        rules: Rules {
            heat: Some(ModeRules {
                base: Setpoint::Temp(18.0),
                blocks: vec![block(60, 300), block(120, 400)],
            }),
            ..Rules::default()
        },
        ..active()
    };
    assert!(matches!(
        hub.set_climate_zone("living", overlapping),
        Err(HubError::Rejected(_))
    ));
    assert!(drain(&mut rx).is_empty());
}

#[tokio::test]
async fn set_zone_rejects_bad_ids_and_unknown_areas() {
    let (hub, mut rx) = hub(true);
    feed_daikin(&hub, 0, "living").await;
    drain(&mut rx);
    for id in ["", "a/b", "a+b", "a#", "#"] {
        assert!(
            matches!(
                hub.set_climate_zone(id, active()),
                Err(HubError::Rejected(_))
            ),
            "{id:?}"
        );
    }
    assert!(matches!(
        hub.set_climate_zone("nowhere", active()),
        Err(HubError::UnknownArea(_))
    ));
    assert!(drain(&mut rx).is_empty());
}
