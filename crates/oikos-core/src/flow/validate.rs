//! Structural validation of a flow.

use super::Flow;

/// Why a flow is not structurally valid.
#[derive(Clone, Debug, PartialEq, Eq, thiserror::Error)]
pub enum FlowError {
    #[error("flow id `{0}` must be 1 to 64 characters of a-z, 0-9, `_` or `-`")]
    BadFlowId(String),
    #[error("node id `{0}` must be 1 to 32 characters of a-z, 0-9, `_` or `-`")]
    BadNodeId(String),
    #[error("node id `{0}` is used twice")]
    DuplicateNodeId(String),
    #[error("edge {edge_index} names a missing node `{node}`")]
    UnknownNode { edge_index: usize, node: String },
    #[error("node `{node}` has no port `{port}`")]
    UnknownPort { node: String, port: String },
    #[error("node `{node}` is an if and needs a port")]
    MissingPort { node: String },
    #[error("node `{node}` has a single output and takes no port")]
    UnexpectedPort { node: String },
    #[error("cycle through {}", .0.join(" → "))]
    Cycle(Vec<String>),
    #[error("the flow has no trigger node")]
    NoTrigger,
    #[error("node `{node}`: {reason}")]
    BadDuration { node: String, reason: String },
    #[error("node `{node}`: value must be a boolean, a number or a string")]
    BadValue { node: String },
}

fn is_valid_id(id: &str, max: usize) -> bool {
    (1..=max).contains(&id.len())
        && id
            .bytes()
            .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || b == b'_' || b == b'-')
}

/// Whether `id` matches `[a-z0-9_-]{1,64}`, the form of a flow id (a topic segment).
pub fn is_valid_flow_id(id: &str) -> bool {
    is_valid_id(id, 64)
}

/// Whether `id` matches `[a-z0-9_-]{1,32}`, the form of a node id.
pub fn is_valid_node_id(id: &str) -> bool {
    is_valid_id(id, 32)
}

/// Checks the structure of a flow: ids, edges and ports, params, at least one trigger and no
/// cycle. It does not look at entities, which only the hub can resolve.
pub fn validate(flow: &Flow) -> Result<(), FlowError> {
    use super::{NodeKind, Scalar, parse_duration};

    if !is_valid_flow_id(&flow.id) {
        return Err(FlowError::BadFlowId(flow.id.clone()));
    }
    let mut seen = std::collections::HashSet::new();
    for node in &flow.nodes {
        if !is_valid_node_id(&node.id) {
            return Err(FlowError::BadNodeId(node.id.clone()));
        }
        if !seen.insert(node.id.as_str()) {
            return Err(FlowError::DuplicateNodeId(node.id.clone()));
        }
    }
    for node in &flow.nodes {
        match &node.kind {
            NodeKind::Delay(p) => {
                if let Err(e) = parse_duration(&p.for_) {
                    return Err(FlowError::BadDuration {
                        node: node.id.clone(),
                        reason: e.to_string(),
                    });
                }
            }
            NodeKind::If(p) if Scalar::from_json(&p.value).is_none() => {
                return Err(FlowError::BadValue {
                    node: node.id.clone(),
                });
            }
            _ => {}
        }
    }
    for (edge_index, edge) in flow.edges.iter().enumerate() {
        for end in [&edge.from, &edge.to] {
            if flow.node(end).is_none() {
                return Err(FlowError::UnknownNode {
                    edge_index,
                    node: end.clone(),
                });
            }
        }
        let Some(from) = flow.node(&edge.from) else {
            continue;
        };
        let ports = from.kind.ports();
        match (&edge.port, ports.is_empty()) {
            (None, false) => {
                return Err(FlowError::MissingPort {
                    node: from.id.clone(),
                });
            }
            (Some(_), true) => {
                return Err(FlowError::UnexpectedPort {
                    node: from.id.clone(),
                });
            }
            (Some(port), false) if !ports.contains(&port.as_str()) => {
                return Err(FlowError::UnknownPort {
                    node: from.id.clone(),
                    port: port.clone(),
                });
            }
            _ => {}
        }
    }
    if flow.triggers().is_empty() {
        return Err(FlowError::NoTrigger);
    }
    match find_cycle(flow) {
        Some(path) => Err(FlowError::Cycle(path)),
        None => Ok(()),
    }
}

/// Iterative DFS with white/grey/black colouring over every edge, whatever its port. Returns the
/// grey path closed on the node that was reached again.
fn find_cycle(flow: &Flow) -> Option<Vec<String>> {
    use std::collections::HashMap;

    let mut next: HashMap<&str, Vec<&str>> = HashMap::new();
    for edge in &flow.edges {
        next.entry(edge.from.as_str())
            .or_default()
            .push(edge.to.as_str());
    }
    // 0 = white (absent), 1 = grey, 2 = black.
    let mut colour: HashMap<&str, u8> = HashMap::new();
    for start in &flow.nodes {
        if colour.contains_key(start.id.as_str()) {
            continue;
        }
        colour.insert(start.id.as_str(), 1);
        // The grey path, each entry with the index of its next child to visit.
        let mut stack: Vec<(&str, usize)> = vec![(start.id.as_str(), 0)];
        while let Some(&(node, i)) = stack.last() {
            let children = next.get(node).map(Vec::as_slice).unwrap_or(&[]);
            let Some(&child) = children.get(i) else {
                colour.insert(node, 2);
                stack.pop();
                continue;
            };
            if let Some(top) = stack.last_mut() {
                top.1 += 1;
            }
            match colour.get(child).copied().unwrap_or(0) {
                0 => {
                    colour.insert(child, 1);
                    stack.push((child, 0));
                }
                1 => {
                    let from = stack.iter().position(|(n, _)| *n == child).unwrap_or(0);
                    let mut path: Vec<String> =
                        stack[from..].iter().map(|(n, _)| (*n).to_owned()).collect();
                    path.push(child.to_owned());
                    return Some(path);
                }
                _ => {}
            }
        }
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::flow::{Edge, Node, NodeKind, reference_flow};

    fn edge(from: &str, to: &str, port: Option<&str>) -> Edge {
        Edge {
            from: from.into(),
            to: to.into(),
            port: port.map(Into::into),
        }
    }

    #[test]
    fn reference_flow_validates() {
        assert_eq!(validate(&reference_flow()), Ok(()));
    }

    #[test]
    fn if_edge_without_port_is_missing_port() {
        let mut f = reference_flow();
        f.edges[1].port = None;
        assert_eq!(
            validate(&f),
            Err(FlowError::MissingPort {
                node: "moving".into()
            })
        );
        assert!(
            validate(&f)
                .unwrap_err()
                .to_string()
                .contains("node `moving` is an if and needs a port")
        );
    }

    #[test]
    fn port_on_a_command_is_unexpected() {
        let mut f = reference_flow();
        f.edges.push(edge("on", "wait", Some("true")));
        assert_eq!(
            validate(&f),
            Err(FlowError::UnexpectedPort { node: "on".into() })
        );
    }

    #[test]
    fn unknown_port_name() {
        let mut f = reference_flow();
        f.edges[1].port = Some("maybe".into());
        assert_eq!(
            validate(&f),
            Err(FlowError::UnknownPort {
                node: "moving".into(),
                port: "maybe".into()
            })
        );
    }

    #[test]
    fn edge_to_missing_node() {
        let mut f = reference_flow();
        f.edges[3].to = "nope".into();
        let err = validate(&f).unwrap_err();
        assert_eq!(
            err,
            FlowError::UnknownNode {
                edge_index: 3,
                node: "nope".into()
            }
        );
        assert_eq!(err.to_string(), "edge 3 names a missing node `nope`");
    }

    #[test]
    fn cycle_is_rejected() {
        let mut f = reference_flow();
        f.edges.push(edge("off", "wait", None));
        match validate(&f) {
            Err(FlowError::Cycle(path)) => {
                assert!(path.contains(&"wait".to_owned()) && path.contains(&"off".to_owned()));
                assert!(!path.contains(&"motion".to_owned()));
            }
            other => panic!("expected a cycle, got {other:?}"),
        }
        let text = FlowError::Cycle(vec!["wait".into(), "off".into(), "wait".into()]).to_string();
        assert_eq!(text, "cycle through wait → off → wait");
    }

    #[test]
    fn flow_without_trigger_is_rejected() {
        let mut f = reference_flow();
        f.nodes.remove(0);
        f.edges.remove(0);
        assert_eq!(validate(&f), Err(FlowError::NoTrigger));
    }

    #[test]
    fn bad_duration_and_bad_if_value() {
        let mut f = reference_flow();
        if let NodeKind::Delay(p) = &mut f.nodes[3].kind {
            p.for_ = "5".into();
        }
        assert!(matches!(
            validate(&f),
            Err(FlowError::BadDuration { node, .. }) if node == "wait"
        ));
        let mut f = reference_flow();
        if let NodeKind::If(p) = &mut f.nodes[1].kind {
            p.value = serde_json::json!([1]);
        }
        let err = validate(&f).unwrap_err();
        assert_eq!(
            err,
            FlowError::BadValue {
                node: "moving".into()
            }
        );
        assert_eq!(
            err.to_string(),
            "node `moving`: value must be a boolean, a number or a string"
        );
    }

    #[test]
    fn ids_are_checked() {
        let mut f = reference_flow();
        f.id = "Living LED".into();
        assert_eq!(validate(&f), Err(FlowError::BadFlowId("Living LED".into())));
        let mut f = reference_flow();
        f.nodes[0].id = "A B".into();
        assert_eq!(validate(&f), Err(FlowError::BadNodeId("A B".into())));
        let mut f = reference_flow();
        let dup: Node = f.nodes[4].clone();
        f.nodes.push(dup);
        assert_eq!(validate(&f), Err(FlowError::DuplicateNodeId("off".into())));
        assert!(is_valid_flow_id(&"a".repeat(64)) && !is_valid_flow_id(&"a".repeat(65)));
        assert!(is_valid_node_id(&"a".repeat(32)) && !is_valid_node_id(&"a".repeat(33)));
        assert!(!is_valid_flow_id("") && !is_valid_node_id(""));
    }
}
