//! The hub driven without a broker: `ingest` feeds it, the publisher channel is inspected.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use oikos_api::{ConnectionStatus, WsEvent};
use oikos_core::command::Publish;
use oikos_core::model::{Area, AreasDocument, Command, Kind, StateValue};
use oikos_core::registry::Change;
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
    hub_with_timeout(50)
}

/// A hub whose broker link is up, so its writers publish.
fn hub_with_timeout(confirm_timeout_ms: u64) -> (Arc<Hub>, mpsc::Receiver<Publish>) {
    let mut cfg = config();
    cfg.commands.confirm_timeout_ms = confirm_timeout_ms;
    let (tx, rx) = mpsc::channel(64);
    let hub = Hub::new(&cfg, tx);
    hub.set_connection(ConnectionStatus::Online);
    (hub, rx)
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

/// Sends `TurnOn` to the switch and ingests `echo` on its state topic once the publish is
/// out. Returns the response, what the echo changed and how long the hub took to answer.
async fn turn_on_with_echo(
    hub: &Arc<Hub>,
    mut publisher: mpsc::Receiver<Publish>,
    echo: &'static [u8],
) -> (oikos_api::CommandResponse, Vec<Change>, Duration) {
    let state_topic = switch_state_topic();
    let echo_task = {
        let hub = Arc::clone(hub);
        tokio::spawn(async move {
            let published = publisher.recv().await.expect("the command publish");
            assert_eq!(published.payload, "ON");
            hub.ingest(&state_topic, echo)
        })
    };
    let started = std::time::Instant::now();
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
    let changes = echo_task.await.expect("echo task");
    (response, changes, started.elapsed())
}

/// `response` was accepted with the switch ON, well before the 50 ms timeout.
fn assert_confirmed_on(response: &oikos_api::CommandResponse, elapsed: Duration) {
    assert!(response.accepted, "{response:?}");
    assert_eq!(response.reason, None);
    assert_eq!(
        response.state.as_ref().expect("current state").value,
        StateValue::OnOff(true)
    );
    assert!(
        elapsed < Duration::from_millis(40),
        "resolved by the echo, not the 50 ms timeout: {elapsed:?}"
    );
}

/// A message on the entity's topic that carries nothing for it (a partial update without
/// `state`, which the switch's template reads) is neither a change nor a report: the device
/// did not speak about this entity, so the command is not confirmed by it.
#[tokio::test]
async fn send_command_ignores_an_echo_without_its_key() {
    let (hub, publisher) = hub();
    feed_switch(&hub);
    hub.ingest(&switch_state_topic(), br#"{"state":"ON"}"#);

    let (response, changes, _) =
        turn_on_with_echo(&hub, publisher, br#"{"linkquality":120}"#).await;

    assert!(
        !changes.contains(&Change::StateChanged(SWITCH_ID.to_owned()))
            && !changes.contains(&Change::StateReported(SWITCH_ID.to_owned())),
        "the echo must say nothing about the switch for this test: {changes:?}"
    );
    assert!(!response.accepted, "{response:?}");
    assert_eq!(
        response.reason.as_deref(),
        Some("no state change within 50 ms")
    );
}

/// Turning on a switch that is already ON: the echoed `{"state":"ON"}` is a
/// `StateReported` (nothing differs), which confirms the command without any event reaching
/// the WebSocket clients.
#[tokio::test]
async fn send_command_resolves_on_same_state_echo() {
    let (hub, publisher) = hub();
    feed_switch(&hub);
    hub.ingest(&switch_state_topic(), br#"{"state":"ON"}"#);
    let mut events = hub.subscribe();

    let (response, changes, elapsed) =
        turn_on_with_echo(&hub, publisher, br#"{"state":"ON"}"#).await;

    assert_eq!(changes, vec![Change::StateReported(SWITCH_ID.to_owned())]);
    assert_confirmed_on(&response, elapsed);
    assert!(
        events
            .try_recv()
            .is_err_and(|err| err == tokio::sync::broadcast::error::TryRecvError::Empty),
        "a report that changed nothing is not broadcast"
    );
}

/// Every Z2M entity lists `bridge/state` as an availability topic. A message there within the
/// window (a retained re-delivery of the same word, or the bridge going offline) says nothing
/// about the command: only a report on the entity's state topic confirms it.
#[tokio::test]
async fn availability_message_does_not_confirm_command() {
    let (hub, mut publisher) = hub_with_timeout(2_000);
    feed_switch(&hub);
    hub.ingest("zigbee2mqtt/bridge/state", br#"{"state":"online"}"#);
    hub.ingest(&switch_state_topic(), br#"{"state":"OFF"}"#);

    let pending = {
        let hub = Arc::clone(&hub);
        tokio::spawn(async move {
            hub.send_command(
                SWITCH_ID,
                Command::TurnOn {
                    brightness: None,
                    color: None,
                    effect: None,
                },
            )
            .await
        })
    };
    let published = tokio::time::timeout(Duration::from_secs(1), publisher.recv())
        .await
        .expect("published within 1 s")
        .expect("the command publish");
    assert_eq!(published.payload, "ON");

    hub.ingest("zigbee2mqtt/bridge/state", br#"{"state":"online"}"#);
    hub.ingest("zigbee2mqtt/bridge/state", br#"{"state":"offline"}"#);
    tokio::time::sleep(Duration::from_millis(100)).await;
    assert!(
        !pending.is_finished(),
        "an availability message must not confirm the command"
    );

    hub.ingest("zigbee2mqtt/bridge/state", br#"{"state":"online"}"#);
    hub.ingest(&switch_state_topic(), br#"{"state":"ON"}"#);
    let response = pending
        .await
        .expect("command task")
        .expect("mapped and published");
    assert!(response.accepted, "{response:?}");
    assert_eq!(
        response.state.expect("confirmed state").value,
        StateValue::OnOff(true)
    );
}

/// While the broker link is down, every writer refuses with `HubError::Publish` before
/// anything is queued or applied locally: a write accepted during an outage would show in
/// the hub, never reach the broker and vanish at the next restart.
#[tokio::test]
async fn write_refused_while_offline() {
    let (hub, mut publisher) = hub();
    feed_switch(&hub);
    hub.set_connection(ConnectionStatus::Offline {
        reason: "connection reset".to_owned(),
    });
    let offline = HubError::Publish("broker offline".to_owned());

    let mut patch = serde_json::Map::new();
    patch.insert("favorite".to_owned(), serde_json::Value::Bool(true));
    assert_eq!(
        hub.set_override(SWITCH_ID, &patch).await.err(),
        Some(offline.clone())
    );
    assert!(
        !hub.entity_view(SWITCH_ID).expect("view").ui.favorite,
        "a refused override is not applied locally"
    );
    assert_eq!(
        hub.clear_override(SWITCH_ID).await.err(),
        Some(offline.clone())
    );
    assert_eq!(
        hub.set_areas(AreasDocument {
            areas: vec![Area {
                id: "stair".to_owned(),
                name: "Stair".to_owned(),
                ..Area::default()
            }],
        })
        .await
        .err(),
        Some(offline.clone())
    );
    let stair = hub
        .snapshot()
        .areas
        .into_iter()
        .find(|a| a.id == "stair")
        .expect("the switch's implicit area");
    assert_eq!(
        stair.name, "stair",
        "still implicit, not declared: {stair:?}"
    );
    assert_eq!(
        hub.send_command(SWITCH_ID, Command::TurnOff).await.err(),
        Some(offline.clone())
    );
    assert!(publisher.try_recv().is_err(), "nothing was queued");

    // Connecting is not online either; once online the same write goes through.
    hub.set_connection(ConnectionStatus::Connecting);
    assert_eq!(
        hub.set_override(SWITCH_ID, &patch).await.err(),
        Some(offline)
    );
    hub.set_connection(ConnectionStatus::Online);
    let view = hub.set_override(SWITCH_ID, &patch).await.expect("override");
    assert!(view.ui.favorite);
    assert_eq!(
        publisher.try_recv().expect("the override publish").payload,
        r#"{"favorite":true}"#
    );
}
