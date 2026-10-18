//! The hub driven without a broker: `ingest` feeds it, the publisher channel is inspected.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use oikos_api::WsEvent;
use oikos_core::command::Publish;
use oikos_core::model::{Command, Kind, StateValue};
use oikos_os::config::Config;
use oikos_os::hub::{Hub, HubError};
use tokio::sync::mpsc;

/// A recorded ZBMINI switch: the `rooms/stair/light` device.
const SWITCH_FIXTURE: &str = "discovery/switch__0x847127fffe24e322__switch.json";
const SWITCH_ID: &str = "0x847127fffe24e322_switch_zigbee2mqtt";
const SWITCH_TOPIC: &str = "oikos/switch/0x847127fffe24e322/switch/config";

fn fixtures() -> PathBuf {
    PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../oikos-core/tests/fixtures/"
    ))
}

fn fixture(name: &str) -> Vec<u8> {
    std::fs::read(fixtures().join(name)).expect("fixture")
}

fn config() -> Config {
    let mut cfg = Config::default();
    cfg.commands.confirm_timeout_ms = 50;
    cfg.topics.discovery_prefixes = vec!["oikos".to_owned()];
    cfg
}

fn hub() -> (Arc<Hub>, mpsc::Receiver<Publish>) {
    let (tx, rx) = mpsc::channel(64);
    (Hub::new(&config(), tx), rx)
}

/// The switch's `state_topic`, from its discovery config.
fn switch_state_topic() -> String {
    let config: serde_json::Value =
        serde_json::from_slice(&fixture(SWITCH_FIXTURE)).expect("switch config");
    config["state_topic"]
        .as_str()
        .expect("state_topic")
        .to_owned()
}

fn feed_switch(hub: &Hub) {
    hub.ingest(SWITCH_TOPIC, &fixture(SWITCH_FIXTURE));
}

async fn next_event(rx: &mut tokio::sync::broadcast::Receiver<WsEvent>) -> WsEvent {
    tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("an event within 1 s")
        .expect("open channel")
}

#[tokio::test]
async fn ingest_broadcasts_and_snapshots() {
    let (hub, _publisher) = hub();
    let mut events = hub.subscribe();

    hub.ingest(
        "zigbee2mqtt/bridge/devices",
        &fixture("bridge_devices.json"),
    );
    assert!(matches!(
        next_event(&mut events).await,
        WsEvent::DevicesChanged(devices) if devices.len() == 66
    ));

    feed_switch(&hub);
    match next_event(&mut events).await {
        WsEvent::EntityUpserted(view) => {
            assert_eq!(view.id, SWITCH_ID);
            assert_eq!(view.kind, Kind::Switch);
            assert_eq!(view.state.value, StateValue::Unknown);
        }
        other => panic!("expected EntityUpserted, got {other:?}"),
    }

    hub.ingest(
        &switch_state_topic(),
        &fixture("states/rooms__stair__light.json"),
    );
    match next_event(&mut events).await {
        WsEvent::StateChanged { id, state } => {
            assert_eq!(id, SWITCH_ID);
            assert_eq!(state.value, StateValue::OnOff(false));
        }
        other => panic!("expected StateChanged, got {other:?}"),
    }

    let snapshot = hub.snapshot();
    assert_eq!(snapshot.entities.len(), 1);
    assert_eq!(snapshot.entities[0].state.value, StateValue::OnOff(false));
    assert_eq!(snapshot.devices.len(), 66);
    assert!(hub.subscription_topics().contains(&switch_state_topic()));
}

#[tokio::test]
async fn set_override_publishes_retained_and_applies() {
    let (hub, mut publisher) = hub();
    feed_switch(&hub);

    let mut patch = serde_json::Map::new();
    patch.insert("asLight".to_owned(), serde_json::Value::Bool(true));
    let view = hub.set_override(SWITCH_ID, &patch).await.expect("override");
    assert_eq!(view.kind, Kind::Light);
    assert!(view.ui.as_light);
    assert_eq!(
        publisher.recv().await.expect("a publish"),
        Publish {
            topic: format!("oikos/ui/{SWITCH_ID}"),
            payload: r#"{"asLight":true}"#.to_owned(),
            retain: true,
        }
    );
    assert_eq!(
        hub.entity_view(SWITCH_ID).expect("view").kind,
        Kind::Light,
        "the registry reflects the override before the broker echoes it"
    );

    let mut clear = serde_json::Map::new();
    clear.insert("asLight".to_owned(), serde_json::Value::Null);
    let view = hub.set_override(SWITCH_ID, &clear).await.expect("override");
    assert_eq!(view.kind, Kind::Switch);
    assert_eq!(
        publisher.recv().await.expect("a publish"),
        Publish {
            topic: format!("oikos/ui/{SWITCH_ID}"),
            payload: String::new(),
            retain: true,
        }
    );

    let mut bad = serde_json::Map::new();
    bad.insert("nope".to_owned(), serde_json::Value::Bool(true));
    assert!(matches!(
        hub.set_override(SWITCH_ID, &bad).await,
        Err(HubError::Patch(_))
    ));
    assert!(matches!(
        hub.set_override("missing", &patch).await,
        Err(HubError::UnknownEntity(id)) if id == "missing"
    ));
}

#[tokio::test]
async fn send_command_publishes_then_times_out() {
    let (hub, mut publisher) = hub();
    feed_switch(&hub);
    let mut events = hub.subscribe();

    let response = hub
        .send_command(
            SWITCH_ID,
            Command::TurnOn {
                brightness: None,
                color: None,
                effect: None,
            },
        )
        .await
        .expect("mapped and published");

    let published = publisher.try_recv().expect("the command publish");
    assert_eq!(published.topic, "zigbee2mqtt/rooms/stair/light/set");
    assert_eq!(published.payload, "ON");
    assert!(!published.retain);

    assert!(!response.accepted);
    assert_eq!(
        response.reason.as_deref(),
        Some("no state change within 50 ms")
    );
    assert_eq!(response.state, None);

    let mut failed = None;
    while let Ok(event) = events.try_recv() {
        if let WsEvent::CommandFailed { id, reason } = event {
            failed = Some((id, reason));
        }
    }
    assert_eq!(
        failed,
        Some((
            SWITCH_ID.to_owned(),
            "no state change within 50 ms".to_owned()
        ))
    );
}

#[tokio::test]
async fn send_command_confirmed_by_state() {
    let (hub, mut publisher) = hub();
    feed_switch(&hub);
    let state_topic = switch_state_topic();

    let echo = {
        let hub = Arc::clone(&hub);
        tokio::spawn(async move {
            let published = publisher.recv().await.expect("the command publish");
            assert_eq!(published.payload, "ON");
            hub.ingest(&state_topic, br#"{"state":"ON"}"#);
        })
    };

    let response = hub
        .send_command(
            SWITCH_ID,
            Command::TurnOn {
                brightness: None,
                color: None,
                effect: None,
            },
        )
        .await
        .expect("mapped and published");
    echo.await.expect("echo task");

    assert!(response.accepted, "{response:?}");
    assert_eq!(response.reason, None);
    assert_eq!(
        response.state.expect("confirmed state").value,
        StateValue::OnOff(true)
    );
}

#[tokio::test]
async fn command_to_offline_entity_is_rejected_without_publish() {
    let (hub, mut publisher) = hub();
    feed_switch(&hub);
    hub.ingest("zigbee2mqtt/bridge/state", br#"{"state":"offline"}"#);

    let result = hub.send_command(SWITCH_ID, Command::TurnOff).await;
    assert_eq!(
        result,
        Err(HubError::Rejected("entity is offline".to_owned()))
    );
    assert!(
        publisher.try_recv().is_err(),
        "nothing is published for an offline entity"
    );
}

#[tokio::test]
async fn unknown_entity_is_unknown_error() {
    let (hub, mut publisher) = hub();
    feed_switch(&hub);

    let result = hub.send_command("nope", Command::TurnOff).await;
    assert_eq!(result, Err(HubError::UnknownEntity("nope".to_owned())));
    assert!(publisher.try_recv().is_err());
    assert!(hub.entity_view("nope").is_none());
}
