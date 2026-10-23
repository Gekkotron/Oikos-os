//! The ambiance store and CRUD on a hub driven without a broker.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use oikos_api::{
    Ambiance, AmbianceAction, Category, CategoryAction, ClimateAction, ConnectionStatus,
    DeviceAction, FlowAction, FlowDo, Target, WsEvent,
};
use oikos_core::command::Publish;
use oikos_core::model::{Area, AreasDocument, Command};
use oikos_os::config::Config;
use oikos_os::hub::{Hub, HubError};
use tokio::sync::mpsc;

const SWITCH_FIXTURE: &str = "discovery/switch__0x847127fffe24e322__switch.json";
const SWITCH_ID: &str = "0x847127fffe24e322_switch_zigbee2mqtt";
const SWITCH_TOPIC: &str = "oikos/switch/0x847127fffe24e322/switch/config";

fn fixture(name: &str) -> Vec<u8> {
    let dir = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../oikos-core/tests/fixtures/"
    ));
    std::fs::read(dir.join(name)).expect("fixture")
}

fn config() -> Config {
    let mut cfg = Config::default();
    cfg.commands.confirm_timeout_ms = 50;
    cfg.topics.discovery_prefixes = vec!["oikos".to_owned()];
    cfg
}

fn hub() -> (Arc<Hub>, mpsc::Receiver<Publish>) {
    let (tx, rx) = mpsc::channel(64);
    let hub = Hub::new(&config(), tx);
    hub.set_connection(ConnectionStatus::Online);
    (hub, rx)
}

fn ambiance(id: &str, actions: Vec<AmbianceAction>) -> Ambiance {
    Ambiance {
        id: id.to_owned(),
        name: id.to_owned(),
        subtitle: None,
        icon: None,
        pinned: false,
        order: 1,
        actions,
        extra: Default::default(),
    }
}

fn category(area: Option<&str>, except: Vec<String>) -> AmbianceAction {
    AmbianceAction::Category(CategoryAction {
        what: Category::Lights,
        to: Target::Off,
        brightness: None,
        area: area.map(str::to_owned),
        except,
    })
}

fn rejected(result: Result<oikos_api::AmbianceView, HubError>) -> String {
    match result {
        Err(HubError::Rejected(why)) => why,
        other => panic!("expected Rejected, got {other:?}"),
    }
}

const VALID: &str = r#"{"id":"a","name":"A","actions":[]}"#;

#[tokio::test]
async fn retained_payload_is_listed_and_empty_payload_deletes() {
    let (hub, _rx) = hub();
    hub.ingest("oikos/ui/ambiances/a", VALID.as_bytes());
    let list = hub.ambiances();
    assert_eq!(list.len(), 1);
    assert_eq!(list[0].id, "a");
    assert!(list[0].ambiance.is_some() && list[0].invalid.is_none());
    assert!(hub.ambiance("a").is_some());

    hub.ingest("oikos/ui/ambiances/a", b"");
    assert!(hub.ambiances().is_empty());
    assert!(hub.ambiance("a").is_none());
}

#[tokio::test]
async fn invalid_payload_is_kept_with_reason() {
    let (hub, _rx) = hub();
    hub.ingest("oikos/ui/ambiances/a", br#"{"id":"a","name":""}"#);
    let list = hub.ambiances();
    assert_eq!(list.len(), 1);
    assert!(list[0].invalid.is_some(), "{list:?}");
    assert!(list[0].ambiance.is_none());
}

#[tokio::test]
async fn topic_id_mismatch_is_invalid() {
    let (hub, _rx) = hub();
    hub.ingest(
        "oikos/ui/ambiances/a",
        br#"{"id":"b","name":"B","actions":[]}"#,
    );
    let list = hub.ambiances();
    assert_eq!(list[0].id, "a");
    let reason = list[0].invalid.as_deref().expect("invalid");
    assert!(reason.contains("does not match the topic"), "{reason}");
}

#[tokio::test]
async fn set_ambiance_publishes_retained_and_lists_it() {
    let (hub, mut rx) = hub();
    let view = hub
        .set_ambiance(ambiance("a", vec![]))
        .await
        .expect("set_ambiance");
    assert_eq!(view.id, "a");
    let published = rx.try_recv().expect("a publish");
    assert_eq!(published.topic, "oikos/ui/ambiances/a");
    assert!(published.retain);
    assert_eq!(hub.ambiances().len(), 1);
}

#[tokio::test]
async fn set_ambiance_rejects_unknown_entity_area_flow() {
    let (hub, _rx) = hub();
    let device = AmbianceAction::Device(DeviceAction {
        entity: "nope".to_owned(),
        command: Command::Toggle,
    });
    assert!(rejected(hub.set_ambiance(ambiance("a", vec![device])).await).contains("nope"));

    let why = rejected(
        hub.set_ambiance(ambiance("a", vec![category(Some("nowhere"), vec![])]))
            .await,
    );
    assert!(why.contains("action 0") && why.contains("nowhere"), "{why}");

    let flow = AmbianceAction::Flow(FlowAction {
        flow: "ghost".to_owned(),
        action: FlowDo::Run,
    });
    assert!(rejected(hub.set_ambiance(ambiance("a", vec![flow])).await).contains("ghost"));

    let why = rejected(
        hub.set_ambiance(ambiance(
            "a",
            vec![category(None, vec!["ghost".to_owned()])],
        ))
        .await,
    );
    assert!(why.contains("except"), "{why}");
    assert!(hub.ambiances().is_empty());
}

#[tokio::test]
async fn set_ambiance_resolves_area_name_to_id() {
    let (hub, _rx) = hub();
    hub.set_areas(AreasDocument {
        areas: vec![Area {
            id: "lounge".to_owned(),
            name: "Lounge".to_owned(),
            ..Area::default()
        }],
    })
    .await
    .expect("areas");
    let view = hub
        .set_ambiance(ambiance("a", vec![category(Some("Lounge"), vec![])]))
        .await
        .expect("set_ambiance");
    let stored = view.ambiance.expect("valid");
    match &stored.actions[0] {
        AmbianceAction::Category(c) => assert_eq!(c.area.as_deref(), Some("lounge")),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn set_ambiance_accepts_a_known_device_and_rejects_climate_while_engine_off() {
    let (hub, _rx) = hub();
    hub.ingest(SWITCH_TOPIC, &fixture(SWITCH_FIXTURE));
    let device = AmbianceAction::Device(DeviceAction {
        entity: SWITCH_ID.to_owned(),
        command: Command::Toggle,
    });
    hub.set_ambiance(ambiance("a", vec![device]))
        .await
        .expect("a known switch");

    let climate = AmbianceAction::Climate(ClimateAction {
        clear: true,
        ..ClimateAction::default()
    });
    let why = rejected(hub.set_ambiance(ambiance("b", vec![climate])).await);
    assert!(why.contains("the climate engine is off"), "{why}");
}

#[tokio::test]
async fn delete_unknown_is_404_kind() {
    let (hub, _rx) = hub();
    match hub.delete_ambiance("ghost").await {
        Err(HubError::UnknownAmbiance(id)) => assert_eq!(id, "ghost"),
        other => panic!("{other:?}"),
    }
}

#[tokio::test]
async fn delete_publishes_an_empty_retained_payload() {
    let (hub, mut rx) = hub();
    hub.set_ambiance(ambiance("a", vec![])).await.expect("set");
    rx.try_recv().expect("set publish");
    hub.delete_ambiance("a").await.expect("delete");
    let published = rx.try_recv().expect("delete publish");
    assert_eq!(published.topic, "oikos/ui/ambiances/a");
    assert!(published.retain && published.payload.is_empty());
    assert!(hub.ambiances().is_empty());
}

#[tokio::test]
async fn ingest_broadcasts_ambiances_changed() {
    let (hub, _rx) = hub();
    let mut events = hub.subscribe();
    hub.ingest("oikos/ui/ambiances/a", VALID.as_bytes());
    let event = tokio::time::timeout(Duration::from_secs(1), events.recv())
        .await
        .expect("an event within 1 s")
        .expect("open channel");
    match event {
        WsEvent::AmbiancesChanged { ambiances } => {
            assert_eq!(ambiances.len(), 1);
            assert_eq!(ambiances[0].id, "a");
        }
        other => panic!("{other:?}"),
    }
    // An identical echo changes nothing, so nothing is broadcast.
    hub.ingest("oikos/ui/ambiances/a", VALID.as_bytes());
    assert!(events.try_recv().is_err());
}
