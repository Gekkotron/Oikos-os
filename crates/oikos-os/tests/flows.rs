//! Flows on the hub, driven without a broker: `ingest` feeds it, the publisher channel is
//! inspected.

use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use oikos_api::{ConnectionStatus, RunStatus, RunView, StepOutcome, StepView, WsEvent};
use oikos_core::command::Publish;
use oikos_core::flow::{Flow, NodeKind};
use oikos_core::model::Command;
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
    assert_eq!(view.definition.id, "living");

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
    motion(&hub, false);
    let waiting = next_run(&mut events, |r| {
        r.status == RunStatus::Running && step_nodes(r) == ["motion", "moving"]
    })
    .await;
    assert_eq!(waiting.steps[1].detail, "false");
    settle().await;

    tokio::time::advance(Duration::from_secs(5 * 60 + 1)).await;
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

#[tokio::test]
async fn boot_replay_starts_nothing() {
    let (hub, _publisher, _engine) = engine_hub();
    let mut events = hub.subscribe();
    hub.ingest(FLOW_TOPIC, reference_json().as_bytes());
    hub.ingest(LED_STATE_TOPIC, br#"{"state":"OFF"}"#);
    motion(&hub, true);
    no_run_event(&mut events).await;
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
