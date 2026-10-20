//! The hub driven without a broker: `ingest` feeds it, the publisher channel is inspected.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use oikos_api::{ConnectionStatus, VirtualKind, VirtualSpec, WsEvent};
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
        WsEvent::DevicesChanged { devices } if devices.len() == 66
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
    patch.insert("showAs".to_owned(), serde_json::Value::from("light"));
    let view = hub.set_override(SWITCH_ID, &patch).await.expect("override");
    assert_eq!(view.kind, Kind::Light);
    assert_eq!(view.ui.show_as.as_deref(), Some("light"));
    assert_eq!(
        publisher.recv().await.expect("a publish"),
        Publish {
            topic: format!("oikos/ui/{SWITCH_ID}"),
            payload: r#"{"asLight":true,"showAs":"light"}"#.to_owned(),
            retain: true,
        }
    );
    assert_eq!(
        hub.entity_view(SWITCH_ID).expect("view").kind,
        Kind::Light,
        "the registry reflects the override before the broker echoes it"
    );

    let mut clear = serde_json::Map::new();
    clear.insert("showAs".to_owned(), serde_json::Value::Null);
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

/// Sends `TurnOn` to the switch and ingests `echo` on its state topic as soon as the publish
/// is out. Returns the response and what the echo changed.
async fn turn_on_with_echo(
    hub: &Arc<Hub>,
    mut publisher: mpsc::Receiver<Publish>,
    echo: &'static [u8],
) -> (oikos_api::CommandResponse, Vec<Change>) {
    let state_topic = switch_state_topic();
    let echo_task = {
        let hub = Arc::clone(hub);
        tokio::spawn(async move {
            let published = publisher.recv().await.expect("the command publish");
            assert_eq!(published.payload, "ON");
            hub.ingest(&state_topic, echo)
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
    let changes = echo_task.await.expect("echo task");
    (response, changes)
}

/// `response` was accepted with the switch ON (a timeout would carry a reason).
fn assert_confirmed_on(response: &oikos_api::CommandResponse) {
    assert!(response.accepted, "{response:?}");
    assert_eq!(response.reason, None);
    assert_eq!(
        response.state.as_ref().expect("current state").value,
        StateValue::OnOff(true)
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

    let (response, changes) = turn_on_with_echo(&hub, publisher, br#"{"linkquality":120}"#).await;

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
    // A timeout well above the immediate echo: the echo must resolve it, not the clock.
    let (hub, publisher) = hub_with_timeout(500);
    feed_switch(&hub);
    hub.ingest(&switch_state_topic(), br#"{"state":"ON"}"#);
    let mut events = hub.subscribe();

    let (response, changes) = turn_on_with_echo(&hub, publisher, br#"{"state":"ON"}"#).await;

    assert_eq!(changes, vec![Change::StateReported(SWITCH_ID.to_owned())]);
    assert_confirmed_on(&response);
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

/// The area written by an override is stored canonically: a declared name resolves to its
/// id (so the retained JSON and the hub agree), a blank string clears the field instead of
/// becoming an implicit area with an empty id.
#[tokio::test]
async fn set_override_canonicalises_area() {
    let (hub, mut publisher) = hub();
    feed_switch(&hub);
    hub.set_areas(AreasDocument {
        areas: vec![Area {
            id: "living".to_owned(),
            name: "Living".to_owned(),
            ..Area::default()
        }],
    })
    .await
    .expect("areas");
    publisher.try_recv().expect("the areas publish");

    let mut patch = serde_json::Map::new();
    patch.insert("area".to_owned(), serde_json::Value::from("Living"));
    let view = hub.set_override(SWITCH_ID, &patch).await.expect("override");
    assert_eq!(view.area, "living");
    assert_eq!(view.ui.area.as_deref(), Some("living"));
    assert_eq!(
        publisher.try_recv().expect("the override publish").payload,
        r#"{"area":"living"}"#
    );
    assert_eq!(
        hub.entity_detail(SWITCH_ID)
            .and_then(|d| d.raw_override)
            .and_then(|o| o.area),
        Some("living".to_owned())
    );

    // An undeclared area is kept as written (an implicit area).
    patch.insert("area".to_owned(), serde_json::Value::from("Attic"));
    let view = hub.set_override(SWITCH_ID, &patch).await.expect("override");
    assert_eq!(view.area, "Attic");
    publisher.try_recv().expect("the override publish");

    for blank in ["", "   "] {
        patch.insert("area".to_owned(), serde_json::Value::from(blank));
        let view = hub.set_override(SWITCH_ID, &patch).await.expect("override");
        assert_eq!(view.area, "stair", "{blank:?} clears the area");
        assert_eq!(view.ui.area, None);
        assert_eq!(
            publisher.try_recv().expect("the override publish").payload,
            "",
            "{blank:?} leaves a default override, retained empty"
        );
        assert!(hub.snapshot().areas.iter().all(|a| !a.id.trim().is_empty()));
    }
}

/// A discovery payload that cannot become an entity is logged once with its topic: not again
/// on re-delivery, not for an ignored component (expected noise), and again only after the
/// topic recovered and broke anew.
#[tokio::test]
async fn invalid_discovery_is_logged_once() {
    let (hub, _publisher) = hub();
    assert_eq!(hub.logged_ignored_len(), 0);

    let garbage: &[u8] = &[0xff, 0xfe, b'{'];
    hub.ingest("oikos/sensor/garbage/x/config", garbage);
    assert_eq!(hub.ignored().len(), 1);
    assert_eq!(hub.logged_ignored_len(), 1);
    hub.ingest("oikos/sensor/garbage/x/config", garbage);
    assert_eq!(
        hub.logged_ignored_len(),
        1,
        "a re-delivery is not logged again"
    );

    hub.ingest("oikos/update/x/config", br#"{"unique_id":"u"}"#);
    assert_eq!(hub.ignored().len(), 2);
    assert_eq!(
        hub.logged_ignored_len(),
        1,
        "an ignored component is not logged"
    );

    hub.ingest(
        "oikos/sensor/garbage/x/config",
        br#"{"unique_id":"g","state_topic":"zigbee2mqtt/g"}"#,
    );
    assert_eq!(hub.ignored().len(), 1);
    assert_eq!(
        hub.logged_ignored_len(),
        0,
        "a topic that recovered is forgotten"
    );

    hub.ingest("oikos/sensor/garbage/x/config", garbage);
    assert_eq!(
        hub.logged_ignored_len(),
        1,
        "and logged again when it breaks anew"
    );
}

// ----- push alerts -----

const HISTORY_TOPIC: &str = "oikos-push/history";
const ALERT_ENTRY: &str = r#"{"v":1,"id":"frigate-front-1696","channel":"camera","severity":"warning","title":"Front Door","body":"Person in Porch","image_url":"http://192.168.1.20:5000/api/events/1696/snapshot.jpg","actions":[{"id":"disarm","label":"Disarm"}],"silent":false,"sent_at":1760880000,"source":"frigate"}"#;

fn history_payload() -> Vec<u8> {
    format!("[{ALERT_ENTRY}]").into_bytes()
}

#[tokio::test]
async fn history_payload_fills_alerts_and_broadcasts() {
    let (hub, _publisher) = hub();
    let mut events = hub.subscribe();

    let changes = hub.ingest(HISTORY_TOPIC, &history_payload());

    assert!(changes.is_empty(), "no registry change: {changes:?}");
    match next_event(&mut events).await {
        WsEvent::AlertsChanged { alerts } => {
            assert_eq!(alerts.len(), 1);
            assert_eq!(alerts[0].id, "frigate-front-1696");
            assert_eq!(hub.alerts(), alerts);
            assert_eq!(hub.snapshot().alerts, alerts);
        }
        other => panic!("expected AlertsChanged, got {other:?}"),
    }
}

#[tokio::test]
async fn identical_history_does_not_broadcast_again() {
    let (hub, _publisher) = hub();
    let mut events = hub.subscribe();
    hub.ingest(HISTORY_TOPIC, &history_payload());
    next_event(&mut events).await;

    hub.ingest(HISTORY_TOPIC, &history_payload());

    assert!(
        tokio::time::timeout(Duration::from_millis(100), events.recv())
            .await
            .is_err(),
        "an unchanged history must not be re-broadcast"
    );
}

#[tokio::test]
async fn empty_history_payload_clears_the_list() {
    let (hub, _publisher) = hub();
    let mut events = hub.subscribe();
    hub.ingest(HISTORY_TOPIC, &history_payload());
    next_event(&mut events).await;

    hub.ingest(HISTORY_TOPIC, b"");

    assert!(matches!(
        next_event(&mut events).await,
        WsEvent::AlertsChanged { alerts } if alerts.is_empty()
    ));
    assert!(hub.alerts().is_empty());
}

#[tokio::test]
async fn non_array_history_keeps_previous_list() {
    let (hub, _publisher) = hub();
    let mut events = hub.subscribe();
    hub.ingest(HISTORY_TOPIC, &history_payload());
    next_event(&mut events).await;

    hub.ingest(HISTORY_TOPIC, b"{}");

    assert_eq!(hub.alerts().len(), 1);
    assert!(
        tokio::time::timeout(Duration::from_millis(100), events.recv())
            .await
            .is_err()
    );
}

#[tokio::test]
async fn reply_publishes_the_app_payload() {
    let (hub, mut publisher) = hub();
    hub.ingest(HISTORY_TOPIC, &history_payload());

    hub.reply("frigate-front-1696", "disarm")
        .expect("reply accepted");

    let published = publisher.recv().await.expect("a publish");
    assert_eq!(published.topic, "oikos-push/responses");
    assert!(!published.retain);
    let value: serde_json::Value = serde_json::from_str(&published.payload).expect("json");
    assert_eq!(value["id"], "frigate-front-1696");
    assert_eq!(value["action"], "disarm");
    assert_eq!(value["install_id"], "oikos-web");
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .expect("clock")
        .as_secs();
    let ts = value["ts"].as_u64().expect("ts in seconds");
    assert!(now.abs_diff(ts) <= 5, "ts {ts} is not within 5 s of {now}");
    let raw = &published.payload;
    let pos = |key: &str| raw.find(key).expect(key);
    assert!(pos("\"id\"") < pos("\"action\""));
    assert!(pos("\"action\"") < pos("\"install_id\""));
    assert!(pos("\"install_id\"") < pos("\"ts\""));
}

#[tokio::test]
async fn reply_rejects_unknown_alert_action_and_offline() {
    let (hub, _publisher) = hub();
    hub.ingest(HISTORY_TOPIC, &history_payload());

    assert!(matches!(
        hub.reply("nope", "disarm"),
        Err(HubError::UnknownAlert(id)) if id == "nope"
    ));
    assert!(matches!(
        hub.reply("frigate-front-1696", "zzz"),
        Err(HubError::UnknownAction(action)) if action == "zzz"
    ));

    hub.set_connection(ConnectionStatus::Offline {
        reason: "test".into(),
    });
    assert!(matches!(
        hub.reply("frigate-front-1696", "disarm"),
        Err(HubError::Publish(_))
    ));
}

fn virtual_switch() -> VirtualSpec {
    VirtualSpec {
        kind: VirtualKind::Switch,
        name: "Force heating".to_owned(),
        min: None,
        max: None,
        step: None,
        unit: None,
        initial: None,
    }
}

const VIRTUAL_STATE: &str = "oikos/virtual/force_heating";
const VIRTUAL_CONFIG: &str = "oikos/switch/virtual/force_heating/config";

fn drain(publisher: &mut mpsc::Receiver<Publish>) -> Vec<Publish> {
    let mut out = Vec::new();
    while let Ok(publish) = publisher.try_recv() {
        out.push(publish);
    }
    out
}

#[tokio::test]
async fn creating_publishes_config_and_initial_off() {
    let (hub, mut publisher) = hub();
    let view = hub
        .set_virtual("force_heating", virtual_switch())
        .await
        .expect("created");
    assert_eq!(view.id, "virtual_force_heating");
    assert_eq!(view.kind, Kind::Switch);
    assert_eq!(view.state.value, StateValue::OnOff(false));

    let published = drain(&mut publisher);
    assert_eq!(published.len(), 2, "{published:?}");
    assert_eq!(published[0].topic, VIRTUAL_CONFIG);
    assert!(published[0].retain);
    assert_eq!(published[1].topic, VIRTUAL_STATE);
    assert_eq!(published[1].payload, "OFF");
    assert!(published[1].retain);
}

#[tokio::test]
async fn recreating_keeps_the_state() {
    let (hub, mut publisher) = hub();
    hub.set_virtual("force_heating", virtual_switch())
        .await
        .expect("created");
    hub.ingest(VIRTUAL_STATE, b"ON");
    drain(&mut publisher);

    let mut renamed = virtual_switch();
    renamed.name = "Force the heating".to_owned();
    let view = hub
        .set_virtual("force_heating", renamed)
        .await
        .expect("recreated");
    assert_eq!(view.label.as_deref(), Some("Force the heating"));
    assert_eq!(view.state.value, StateValue::OnOff(true));
    let published = drain(&mut publisher);
    assert_eq!(published.len(), 1, "{published:?}");
    assert_eq!(published[0].topic, VIRTUAL_CONFIG);
}

#[tokio::test]
async fn number_defaults_and_initial_override() {
    let (hub, mut publisher) = hub();
    let spec = VirtualSpec {
        kind: VirtualKind::Number,
        name: "Target".to_owned(),
        min: Some(10.0),
        max: Some(30.0),
        step: Some(0.5),
        unit: Some("°C".to_owned()),
        initial: None,
    };
    let view = hub
        .set_virtual("target", spec.clone())
        .await
        .expect("number");
    assert_eq!(view.kind, Kind::Number);
    let published = drain(&mut publisher);
    assert_eq!(published[0].topic, "oikos/number/virtual/target/config");
    assert_eq!(published[1].topic, "oikos/virtual/target");
    assert_eq!(published[1].payload, "10");

    let mut bad = spec.clone();
    bad.initial = Some(serde_json::json!(99));
    assert!(matches!(
        hub.set_virtual("other", bad).await,
        Err(HubError::Rejected(_))
    ));
    assert!(drain(&mut publisher).is_empty());
}

#[tokio::test]
async fn bad_virtual_id_is_rejected_without_publish() {
    let (hub, mut publisher) = hub();
    for id in ["Force", "", "a/b", &"x".repeat(49)] {
        assert!(
            matches!(
                hub.set_virtual(id, virtual_switch()).await,
                Err(HubError::Rejected(_))
            ),
            "{id}"
        );
    }
    assert!(drain(&mut publisher).is_empty());
}

#[tokio::test]
async fn virtual_switch_command_is_accepted_and_retained() {
    let (hub, mut publisher) = hub_with_timeout(500);
    hub.set_virtual("force_heating", virtual_switch())
        .await
        .expect("created");
    drain(&mut publisher);

    let echo_hub = Arc::clone(&hub);
    let echo = tokio::spawn(async move {
        let published = publisher.recv().await.expect("the command publish");
        assert_eq!(published.topic, VIRTUAL_STATE);
        assert_eq!(published.payload, "ON");
        assert!(published.retain, "a virtual command is retained");
        echo_hub.ingest(VIRTUAL_STATE, b"ON");
    });
    let response = hub
        .send_command(
            "virtual_force_heating",
            Command::TurnOn {
                brightness: None,
                color: None,
                effect: None,
            },
        )
        .await
        .expect("published");
    echo.await.expect("echo task");
    assert_confirmed_on(&response);
}

#[tokio::test]
async fn virtual_topics_are_not_ignored_discovery() {
    let (hub, _publisher) = hub();
    hub.ingest("oikos/virtual/x", b"ON");
    hub.ingest("oikos/virtual/x/extra/config", b"{}");
    assert!(hub.ignored().is_empty(), "{:?}", hub.ignored());
}

#[tokio::test]
async fn delete_virtual_clears_both_topics() {
    let (hub, mut publisher) = hub();
    hub.set_virtual("force_heating", virtual_switch())
        .await
        .expect("created");
    drain(&mut publisher);

    hub.delete_virtual("force_heating").await.expect("deleted");
    let published = drain(&mut publisher);
    let topics: Vec<&str> = published.iter().map(|p| p.topic.as_str()).collect();
    assert_eq!(topics, [VIRTUAL_CONFIG, VIRTUAL_STATE]);
    assert!(published.iter().all(|p| p.payload.is_empty() && p.retain));
    assert!(hub.entity_view("virtual_force_heating").is_none());

    assert!(matches!(
        hub.delete_virtual("force_heating").await,
        Err(HubError::UnknownEntity(_))
    ));
    // A real device is not a virtual entity.
    feed_switch(&hub);
    assert!(matches!(
        hub.delete_virtual(SWITCH_ID).await,
        Err(HubError::UnknownEntity(_))
    ));
}

#[tokio::test]
async fn virtual_button_has_no_state_and_no_retain() {
    let (hub, mut publisher) = hub();
    let spec = VirtualSpec {
        kind: VirtualKind::Button,
        name: "Boost".to_owned(),
        min: None,
        max: None,
        step: None,
        unit: None,
        initial: None,
    };
    let view = hub.set_virtual("boost", spec).await.expect("button");
    assert_eq!(view.kind, Kind::Button);
    let published = drain(&mut publisher);
    assert_eq!(published.len(), 1, "{published:?}");
    assert_eq!(published[0].topic, "oikos/button/virtual/boost/config");
}
