//! Flows on the hub, driven without a broker: `ingest` feeds it, the publisher channel is
//! inspected.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oikos_api::{ConnectionStatus, RunStatus, RunView, StepOutcome, StepView, WsEvent};
use oikos_core::command::Publish;
use oikos_core::flow::{Flow, NodeKind};
use oikos_core::model::{Area, AreasDocument, Command};
use oikos_os::config::Config;
use oikos_os::hub::{Hub, HubError};
use tokio::sync::mpsc;

const LED_ID: &str = "0x60a423fffe94d1c6_light_zigbee2mqtt";
const MOTION_ID: &str = "0x00158d008b5a2d65_occupancy_zigbee2mqtt";
const LED_STATE_TOPIC: &str = "zigbee2mqtt/rooms/living/led_ceiling";
const MOTION_STATE_TOPIC: &str = "zigbee2mqtt/rooms/living/motion_north";
const LED_COMMAND_TOPIC: &str = "zigbee2mqtt/rooms/living/led_ceiling/set";
const FLOW_TOPIC: &str = "oikos/automation/living";

fn fixture(name: &str) -> Vec<u8> {
    let path = PathBuf::from(concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../oikos-core/tests/fixtures/"
    ));
    std::fs::read(path.join(name)).expect("fixture")
}

fn hub() -> (Arc<Hub>, mpsc::Receiver<Publish>) {
    let mut cfg = Config::default();
    cfg.commands.confirm_timeout_ms = 50;
    cfg.topics.discovery_prefixes = vec!["oikos".to_owned()];
    let (tx, rx) = mpsc::channel(64);
    let hub = Hub::new(&cfg, tx);
    hub.set_connection(ConnectionStatus::Online);
    (hub, rx)
}

/// A hub that knows the LED and the motion sensor.
fn hub_with_entities() -> (Arc<Hub>, mpsc::Receiver<Publish>) {
    let (hub, rx) = hub();
    hub.ingest(
        "oikos/light/0x60a423fffe94d1c6/light/config",
        &fixture("discovery/light__0x60a423fffe94d1c6__light.json"),
    );
    hub.ingest(
        "oikos/binary_sensor/0x00158d008b5a2d65/occupancy/config",
        &fixture("discovery/binary_sensor__0x00158d008b5a2d65__occupancy.json"),
    );
    (hub, rx)
}

fn reference_json() -> String {
    serde_json::json!({
        "id": "living", "name": "Living LED on motion", "enabled": true, "mode": "restart",
        "nodes": [
            { "id": "motion", "type": "trigger.state", "params": { "entity": MOTION_ID } },
            { "id": "moving", "type": "if", "params": { "op": "eq", "value": true } },
            { "id": "on", "type": "command",
              "params": { "entity": LED_ID, "command": { "type": "turn_on" } } },
            { "id": "wait", "type": "delay", "params": { "for": "5m" } },
            { "id": "off", "type": "command",
              "params": { "entity": LED_ID, "command": { "type": "turn_off" } } }
        ],
        "edges": [
            { "from": "motion", "to": "moving" },
            { "from": "moving", "port": "true", "to": "on" },
            { "from": "moving", "port": "false", "to": "wait" },
            { "from": "wait", "to": "off" }
        ]
    })
    .to_string()
}

fn reference_flow() -> Flow {
    Flow::decode(reference_json().as_bytes()).expect("reference flow")
}

async fn next_event(rx: &mut tokio::sync::broadcast::Receiver<WsEvent>) -> WsEvent {
    tokio::time::timeout(Duration::from_secs(1), rx.recv())
        .await
        .expect("an event within 1 s")
        .expect("open channel")
}

fn step(node: &str) -> StepView {
    StepView {
        node: node.to_owned(),
        at: 1,
        outcome: StepOutcome::Ok,
        detail: String::new(),
    }
}

#[tokio::test]
async fn retained_flow_is_listed_and_broadcast() {
    let (hub, _publisher) = hub_with_entities();
    let mut events = hub.subscribe();

    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    match next_event(&mut events).await {
        WsEvent::FlowsChanged { flows } => assert_eq!(flows.len(), 1),
        other => panic!("expected FlowsChanged, got {other:?}"),
    }
    assert_eq!(hub.snapshot().flows.len(), 1);
    assert_eq!(hub.flow_definitions().len(), 1);
    assert!(hub.flow("living").is_some());

    // The same payload again changes nothing.
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    assert!(events.try_recv().is_err());
}

#[tokio::test]
async fn set_flow_publishes_retained_and_lists() {
    let (hub, mut publisher) = hub_with_entities();
    let view = hub.set_flow(reference_flow()).await.expect("set_flow");
    assert_eq!(view.flow.definition.id, "living");

    let publish = publisher.try_recv().expect("a publish");
    assert_eq!(publish.topic, FLOW_TOPIC);
    assert!(publish.retain);
    assert!(hub.flow("living").is_some());
}

#[tokio::test]
async fn set_flow_rejects_unknown_entity_and_bad_command() {
    let (hub, mut publisher) = hub_with_entities();

    let mut flow = reference_flow();
    for node in &mut flow.nodes {
        if let NodeKind::TriggerState(params) = &mut node.kind {
            params.entity = "nope".to_owned();
        }
    }
    match hub.set_flow(flow).await {
        Err(HubError::InvalidFlow(reason)) => assert!(reason.contains("nope"), "{reason}"),
        other => panic!("expected InvalidFlow, got {other:?}"),
    }

    let mut flow = reference_flow();
    for node in &mut flow.nodes {
        if node.id == "off"
            && let NodeKind::Command(params) = &mut node.kind
        {
            params.command = Command::SetPosition { position: 50 };
        }
    }
    match hub.set_flow(flow).await {
        Err(HubError::InvalidFlow(reason)) => assert!(reason.contains("off"), "{reason}"),
        other => panic!("expected InvalidFlow, got {other:?}"),
    }
    assert!(publisher.try_recv().is_err(), "nothing was published");
}

#[tokio::test]
async fn set_flow_refused_while_offline() {
    let (hub, _publisher) = hub_with_entities();
    hub.set_connection(ConnectionStatus::Connecting);
    assert!(matches!(
        hub.set_flow(reference_flow()).await,
        Err(HubError::Publish(_))
    ));
    assert!(hub.flow("living").is_none());
}

#[tokio::test]
async fn set_flow_refuses_alarm_codes() {
    let (hub, mut publisher) = hub_with_entities();
    for command in [
        Command::Arm {
            mode: oikos_core::model::AlarmMode::Away,
            code: Some("1234".to_owned()),
        },
        Command::Disarm {
            code: Some("1234".to_owned()),
        },
    ] {
        let mut flow = reference_flow();
        for node in &mut flow.nodes {
            if let NodeKind::Command(params) = &mut node.kind {
                params.command = command.clone();
            }
        }
        match hub.set_flow(flow).await {
            Err(HubError::InvalidFlow(reason)) => {
                assert!(reason.contains("alarm codes are not allowed"), "{reason}");
            }
            other => panic!("expected InvalidFlow, got {other:?}"),
        }
    }
    assert!(publisher.try_recv().is_err());
}

#[tokio::test]
async fn run_flow_refused_while_offline() {
    let (hub, _publisher) = hub_with_entities();
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    hub.set_connection(ConnectionStatus::Connecting);
    assert_eq!(
        hub.run_flow("living"),
        Err(HubError::Publish("broker offline".to_owned()))
    );
}

#[tokio::test]
async fn delete_flow_publishes_empty_retained() {
    let (hub, mut publisher) = hub_with_entities();
    assert!(matches!(
        hub.delete_flow("living").await,
        Err(HubError::UnknownFlow(_))
    ));
    hub.set_flow(reference_flow()).await.expect("set_flow");
    publisher.try_recv().expect("the definition");

    hub.delete_flow("living").await.expect("delete");
    let publish = publisher.try_recv().expect("a publish");
    assert_eq!(publish.topic, FLOW_TOPIC);
    assert!(publish.payload.is_empty());
    assert!(publish.retain);
    assert!(hub.flow("living").is_none());
}

#[tokio::test]
async fn set_flow_enabled_round_trips() {
    let (hub, mut publisher) = hub_with_entities();
    hub.set_flow(reference_flow()).await.expect("set_flow");
    publisher.try_recv().expect("the definition");

    let view = hub.set_flow_enabled("living", false).await.expect("toggle");
    assert!(!view.definition.enabled);
    let publish = publisher.try_recv().expect("a publish");
    assert!(
        publish.payload.contains("\"enabled\":false"),
        "{}",
        publish.payload
    );

    assert!(matches!(
        hub.set_flow_enabled("ghost", true).await,
        Err(HubError::UnknownFlow(_))
    ));
    hub.ingest("oikos/automation/broken", b"not json");
    assert!(matches!(
        hub.set_flow_enabled("broken", true).await,
        Err(HubError::InvalidFlow(_))
    ));
}

#[tokio::test]
async fn run_flow_rejects_disabled_invalid_unknown_and_active() {
    let (hub, _publisher) = hub_with_entities();
    assert!(matches!(
        hub.run_flow("ghost"),
        Err(HubError::UnknownFlow(_))
    ));

    hub.ingest("oikos/automation/broken", b"not json");
    assert!(matches!(
        hub.run_flow("broken"),
        Err(HubError::InvalidFlow(_))
    ));

    let mut disabled = reference_flow();
    disabled.id = "off_flow".to_owned();
    disabled.enabled = false;
    hub.ingest("oikos/automation/off_flow", disabled.encode().as_bytes());
    assert_eq!(
        hub.run_flow("off_flow"),
        Err(HubError::Rejected("flow is disabled".to_owned()))
    );

    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    let mut requests = hub.take_engine_requests().expect("the receiver");
    let run = hub.run_flow("living").expect("run");
    assert_eq!(run.status, RunStatus::Running);
    assert!(run.trigger.is_none());
    assert_eq!(
        hub.run_flow("living"),
        Err(HubError::Rejected("a run is active".to_owned()))
    );
    assert!(hub.take_engine_requests().is_none());

    // The edit reloaded the engine, then the run was requested.
    assert!(matches!(
        requests.try_recv(),
        Ok(oikos_os::hub::EngineRequest::Reload)
    ));
    let mut saw_run = false;
    while let Ok(request) = requests.try_recv() {
        if let oikos_os::hub::EngineRequest::Run { flow_id, run_id } = request
            && flow_id == "living"
            && run_id == run.id
        {
            saw_run = true;
        }
    }
    assert!(saw_run);
}

#[tokio::test]
async fn run_bookkeeping_broadcasts() {
    let (hub, _publisher) = hub_with_entities();
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    let mut events = hub.subscribe();

    let run = hub.begin_run("living", None).expect("begin");
    match next_event(&mut events).await {
        WsEvent::FlowRunChanged { flow_id, run: seen } => {
            assert_eq!(flow_id, "living");
            assert_eq!(seen.status, RunStatus::Running);
        }
        other => panic!("expected FlowRunChanged, got {other:?}"),
    }
    assert!(matches!(
        hub.begin_run("ghost", None),
        Err(HubError::UnknownFlow(_))
    ));

    hub.record_step("living", &run.id, step("motion"));
    match next_event(&mut events).await {
        WsEvent::FlowRunChanged { run: seen, .. } => assert_eq!(seen.steps.len(), 1),
        other => panic!("expected FlowRunChanged, got {other:?}"),
    }

    hub.end_run("living", &run.id, RunStatus::Done);
    match next_event(&mut events).await {
        WsEvent::FlowRunChanged { run: seen, .. } => {
            assert_eq!(seen.status, RunStatus::Done);
            assert!(seen.ended_at.is_some());
        }
        other => panic!("expected FlowRunChanged, got {other:?}"),
    }
    assert!(hub.flow("living").expect("flow").active_run.is_none());

    let skipped = hub
        .add_skipped_run("living", "motion", None)
        .expect("skipped");
    assert_eq!(skipped.status, RunStatus::Skipped);
    assert!(matches!(
        next_event(&mut events).await,
        WsEvent::FlowRunChanged { .. }
    ));
}

#[tokio::test]
async fn entity_scalar_reads_the_registry() {
    let (hub, _publisher) = hub_with_entities();
    assert!(hub.entity_scalar(MOTION_ID).is_none());
    hub.ingest(MOTION_STATE_TOPIC, br#"{"occupancy": true}"#);
    assert_eq!(
        hub.entity_scalar(MOTION_ID),
        Some(oikos_core::flow::Scalar::Bool(true))
    );
    assert!(hub.entity_scalar("nope").is_none());
    let _ = LED_STATE_TOPIC;
}

#[tokio::test]
async fn nested_automation_topics_are_ignored() {
    let (hub, _publisher) = hub_with_entities();
    hub.ingest("oikos/automation/a/b", reference_json().as_bytes());
    assert!(hub.flows().is_empty());
}

// ---- the engine ----

/// Lets the engine task drain what it has been sent (current-thread runtimes only).
async fn settle() {
    for _ in 0..8 {
        tokio::task::yield_now().await;
    }
}

/// A hub with the entities and a running engine.
fn engine_hub() -> (
    Arc<Hub>,
    mpsc::Receiver<Publish>,
    tokio::task::JoinHandle<()>,
) {
    let (hub, rx) = hub_with_entities();
    let engine = hub.start_engine();
    (hub, rx, engine)
}

/// Plays the broker: records every command published to the LED and echoes the state it asks
/// for, so `send_command` confirms.
fn echo(mut publisher: mpsc::Receiver<Publish>, hub: Arc<Hub>) -> Arc<Mutex<Vec<String>>> {
    let log = Arc::new(Mutex::new(Vec::new()));
    let sink = Arc::clone(&log);
    tokio::spawn(async move {
        while let Some(publish) = publisher.recv().await {
            if publish.topic != LED_COMMAND_TOPIC {
                continue;
            }
            sink.lock().expect("log").push(publish.payload.clone());
            let on = serde_json::from_str::<serde_json::Value>(&publish.payload)
                .is_ok_and(|v| v["state"] == "ON");
            let echo: &[u8] = if on {
                br#"{"state":"ON","brightness":100}"#
            } else {
                br#"{"state":"OFF","brightness":100}"#
            };
            hub.ingest(LED_STATE_TOPIC, echo);
        }
    });
    log
}

fn motion(hub: &Hub, on: bool) {
    let payload = format!(r#"{{"occupancy":{on},"battery":97}}"#);
    hub.ingest(MOTION_STATE_TOPIC, payload.as_bytes());
}

/// The next run event the predicate accepts, within 2 s.
async fn next_run(
    rx: &mut tokio::sync::broadcast::Receiver<WsEvent>,
    accept: impl Fn(&RunView) -> bool,
) -> RunView {
    tokio::time::timeout(Duration::from_secs(2), async {
        loop {
            if let WsEvent::FlowRunChanged { run, .. } = rx.recv().await.expect("open channel")
                && accept(&run)
            {
                return run;
            }
        }
    })
    .await
    .expect("the expected run event within 2 s")
}

async fn next_run_event(
    rx: &mut tokio::sync::broadcast::Receiver<WsEvent>,
    status: RunStatus,
) -> RunView {
    next_run(rx, |run| run.status == status).await
}

/// Asserts no run event arrives for 200 ms.
async fn no_run_event(rx: &mut tokio::sync::broadcast::Receiver<WsEvent>) {
    let quiet = tokio::time::timeout(Duration::from_millis(200), async {
        loop {
            if let Ok(WsEvent::FlowRunChanged { run, .. }) = rx.recv().await {
                return run;
            }
        }
    })
    .await;
    assert!(quiet.is_err(), "unexpected run event: {quiet:?}");
}

/// The reference flow with `edit` applied to its JSON (nodes: 0 motion, 1 moving, 2 on, 3 wait,
/// 4 off).
fn tweaked(edit: impl FnOnce(&mut serde_json::Value)) -> String {
    let mut value: serde_json::Value = serde_json::from_str(&reference_json()).expect("json");
    edit(&mut value);
    value.to_string()
}

fn published(log: &Arc<Mutex<Vec<String>>>) -> Vec<String> {
    log.lock().expect("log").clone()
}

fn step_nodes(run: &RunView) -> Vec<&str> {
    run.steps.iter().map(|s| s.node.as_str()).collect()
}

#[tokio::test]
async fn motion_true_turns_the_led_on() {
    let (hub, publisher, _engine) = engine_hub();
    let log = echo(publisher, Arc::clone(&hub));
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    motion(&hub, false);
    motion(&hub, true);
    let run = next_run_event(&mut events, RunStatus::Done).await;

    assert_eq!(step_nodes(&run), ["motion", "moving", "on"]);
    assert_eq!(run.steps[0].detail, format!("{MOTION_ID} = true"));
    assert_eq!(run.steps[1].detail, "true");
    assert_eq!(run.steps[2].detail, "accepted");
    let trigger = run.trigger.expect("trigger");
    assert_eq!(trigger.value, serde_json::json!(true));
    assert_eq!(trigger.previous, serde_json::json!(false));
    assert!(published(&log)[0].contains("\"state\":\"ON\""), "{log:?}");
}

#[tokio::test(start_paused = true)]
async fn motion_false_turns_off_after_the_delay() {
    let (hub, publisher, _engine) = engine_hub();
    let log = echo(publisher, Arc::clone(&hub));
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    motion(&hub, false);
    motion(&hub, true);
    next_run_event(&mut events, RunStatus::Done).await;
    assert_eq!(published(&log).len(), 1, "only the ON so far");
    motion(&hub, false);
    let waiting = next_run(&mut events, |r| {
        r.status == RunStatus::Running && step_nodes(r) == ["motion", "moving"]
    })
    .await;
    assert_eq!(waiting.steps[1].detail, "false");
    settle().await;

    tokio::time::advance(Duration::from_secs(4 * 60 + 59)).await;
    settle().await;
    assert_eq!(published(&log).len(), 1, "still waiting at 4m59s");
    no_run_event(&mut events).await;
    tokio::time::advance(Duration::from_secs(2)).await;
    let run = next_run_event(&mut events, RunStatus::Done).await;
    assert_eq!(step_nodes(&run), ["motion", "moving", "wait", "off"]);
    assert_eq!(run.steps[2].detail, "waited 5m");
    let log = published(&log);
    assert_eq!(log.len(), 2, "{log:?}");
    assert!(log[1].contains("OFF"), "{log:?}");
}

#[tokio::test(start_paused = true)]
async fn motion_back_cancels_the_pending_off() {
    let (hub, publisher, _engine) = engine_hub();
    let log = echo(publisher, Arc::clone(&hub));
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    motion(&hub, true);
    motion(&hub, false);
    let first = next_run(&mut events, |r| r.steps.len() == 2).await;
    settle().await;
    tokio::time::advance(Duration::from_secs(2 * 60)).await;
    motion(&hub, true);

    let cancelled = next_run_event(&mut events, RunStatus::Cancelled).await;
    assert_eq!(cancelled.id, first.id);
    let second = next_run_event(&mut events, RunStatus::Done).await;
    assert_ne!(second.id, first.id);

    tokio::time::advance(Duration::from_secs(10 * 60)).await;
    settle().await;
    assert!(
        published(&log).iter().all(|p| !p.contains("OFF")),
        "{log:?}"
    );
}

/// The flow is armed while the entities have no value yet (the seed holds nothing); the
/// retained replay that follows must only be recorded.
#[tokio::test]
async fn boot_replay_starts_nothing() {
    let (hub, _publisher, _engine) = engine_hub();
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    hub.ingest(LED_STATE_TOPIC, br#"{"state":"OFF"}"#);
    motion(&hub, true);
    no_run_event(&mut events).await;
}

#[tokio::test]
async fn boot_replay_after_reconnect_starts_nothing() {
    let (hub, _publisher, _engine) = engine_hub();
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    hub.set_connection(ConnectionStatus::Offline {
        reason: "test".to_owned(),
    });
    hub.set_connection(ConnectionStatus::Online);
    hub.ingest(LED_STATE_TOPIC, br#"{"state":"OFF"}"#);
    motion(&hub, true);
    no_run_event(&mut events).await;
}

/// A flow edited between the reconnect and the replay must not re-seed the entities the
/// disconnect made unknown from the registry's stale values.
#[tokio::test]
async fn reload_after_reconnect_does_not_reseed() {
    let (hub, publisher, _engine) = engine_hub();
    let _log = echo(publisher, Arc::clone(&hub));
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    motion(&hub, false);
    hub.set_connection(ConnectionStatus::Offline {
        reason: "test".to_owned(),
    });
    hub.set_connection(ConnectionStatus::Online);
    settle().await;
    let mut flow = reference_flow();
    flow.name = "edited".to_owned();
    hub.ingest(FLOW_TOPIC, flow.encode().as_bytes());
    settle().await;
    motion(&hub, true);
    no_run_event(&mut events).await;

    motion(&hub, false);
    next_run_event(&mut events, RunStatus::Running).await;
}

#[tokio::test]
async fn reconnect_replay_is_silent() {
    let (hub, publisher, _engine) = engine_hub();
    let _log = echo(publisher, Arc::clone(&hub));
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    motion(&hub, false);
    hub.set_connection(ConnectionStatus::Offline {
        reason: "test".to_owned(),
    });
    hub.set_connection(ConnectionStatus::Online);
    motion(&hub, true);
    no_run_event(&mut events).await;

    motion(&hub, false);
    next_run_event(&mut events, RunStatus::Running).await;
}

/// The retained replay after a reconnect is usually byte-identical to the last state, so the
/// registry emits no change for it; the engine must still be armed for the next real change.
#[tokio::test]
async fn identical_replay_then_real_motion_fires() {
    let (hub, publisher, _engine) = engine_hub();
    let _log = echo(publisher, Arc::clone(&hub));
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    motion(&hub, false);
    hub.set_connection(ConnectionStatus::Offline {
        reason: "test".to_owned(),
    });
    hub.set_connection(ConnectionStatus::Online);
    motion(&hub, false);
    no_run_event(&mut events).await;

    motion(&hub, true);
    let run = next_run_event(&mut events, RunStatus::Done).await;
    assert_eq!(step_nodes(&run), ["motion", "moving", "on"]);
    assert_eq!(run.steps[2].detail, "accepted");
}

/// The seed path itself: the entity has no value when the flow is armed, its first report is
/// recorded silently and the next change fires.
#[tokio::test]
async fn lagged_then_identical_report_reseeds() {
    let (hub, publisher, _engine) = engine_hub();
    let _log = echo(publisher, Arc::clone(&hub));
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    motion(&hub, false);
    no_run_event(&mut events).await;

    motion(&hub, true);
    next_run_event(&mut events, RunStatus::Done).await;
}

#[tokio::test]
async fn availability_only_change_does_not_fire() {
    let (hub, _publisher, _engine) = engine_hub();
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    motion(&hub, false);
    hub.ingest("zigbee2mqtt/bridge/state", br#"{"state":"offline"}"#);
    no_run_event(&mut events).await;
}

#[tokio::test]
async fn single_mode_records_a_skipped_run() {
    let (hub, publisher, _engine) = engine_hub();
    let _log = echo(publisher, Arc::clone(&hub));
    hub.ingest(
        FLOW_TOPIC,
        tweaked(|v| v["mode"] = "single".into()).as_bytes(),
    );
    settle().await;
    let mut events = hub.subscribe();

    motion(&hub, false);
    motion(&hub, true);
    next_run_event(&mut events, RunStatus::Done).await;
    motion(&hub, false);
    next_run(&mut events, |r| r.steps.len() == 2).await;
    motion(&hub, true);

    let skipped = next_run_event(&mut events, RunStatus::Skipped).await;
    assert_eq!(skipped.steps[0].detail, "a run is active");
}

/// Starts the reference flow waiting in its delay; returns the event stream and the log.
async fn waiting_flow(
    hub: &Arc<Hub>,
    publisher: mpsc::Receiver<Publish>,
) -> (
    tokio::sync::broadcast::Receiver<WsEvent>,
    Arc<Mutex<Vec<String>>>,
    RunView,
) {
    let log = echo(publisher, Arc::clone(hub));
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    let mut events = hub.subscribe();
    motion(hub, true);
    motion(hub, false);
    let run = next_run(&mut events, |r| r.steps.len() == 2).await;
    settle().await;
    (events, log, run)
}

async fn assert_cancelled_without_off(
    events: &mut tokio::sync::broadcast::Receiver<WsEvent>,
    log: &Arc<Mutex<Vec<String>>>,
    run: &RunView,
) {
    let cancelled = next_run_event(events, RunStatus::Cancelled).await;
    assert_eq!(cancelled.id, run.id);
    tokio::time::advance(Duration::from_secs(10 * 60)).await;
    settle().await;
    assert!(published(log).iter().all(|p| !p.contains("OFF")), "{log:?}");
}

#[tokio::test(start_paused = true)]
async fn disabling_cancels_the_run() {
    let (hub, publisher, _engine) = engine_hub();
    let (mut events, log, run) = waiting_flow(&hub, publisher).await;
    hub.set_flow_enabled("living", false).await.expect("toggle");
    assert_cancelled_without_off(&mut events, &log, &run).await;
}

#[tokio::test(start_paused = true)]
async fn deleting_cancels_the_run() {
    let (hub, publisher, _engine) = engine_hub();
    let (_events, log, _run) = waiting_flow(&hub, publisher).await;
    hub.delete_flow("living").await.expect("delete");
    // The store drops a deleted flow's runs, so there is no `Cancelled` to observe; what
    // matters is that the pending OFF never goes out.
    settle().await;
    tokio::time::advance(Duration::from_secs(10 * 60)).await;
    settle().await;
    assert!(
        published(&log).iter().all(|p| !p.contains("OFF")),
        "{log:?}"
    );
}

#[tokio::test(start_paused = true)]
async fn replacing_cancels_the_run() {
    let (hub, publisher, _engine) = engine_hub();
    let (mut events, log, run) = waiting_flow(&hub, publisher).await;
    let mut flow = reference_flow();
    for node in &mut flow.nodes {
        if let NodeKind::Delay(params) = &mut node.kind {
            params.for_ = "4m".to_owned();
        }
    }
    hub.set_flow(flow).await.expect("set_flow");
    assert_cancelled_without_off(&mut events, &log, &run).await;
}

#[tokio::test]
async fn gt_on_a_bool_fails_the_run() {
    let (hub, publisher, _engine) = engine_hub();
    let _log = echo(publisher, Arc::clone(&hub));
    let flow = tweaked(|v| v["nodes"][1]["params"] = serde_json::json!({"op": "gt", "value": 1}));
    hub.ingest(FLOW_TOPIC, flow.as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    motion(&hub, false);
    motion(&hub, true);
    let run = next_run_event(&mut events, RunStatus::Failed).await;
    let last = run.steps.last().expect("a step");
    assert_eq!(last.node, "moving");
    assert_eq!(last.outcome, StepOutcome::Failed);
    assert_eq!(last.detail, "both sides must be numbers");
}

#[tokio::test]
async fn string_to_filter_never_fires() {
    let (hub, _publisher, _engine) = engine_hub();
    let flow = tweaked(|v| v["nodes"][0]["params"]["to"] = "true".into());
    hub.ingest(FLOW_TOPIC, flow.as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    motion(&hub, false);
    motion(&hub, true);
    no_run_event(&mut events).await;
}

#[tokio::test]
async fn command_failure_ends_the_run_failed() {
    let (hub, _publisher, _engine) = engine_hub();
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    motion(&hub, false);
    motion(&hub, true);
    let run = next_run_event(&mut events, RunStatus::Failed).await;
    let last = run.steps.last().expect("a step");
    assert_eq!(last.node, "on");
    assert_eq!(last.outcome, StepOutcome::Failed);
    assert_eq!(last.detail, "no state change within 50 ms");
}

#[tokio::test]
async fn run_flow_executes_without_trigger_data() {
    let (hub, publisher, _engine) = engine_hub();
    let _log = echo(publisher, Arc::clone(&hub));
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    hub.run_flow("living").expect("run");
    let run = next_run_event(&mut events, RunStatus::Failed).await;
    assert_eq!(run.steps[0].detail, "manual");
    assert_eq!(run.steps[1].detail, "no triggering entity");

    motion(&hub, true);
    let flow = tweaked(|v| v["nodes"][1]["params"]["entity"] = MOTION_ID.into());
    hub.ingest(FLOW_TOPIC, flow.as_bytes());
    settle().await;
    hub.run_flow("living").expect("run");
    let run = next_run_event(&mut events, RunStatus::Done).await;
    assert!(run.trigger.is_none());
    assert_eq!(step_nodes(&run), ["motion", "moving", "on"]);
}

#[tokio::test]
async fn missing_entity_at_run_time_fails_the_step() {
    let (hub, _publisher, _engine) = engine_hub();
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    hub.ingest("oikos/light/0x60a423fffe94d1c6/light/config", b"");
    let mut events = hub.subscribe();

    motion(&hub, false);
    motion(&hub, true);
    let run = next_run_event(&mut events, RunStatus::Failed).await;
    let last = run.steps.last().expect("a step");
    assert_eq!(last.node, "on");
    assert!(last.detail.contains("unknown entity"), "{}", last.detail);
}

#[tokio::test(start_paused = true)]
async fn a_deleted_flow_never_publishes_after_its_delay() {
    let (hub, mut publisher, _engine) = engine_hub();
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    settle().await;
    let mut events = hub.subscribe();
    motion(&hub, true);
    motion(&hub, false);
    next_run(&mut events, |r| r.steps.len() == 2).await;
    settle().await;

    // No settle between the delete and the clock: the delay may wake before the engine has
    // processed the reload that aborts it.
    hub.delete_flow("living").await.expect("delete");
    tokio::time::advance(Duration::from_secs(10 * 60)).await;

    while let Ok(publish) = publisher.try_recv() {
        assert_ne!(publish.topic, LED_COMMAND_TOPIC, "{publish:?}");
    }
}

#[tokio::test(start_paused = true)]
async fn if_without_entity_reads_the_registry() {
    let (hub, publisher, _engine) = engine_hub();
    let _log = echo(publisher, Arc::clone(&hub));
    let flow = tweaked(|v| {
        v["mode"] = "single".into();
        v["nodes"]
            .as_array_mut()
            .expect("nodes")
            .push(serde_json::json!(
                { "id": "pause", "type": "delay", "params": { "for": "1s" } }
            ));
        v["edges"][0] = serde_json::json!({ "from": "motion", "to": "pause" });
        v["edges"]
            .as_array_mut()
            .expect("edges")
            .push(serde_json::json!(
                { "from": "pause", "to": "moving" }
            ));
    });
    hub.ingest(FLOW_TOPIC, flow.as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    motion(&hub, false);
    motion(&hub, true);
    next_run(&mut events, |r| r.steps.len() == 1).await;
    settle().await;
    motion(&hub, false);
    tokio::time::advance(Duration::from_secs(2)).await;

    let run = next_run(&mut events, |r| {
        r.status == RunStatus::Running && step_nodes(r).contains(&"moving")
    })
    .await;
    assert_eq!(run.trigger.expect("trigger").value, serde_json::json!(true));
    assert_eq!(run.steps.last().expect("step").detail, "false");
}

/// A flow that sends `turn_on` to the LED on a cron schedule.
fn schedule_json(enabled: bool) -> String {
    serde_json::json!({
        "id": "tick", "name": "Every ten minutes", "enabled": enabled, "mode": "single",
        "nodes": [
            { "id": "clock", "type": "trigger.schedule", "params": { "cron": "*/10 * * * *" } },
            { "id": "on", "type": "command",
              "params": { "entity": LED_ID, "command": { "type": "turn_on" } } }
        ],
        "edges": [ { "from": "clock", "to": "on" } ]
    })
    .to_string()
}

/// Seconds from now to the next `*/10` tick, as the engine sees it.
fn until_next_ten_minutes() -> Duration {
    let now = chrono::Utc::now().timestamp();
    Duration::from_secs((600 - now.rem_euclid(600)) as u64)
}

#[tokio::test(start_paused = true)]
async fn schedule_fires_each_tick_while_enabled() {
    let (hub, publisher, _engine) = engine_hub();
    let log = echo(publisher, Arc::clone(&hub));
    hub.ingest("oikos/automation/tick", schedule_json(true).as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    tokio::time::advance(until_next_ten_minutes() + Duration::from_secs(1)).await;
    let run = next_run_event(&mut events, RunStatus::Done).await;
    assert_eq!(step_nodes(&run), ["clock", "on"]);
    assert_eq!(run.steps[0].detail, "schedule */10 * * * *");
    assert!(run.trigger.is_none());
    assert_eq!(published(&log).len(), 1);

    tokio::time::advance(Duration::from_secs(10 * 60)).await;
    next_run_event(&mut events, RunStatus::Done).await;
    assert_eq!(published(&log).len(), 2);
}

#[tokio::test(start_paused = true)]
async fn schedule_never_fires_on_enable() {
    let (hub, publisher, _engine) = engine_hub();
    let log = echo(publisher, Arc::clone(&hub));
    hub.ingest("oikos/automation/tick", schedule_json(true).as_bytes());
    settle().await;
    assert!(published(&log).is_empty());
    tokio::time::advance(Duration::from_secs(1)).await;
    settle().await;
    assert!(published(&log).is_empty());
}

#[tokio::test(start_paused = true)]
async fn disabled_schedule_never_fires() {
    let (hub, publisher, _engine) = engine_hub();
    let log = echo(publisher, Arc::clone(&hub));
    hub.ingest("oikos/automation/tick", schedule_json(true).as_bytes());
    settle().await;
    hub.set_flow_enabled("tick", false).await.expect("disable");
    settle().await;
    tokio::time::advance(Duration::from_secs(30 * 60)).await;
    settle().await;
    assert!(published(&log).is_empty());
}

#[tokio::test(start_paused = true)]
async fn tick_while_offline_is_skipped() {
    let (hub, publisher, _engine) = engine_hub();
    let log = echo(publisher, Arc::clone(&hub));
    hub.ingest("oikos/automation/tick", schedule_json(true).as_bytes());
    settle().await;
    let mut events = hub.subscribe();

    hub.set_connection(ConnectionStatus::Offline {
        reason: "test".to_owned(),
    });
    settle().await;
    tokio::time::advance(until_next_ten_minutes() + Duration::from_secs(1)).await;
    let skipped = next_run_event(&mut events, RunStatus::Skipped).await;
    assert_eq!(skipped.steps[0].node, "clock");
    assert!(
        skipped.steps[0].detail.contains("offline"),
        "{:?}",
        skipped.steps[0]
    );
    assert!(published(&log).is_empty());

    hub.set_connection(ConnectionStatus::Online);
    settle().await;
    tokio::time::advance(Duration::from_secs(10 * 60)).await;
    next_run_event(&mut events, RunStatus::Done).await;
    assert_eq!(published(&log).len(), 1);
}

/// A manual-run flow `id`: a state trigger, then `nodes` chained in order (no ports).
fn chain_json(id: &str, enabled: bool, nodes: Vec<serde_json::Value>) -> String {
    let mut all = vec![serde_json::json!(
        { "id": "t", "type": "trigger.state", "params": { "entity": MOTION_ID } }
    )];
    all.extend(nodes);
    let edges: Vec<_> = all
        .windows(2)
        .map(|pair| serde_json::json!({ "from": pair[0]["id"], "to": pair[1]["id"] }))
        .collect();
    serde_json::json!({
        "id": id, "name": id, "enabled": enabled, "mode": "single",
        "nodes": all, "edges": edges
    })
    .to_string()
}

fn turn(id: &str, command: &str) -> serde_json::Value {
    serde_json::json!({ "id": id, "type": "command",
        "params": { "entity": LED_ID, "command": { "type": command } } })
}

#[tokio::test]
async fn flow_enable_enables_another_flow() {
    let (hub, _publisher, _engine) = engine_hub();
    let flow_enable = serde_json::json!({ "id": "e", "type": "flow.enable",
        "params": { "flow": "b", "enabled": true } });
    hub.ingest(
        "oikos/automation/a",
        chain_json("a", true, vec![flow_enable]).as_bytes(),
    );
    hub.ingest(
        "oikos/automation/b",
        chain_json("b", false, vec![turn("on", "turn_on")]).as_bytes(),
    );
    settle().await;
    let mut events = hub.subscribe();
    hub.run_flow("a").expect("run");
    let run = next_run_event(&mut events, RunStatus::Done).await;
    assert_eq!(run.steps.last().expect("step").detail, "enabled b");
    assert!(hub.flow("b").expect("b").definition.enabled);
}

#[tokio::test]
async fn self_disable_keeps_the_running_run() {
    let (hub, publisher, _engine) = engine_hub();
    let log = echo(publisher, Arc::clone(&hub));
    let flow_enable = serde_json::json!({ "id": "e", "type": "flow.enable",
        "params": { "enabled": false } });
    hub.ingest(
        "oikos/automation/a",
        chain_json("a", true, vec![flow_enable, turn("on", "turn_on")]).as_bytes(),
    );
    settle().await;
    let mut events = hub.subscribe();
    hub.run_flow("a").expect("run");
    let run = next_run_event(&mut events, RunStatus::Done).await;
    assert_eq!(step_nodes(&run), ["t", "e", "on"]);
    assert_eq!(run.steps[1].detail, "disabled a");
    assert!(published(&log).iter().any(|p| p.contains("ON")), "{log:?}");
    assert!(!hub.flow("a").expect("a").definition.enabled);
}

fn missing_flow(missing: Option<&str>) -> String {
    let mut params = serde_json::json!({ "op": "eq", "value": "ON", "entity": LED_ID });
    if let Some(missing) = missing {
        params["missing"] = missing.into();
    }
    let check = serde_json::json!({ "id": "chk", "type": "if", "params": params });
    let mut value: serde_json::Value =
        serde_json::from_str(&chain_json("a", true, vec![check, turn("on", "turn_on")]))
            .expect("json");
    value["nodes"]
        .as_array_mut()
        .expect("nodes")
        .push(turn("off", "turn_off"));
    value["edges"] = serde_json::json!([
        { "from": "t", "to": "chk" },
        { "from": "chk", "port": "true", "to": "on" },
        { "from": "chk", "port": "false", "to": "off" }
    ]);
    value.to_string()
}

#[tokio::test]
async fn if_missing_false_takes_false() {
    let (hub, publisher, _engine) = engine_hub();
    let log = echo(publisher, Arc::clone(&hub));
    hub.ingest("oikos/automation/a", missing_flow(Some("false")).as_bytes());
    settle().await;
    let mut events = hub.subscribe();
    hub.run_flow("a").expect("run");
    let run = next_run_event(&mut events, RunStatus::Done).await;
    assert_eq!(run.steps[1].detail, "missing → false");
    let sent = published(&log);
    assert!(sent.len() == 1 && sent[0].contains("OFF"), "{sent:?}");
}

#[tokio::test]
async fn if_missing_default_still_fails() {
    let (hub, publisher, _engine) = engine_hub();
    let log = echo(publisher, Arc::clone(&hub));
    hub.ingest("oikos/automation/a", missing_flow(None).as_bytes());
    settle().await;
    let mut events = hub.subscribe();
    hub.run_flow("a").expect("run");
    // Without `missing` the step behaves exactly as before: `compare` of a missing value is
    // plain false, with no "missing →" note.
    let run = next_run_event(&mut events, RunStatus::Done).await;
    assert_eq!(run.steps[1].detail, "false");
    let sent = published(&log);
    assert!(sent.len() == 1 && sent[0].contains("OFF"), "{sent:?}");
}

#[tokio::test]
async fn set_flow_rejects_unknown_target_flow() {
    let (hub, _publisher) = hub_with_entities();
    let flow_enable = serde_json::json!({ "id": "e", "type": "flow.enable",
        "params": { "flow": "nope", "enabled": true } });
    let flow = Flow::decode(chain_json("a", true, vec![flow_enable]).as_bytes()).expect("flow");
    match hub.set_flow(flow).await {
        Err(HubError::InvalidFlow(reason)) => {
            assert!(reason.contains("unknown flow `nope`"), "{reason}");
        }
        other => panic!("expected InvalidFlow, got {other:?}"),
    }
    // Targeting itself or an existing flow validates.
    let own = serde_json::json!({ "id": "e", "type": "flow.enable",
        "params": { "flow": "a", "enabled": true } });
    let flow = Flow::decode(chain_json("a", true, vec![own]).as_bytes()).expect("flow");
    hub.set_flow(flow).await.expect("self target validates");
}

const BUTTON_ID: &str = "virtual_boost";
const BUTTON_TOPIC: &str = "oikos/virtual/boost";

fn button_flow(trigger_params: serde_json::Value) -> Flow {
    Flow::decode(
        serde_json::json!({
            "id": "boost", "name": "Boost", "enabled": true, "mode": "single",
            "nodes": [
                { "id": "press", "type": "trigger.state", "params": trigger_params },
                { "id": "on", "type": "command",
                  "params": { "entity": LED_ID, "command": { "type": "turn_on" } } }
            ],
            "edges": [{ "from": "press", "to": "on" }]
        })
        .to_string()
        .as_bytes(),
    )
    .expect("button flow")
}

async fn hub_with_button() -> (
    Arc<Hub>,
    mpsc::Receiver<Publish>,
    tokio::task::JoinHandle<()>,
) {
    let (hub, publisher, engine) = engine_hub();
    let spec = oikos_api::VirtualSpec {
        kind: oikos_api::VirtualKind::Button,
        name: "Boost".to_owned(),
        min: None,
        max: None,
        step: None,
        unit: None,
        initial: None,
    };
    hub.set_virtual("boost", spec).await.expect("button");
    (hub, publisher, engine)
}

#[tokio::test]
async fn button_press_fires_its_flow() {
    let (hub, publisher, _engine) = hub_with_button().await;
    let log = echo(publisher, Arc::clone(&hub));
    hub.set_flow(button_flow(serde_json::json!({ "entity": BUTTON_ID })))
        .await
        .expect("set_flow");
    settle().await;
    assert!(hub.subscription_topics().contains(BUTTON_TOPIC));
    let mut events = hub.subscribe();

    hub.ingest_message(BUTTON_TOPIC, b"PRESS", false);
    let run = next_run_event(&mut events, RunStatus::Done).await;

    assert_eq!(step_nodes(&run), ["press", "on"]);
    assert_eq!(published(&log).len(), 1, "{log:?}");
}

#[tokio::test]
async fn retained_button_message_never_fires() {
    let (hub, publisher, _engine) = hub_with_button().await;
    let log = echo(publisher, Arc::clone(&hub));
    hub.set_flow(button_flow(serde_json::json!({ "entity": BUTTON_ID })))
        .await
        .expect("set_flow");
    settle().await;
    let mut events = hub.subscribe();

    hub.ingest_message(BUTTON_TOPIC, b"PRESS", true);
    settle().await;

    assert!(published(&log).is_empty(), "{log:?}");
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(
                event,
                WsEvent::ButtonPressed { .. } | WsEvent::FlowRunChanged { .. }
            ),
            "{event:?}"
        );
    }
}

#[tokio::test]
async fn button_trigger_refuses_to_filter() {
    let (hub, _publisher, _engine) = hub_with_button().await;
    for params in [
        serde_json::json!({ "entity": BUTTON_ID, "to": "PRESS" }),
        serde_json::json!({ "entity": BUTTON_ID, "from": "PRESS" }),
    ] {
        match hub.set_flow(button_flow(params)).await {
            Err(HubError::InvalidFlow(reason)) => {
                assert!(
                    reason.contains("a button trigger takes no to/from"),
                    "{reason}"
                );
            }
            other => panic!("expected InvalidFlow, got {other:?}"),
        }
    }
}

/// Whether the MQTT task's resubscribe wake-up fired (consuming it).
async fn woken(wakeup: &tokio::sync::Notify) -> bool {
    tokio::time::timeout(Duration::from_millis(50), wakeup.notified())
        .await
        .is_ok()
}

#[tokio::test]
async fn local_flow_writes_wake_the_resubscribe() {
    let (hub, _publisher, _engine) = hub_with_button().await;
    let wakeup = hub.subscriptions_wakeup();
    assert!(!woken(&wakeup).await, "nothing watches the button yet");

    let flow = || button_flow(serde_json::json!({ "entity": BUTTON_ID }));
    hub.set_flow(flow()).await.expect("set_flow");
    assert!(woken(&wakeup).await, "a new watched button must wake it");

    // The same flow again, and the broker's echo of it, change nothing.
    hub.set_flow(flow()).await.expect("set_flow again");
    hub.ingest("oikos/automation/boost", flow().encode().as_bytes());
    assert!(!woken(&wakeup).await, "an unchanged set must not wake it");

    hub.delete_flow("boost").await.expect("delete");
    assert!(woken(&wakeup).await, "a dropped watch must wake it");
    assert!(!hub.subscription_topics().contains(BUTTON_TOPIC));
}

const REMOTE_ID: &str = "device_automation.0xd4fe12345";
const REMOTE_TOPIC: &str = "zigbee2mqtt/rooms/hall/remote/action";

/// Teaches the hub a remote with the `on` and `off` actions.
fn add_remote(hub: &Hub) {
    for payload in ["on", "off"] {
        let config = serde_json::json!({
            "automation_type": "trigger",
            "topic": REMOTE_TOPIC,
            "payload": payload,
            "type": "action",
            "subtype": payload,
            "device": {"identifiers": ["zigbee2mqtt_0xd4fe12345"], "name": "rooms/hall/remote"}
        });
        hub.ingest(
            &format!("oikos/device_automation/0xd4fe12345/action_{payload}/config"),
            config.to_string().as_bytes(),
        );
    }
}

fn signal_flow(trigger_params: serde_json::Value) -> Flow {
    Flow::decode(
        serde_json::json!({
            "id": "remote", "name": "Remote", "enabled": true, "mode": "single",
            "nodes": [
                { "id": "press", "type": "trigger.signal", "params": trigger_params },
                { "id": "on", "type": "command",
                  "params": { "entity": LED_ID, "command": { "type": "turn_on" } } }
            ],
            "edges": [{ "from": "press", "to": "on" }]
        })
        .to_string()
        .as_bytes(),
    )
    .expect("signal flow")
}

#[tokio::test]
async fn signal_fires_matching_trigger_only() {
    let (hub, publisher, _engine) = engine_hub();
    add_remote(&hub);
    let log = echo(publisher, Arc::clone(&hub));
    hub.set_flow(signal_flow(
        serde_json::json!({ "entity": REMOTE_ID, "signal": "on" }),
    ))
    .await
    .expect("set_flow");
    settle().await;
    assert!(hub.subscription_topics().contains(REMOTE_TOPIC));
    let mut events = hub.subscribe();

    hub.ingest_message(REMOTE_TOPIC, b"off", false);
    no_run_event(&mut events).await;
    assert!(published(&log).is_empty(), "{log:?}");

    hub.ingest_message(REMOTE_TOPIC, b"on", false);
    let run = next_run_event(&mut events, RunStatus::Done).await;
    assert_eq!(step_nodes(&run), ["press", "on"]);
    assert_eq!(published(&log).len(), 1, "{log:?}");
}

#[tokio::test]
async fn retained_signal_never_fires_a_flow() {
    let (hub, publisher, _engine) = engine_hub();
    add_remote(&hub);
    let log = echo(publisher, Arc::clone(&hub));
    hub.set_flow(signal_flow(serde_json::json!({ "entity": REMOTE_ID })))
        .await
        .expect("set_flow");
    settle().await;
    let mut events = hub.subscribe();

    hub.ingest_message(REMOTE_TOPIC, b"on", true);
    settle().await;

    assert!(published(&log).is_empty(), "{log:?}");
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(
                event,
                WsEvent::SignalReceived { .. } | WsEvent::FlowRunChanged { .. }
            ),
            "{event:?}"
        );
    }
}

#[tokio::test]
async fn signal_trigger_rejects_unannounced_signal() {
    let (hub, _publisher, _engine) = engine_hub();
    add_remote(&hub);
    match hub
        .set_flow(signal_flow(
            serde_json::json!({ "entity": REMOTE_ID, "signal": "warp" }),
        ))
        .await
    {
        Err(HubError::InvalidFlow(reason)) => assert!(reason.contains("warp"), "{reason}"),
        other => panic!("expected InvalidFlow, got {other:?}"),
    }
    // A card entity is no signal source.
    match hub
        .set_flow(signal_flow(serde_json::json!({ "entity": LED_ID })))
        .await
    {
        Err(HubError::InvalidFlow(_)) => {}
        other => panic!("expected InvalidFlow, got {other:?}"),
    }
    hub.set_flow(signal_flow(
        serde_json::json!({ "entity": REMOTE_ID, "signal": "off" }),
    ))
    .await
    .expect("announced signal validates");
}

#[tokio::test]
async fn state_trigger_on_a_remote_is_rejected() {
    let (hub, _publisher, _engine) = engine_hub();
    add_remote(&hub);
    let flow = button_flow(serde_json::json!({ "entity": REMOTE_ID }));
    match hub.set_flow(flow).await {
        Err(HubError::InvalidFlow(reason)) => {
            assert!(reason.contains("trigger.signal"), "{reason}");
        }
        other => panic!("expected InvalidFlow, got {other:?}"),
    }
}

#[tokio::test]
async fn repeated_signal_records_last_and_at_every_time() {
    let (hub, _publisher) = hub_with_entities();
    add_remote(&hub);
    let mut events = hub.subscribe();

    hub.ingest_message(REMOTE_TOPIC, b"on", false);
    hub.ingest_message(REMOTE_TOPIC, b"on", false);

    let (mut changed, mut signals) = (0, 0);
    while let Ok(event) = events.try_recv() {
        match event {
            WsEvent::StateChanged { id, .. } if id == REMOTE_ID => changed += 1,
            WsEvent::SignalReceived { .. } => signals += 1,
            _ => {}
        }
    }
    assert_eq!((changed, signals), (2, 2));
    let state = remote_state(&hub);
    let oikos_core::model::StateValue::Generic(generic) = state.value else {
        panic!("not generic: {state:?}");
    };
    assert_eq!(
        generic.fields["last"],
        oikos_core::model::GenericValue::Text("on".to_owned())
    );
    assert!(matches!(
        generic.fields["at"],
        oikos_core::model::GenericValue::Number(_)
    ));

    // A retained replay neither signals nor touches the state.
    let before = generic.fields["at"].clone();
    hub.ingest_message(REMOTE_TOPIC, b"on", true);
    while let Ok(event) = events.try_recv() {
        assert!(
            !matches!(event, WsEvent::SignalReceived { .. }),
            "{event:?}"
        );
    }
    let state = remote_state(&hub);
    let oikos_core::model::StateValue::Generic(generic) = state.value else {
        panic!()
    };
    assert_eq!(generic.fields["at"], before);
}

fn remote_state(hub: &Hub) -> oikos_core::model::EntityState {
    hub.snapshot()
        .entities
        .into_iter()
        .find(|e| e.id == REMOTE_ID)
        .map(|e| e.state)
        .expect("remote state")
}

async fn with_living_area(hub: &Hub) {
    hub.set_areas(AreasDocument {
        areas: vec![Area {
            id: "living".into(),
            name: "Living room".into(),
            ..Area::default()
        }],
    })
    .await
    .expect("set_areas");
}

fn in_area(area: &str) -> Flow {
    let mut flow = reference_flow();
    flow.area = Some(area.to_owned());
    flow
}

#[tokio::test]
async fn set_flow_keeps_known_area() {
    let (hub, mut publisher) = hub_with_entities();
    with_living_area(&hub).await;
    while publisher.try_recv().is_ok() {}
    let outcome = hub.set_flow(in_area("living")).await.expect("set_flow");
    assert_eq!(outcome.flow.definition.area.as_deref(), Some("living"));
    let publish = publisher.try_recv().expect("the definition");
    assert!(
        publish.payload.contains("\"area\":\"living\""),
        "{}",
        publish.payload
    );
}

#[tokio::test]
async fn area_name_resolves_to_id() {
    let (hub, _publisher) = hub_with_entities();
    with_living_area(&hub).await;
    let outcome = hub
        .set_flow(in_area("Living room"))
        .await
        .expect("set_flow");
    assert_eq!(outcome.flow.definition.area.as_deref(), Some("living"));
}

#[tokio::test]
async fn unknown_area_is_rejected() {
    let (hub, _publisher) = hub_with_entities();
    with_living_area(&hub).await;
    match hub.set_flow(in_area("ghost")).await {
        Err(HubError::InvalidFlow(reason)) => assert!(reason.contains("unknown area"), "{reason}"),
        other => panic!("expected InvalidFlow, got {other:?}"),
    }
}

#[tokio::test]
async fn unassigned_is_not_a_flow_area() {
    let (hub, _publisher) = hub_with_entities();
    with_living_area(&hub).await;
    assert!(matches!(
        hub.set_flow(in_area("Unassigned")).await,
        Err(HubError::InvalidFlow(_))
    ));
}

#[tokio::test]
async fn stale_area_is_kept_on_resave() {
    let (hub, _publisher) = hub_with_entities();
    with_living_area(&hub).await;
    hub.set_flow(in_area("living")).await.expect("first save");
    hub.set_areas(AreasDocument::default())
        .await
        .expect("set_areas");
    let outcome = hub
        .set_flow(in_area("living"))
        .await
        .expect("resave keeps the stale area");
    assert_eq!(outcome.flow.definition.area.as_deref(), Some("living"));
    hub.set_flow_enabled("living", false).await.expect("toggle");
}

#[tokio::test]
async fn set_flow_area_moves_and_clears() {
    let (hub, mut publisher) = hub_with_entities();
    with_living_area(&hub).await;
    hub.set_flow(reference_flow()).await.expect("set_flow");
    while publisher.try_recv().is_ok() {}

    let view = hub
        .set_flow_area("living", Some("living".into()))
        .await
        .expect("move");
    assert_eq!(view.definition.area.as_deref(), Some("living"));
    publisher.try_recv().expect("a publish");

    let view = hub.set_flow_area("living", None).await.expect("clear");
    assert_eq!(view.definition.area, None);
    let publish = publisher.try_recv().expect("a publish");
    assert!(!publish.payload.contains("\"area\""), "{}", publish.payload);

    assert!(matches!(
        hub.set_flow_area("living", Some("ghost".into())).await,
        Err(HubError::InvalidFlow(_))
    ));
    assert!(matches!(
        hub.set_flow_area("nobody", None).await,
        Err(HubError::UnknownFlow(_))
    ));
    hub.ingest("oikos/automation/broken", b"not json");
    assert!(matches!(
        hub.set_flow_area("broken", None).await,
        Err(HubError::InvalidFlow(_))
    ));
}

#[tokio::test]
async fn retained_unknown_area_loads_valid() {
    let (hub, _publisher) = hub_with_entities();
    let mut value: serde_json::Value = serde_json::from_str(&reference_json()).unwrap();
    value["area"] = "ghost".into();
    hub.ingest(FLOW_TOPIC, value.to_string().as_bytes());
    let view = hub.flow("living").expect("flow");
    assert!(view.invalid.is_none(), "{:?}", view.invalid);
}
