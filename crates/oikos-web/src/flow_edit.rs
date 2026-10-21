//! Draft operations on a flow being edited: connect rules, delete, duplicate, add, node ids,
//! flow-id slugs and the undo history. Pure functions over `Flow`, nothing is sent to the hub.

use std::collections::BTreeSet;

use oikos_core::flow::{Edge, Flow, Node, NodeKind, Position};

use crate::flow_layout::Pt;

/// Snapshots kept per direction of the undo history.
const HISTORY_CAP: usize = 100;
/// Longest slug before a `_<n>` suffix, so the id stays within 32 characters.
const SLUG_MAX: usize = 29;
/// Offset of a duplicated card from its original, in canvas units.
const DUPLICATE_OFFSET: f64 = 24.0;

/// Why an edge cannot be drawn.
#[derive(Debug, PartialEq)]
pub enum ConnectError {
    /// Triggers start runs; nothing flows into them.
    IntoTrigger,
    /// A node cannot feed itself.
    SelfLoop,
    /// The same edge already exists.
    Duplicate,
    /// `to` already reaches `from`.
    Cycle,
    /// An endpoint is not in the flow.
    UnknownNode,
    /// An `if` needs a `true`/`false` port; every other node takes none.
    BadPort,
}

/// An edge identity: `(from, port, to)`.
pub type EdgeKey = (String, Option<String>, String);

/// Checks whether `from` (on `port`) may feed `to`. Mirrors the hub's validator rules; fan-in
/// and fan-out are allowed.
pub fn can_connect(
    flow: &Flow,
    from: &str,
    port: Option<&str>,
    to: &str,
) -> Result<(), ConnectError> {
    let (Some(source), Some(target)) = (flow.node(from), flow.node(to)) else {
        return Err(ConnectError::UnknownNode);
    };
    if from == to {
        return Err(ConnectError::SelfLoop);
    }
    if target.kind.is_trigger() {
        return Err(ConnectError::IntoTrigger);
    }
    let ports = source.kind.ports();
    let port_ok = match port {
        Some(p) => ports.contains(&p),
        None => ports.is_empty(),
    };
    if !port_ok {
        return Err(ConnectError::BadPort);
    }
    if flow
        .edges
        .iter()
        .any(|e| e.from == from && e.to == to && e.port.as_deref() == port)
    {
        return Err(ConnectError::Duplicate);
    }
    if reaches(flow, to, from) {
        return Err(ConnectError::Cycle);
    }
    Ok(())
}

/// Whether `target` is reachable from `start` along edges, whatever their port.
fn reaches(flow: &Flow, start: &str, target: &str) -> bool {
    let mut seen = BTreeSet::new();
    let mut pending = vec![start];
    while let Some(id) = pending.pop() {
        if id == target {
            return true;
        }
        if seen.insert(id) {
            pending.extend(
                flow.edges
                    .iter()
                    .filter(|e| e.from == id)
                    .map(|e| e.to.as_str()),
            );
        }
    }
    false
}

/// Adds the edge if [`can_connect`] allows it.
pub fn connect(
    flow: &mut Flow,
    from: &str,
    port: Option<&str>,
    to: &str,
) -> Result<(), ConnectError> {
    can_connect(flow, from, port, to)?;
    flow.edges.push(Edge {
        from: from.to_owned(),
        to: to.to_owned(),
        port: port.map(str::to_owned),
    });
    Ok(())
}

/// Removes the edge; whether it existed.
pub fn remove_edge(flow: &mut Flow, key: &EdgeKey) -> bool {
    let before = flow.edges.len();
    flow.edges
        .retain(|e| !(e.from == key.0 && e.port == key.1 && e.to == key.2));
    flow.edges.len() != before
}

/// Removes the node and every edge touching it; whether it existed.
pub fn remove_node(flow: &mut Flow, id: &str) -> bool {
    let before = flow.nodes.len();
    flow.nodes.retain(|n| n.id != id);
    flow.edges.retain(|e| e.from != id && e.to != id);
    flow.nodes.len() != before
}

/// Copies a node (kind, params, name, extra) 24 px down-right with a fresh id and no edges.
pub fn duplicate_node(flow: &mut Flow, id: &str) -> Option<String> {
    let mut copy = flow.node(id)?.clone();
    copy.id = next_node_id(flow, &copy.kind);
    copy.position = copy.position.map(|p| Position {
        x: p.x + DUPLICATE_OFFSET,
        y: p.y + DUPLICATE_OFFSET,
    });
    let new_id = copy.id.clone();
    flow.nodes.push(copy);
    Some(new_id)
}

/// Appends a node of `kind` at `at`; returns its new id.
pub fn add_node(flow: &mut Flow, kind: NodeKind, at: Pt) -> String {
    let id = next_node_id(flow, &kind);
    flow.nodes.push(Node {
        id: id.clone(),
        name: None,
        position: Some(Position { x: at.x, y: at.y }),
        kind,
        extra: Default::default(),
    });
    id
}

/// `<prefix>_<n>` with the lowest free `n >= 1`.
pub fn next_node_id(flow: &Flow, kind: &NodeKind) -> String {
    let prefix = match kind {
        NodeKind::TriggerState(_) => "trigger",
        NodeKind::TriggerSchedule(_) => "schedule",
        NodeKind::TriggerSun(_) => "sun",
        NodeKind::TriggerSignal(_) => "signal",
        NodeKind::If(_) => "if",
        NodeKind::Delay(_) => "delay",
        NodeKind::Command(_) => "command",
        NodeKind::FlowEnable(_) => "flow_switch",
    };
    (1..)
        .map(|n| format!("{prefix}_{n}"))
        .find(|id| flow.node(id).is_none())
        .expect("an unbounded range always yields a free id")
}

/// The ASCII spelling of a lowercase accented letter; `None` for any other character.
fn fold_char(c: char) -> Option<&'static str> {
    Some(match c {
        'à' | 'â' | 'ä' => "a",
        'é' | 'è' | 'ê' | 'ë' => "e",
        'î' | 'ï' => "i",
        'ô' | 'ö' => "o",
        'ù' | 'û' | 'ü' => "u",
        'ç' => "c",
        'œ' => "oe",
        'æ' => "ae",
        _ => return None,
    })
}

/// Lowercases `text` and folds accented letters to ASCII (`Lumière` becomes `lumiere`), for
/// case- and accent-insensitive matching.
pub(crate) fn fold_accents(text: &str) -> String {
    let mut out = String::with_capacity(text.len());
    for c in text.chars().flat_map(char::to_lowercase) {
        match fold_char(c) {
            Some(folded) => out.push_str(folded),
            None => out.push(c),
        }
    }
    out
}

/// A valid, unreserved, untaken flow id derived from a display name: lowercase ASCII with
/// accents folded, other characters collapsed to `_`, at most 29 characters, `_2`, `_3`, …
/// appended when the result is `new` or already taken.
pub fn slug_id(name: &str, taken: &BTreeSet<String>) -> String {
    let mut slug = String::new();
    for c in name.chars().flat_map(char::to_lowercase) {
        let folded = match fold_char(c) {
            Some(folded) => folded,
            None if c.is_ascii_lowercase() || c.is_ascii_digit() => {
                slug.push(c);
                continue;
            }
            None => "_",
        };
        for f in folded.chars() {
            if f != '_' || !slug.ends_with('_') {
                slug.push(f);
            }
        }
    }
    let mut slug = slug.trim_matches('_').to_owned();
    slug.truncate(SLUG_MAX);
    let slug = slug.trim_end_matches('_');
    let base = if slug.is_empty() { "flow" } else { slug };
    let free = |id: &str| id != "new" && !taken.contains(id);
    if free(base) {
        return base.to_owned();
    }
    (2..)
        .map(|n| format!("{base}_{n}"))
        .find(|id| free(id))
        .expect("an unbounded range always yields a free id")
}

/// Undo/redo stacks of whole-flow snapshots, 100 deep each. Callers [`record`](Self::record)
/// the state before a gesture once, so a drag of many moves is a single undo step.
#[derive(Debug, Default)]
pub struct History {
    past: Vec<Flow>,
    future: Vec<Flow>,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// Records the state before a gesture; clears the redo stack.
    pub fn record(&mut self, before: Flow) {
        self.future.clear();
        push_capped(&mut self.past, before);
    }

    /// Steps back: returns the previous state and keeps `current` for redo.
    pub fn undo(&mut self, current: Flow) -> Option<Flow> {
        let previous = self.past.pop()?;
        push_capped(&mut self.future, current);
        Some(previous)
    }

    /// Steps forward again: returns the undone state and keeps `current` for undo.
    pub fn redo(&mut self, current: Flow) -> Option<Flow> {
        let next = self.future.pop()?;
        push_capped(&mut self.past, current);
        Some(next)
    }

    /// Forgets everything, e.g. when the draft is replaced or saved.
    pub fn clear(&mut self) {
        self.past.clear();
        self.future.clear();
    }

    pub fn can_undo(&self) -> bool {
        !self.past.is_empty()
    }

    pub fn can_redo(&self) -> bool {
        !self.future.is_empty()
    }
}

fn push_capped(stack: &mut Vec<Flow>, flow: Flow) {
    if stack.len() == HISTORY_CAP {
        stack.remove(0);
    }
    stack.push(flow);
}

#[cfg(test)]
mod tests {
    use oikos_core::flow::{
        CommandParams, DelayParams, FlowEnableParams, IfParams, Op, TriggerScheduleParams,
        TriggerStateParams,
    };
    use serde_json::json;

    use super::*;

    fn node(id: &str, kind: NodeKind) -> Node {
        Node {
            id: id.into(),
            name: None,
            position: None,
            kind,
            extra: Default::default(),
        }
    }
    fn trigger(id: &str) -> Node {
        node(id, NodeKind::TriggerState(TriggerStateParams::default()))
    }
    fn delay(id: &str) -> Node {
        node(id, NodeKind::Delay(DelayParams::default()))
    }
    fn if_kind() -> NodeKind {
        NodeKind::If(IfParams {
            op: Op::Eq,
            value: Some(json!(true)),
            entity: None,
            missing: None,
            extra: Default::default(),
        })
    }
    fn flow(nodes: Vec<Node>) -> Flow {
        Flow {
            id: "f".into(),
            name: "F".into(),
            nodes,
            ..Flow::default()
        }
    }
    fn taken(ids: &[&str]) -> BTreeSet<String> {
        ids.iter().map(|s| s.to_string()).collect()
    }

    #[test]
    fn rejects_into_trigger() {
        let f = flow(vec![trigger("t"), delay("d")]);
        assert_eq!(
            can_connect(&f, "d", None, "t"),
            Err(ConnectError::IntoTrigger)
        );
    }

    #[test]
    fn rejects_self_loop() {
        let f = flow(vec![delay("d")]);
        assert_eq!(can_connect(&f, "d", None, "d"), Err(ConnectError::SelfLoop));
    }

    #[test]
    fn rejects_unknown_node() {
        let f = flow(vec![delay("d")]);
        assert_eq!(
            can_connect(&f, "d", None, "x"),
            Err(ConnectError::UnknownNode)
        );
        assert_eq!(
            can_connect(&f, "x", None, "d"),
            Err(ConnectError::UnknownNode)
        );
    }

    #[test]
    fn rejects_duplicate() {
        let mut f = flow(vec![trigger("t"), delay("d")]);
        connect(&mut f, "t", None, "d").unwrap();
        assert_eq!(
            connect(&mut f, "t", None, "d"),
            Err(ConnectError::Duplicate)
        );
        assert_eq!(f.edges.len(), 1);
    }

    #[test]
    fn rejects_cycle() {
        let mut f = flow(vec![delay("a"), delay("b"), delay("c")]);
        connect(&mut f, "a", None, "b").unwrap();
        connect(&mut f, "b", None, "c").unwrap();
        assert_eq!(connect(&mut f, "c", None, "a"), Err(ConnectError::Cycle));
        assert_eq!(f.edges.len(), 2);
    }

    #[test]
    fn if_needs_port() {
        let mut f = flow(vec![trigger("t"), node("i", if_kind()), delay("d")]);
        assert_eq!(can_connect(&f, "i", None, "d"), Err(ConnectError::BadPort));
        assert_eq!(
            can_connect(&f, "i", Some("maybe"), "d"),
            Err(ConnectError::BadPort)
        );
        assert_eq!(
            can_connect(&f, "t", Some("true"), "d"),
            Err(ConnectError::BadPort)
        );
        assert_eq!(connect(&mut f, "i", Some("true"), "d"), Ok(()));
        assert_eq!(connect(&mut f, "i", Some("false"), "d"), Ok(()));
        assert_eq!(f.edges[0].port.as_deref(), Some("true"));
    }

    #[test]
    fn allows_fan_in_and_out() {
        let mut f = flow(vec![trigger("t"), trigger("u"), delay("a"), delay("b")]);
        assert_eq!(connect(&mut f, "t", None, "a"), Ok(()));
        assert_eq!(connect(&mut f, "t", None, "b"), Ok(()));
        assert_eq!(connect(&mut f, "u", None, "a"), Ok(()));
        assert_eq!(f.edges.len(), 3);
    }

    #[test]
    fn remove_edge_by_key() {
        let mut f = flow(vec![trigger("t"), delay("d")]);
        connect(&mut f, "t", None, "d").unwrap();
        let key: EdgeKey = ("t".into(), None, "d".into());
        assert!(remove_edge(&mut f, &key));
        assert!(!remove_edge(&mut f, &key));
        assert!(f.edges.is_empty());
    }

    #[test]
    fn remove_node_drops_edges() {
        let mut f = flow(vec![trigger("t"), delay("a"), delay("b")]);
        connect(&mut f, "t", None, "a").unwrap();
        connect(&mut f, "a", None, "b").unwrap();
        assert!(remove_node(&mut f, "a"));
        assert!(f.edges.is_empty());
        assert_eq!(f.nodes.len(), 2);
        assert!(!remove_node(&mut f, "a"));
    }

    #[test]
    fn duplicate_offsets_and_unwires() {
        let mut original = delay("delay_1");
        original.name = Some("Wait".into());
        original.position = Some(Position { x: 10.0, y: 20.0 });
        original.extra.insert("note".into(), json!("keep"));
        let mut f = flow(vec![trigger("t"), original.clone()]);
        connect(&mut f, "t", None, "delay_1").unwrap();
        let id = duplicate_node(&mut f, "delay_1").unwrap();
        assert_eq!(id, "delay_2");
        let copy = f.node(&id).unwrap();
        assert_eq!(copy.kind, original.kind);
        assert_eq!(copy.name, original.name);
        assert_eq!(copy.extra, original.extra);
        assert_eq!(copy.position, Some(Position { x: 34.0, y: 44.0 }));
        assert_eq!(f.edges.len(), 1);
        assert_eq!(duplicate_node(&mut f, "nope"), None);
    }

    #[test]
    fn add_node_places_and_names_it() {
        let mut f = flow(vec![]);
        let id = add_node(&mut f, if_kind(), Pt { x: 5.0, y: 6.0 });
        assert_eq!(id, "if_1");
        let n = f.node("if_1").unwrap();
        assert_eq!(n.position, Some(Position { x: 5.0, y: 6.0 }));
        assert_eq!(n.name, None);
        assert!(n.extra.is_empty());
    }

    #[test]
    fn next_id_fills_gap() {
        let f = flow(vec![node("if_1", if_kind()), node("if_3", if_kind())]);
        assert_eq!(next_node_id(&f, &if_kind()), "if_2");
    }

    #[test]
    fn next_id_prefixes() {
        let f = flow(vec![]);
        let cases = [
            (NodeKind::TriggerState(Default::default()), "trigger_1"),
            (
                NodeKind::TriggerSchedule(TriggerScheduleParams::default()),
                "schedule_1",
            ),
            (NodeKind::Delay(DelayParams::default()), "delay_1"),
            (
                NodeKind::FlowEnable(FlowEnableParams::default()),
                "flow_switch_1",
            ),
        ];
        for (kind, want) in cases {
            assert_eq!(next_node_id(&f, &kind), want);
        }
        let command = NodeKind::Command(CommandParams {
            entity: "e".into(),
            command: serde_json::from_value(json!({"type": "turn_on"})).unwrap(),
            extra: Default::default(),
        });
        assert_eq!(next_node_id(&f, &command), "command_1");
    }

    #[test]
    fn slug_folds_accents() {
        assert_eq!(slug_id("Lumière été", &taken(&[])), "lumiere_ete");
        assert_eq!(slug_id("Cœur à ça", &taken(&[])), "coeur_a_ca");
    }

    #[test]
    fn slug_avoids_reserved_and_taken() {
        assert_eq!(slug_id("New", &taken(&[])), "new_2");
        assert_eq!(
            slug_id("Force heating loop", &taken(&["force_heating_loop"])),
            "force_heating_loop_2"
        );
        assert_eq!(slug_id("a", &taken(&["a", "a_2", "a_3"])), "a_4");
    }

    #[test]
    fn slug_cleans_punctuation_and_empty() {
        assert_eq!(slug_id("  --Hello,   World!! ", &taken(&[])), "hello_world");
        assert_eq!(slug_id("!!!", &taken(&[])), "flow");
        assert_eq!(slug_id("", &taken(&[])), "flow");
    }

    #[test]
    fn slug_max_len() {
        let long = "x".repeat(60);
        let id = slug_id(&long, &taken(&[]));
        assert_eq!(id.len(), 29);
        let again = slug_id(&long, &taken(&[id.as_str()]));
        assert!(again.len() <= 32);
        assert!(again.ends_with("_2"));
    }

    #[test]
    fn history_undo_redo() {
        let (a, b, c) = (
            flow(vec![]),
            flow(vec![delay("d")]),
            flow(vec![trigger("t")]),
        );
        let mut h = History::new();
        assert!(!h.can_undo());
        h.record(a.clone());
        assert!(h.can_undo());
        let back = h.undo(b.clone()).unwrap();
        assert_eq!(back, a);
        assert!(!h.can_undo());
        assert!(h.can_redo());
        assert_eq!(h.redo(a.clone()), Some(b.clone()));
        assert_eq!(h.undo(b.clone()), Some(a.clone()));
        // A new record clears the future.
        h.record(a);
        assert!(!h.can_redo());
        assert_eq!(h.redo(c), None);
        h.clear();
        assert!(!h.can_undo());
        assert_eq!(h.undo(b), None);
    }

    #[test]
    fn history_caps_at_100() {
        let mut h = History::new();
        for i in 0..150 {
            let mut f = flow(vec![]);
            f.name = i.to_string();
            h.record(f);
        }
        let mut current = flow(vec![]);
        let mut steps = 0;
        while let Some(prev) = h.undo(current.clone()) {
            current = prev;
            steps += 1;
        }
        assert_eq!(steps, 100);
        assert_eq!(current.name, "50");
    }

    #[test]
    fn history_records_once_per_gesture() {
        // Callers record the pre-gesture snapshot once, then mutate freely.
        let mut f = flow(vec![trigger("t")]);
        f.nodes[0].position = Some(Position { x: 0.0, y: 0.0 });
        let original = f.clone();
        let mut h = History::new();
        h.record(f.clone());
        for x in [10.0, 20.0, 30.0] {
            f.nodes[0].position = Some(Position { x, y: 0.0 });
        }
        let restored = h.undo(f).unwrap();
        assert_eq!(restored, original);
        assert!(!h.can_undo());
    }
}
