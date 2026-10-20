//! Sentences for flow cards: "Turn off LED", "Entrée > 16 lx?". Pure, tested natively.

use std::collections::BTreeMap;

use oikos_api::{AreaView, EntityView};
use oikos_core::command::command_name;
use oikos_core::flow::{Node, NodeKind, Op, parse_duration};
use oikos_core::model::Command;
use serde_json::Value;

use crate::flows::{humanise, value_text};
use crate::format::entity_title;
use crate::schedule::describe;

/// What a card does in a flow; drives its colour.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Trigger,
    Check,
    Action,
    Wait,
}

/// The role of a node type.
pub fn role(kind: &NodeKind) -> Role {
    match kind {
        NodeKind::TriggerState(_) | NodeKind::TriggerSchedule(_) => Role::Trigger,
        NodeKind::If(_) => Role::Check,
        NodeKind::Command(_) | NodeKind::FlowEnable(_) => Role::Action,
        NodeKind::Delay(_) => Role::Wait,
    }
}

/// The house a sentence is read against.
pub struct Ctx<'a> {
    pub entities: &'a BTreeMap<String, EntityView>,
    pub areas: &'a [AreaView],
    /// `(id, name)` of every flow.
    pub flows: &'a [(String, String)],
}

fn entity_text(id: &str, ctx: &Ctx) -> String {
    ctx.entities
        .get(id)
        .map_or_else(|| id.to_owned(), entity_title)
}

/// A number without a trailing `.0`.
fn number_text(n: f64) -> String {
    if n.fract() == 0.0 && n.abs() < 1e15 {
        format!("{}", n as i64)
    } else {
        n.to_string()
    }
}

/// A value as a sentence reads it: booleans `on`/`off`, numbers followed by the unit.
fn value_words(value: &Value, unit: Option<&str>) -> String {
    match value {
        Value::Bool(true) => "on".into(),
        Value::Bool(false) => "off".into(),
        Value::Number(_) => match unit.filter(|u| !u.is_empty()) {
            Some(u) => format!("{} {u}", value_text(value)),
            None => value_text(value),
        },
        other => value_text(other),
    }
}

fn unit_of<'a>(id: &str, ctx: &'a Ctx) -> Option<&'a str> {
    ctx.entities.get(id).and_then(|e| e.unit.as_deref())
}

fn command_sentence(command: &Command, entity: &str, ctx: &Ctx) -> String {
    let e = entity_text(entity, ctx);
    match command {
        Command::TurnOn {
            brightness: Some(b),
            ..
        } => format!(
            "Turn on {e} at {} %",
            (f64::from(*b) * 100.0 / 255.0).round()
        ),
        Command::TurnOn { .. } => format!("Turn on {e}"),
        Command::TurnOff => format!("Turn off {e}"),
        Command::Toggle => format!("Toggle {e}"),
        Command::Open => format!("Open {e}"),
        Command::Close => format!("Close {e}"),
        Command::Stop => format!("Stop {e}"),
        Command::Press => format!("Press {e}"),
        Command::SetPosition { position } => format!("Set {e} to {position} %"),
        Command::SetTemperature { temperature } => {
            format!("Set {e} to {} °C", number_text(*temperature))
        }
        Command::SetNumber { value } => {
            let v = number_text(*value);
            match unit_of(entity, ctx).filter(|u| !u.is_empty()) {
                Some(u) => format!("Set {e} to {v} {u}"),
                None => format!("Set {e} to {v}"),
            }
        }
        Command::SelectOption { option } => format!("Set {e} to {option}"),
        Command::SetHvacMode { mode } => format!("Set {e} mode to {mode}"),
        other => format!("{} {e}", humanise(command_name(other))),
    }
}

fn op_words(op: Op) -> &'static str {
    match op {
        Op::Gt => ">",
        Op::Lt => "<",
        Op::Gte => "≥",
        Op::Lte => "≤",
        Op::Eq | Op::Ne => "",
    }
}

/// The card's sentence, generated from its parameters; ignores the node's name.
pub fn sentence(node: &Node, ctx: &Ctx) -> String {
    match &node.kind {
        NodeKind::TriggerState(p) => {
            let e = entity_text(&p.entity, ctx);
            match &p.to {
                None => format!("When {e} changes"),
                Some(Value::Bool(b)) => {
                    format!("When {e} turns {}", if *b { "on" } else { "off" })
                }
                Some(v) => format!(
                    "When {e} becomes {}",
                    value_words(v, unit_of(&p.entity, ctx))
                ),
            }
        }
        NodeKind::TriggerSchedule(p) => describe(&p.cron),
        NodeKind::If(p) => {
            let (e, unit) = match &p.entity {
                Some(id) => (entity_text(id, ctx), unit_of(id, ctx)),
                None => ("Trigger value".to_owned(), None),
            };
            let v = value_words(&p.value, unit);
            match p.op {
                Op::Eq => format!("{e} is {v}?"),
                Op::Ne => format!("{e} is not {v}?"),
                op => format!("{e} {} {v}?", op_words(op)),
            }
        }
        NodeKind::Delay(p) => format!("Wait {}", duration_words(&p.for_)),
        NodeKind::Command(p) => command_sentence(&p.command, &p.entity, ctx),
        NodeKind::FlowEnable(p) => {
            let verb = if p.enabled { "Enable" } else { "Disable" };
            let target = match &p.flow {
                None => "this flow".to_owned(),
                Some(id) => ctx
                    .flows
                    .iter()
                    .find(|(fid, _)| fid == id)
                    .map_or_else(|| id.clone(), |(_, name)| name.clone()),
            };
            format!("{verb} {target}")
        }
    }
}

fn named(node: &Node) -> Option<&str> {
    node.name.as_deref().filter(|n| !n.trim().is_empty())
}

/// The node's name when it has one, else its [`sentence`].
pub fn title(node: &Node, ctx: &Ctx) -> String {
    named(node).map_or_else(|| sentence(node, ctx), str::to_owned)
}

fn entity_id(node: &Node) -> Option<&str> {
    match &node.kind {
        NodeKind::TriggerState(p) => Some(&p.entity),
        NodeKind::If(p) => p.entity.as_deref(),
        NodeKind::Command(p) => Some(&p.entity),
        _ => None,
    }
}

/// The small second line: `"{area} · {device}"` for a node with an entity, the sentence for
/// another node that carries a name.
pub fn subline(node: &Node, ctx: &Ctx) -> Option<String> {
    let Some(id) = entity_id(node) else {
        return named(node).map(|_| sentence(node, ctx));
    };
    let entity = ctx.entities.get(id)?;
    let area = ctx
        .areas
        .iter()
        .find(|a| a.id == entity.area)
        .map(|a| a.name.as_str());
    let device = entity.device_name.as_deref();
    let parts: Vec<&str> = [area, device]
        .into_iter()
        .flatten()
        .filter(|s| !s.is_empty())
        .collect();
    (!parts.is_empty()).then(|| parts.join(" · "))
}

/// `"5m"` as "5 min", `"1h30m"` as "1 h 30 min", `"45s"` as "45 s"; text that is not a duration
/// comes back unchanged.
pub fn duration_words(spec: &str) -> String {
    let Ok(d) = parse_duration(spec) else {
        return spec.to_owned();
    };
    let secs = d.as_secs();
    let (h, m, s) = (secs / 3600, secs % 3600 / 60, secs % 60);
    let parts: Vec<String> = [(h, "h"), (m, "min"), (s, "s")]
        .into_iter()
        .filter(|(n, _)| *n > 0)
        .map(|(n, unit)| format!("{n} {unit}"))
        .collect();
    parts.join(" ")
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_core::model::{Capabilities, EntityCategory, EntityState, Kind, UiOverride};

    fn ent(
        id: &str,
        name: &str,
        device: Option<&str>,
        kind: Kind,
        unit: Option<&str>,
    ) -> EntityView {
        EntityView {
            id: id.into(),
            device_id: None,
            name: name.into(),
            device_name: device.map(Into::into),
            label: None,
            kind,
            protocol_kind: kind,
            device_class: None,
            unit: unit.map(Into::into),
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: "salon".into(),
            part_of: None,
            capabilities: Capabilities::default(),
            ui: UiOverride::default(),
            state: EntityState::default(),
        }
    }

    fn house() -> BTreeMap<String, EntityView> {
        [
            ent("l1", "Lumière TV", None, Kind::Light, None),
            ent("lux", "Entrée", None, Kind::Sensor, Some("lx")),
            ent("led", "LED", Some("LED strip"), Kind::Light, None),
            ent("p1", "Power 1", None, Kind::Number, Some("%")),
            ent("mot", "Motion", None, Kind::BinarySensor, None),
        ]
        .into_iter()
        .map(|e| (e.id.clone(), e))
        .collect()
    }

    fn areas() -> Vec<AreaView> {
        serde_json::from_str(
            r#"[{"id":"salon","name":"Salon","icon":null,"order":null,"entity_count":0,"primary_count":0}]"#,
        )
        .unwrap()
    }

    fn node(json: &str) -> Node {
        serde_json::from_str(json).unwrap()
    }

    fn text(json: &str) -> String {
        let entities = house();
        let areas = areas();
        let flows = vec![(
            "force-heating-loop".to_owned(),
            "Force heating loop".to_owned(),
        )];
        title(
            &node(json),
            &Ctx {
                entities: &entities,
                areas: &areas,
                flows: &flows,
            },
        )
    }

    #[test]
    fn if_eq_bool_reads_on() {
        let n =
            r#"{"id":"a","type":"if","name":"  ","params":{"entity":"l1","op":"eq","value":true}}"#;
        assert_eq!(text(n), "Lumière TV is on?");
    }

    #[test]
    fn if_gt_uses_unit() {
        let n = r#"{"id":"a","type":"if","params":{"entity":"lux","op":"gt","value":16}}"#;
        assert_eq!(text(n), "Entrée > 16 lx?");
    }

    #[test]
    fn command_turn_off() {
        let n = r#"{"id":"a","type":"command","params":{"entity":"led","command":{"type":"turn_off"}}}"#;
        assert_eq!(text(n), "Turn off LED");
    }

    #[test]
    fn command_set_number() {
        let n = r#"{"id":"a","type":"command","params":{"entity":"p1","command":{"type":"set_number","value":100}}}"#;
        assert_eq!(text(n), "Set Power 1 to 100 %");
    }

    #[test]
    fn trigger_to_true() {
        let n = r#"{"id":"a","type":"trigger.state","params":{"entity":"mot","to":true}}"#;
        assert_eq!(text(n), "When Motion turns on");
    }

    #[test]
    fn delay_words() {
        let d = |s: &str| {
            text(&format!(
                r#"{{"id":"a","type":"delay","params":{{"for":"{s}"}}}}"#
            ))
        };
        assert_eq!(d("5m"), "Wait 5 min");
        assert_eq!(d("1h30m"), "Wait 1 h 30 min");
        assert_eq!(duration_words("45s"), "45 s");
        assert_eq!(duration_words("junk"), "junk");
    }

    #[test]
    fn flow_enable_names() {
        let named = r#"{"id":"a","type":"flow.enable","params":{"flow":"force-heating-loop","enabled":false}}"#;
        assert_eq!(text(named), "Disable Force heating loop");
        let own = r#"{"id":"a","type":"flow.enable","params":{"enabled":false}}"#;
        assert_eq!(text(own), "Disable this flow");
    }

    #[test]
    fn name_wins_and_subline() {
        let entities = house();
        let areas = areas();
        let ctx = Ctx {
            entities: &entities,
            areas: &areas,
            flows: &[],
        };
        let n = node(
            r#"{"id":"a","name":"LED is on","type":"if","params":{"entity":"led","op":"eq","value":true}}"#,
        );
        assert_eq!(title(&n, &ctx), "LED is on");
        assert_eq!(subline(&n, &ctx), Some("Salon · LED strip".to_owned()));
    }

    #[test]
    fn unnamed_delay_has_no_subline() {
        let entities = house();
        let ctx = Ctx {
            entities: &entities,
            areas: &[],
            flows: &[],
        };
        let n = node(r#"{"id":"a","type":"delay","params":{"for":"5m"}}"#);
        assert_eq!(subline(&n, &ctx), None);
        assert_eq!(role(&n.kind), Role::Wait);
    }
}
