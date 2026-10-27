//! The values an If card can compare an entity with, as core's [`oikos_core::vocabulary`]
//! lists them from discovery, worded for the screen; plus the rules the card follows when the
//! value is typed by hand or the entity changes.

use std::collections::BTreeSet;

use oikos_api::EntityView;
use oikos_core::flow::{Flow, NodeKind, Scalar};
use oikos_core::model::{Kind, StateValue};
use oikos_core::vocabulary::{Reports, reports};
use serde_json::Value;

use crate::i18n::Lang;
use crate::i18n::format::parse_decimal;
use crate::i18n::states::{StateCtx, on_off, state_word};

/// One entry of the value list: what is stored and what the user reads.
#[derive(Clone, Debug, PartialEq)]
pub struct Choice {
    pub value: Value,
    pub label: String,
}

/// How the value is picked.
#[derive(Clone, Debug, PartialEq)]
pub enum ValueInput {
    List(Vec<Choice>),
    Number { unit: Option<String> },
    Text,
}

/// The input for comparing `e`; a text box when the entity is unknown.
pub fn value_input(lang: Lang, e: Option<&EntityView>) -> ValueInput {
    let Some(e) = e else {
        return ValueInput::Text;
    };
    match reports(e.kind, &e.component, &e.capabilities, e.unit.as_deref()) {
        Reports::Values { values } => ValueInput::List(
            values
                .iter()
                .map(|v| Choice {
                    value: v.to_json(),
                    label: label(lang, e, &v.to_json()),
                })
                .collect(),
        ),
        Reports::Number { unit, .. } => ValueInput::Number { unit },
        // A reading that is a number is compared as one, whatever discovery said.
        Reports::Text | Reports::Nothing => match e.state.value {
            StateValue::Number(_) => ValueInput::Number {
                unit: e.unit.clone(),
            },
            _ => ValueInput::Text,
        },
    }
}

/// The word for a value of `e`: on/off worded by its kind, a state word in its vocabulary.
fn label(lang: Lang, e: &EntityView, value: &Value) -> String {
    match value {
        Value::Bool(on) => match e.kind {
            Kind::BinarySensor => state_word(
                lang,
                StateCtx::BinarySensor(e.device_class.clone()),
                if *on { "on" } else { "off" },
            ),
            kind => on_off(lang, kind == Kind::Light, *on).to_owned(),
        },
        Value::String(word) => state_word(lang, word_context(e.kind), word),
        other => other.to_string(),
    }
}

/// Which vocabulary translates a kind's state words; only the wording depends on it.
fn word_context(kind: Kind) -> StateCtx {
    match kind {
        Kind::Cover => StateCtx::Cover,
        Kind::AlarmPanel => StateCtx::Alarm,
        Kind::Climate | Kind::WaterHeater => StateCtx::Hvac,
        Kind::Vacuum => StateCtx::Vacuum,
        Kind::Generic => StateCtx::Choice,
        _ => StateCtx::Other,
    }
}

/// `input` with `current` first when it is a list that does not hold it, so a saved value
/// stays selected rather than replaced.
pub fn with_current(
    input: ValueInput,
    current: Option<&Value>,
    lang: Lang,
    e: Option<&EntityView>,
) -> ValueInput {
    match (input, current) {
        (ValueInput::List(mut list), Some(current))
            if !list.iter().any(|c| &c.value == current) =>
        {
            let label = match e {
                Some(e) => label(lang, e, current),
                None => current.to_string(),
            };
            list.insert(
                0,
                Choice {
                    value: current.clone(),
                    label,
                },
            );
            ValueInput::List(list)
        }
        (input, _) => input,
    }
}

/// A value typed by hand: a number (decimal comma accepted), `true` / `false`, else text.
/// Blank is nothing.
pub fn parse_other(text: &str) -> Option<Value> {
    let text = text.trim();
    if text.is_empty() {
        return None;
    }
    Some(match (text, parse_decimal(text)) {
        (_, Some(n)) => Scalar::Number(n).to_json(),
        ("true", _) => Value::Bool(true),
        ("false", _) => Value::Bool(false),
        _ => Value::String(text.to_owned()),
    })
}

/// The value after the user picks another entity: kept when the new input can hold it, else
/// the new list's first value, else nothing.
pub fn after_entity_change(current: Option<&Value>, input: &ValueInput) -> Option<Value> {
    match input {
        ValueInput::List(list) => match current {
            Some(v) if list.iter().any(|c| &c.value == v) => Some(v.clone()),
            _ => list.first().map(|c| c.value.clone()),
        },
        ValueInput::Number { .. } => current.filter(|v| v.is_number()).cloned(),
        ValueInput::Text => current.filter(|v| v.is_string()).cloned(),
    }
}

/// The entity whose value reaches `node_id`: the one state trigger every path back from the
/// node starts at. `None` when the paths start at several entities or at another trigger.
pub fn upstream_trigger_entity(flow: &Flow, node_id: &str) -> Option<String> {
    let mut seen = BTreeSet::from([node_id.to_owned()]);
    let mut queue = vec![node_id.to_owned()];
    let mut entities = BTreeSet::new();
    while let Some(id) = queue.pop() {
        for edge in flow.edges.iter().filter(|e| e.to == id) {
            if !seen.insert(edge.from.clone()) {
                continue;
            }
            match flow.node(&edge.from).map(|n| &n.kind) {
                Some(NodeKind::TriggerState(t)) => {
                    entities.insert(Some(t.entity.clone()));
                }
                Some(
                    NodeKind::TriggerSchedule(_)
                    | NodeKind::TriggerSun(_)
                    | NodeKind::TriggerSignal(_),
                ) => {
                    entities.insert(None);
                }
                _ => queue.push(edge.from.clone()),
            }
        }
    }
    match entities.len() {
        1 => entities.into_iter().next().flatten(),
        _ => None,
    }
}

#[cfg(test)]
mod tests {
    use oikos_core::flow::{Edge, Node, NodeKind, TriggerScheduleParams, TriggerStateParams};
    use oikos_core::model::{
        Capabilities, EntityCategory, EntityState, Kind, StateValue, UiOverride,
    };
    use serde_json::json;

    use super::*;

    fn ent(kind: Kind) -> EntityView {
        EntityView {
            id: "e".into(),
            device_id: None,
            name: "E".into(),
            device_name: None,
            label: None,
            kind,
            component: kind.component().to_string(),
            protocol_kind: kind,
            device_class: None,
            unit: None,
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: "a".into(),
            part_of: None,
            capabilities: Capabilities::default(),
            ui: UiOverride::default(),
            state: EntityState::default(),
        }
    }

    fn node(id: &str, kind: NodeKind) -> Node {
        Node {
            id: id.into(),
            name: None,
            position: None,
            kind,
            extra: Default::default(),
        }
    }

    fn state(id: &str, entity: &str) -> Node {
        node(
            id,
            NodeKind::TriggerState(TriggerStateParams {
                entity: entity.into(),
                ..Default::default()
            }),
        )
    }

    fn schedule(id: &str) -> Node {
        node(
            id,
            NodeKind::TriggerSchedule(TriggerScheduleParams {
                cron: "0 * * * *".into(),
                ..Default::default()
            }),
        )
    }

    fn plain(id: &str) -> Node {
        node(
            id,
            NodeKind::Delay(oikos_core::flow::DelayParams {
                for_: "1m".into(),
                ..Default::default()
            }),
        )
    }

    fn edge(from: &str, to: &str) -> Edge {
        Edge {
            from: from.into(),
            to: to.into(),
            port: None,
        }
    }

    fn flow(nodes: Vec<Node>, edges: Vec<Edge>) -> Flow {
        Flow {
            nodes,
            edges,
            ..Default::default()
        }
    }

    fn list(input: ValueInput) -> Vec<Choice> {
        match input {
            ValueInput::List(l) => l,
            other => panic!("not a list: {other:?}"),
        }
    }

    #[test]
    fn cover_lists_core_words_in_french() {
        let l = list(value_input(Lang::Fr, Some(&ent(Kind::Cover))));
        assert_eq!(l[1].value, json!("closed"));
        assert_eq!(l[1].label, "Fermé");
    }

    #[test]
    fn door_sensor_is_worded_by_device_class() {
        let mut door = ent(Kind::BinarySensor);
        door.device_class = Some("door".into());
        let l = list(value_input(Lang::En, Some(&door)));
        assert_eq!(
            (l[0].value.clone(), l[0].label.as_str()),
            (json!(true), "Open")
        );
    }

    #[test]
    fn numeric_sensor_is_a_number_with_its_unit() {
        let mut t = ent(Kind::Sensor);
        t.unit = Some("°C".into());
        assert_eq!(
            value_input(Lang::En, Some(&t)),
            ValueInput::Number {
                unit: Some("°C".into())
            }
        );
    }

    #[test]
    fn text_sensor_with_a_numeric_state_gets_a_number_box() {
        let mut s = ent(Kind::Sensor);
        s.state.value = StateValue::Number(3.0);
        assert_eq!(
            value_input(Lang::En, Some(&s)),
            ValueInput::Number { unit: None }
        );
        assert_eq!(
            value_input(Lang::En, Some(&ent(Kind::Sensor))),
            ValueInput::Text
        );
        assert_eq!(value_input(Lang::En, None), ValueInput::Text);
    }

    #[test]
    fn unlisted_current_value_is_kept_first() {
        let sw = ent(Kind::Switch);
        let l = list(with_current(
            value_input(Lang::En, Some(&sw)),
            Some(&json!("closed")),
            Lang::En,
            Some(&sw),
        ));
        assert_eq!((l[0].value.clone(), l.len()), (json!("closed"), 3));
        let kept = list(with_current(
            value_input(Lang::En, Some(&sw)),
            Some(&json!(true)),
            Lang::En,
            Some(&sw),
        ));
        assert_eq!(kept.len(), 2);
    }

    #[test]
    fn other_value_parses_numbers_bools_and_text() {
        assert_eq!(parse_other("21,5"), Some(json!(21.5)));
        assert_eq!(parse_other("true"), Some(json!(true)));
        assert_eq!(parse_other(" on "), Some(json!("on")));
        assert_eq!(parse_other("  "), None);
    }

    #[test]
    fn entity_change_keeps_an_offered_value_else_resets() {
        let cover = value_input(Lang::En, Some(&ent(Kind::Cover)));
        assert_eq!(
            after_entity_change(Some(&json!("closed")), &cover),
            Some(json!("closed"))
        );
        assert_eq!(
            after_entity_change(Some(&json!(true)), &cover),
            Some(json!("open"))
        );
        let number = ValueInput::Number { unit: None };
        assert_eq!(
            after_entity_change(Some(&json!(3)), &number),
            Some(json!(3))
        );
        assert_eq!(after_entity_change(Some(&json!("x")), &number), None);
    }

    #[test]
    fn upstream_entity_follows_the_feeding_state_trigger() {
        let f = flow(
            vec![
                state("t1", "a"),
                state("t2", "b"),
                plain("d0"),
                plain("if1"),
                plain("d1"),
            ],
            vec![edge("t1", "d0"), edge("d0", "if1"), edge("t2", "d1")],
        );
        assert_eq!(upstream_trigger_entity(&f, "if1"), Some("a".into()));
    }

    #[test]
    fn upstream_entity_is_none_for_mixed_triggers() {
        let mixed = flow(
            vec![state("t1", "a"), schedule("t2"), plain("if1")],
            vec![edge("t1", "if1"), edge("t2", "if1")],
        );
        assert_eq!(upstream_trigger_entity(&mixed, "if1"), None);
        let two = flow(
            vec![state("t1", "a"), state("t2", "b"), plain("if1")],
            vec![edge("t1", "if1"), edge("t2", "if1")],
        );
        assert_eq!(upstream_trigger_entity(&two, "if1"), None);
        let same = flow(
            vec![state("t1", "a"), state("t2", "a"), plain("if1")],
            vec![edge("t1", "if1"), edge("t2", "if1")],
        );
        assert_eq!(upstream_trigger_entity(&same, "if1"), Some("a".into()));
    }

    #[test]
    fn upstream_walk_survives_a_cycle() {
        let looped = flow(
            vec![state("t1", "a"), plain("x"), plain("if1")],
            vec![edge("t1", "x"), edge("x", "if1"), edge("if1", "x")],
        );
        assert_eq!(upstream_trigger_entity(&looped, "if1"), Some("a".into()));
    }
}
