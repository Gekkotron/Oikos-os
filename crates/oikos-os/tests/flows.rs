//! Flows on the hub, driven without a broker: `ingest` feeds it, the publisher channel is
//! inspected.

use std::path::PathBuf;
use std::sync::Arc;
use std::time::Duration;

use oikos_api::{ConnectionStatus, RunStatus, StepOutcome, StepView, WsEvent};
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
