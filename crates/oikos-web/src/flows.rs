//! Pure helpers behind the flow screens: labels, the run highlight and run times (geometry
//! lives in `flow_layout`, sentences in `flow_text`, pan and zoom in `flow_viewport`). No
//! leptos and no browser types, so everything here is tested natively.

use std::collections::{BTreeMap, BTreeSet};

use oikos_api::{EntityView, RunStatus, RunView, StepOutcome, TriggerInfo};
use oikos_core::command::command_name;
use oikos_core::flow::{Flow, Node, NodeKind, parse_duration};

use crate::format::entity_title;
use crate::schedule::describe;

fn entity_label(id: &str, entities: &BTreeMap<String, EntityView>) -> String {
    entities.get(id).map_or_else(|| id.to_owned(), entity_title)
}

pub(crate) fn humanise(command: &str) -> String {
    let spaced = command.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

/// `Enable <flow>` / `Disable this flow`: what a flow switch does, in words.
fn flow_switch_text(p: &oikos_core::flow::FlowEnableParams) -> String {
    format!(
        "{} {}",
        if p.enabled { "Enable" } else { "Disable" },
        p.flow.as_deref().unwrap_or("this flow")
    )
}

fn summary_label(node: &Node) -> String {
    if let Some(name) = &node.name {
        return name.clone();
    }
    match &node.kind {
        NodeKind::TriggerState(_) => "Trigger".into(),
        NodeKind::TriggerSchedule(p) => describe(&p.cron),
        NodeKind::TriggerSun(_) => "Sun".into(),
        NodeKind::TriggerSignal(_) => "Signal".into(),
        NodeKind::If(_) => "If".into(),
        NodeKind::Delay(p) => format!("Delay {}", p.for_),
        NodeKind::Command(p) => humanise(command_name(&p.command)),
        NodeKind::FlowEnable(p) => flow_switch_text(p),
    }
}

/// Just the trigger node(s) in words: "Every 10 minutes" for a schedule, the entity label
/// for a state trigger. Multiple triggers are joined by " · ".
pub fn trigger_summary(flow: &Flow, entities: &BTreeMap<String, EntityView>) -> String {
    let parts: Vec<String> = flow
        .nodes
        .iter()
        .filter_map(|n| match &n.kind {
            NodeKind::TriggerState(p) => Some(entity_label(&p.entity, entities)),
            NodeKind::TriggerSignal(p) => Some(match &p.signal {
                Some(signal) => format!("{} sends {signal}", entity_label(&p.entity, entities)),
                None => format!("{} signals", entity_label(&p.entity, entities)),
            }),
            NodeKind::TriggerSchedule(p) => Some(describe(&p.cron)),
            _ => None,
        })
        .collect();
    if parts.is_empty() {
        "No trigger".to_owned()
    } else {
        parts.join(" · ")
    }
}

/// One line for the flow list: the layers joined by ` → `, siblings by ` / `.
pub fn summary(flow: &Flow) -> String {
    flow.layers()
        .iter()
        .map(|layer| {
            layer
                .iter()
                .filter_map(|id| flow.node(id))
                .map(summary_label)
                .collect::<Vec<_>>()
                .join(" / ")
        })
        .collect::<Vec<_>>()
        .join(" → ")
}

/// A JSON value as the cards print it: strings bare, everything else as JSON.
pub fn value_text(value: &serde_json::Value) -> String {
    match value {
        serde_json::Value::String(s) => s.clone(),
        other => other.to_string(),
    }
}

/// What the trigger step shows: `"{entity title} = {value}"` from the run's trigger, or
/// `manual` for a run started by hand. Computed here instead of printing the engine's detail,
/// which carries the raw entity id.
pub fn trigger_line(
    trigger: Option<&TriggerInfo>,
    entities: &BTreeMap<String, EntityView>,
) -> String {
    match trigger {
        Some(t) => format!(
            "{} = {}",
            entity_label(&t.entity, entities),
            value_text(&t.value)
        ),
        None => "manual".to_owned(),
    }
}

/// The non-trigger nodes nothing leads to; they can never run and are drawn faded.
pub fn orphans(flow: &Flow) -> BTreeSet<String> {
    flow.nodes
        .iter()
        .filter(|n| {
            !matches!(
                n.kind,
                NodeKind::TriggerState(_)
                    | NodeKind::TriggerSchedule(_)
                    | NodeKind::TriggerSignal(_)
            )
        })
        .filter(|n| !flow.edges.iter().any(|e| e.to == n.id))
        .map(|n| n.id.clone())
        .collect()
}

/// The `delay` node a running run is parked on, with the second it started waiting at. The
/// engine records a `delay` step only after the wait, so while it waits the run's last step is
/// the node before it (an `if`, whose `detail` names the port taken, or a command): the parked
/// node is that step's first successor that is a `delay`. The countdown ends at
/// `since + parse_duration(for)`.
pub fn parked_node(flow: &Flow, run: &RunView) -> Option<(String, u64)> {
    if run.status != RunStatus::Running {
        return None;
    }
    let step = run.steps.last()?;
    let node = flow.node(&step.node)?;
    let port = matches!(node.kind, NodeKind::If(_)).then_some(step.detail.as_str());
    flow.successors(&step.node, port)
        .into_iter()
        .find(|n| matches!(n.kind, NodeKind::Delay(_)))
        .map(|n| (n.id.clone(), step.at))
}

/// The second a parked delay ends at: `since` plus the delay's duration.
pub fn parked_deadline(flow: &Flow, node_id: &str, since: u64) -> Option<u64> {
    let NodeKind::Delay(p) = &flow.node(node_id)?.kind else {
        return None;
    };
    Some(since + parse_duration(&p.for_).ok()?.as_secs())
}

/// What a run touched: the latest outcome of each visited node and the edges it followed.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Highlight {
    pub nodes: BTreeMap<String, StepOutcome>,
    /// `(from, port, to)` of every lit edge.
    pub edges: BTreeSet<(String, Option<String>, String)>,
}

/// The highlight of `run` on `flow`. An edge is lit when both ends were visited (or its end is
/// the delay the run is parked on) and, for an `if`, its port is the branch the latest step of
/// that `if` took (its `detail`).
pub fn highlight(flow: &Flow, run: Option<&RunView>) -> Highlight {
    let mut out = Highlight::default();
    let Some(run) = run else { return out };
    let mut details: BTreeMap<&str, &str> = BTreeMap::new();
    for step in &run.steps {
        out.nodes.insert(step.node.clone(), step.outcome);
        details.insert(&step.node, &step.detail);
    }
    let parked = parked_node(flow, run).map(|(id, _)| id);
    for edge in &flow.edges {
        let to_parked = parked.as_deref() == Some(edge.to.as_str());
        if !out.nodes.contains_key(&edge.from) || !(out.nodes.contains_key(&edge.to) || to_parked) {
            continue;
        }
        let from_is_if = flow
            .node(&edge.from)
            .is_some_and(|n| matches!(n.kind, NodeKind::If(_)));
        if from_is_if && details.get(edge.from.as_str()).copied() != edge.port.as_deref() {
            continue;
        }
        out.edges
            .insert((edge.from.clone(), edge.port.clone(), edge.to.clone()));
    }
    out
}

/// The word shown on a run's badge.
pub fn status_word(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Running => "Running",
        RunStatus::Done => "Done",
        RunStatus::Failed => "Failed",
        RunStatus::Cancelled => "Cancelled",
        RunStatus::Skipped => "Skipped",
    }
}

/// `at - start` as a short log offset: `+0s`, `+45s`, `+2m 10s`, `+1h 03m`. A step whose
/// timestamp is before the run's start reads `+0s` (clock skew should not go backwards here,
/// but we clamp instead of printing a negative).
pub fn step_offset(at: u64, start: u64) -> String {
    let secs = at.saturating_sub(start);
    if secs < 60 {
        format!("+{secs}s")
    } else if secs < 3_600 {
        format!("+{}m {:02}s", secs / 60, secs % 60)
    } else {
        format!("+{}h {:02}m", secs / 3_600, (secs % 3_600) / 60)
    }
}

/// `end - start` as a short run duration (same scale as [`step_offset`] without the `+`), or
/// `None` when the run is still running.
pub fn run_duration(start: u64, end: Option<u64>) -> Option<String> {
    let secs = end?.saturating_sub(start);
    if secs < 60 {
        Some(format!("{secs}s"))
    } else if secs < 3_600 {
        Some(format!("{}m {:02}s", secs / 60, secs % 60))
    } else {
        Some(format!("{}h {:02}m", secs / 3_600, (secs % 3_600) / 60))
    }
}

/// The CSS class of a run's badge.
pub fn status_class(status: RunStatus) -> &'static str {
    match status {
        RunStatus::Running => "run--running",
        RunStatus::Done => "run--done",
        RunStatus::Failed => "run--failed",
        RunStatus::Cancelled => "run--cancelled",
        RunStatus::Skipped => "run--skipped",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_api::StepView;

    fn reference() -> Flow {
        Flow::decode(
            br#"{
      "id": "f", "name": "F", "mode": "restart",
      "nodes": [
        { "id": "motion", "type": "trigger.state", "name": "Motion", "params": { "entity": "m" } },
        { "id": "moving", "type": "if", "params": { "op": "eq", "value": true } },
        { "id": "on", "type": "command", "params": { "entity": "l", "command": { "type": "turn_on" } } },
        { "id": "wait", "type": "delay", "params": { "for": "5m" } },
        { "id": "off", "type": "command", "params": { "entity": "l", "command": { "type": "turn_off" } } }
      ],
      "edges": [
        { "from": "motion", "to": "moving" },
        { "from": "moving", "port": "true", "to": "on" },
        { "from": "moving", "port": "false", "to": "wait" },
        { "from": "wait", "to": "off" }
      ]
    }"#,
        )
        .unwrap()
    }

    #[test]
    fn summary_of_reference_flow() {
        assert_eq!(
            summary(&reference()),
            "Motion → If → Turn on / Delay 5m → Turn off"
        );
    }

    fn step(node: &str, outcome: StepOutcome, detail: &str) -> StepView {
        StepView {
            node: node.into(),
            at: 1,
            outcome,
            detail: detail.into(),
        }
    }

    #[test]
    fn highlight_lights_the_false_branch() {
        let run = RunView {
            id: "r".into(),
            flow_id: "f".into(),
            status: RunStatus::Running,
            started_at: 1,
            ended_at: None,
            trigger: None,
            steps: vec![
                step("motion", StepOutcome::Ok, ""),
                step("moving", StepOutcome::Ok, "false"),
                step("wait", StepOutcome::Ok, "5m"),
            ],
        };
        let h = highlight(&reference(), Some(&run));
        let nodes: Vec<_> = h.nodes.keys().map(String::as_str).collect();
        assert_eq!(nodes, ["motion", "moving", "wait"]);
        let edge =
            |f: &str, p: Option<&str>, t: &str| (f.to_owned(), p.map(str::to_owned), t.to_owned());
        assert_eq!(
            h.edges,
            BTreeSet::from([
                edge("motion", None, "moving"),
                edge("moving", Some("false"), "wait"),
            ])
        );
        assert!(!h.edges.contains(&edge("moving", Some("true"), "on")));
        assert_eq!(highlight(&reference(), None), Highlight::default());
    }

    fn running(steps: Vec<StepView>) -> RunView {
        RunView {
            id: "r".into(),
            flow_id: "f".into(),
            status: RunStatus::Running,
            started_at: 1,
            ended_at: None,
            trigger: None,
            steps,
        }
    }

    #[test]
    fn parked_node_follows_the_branch_taken() {
        let flow = reference();
        let mut moving = step("moving", StepOutcome::Ok, "false");
        moving.at = 100;
        let run = running(vec![step("motion", StepOutcome::Ok, ""), moving]);
        assert_eq!(parked_node(&flow, &run), Some(("wait".to_owned(), 100)));
        assert_eq!(parked_deadline(&flow, "wait", 100), Some(400));
        // The true branch leads to a command, not a delay.
        let run = running(vec![step("moving", StepOutcome::Ok, "true")]);
        assert_eq!(parked_node(&flow, &run), None);
        // A finished run, or one with no steps, parks nowhere.
        let mut done = running(vec![step("moving", StepOutcome::Ok, "false")]);
        done.status = RunStatus::Done;
        assert_eq!(parked_node(&flow, &done), None);
        assert_eq!(parked_node(&flow, &running(vec![])), None);
        // A plain node's successors are followed without a port.
        let mut chain = reference();
        chain.edges.retain(|e| e.from != "motion");
        chain.edges.push(oikos_core::flow::Edge {
            from: "motion".into(),
            port: None,
            to: "wait".into(),
        });
        let run = running(vec![step("motion", StepOutcome::Ok, "")]);
        assert_eq!(parked_node(&chain, &run).map(|p| p.0), Some("wait".into()));
    }

    #[test]
    fn highlight_lights_the_edge_into_the_parked_delay() {
        let run = running(vec![
            step("motion", StepOutcome::Ok, ""),
            step("moving", StepOutcome::Ok, "false"),
        ]);
        let h = highlight(&reference(), Some(&run));
        assert!(h.edges.contains(&(
            "moving".to_owned(),
            Some("false".to_owned()),
            "wait".to_owned()
        )));
        assert!(!h.nodes.contains_key("wait"));
    }

    #[test]
    fn trigger_line_uses_the_title_and_value() {
        let t = TriggerInfo {
            entity: "m".into(),
            value: serde_json::json!(true),
            previous: serde_json::json!(false),
        };
        assert_eq!(trigger_line(Some(&t), &BTreeMap::new()), "m = true");
        assert_eq!(trigger_line(None, &BTreeMap::new()), "manual");
    }

    #[test]
    fn orphans_are_non_triggers_without_incoming_edges() {
        let mut flow = reference();
        assert!(orphans(&flow).is_empty());
        flow.edges.retain(|e| e.to != "off");
        assert_eq!(orphans(&flow), BTreeSet::from(["off".to_owned()]));
    }

    fn node(json: &str) -> Node {
        serde_json::from_str(json).unwrap()
    }

    #[test]
    fn new_nodes_have_summary_labels() {
        let schedule = node(
            r#"{ "id": "s", "type": "trigger.schedule", "params": { "cron": "*/10 * * * *" } }"#,
        );
        let this = node(r#"{ "id": "a", "type": "flow.enable", "params": { "enabled": false } }"#);
        let other = node(
            r#"{ "id": "b", "type": "flow.enable", "params": { "flow": "force-heating-loop", "enabled": true } }"#,
        );
        assert_eq!(summary_label(&schedule), "Every 10 minutes");
        assert_eq!(summary_label(&this), "Disable this flow");
        assert_eq!(summary_label(&other), "Enable force-heating-loop");
    }

    #[test]
    fn trigger_summary_joins_every_trigger() {
        let entities = BTreeMap::new();
        let mut flow = reference();
        assert_eq!(trigger_summary(&flow, &entities), "m");
        flow.nodes.push(node(
            r#"{ "id": "s", "type": "trigger.schedule", "params": { "cron": "*/10 * * * *" } }"#,
        ));
        assert_eq!(trigger_summary(&flow, &entities), "m · Every 10 minutes");
        flow.nodes.retain(|n| !n.kind.is_trigger());
        assert_eq!(trigger_summary(&flow, &entities), "No trigger");
    }

    #[test]
    fn schedule_triggers_are_not_orphans() {
        let flow = Flow::decode(
            br#"{
          "id": "f", "name": "F",
          "nodes": [
            { "id": "tick", "type": "trigger.schedule", "params": { "cron": "* * * * *" } },
            { "id": "on", "type": "command", "params": { "entity": "l", "command": { "type": "turn_on" } } }
          ],
          "edges": [ { "from": "tick", "to": "on" } ]
        }"#,
        )
        .unwrap();
        assert!(orphans(&flow).is_empty());
    }

    #[test]
    fn status_words_and_classes() {
        assert_eq!(status_word(RunStatus::Cancelled), "Cancelled");
        assert_eq!(status_class(RunStatus::Running), "run--running");
        assert_eq!(status_class(RunStatus::Skipped), "run--skipped");
    }

    #[test]
    fn step_offsets_and_run_durations_read_as_short_log_times() {
        assert_eq!(step_offset(1_000, 1_000), "+0s");
        assert_eq!(step_offset(1_045, 1_000), "+45s");
        assert_eq!(step_offset(1_130, 1_000), "+2m 10s");
        assert_eq!(step_offset(1_000 + 3_600 + 180, 1_000), "+1h 03m");
        // Clock skew does not print a negative.
        assert_eq!(step_offset(100, 500), "+0s");
        assert_eq!(run_duration(1_000, Some(1_045)), Some("45s".into()));
        assert_eq!(run_duration(1_000, Some(1_130)), Some("2m 10s".into()));
        assert_eq!(run_duration(1_000, None), None);
    }
}
