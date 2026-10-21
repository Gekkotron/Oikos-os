//! The flow document: nodes, edges and the lenient JSON form stored on the broker.

use std::collections::BTreeMap;

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::model::Command;
use crate::model::ui_override::strip_nulls;

/// What a trigger does while a run of the same flow is still active.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum FlowMode {
    /// The new trigger is ignored and recorded as a skipped run.
    #[default]
    Single,
    /// The active run is cancelled and a new one starts ("N minutes after the last motion").
    Restart,
}

/// A whole automation. Unknown keys are kept in [`Flow::extra`] so a newer writer's fields survive.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Flow {
    /// `[a-z0-9_-]{1,64}`, the last topic segment.
    pub id: String,
    /// Free text shown in the UI.
    pub name: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub description: Option<String>,
    /// A disabled flow keeps its definition but never runs. Defaults to `true`.
    #[serde(default = "default_true")]
    pub enabled: bool,
    /// `single` (the default) ignores a trigger while a run is active; `restart` cancels the
    /// active run and starts a new one, which is how to express "N minutes after the last event".
    #[serde(default)]
    pub mode: FlowMode,
    /// The steps; the trigger nodes (`trigger.state`, `trigger.schedule`) start a run.
    #[serde(default)]
    pub nodes: Vec<Node>,
    /// The connections between nodes, followed from a trigger onwards.
    #[serde(default)]
    pub edges: Vec<Edge>,
    /// Fields this hub does not know, preserved verbatim.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

impl Default for Flow {
    fn default() -> Self {
        Self {
            id: String::new(),
            name: String::new(),
            description: None,
            enabled: true,
            mode: FlowMode::default(),
            nodes: Vec::new(),
            edges: Vec::new(),
            extra: BTreeMap::new(),
        }
    }
}

/// True when the flow reads the sun: it has a `trigger.sun` node or an `if` with `op: is_dark`.
pub fn uses_sun_or_is_dark(flow: &Flow) -> bool {
    flow.nodes.iter().any(|node| match &node.kind {
        NodeKind::TriggerSun(_) => true,
        NodeKind::If(params) => params.op == Op::IsDark,
        _ => false,
    })
}

impl Flow {
    /// Decodes a retained payload leniently: nulls read as absent (at the top level, on each node
    /// and in each node's params), unknown keys are kept. The error is serde's message.
    pub fn decode(payload: &[u8]) -> Result<Flow, String> {
        let mut value: Value = serde_json::from_slice(payload).map_err(|e| e.to_string())?;
        let Some(object) = value.as_object_mut() else {
            return Err("a flow must be a JSON object".to_owned());
        };
        strip_nulls(object);
        if let Some(Value::Array(nodes)) = object.get_mut("nodes") {
            for node in nodes {
                if let Some(node) = node.as_object_mut() {
                    strip_nulls(node);
                    if let Some(Value::Object(params)) = node.get_mut("params") {
                        strip_nulls(params);
                    }
                }
            }
        }
        serde_json::from_value(value).map_err(|e| e.to_string())
    }

    /// Encodes as compact JSON; `{}` only if serialisation somehow fails.
    pub fn encode(&self) -> String {
        serde_json::to_string(self).unwrap_or_else(|_| "{}".to_owned())
    }
}

/// One step of a flow. On the wire: `{"id", "type", "params", "name"?, "position"?}`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Node {
    /// Unique within the flow; edges refer to it.
    pub id: String,
    /// Label shown on the canvas.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Canvas placement; the hub ignores it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub position: Option<Position>,
    #[serde(flatten)]
    pub kind: NodeKind,
    /// Fields this hub does not know, preserved verbatim.
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

/// Canvas coordinates of a node.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Position {
    pub x: f64,
    pub y: f64,
}

/// The node types, tagged by `type` with their `params`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(tag = "type", content = "params")]
pub enum NodeKind {
    /// Starts a run when an entity's state changes.
    #[serde(rename = "trigger.state")]
    TriggerState(TriggerStateParams),
    /// Starts a run on a cron schedule (five fields, local time).
    #[serde(rename = "trigger.schedule")]
    TriggerSchedule(TriggerScheduleParams),
    /// Starts a run at a solar event (sunrise, sunset, dusk or dawn), with an optional offset.
    #[serde(rename = "trigger.sun")]
    TriggerSun(TriggerSunParams),
    /// Branches on a comparison of an entity (or the trigger value) with a constant.
    #[serde(rename = "if")]
    If(IfParams),
    /// Waits before continuing.
    #[serde(rename = "delay")]
    Delay(DelayParams),
    /// Sends a command to an entity.
    #[serde(rename = "command")]
    Command(CommandParams),
    /// Enables or disables a flow (its own by default).
    #[serde(rename = "flow.enable")]
    FlowEnable(FlowEnableParams),
}

impl NodeKind {
    /// The wire tag of this node type.
    pub fn type_name(&self) -> &'static str {
        match self {
            NodeKind::TriggerState(_) => "trigger.state",
            NodeKind::TriggerSchedule(_) => "trigger.schedule",
            NodeKind::TriggerSun(_) => "trigger.sun",
            NodeKind::If(_) => "if",
            NodeKind::Delay(_) => "delay",
            NodeKind::Command(_) => "command",
            NodeKind::FlowEnable(_) => "flow.enable",
        }
    }

    /// Whether this node type starts a run.
    pub fn is_trigger(&self) -> bool {
        matches!(
            self,
            NodeKind::TriggerState(_) | NodeKind::TriggerSchedule(_) | NodeKind::TriggerSun(_)
        )
    }

    /// The named output ports: `true` and `false` for `if`, none for the others (which have one
    /// unnamed output).
    pub fn ports(&self) -> &'static [&'static str] {
        match self {
            NodeKind::If(_) => &["true", "false"],
            _ => &[],
        }
    }
}

/// Params of `trigger.state`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct TriggerStateParams {
    /// Entity id (from `search` / `list_entities`).
    pub entity: String,
    /// Only trigger when the new value equals this (a JSON value such as `true`, `"ON"`, `21.5`).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub to: Option<Value>,
    /// Only trigger when the previous value equalled this.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub from: Option<Value>,
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

/// Params of `trigger.schedule`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct TriggerScheduleParams {
    /// Five-field cron expression: minute hour day-of-month month day-of-week.
    pub cron: String,
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

/// Which solar event a `trigger.sun` fires on.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum SunWhich {
    Sunrise,
    #[default]
    Sunset,
    Dusk,
    Dawn,
}

/// Params of `trigger.sun`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct TriggerSunParams {
    /// The solar event to fire on.
    pub which: SunWhich,
    /// Minutes added to the event time (negative fires earlier); absent means 0.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub offset_minutes: Option<i32>,
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

impl TriggerSunParams {
    /// The offset in minutes, 0 when absent.
    pub fn offset(&self) -> i32 {
        self.offset_minutes.unwrap_or(0)
    }
}

/// Params of `if`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct IfParams {
    /// How the value is compared with `value`: `eq`, `ne`, `gt`, `lt`, `gte` or `lte`.
    pub op: Op,
    /// The constant the value is compared with.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub value: Option<Value>,
    /// The entity whose current value is read; when absent the value that fired the trigger.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub entity: Option<String>,
    /// What to do when the value is missing (unknown or unavailable); absent behaves as `fail`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub missing: Option<Missing>,
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

/// How an `if` behaves when the compared entity has no value.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Missing {
    /// The step fails (the default).
    Fail,
    /// Take the `true` port.
    True,
    /// Take the `false` port.
    False,
}

/// Params of `flow.enable`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct FlowEnableParams {
    /// The flow to switch; absent means the flow this node belongs to.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub flow: Option<String>,
    /// The state to set: `true` enables, `false` disables.
    pub enabled: bool,
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

/// Params of `delay`.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct DelayParams {
    /// A duration such as `90s`, `5m` or `1h30m`.
    #[serde(rename = "for")]
    pub for_: String,
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

/// Params of `command`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct CommandParams {
    /// The entity to command (an id from `search` / `list_entities`).
    pub entity: String,
    /// The command, the same JSON as `send_command`'s, e.g. `{"type": "turn_on"}`.
    pub command: Command,
    #[serde(flatten, skip_serializing_if = "BTreeMap::is_empty")]
    pub extra: BTreeMap<String, Value>,
}

/// A connection from a node's output to another node; `port` picks an `if` branch.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
pub struct Edge {
    /// The id of the source node.
    pub from: String,
    /// The id of the node that runs next.
    pub to: String,
    /// `true` or `false` for an edge leaving an `if`, which requires it; absent for every
    /// other node.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub port: Option<String>,
}

/// Comparison operators of the `if` node.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[cfg_attr(feature = "schemars", derive(schemars::JsonSchema))]
#[serde(rename_all = "snake_case")]
pub enum Op {
    Eq,
    Ne,
    Gt,
    Lt,
    Gte,
    Lte,
    IsDark,
}

impl Op {
    /// The lowercase name this operator carries in the flow JSON.
    pub fn wire_name(&self) -> &'static str {
        match self {
            Op::Eq => "eq",
            Op::Ne => "ne",
            Op::Gt => "gt",
            Op::Lt => "lt",
            Op::Gte => "gte",
            Op::Lte => "lte",
            Op::IsDark => "is_dark",
        }
    }
}

fn default_true() -> bool {
    true
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    #[test]
    fn flow_enable_defaults_to_self() {
        let node: Node = serde_json::from_str(
            r#"{"id":"stop","type":"flow.enable","params":{"enabled":false}}"#,
        )
        .unwrap();
        let NodeKind::FlowEnable(p) = node.kind else {
            panic!()
        };
        assert_eq!((p.flow, p.enabled), (None, false));
    }

    #[test]
    fn trigger_sun_round_trips_with_offset_and_extra() {
        let node: Node = serde_json::from_str(
            r#"{"id":"ev","type":"trigger.sun","params":{"which":"sunset","offset_minutes":-30,"note":"n"}}"#,
        )
        .unwrap();
        assert!(node.kind.is_trigger());
        assert_eq!(node.kind.type_name(), "trigger.sun");
        match &node.kind {
            NodeKind::TriggerSun(p) => {
                assert_eq!(p.which, SunWhich::Sunset);
                assert_eq!(p.offset(), -30);
                assert_eq!(p.extra.get("note"), Some(&json!("n")));
            }
            other => panic!("expected trigger.sun, got {other:?}"),
        }
        let again: Node = serde_json::from_value(serde_json::to_value(&node).unwrap()).unwrap();
        assert_eq!(again, node);
    }

    #[test]
    fn trigger_sun_offset_defaults_to_zero_and_is_omitted_from_wire() {
        let node: Node =
            serde_json::from_str(r#"{"id":"dawn","type":"trigger.sun","params":{"which":"dawn"}}"#)
                .unwrap();
        match &node.kind {
            NodeKind::TriggerSun(p) => assert_eq!(p.offset(), 0),
            other => panic!("{other:?}"),
        }
        let wire = serde_json::to_value(&node).unwrap();
        assert!(wire["params"].get("offset_minutes").is_none());
    }

    #[test]
    fn if_is_dark_round_trips_without_value_entity_or_missing() {
        let node: Node =
            serde_json::from_str(r#"{"id":"dark","type":"if","params":{"op":"is_dark"}}"#).unwrap();
        match &node.kind {
            NodeKind::If(p) => {
                assert_eq!(p.op, Op::IsDark);
                assert!(p.value.is_none());
                assert!(p.entity.is_none());
                assert!(p.missing.is_none());
            }
            other => panic!("{other:?}"),
        }
        let wire = serde_json::to_value(&node).unwrap();
        assert_eq!(wire["params"]["op"], json!("is_dark"));
        assert!(wire["params"].get("value").is_none());
    }

    #[test]
    fn existing_if_round_trips_with_value_present() {
        let p: IfParams = serde_json::from_str(r#"{"op":"lt","value":65,"entity":"t"}"#).unwrap();
        assert_eq!(p.value, Some(json!(65)));
        assert_eq!(p.op, Op::Lt);
    }

    #[test]
    fn if_missing_round_trips_and_defaults() {
        let p: IfParams =
            serde_json::from_str(r#"{"op":"lt","value":65,"entity":"t","missing":"false"}"#)
                .unwrap();
        assert_eq!(p.missing, Some(Missing::False));
        let p: IfParams = serde_json::from_str(r#"{"op":"lt","value":65}"#).unwrap();
        assert_eq!(p.missing, None);
        assert!(serde_json::to_value(&p).unwrap().get("missing").is_none());
    }

    const REFERENCE: &str = r#"{
      "id": "living_led_motion", "name": "Living LED on motion", "enabled": true, "mode": "restart",
      "nodes": [
        { "id": "motion", "type": "trigger.state", "name": "Motion",
          "params": { "entity": "0x00158d00044c4d4c_occupancy_zigbee2mqtt" } },
        { "id": "moving", "type": "if", "params": { "op": "eq", "value": true } },
        { "id": "on", "type": "command",
          "params": { "entity": "0x9035eafffec07012_light_zigbee2mqtt", "command": { "type": "turn_on" } } },
        { "id": "wait", "type": "delay", "params": { "for": "5m" } },
        { "id": "off", "type": "command",
          "params": { "entity": "0x9035eafffec07012_light_zigbee2mqtt", "command": { "type": "turn_off" } } }
      ],
      "edges": [
        { "from": "motion", "to": "moving" },
        { "from": "moving", "port": "true", "to": "on" },
        { "from": "moving", "port": "false", "to": "wait" },
        { "from": "wait", "to": "off" }
      ]
    }"#;

    #[test]
    fn schedule_node_round_trips() {
        let node: Node = serde_json::from_str(
            r#"{"id":"tick","type":"trigger.schedule","params":{"cron":"*/10 * * * *","x":1}}"#,
        )
        .unwrap();
        assert!(node.kind.is_trigger());
        assert_eq!(node.kind.type_name(), "trigger.schedule");
        assert_eq!(serde_json::to_value(&node).unwrap()["params"]["x"], 1);
    }

    #[test]
    fn decodes_the_reference_flow() {
        let flow = Flow::decode(REFERENCE.as_bytes()).unwrap();
        assert_eq!(flow.nodes.len(), 5);
        assert_eq!(flow.edges.len(), 4);
        assert_eq!(flow.mode, FlowMode::Restart);
        assert!(flow.enabled);
        assert_eq!(flow.nodes[0].name.as_deref(), Some("Motion"));
        match &flow.nodes[1].kind {
            NodeKind::If(p) => {
                assert_eq!(p.op, Op::Eq);
                assert_eq!(p.value, Some(json!(true)));
                assert_eq!(p.entity, None);
            }
            other => panic!("expected if, got {other:?}"),
        }
        match &flow.nodes[3].kind {
            NodeKind::Delay(p) => assert_eq!(p.for_, "5m"),
            other => panic!("expected delay, got {other:?}"),
        }
        assert_eq!(flow.edges[1].port.as_deref(), Some("true"));
        // Wire shape survives a round trip.
        assert_eq!(Flow::decode(flow.encode().as_bytes()).unwrap(), flow);
        let wire: Value = serde_json::from_str(&flow.encode()).unwrap();
        assert_eq!(wire["nodes"][3]["params"]["for"], json!("5m"));
        assert_eq!(wire["nodes"][3]["type"], json!("delay"));
    }

    #[test]
    fn enabled_defaults_true_and_mode_defaults_single() {
        let flow = Flow::decode(br#"{"id":"a","name":"A"}"#).unwrap();
        assert!(flow.enabled);
        assert_eq!(flow.mode, FlowMode::Single);
        assert!(flow.nodes.is_empty() && flow.edges.is_empty());
        assert!(Flow::default().enabled);
        let nulls = Flow::decode(br#"{"id":"a","name":"A","enabled":null,"mode":null}"#).unwrap();
        assert!(nulls.enabled);
    }

    #[test]
    fn unknown_keys_round_trip() {
        let payload = br#"{"id":"a","name":"A","colour":"red","nodes":[
            {"id":"w","type":"delay","params":{"for":"1m","note":"x"},"tag":1}]}"#;
        let flow = Flow::decode(payload).unwrap();
        assert_eq!(flow.extra.get("colour"), Some(&json!("red")));
        assert_eq!(flow.nodes[0].extra.get("tag"), Some(&json!(1)));
        let again = Flow::decode(flow.encode().as_bytes()).unwrap();
        assert_eq!(again, flow);
        match &again.nodes[0].kind {
            NodeKind::Delay(p) => assert_eq!(p.extra.get("note"), Some(&json!("x"))),
            other => panic!("expected delay, got {other:?}"),
        }
    }

    #[test]
    fn unknown_node_type_is_an_error() {
        let payload =
            br#"{"id":"a","name":"A","nodes":[{"id":"t","type":"trigger.time","params":{}}]}"#;
        let err = Flow::decode(payload).unwrap_err();
        assert!(err.contains("trigger.time"), "{err}");
    }

    #[test]
    fn ports_of_if_are_true_false_and_others_have_none() {
        let flow = Flow::decode(REFERENCE.as_bytes()).unwrap();
        assert_eq!(flow.nodes[1].kind.ports(), &["true", "false"]);
        assert_eq!(flow.nodes[1].kind.type_name(), "if");
        for i in [0, 2, 3, 4] {
            assert!(flow.nodes[i].kind.ports().is_empty());
        }
        assert_eq!(flow.nodes[0].kind.type_name(), "trigger.state");
    }
}
