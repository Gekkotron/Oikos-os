//! Pure helpers behind the flow screens: canvas layout, wire geometry, labels and the run
//! highlight. No leptos and no browser types, so everything here is tested natively.

use std::collections::{BTreeMap, BTreeSet};

use oikos_api::{EntityView, RunStatus, RunView, StepOutcome, TriggerInfo};
use oikos_core::command::command_name;
use oikos_core::flow::{Flow, Node, NodeKind, Op, Position, parse_duration};

use crate::format::entity_title;

/// Width of a node card on the canvas.
pub const NODE_W: f64 = 200.0;
/// Height of a node card on the canvas.
pub const NODE_H: f64 = 72.0;
/// Horizontal space between two columns.
pub const COL_GAP: f64 = 56.0;
/// Vertical space between two rows.
pub const ROW_GAP: f64 = 40.0;
/// Margin around the whole graph.
pub const PAD: f64 = 24.0;

/// Lowest zoom the canvas allows.
pub const ZOOM_MIN: f64 = 0.4;
/// Highest zoom the canvas allows.
pub const ZOOM_MAX: f64 = 2.0;
/// What `+` / `−` buttons change the zoom by.
pub const ZOOM_STEP: f64 = 0.1;

/// `zoom` kept inside `[ZOOM_MIN, ZOOM_MAX]`, rounded to the hundredth so the readout is stable.
pub fn clamp_zoom(zoom: f64) -> f64 {
    (zoom.clamp(ZOOM_MIN, ZOOM_MAX) * 100.0).round() / 100.0
}

/// `flow` with every node's `position` set, either kept as-is or filled from the auto layout.
/// Used to start edit mode from the same coordinates the canvas is drawing.
pub fn with_resolved_positions(flow: &Flow) -> Flow {
    let placed = layout(flow);
    let mut out = flow.clone();
    for node in &mut out.nodes {
        if node.position.is_none()
            && let Some(p) = placed.iter().find(|p| p.id == node.id)
        {
            node.position = Some(Position { x: p.x, y: p.y });
        }
    }
    out
}

/// A node's top-left corner on the canvas.
#[derive(Clone, Debug, PartialEq)]
pub struct Placed {
    pub id: String,
    pub x: f64,
    pub y: f64,
}

/// Where each node goes: the stored positions when every node has one (a flow edited elsewhere
/// keeps its look), else the layers laid out left to right, one row per sibling.
pub fn layout(flow: &Flow) -> Vec<Placed> {
    if !flow.nodes.is_empty() && flow.nodes.iter().all(|n| n.position.is_some()) {
        return flow
            .nodes
            .iter()
            .filter_map(|n| {
                n.position.map(|p| Placed {
                    id: n.id.clone(),
                    x: p.x,
                    y: p.y,
                })
            })
            .collect();
    }
    flow.layers()
        .into_iter()
        .enumerate()
        .flat_map(|(col, ids)| {
            ids.into_iter().enumerate().map(move |(row, id)| Placed {
                id,
                x: PAD + col as f64 * (NODE_W + COL_GAP),
                y: PAD + row as f64 * (NODE_H + ROW_GAP),
            })
        })
        .collect()
}

/// The canvas size that holds every node plus the margin.
pub fn canvas_size(placed: &[Placed]) -> (f64, f64) {
    let right = placed.iter().map(|p| p.x + NODE_W).fold(0.0, f64::max);
    let bottom = placed.iter().map(|p| p.y + NODE_H).fold(0.0, f64::max);
    (right + PAD, bottom + PAD)
}

/// The height, from a node's top, at which an output port sits: the middle for a single
/// output, a third and two thirds down for the `true` and `false` ports of an `if`.
pub fn port_offset(kind: &NodeKind, port: Option<&str>) -> f64 {
    match (kind, port) {
        (NodeKind::If(_), Some("true")) => NODE_H * 0.33,
        (NodeKind::If(_), Some("false")) => NODE_H * 0.67,
        _ => NODE_H / 2.0,
    }
}

/// The SVG path of a wire: a cubic that leaves and enters horizontally.
pub fn wire_path(x1: f64, y1: f64, x2: f64, y2: f64) -> String {
    format!(
        "M{:.1} {:.1} C{:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}",
        x1,
        y1,
        x1 + 60.0,
        y1,
        x2 - 60.0,
        y2,
        x2,
        y2
    )
}

/// The card title: the node's name, else its type in words.
pub fn node_title(node: &Node) -> String {
    node.name.clone().unwrap_or_else(|| {
        match node.kind {
            NodeKind::TriggerState(_) => "Trigger",
            NodeKind::TriggerSchedule(_) => "Schedule",
            NodeKind::If(_) => "If",
            NodeKind::Delay(_) => "Delay",
            NodeKind::Command(_) => "Command",
            NodeKind::FlowEnable(_) => "Flow",
        }
        .into()
    })
}

fn entity_label(id: &str, entities: &BTreeMap<String, EntityView>) -> String {
    entities.get(id).map_or_else(|| id.to_owned(), entity_title)
}

fn humanise(command: &str) -> String {
    let spaced = command.replace('_', " ");
    let mut chars = spaced.chars();
    match chars.next() {
        Some(first) => first.to_uppercase().chain(chars).collect(),
        None => String::new(),
    }
}

fn op_symbol(op: Op) -> &'static str {
    match op {
        Op::Eq => "=",
        Op::Ne => "≠",
        Op::Gt => ">",
        Op::Lt => "<",
        Op::Gte => "≥",
        Op::Lte => "≤",
    }
}

/// The card's second line: the entity, the comparison, the delay or the command.
pub fn node_detail(node: &Node, entities: &BTreeMap<String, EntityView>) -> String {
    match &node.kind {
        NodeKind::TriggerState(p) => entity_label(&p.entity, entities),
        NodeKind::TriggerSchedule(p) => p.cron.clone(),
        NodeKind::If(p) => {
            let value = match &p.value {
                serde_json::Value::String(s) => s.clone(),
                other => other.to_string(),
            };
            format!("{} {value}", op_symbol(p.op))
        }
        NodeKind::Delay(p) => p.for_.clone(),
        NodeKind::FlowEnable(p) => p.flow.clone().unwrap_or_else(|| "this flow".into()),
        NodeKind::Command(p) => format!(
            "{} · {}",
            entity_label(&p.entity, entities),
            command_name(&p.command).replace('_', " ")
        ),
    }
}

fn summary_label(node: &Node) -> String {
    if let Some(name) = &node.name {
        return name.clone();
    }
    match &node.kind {
        NodeKind::TriggerState(_) => "Trigger".into(),
        NodeKind::TriggerSchedule(_) => "Schedule".into(),
        NodeKind::If(_) => "If".into(),
        NodeKind::Delay(p) => format!("Delay {}", p.for_),
        NodeKind::Command(p) => humanise(command_name(&p.command)),
        NodeKind::FlowEnable(_) => "Flow".into(),
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
        .filter(|n| !matches!(n.kind, NodeKind::TriggerState(_)))
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
    use oikos_core::flow::Position;

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

    fn at(placed: &[Placed], id: &str) -> (f64, f64) {
        let p = placed.iter().find(|p| p.id == id).unwrap();
        (p.x, p.y)
    }

    #[test]
    fn auto_layout_of_reference_flow() {
        let placed = layout(&reference());
        let col = |c: f64| PAD + c * (NODE_W + COL_GAP);
        let row = |r: f64| PAD + r * (NODE_H + ROW_GAP);
        assert_eq!(at(&placed, "motion"), (col(0.0), row(0.0)));
        assert_eq!(at(&placed, "moving"), (col(1.0), row(0.0)));
        assert_eq!(at(&placed, "on"), (col(2.0), row(0.0)));
        assert_eq!(at(&placed, "wait"), (col(2.0), row(1.0)));
        assert_eq!(at(&placed, "off"), (col(3.0), row(0.0)));
        assert_eq!(
            canvas_size(&placed),
            (
                24.0 + 3.0 * 256.0 + 200.0 + 24.0,
                24.0 + 112.0 + 72.0 + 24.0
            )
        );
    }

    #[test]
    fn stored_positions_win() {
        let mut flow = reference();
        for (i, n) in flow.nodes.iter_mut().enumerate() {
            n.position = Some(Position {
                x: 10.0 * i as f64,
                y: 5.0,
            });
        }
        let placed = layout(&flow);
        assert_eq!(at(&placed, "wait"), (30.0, 5.0));
        // One missing position falls back to the auto layout.
        flow.nodes[0].position = None;
        assert_eq!(at(&layout(&flow), "motion"), (PAD, PAD));
    }

    #[test]
    fn if_ports_have_two_heights() {
        let flow = reference();
        let kind = |id: &str| &flow.node(id).unwrap().kind;
        assert_eq!(port_offset(kind("moving"), Some("true")), NODE_H * 0.33);
        assert_eq!(port_offset(kind("moving"), Some("false")), NODE_H * 0.67);
        assert_eq!(port_offset(kind("wait"), None), 36.0);
        assert_eq!(port_offset(kind("motion"), None), 36.0);
    }

    #[test]
    fn wire_path_is_a_cubic() {
        assert_eq!(
            wire_path(224.0, 60.0, 280.0, 100.5),
            "M224.0 60.0 C284.0 60.0, 220.0 100.5, 280.0 100.5"
        );
    }

    #[test]
    fn summary_of_reference_flow() {
        assert_eq!(
            summary(&reference()),
            "Motion → If → Turn on / Delay 5m → Turn off"
        );
    }

    #[test]
    fn titles_and_details() {
        let flow = reference();
        let none = BTreeMap::new();
        assert_eq!(node_title(flow.node("motion").unwrap()), "Motion");
        assert_eq!(node_title(flow.node("moving").unwrap()), "If");
        assert_eq!(node_detail(flow.node("motion").unwrap(), &none), "m");
        assert_eq!(node_detail(flow.node("moving").unwrap(), &none), "= true");
        assert_eq!(node_detail(flow.node("wait").unwrap(), &none), "5m");
        assert_eq!(node_detail(flow.node("on").unwrap(), &none), "l · turn on");
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

    #[test]
    fn status_words_and_classes() {
        assert_eq!(status_word(RunStatus::Cancelled), "Cancelled");
        assert_eq!(status_class(RunStatus::Running), "run--running");
        assert_eq!(status_class(RunStatus::Skipped), "run--skipped");
    }

    #[test]
    fn zoom_is_clamped_and_rounded() {
        assert_eq!(clamp_zoom(1.0), 1.0);
        assert_eq!(clamp_zoom(0.1), ZOOM_MIN);
        assert_eq!(clamp_zoom(5.0), ZOOM_MAX);
        assert_eq!(clamp_zoom(1.237), 1.24);
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

    #[test]
    fn resolve_positions_fills_the_missing_ones() {
        let flow = reference();
        let resolved = with_resolved_positions(&flow);
        for n in &resolved.nodes {
            assert!(n.position.is_some(), "{} has no position", n.id);
        }
        let motion = resolved.node("motion").unwrap().position.unwrap();
        assert_eq!((motion.x, motion.y), (PAD, PAD));
        // Stored positions are kept.
        let mut mixed = flow.clone();
        mixed.nodes[0].position = Some(Position { x: 10.0, y: 20.0 });
        let resolved = with_resolved_positions(&mixed);
        assert_eq!(
            resolved.node("motion").unwrap().position,
            Some(Position { x: 10.0, y: 20.0 })
        );
    }
}
