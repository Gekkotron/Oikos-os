//! Entries of the "Add" picker: what a query offers to put on the canvas. Pure, tested natively.

use oikos_api::EntityView;
use oikos_core::flow::TriggerSignalParams;
use oikos_core::flow::{
    CommandParams, DelayParams, FlowEnableParams, IfParams, Node, NodeKind, NotifyParams, Op,
    Scalar, SunWhich, TriggerScheduleParams, TriggerStateParams,
};
use oikos_core::model::{Command, Kind};
use oikos_core::types::{Shape, type_def};
use serde_json::{Value, json};

use crate::flow_edit::fold_accents;
use crate::flow_text::{Ctx, sentence, subline};
use crate::format::entity_title;
use crate::screens::flow_canvas::sun::sun_params_from_preset;

/// Most entities a query lists, so the list stays short.
const MAX_ENTITIES: usize = 8;

/// The section of the picker an entry sits in, in display order.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Group {
    When,
    And,
    Then,
    Wait,
    Flow,
}

/// One card the picker can add.
#[derive(Clone, Debug, PartialEq)]
pub struct Entry {
    pub group: Group,
    /// The sentence the card will read.
    pub label: String,
    /// The row's second line: what a generic card does, or an entity card's room and device.
    pub sub: Option<String>,
    pub kind: NodeKind,
}

fn entry(group: Group, kind: NodeKind, ctx: &Ctx) -> Entry {
    let node = Node {
        id: String::new(),
        name: None,
        position: None,
        kind,
        extra: Default::default(),
    };
    Entry {
        group,
        label: sentence(&node, ctx),
        sub: subline(&node, ctx),
        kind: node.kind,
    }
}

/// What the picker appends to a card that targets a group entity.
const GROUP_SUFFIX: &str = " (Group)";

fn is_group_id(entity_id: &str) -> bool {
    entity_id.starts_with("group_")
}

fn command(entity: &str, command: Command) -> NodeKind {
    NodeKind::Command(CommandParams {
        entity: entity.to_owned(),
        command,
        extra: Default::default(),
    })
}

fn turn_on() -> Command {
    Command::TurnOn {
        brightness: None,
        color: None,
        effect: None,
    }
}

/// The commands offered for an entity of this kind.
fn commands(entity: &EntityView, current: Option<&Scalar>) -> Vec<Command> {
    match entity.kind {
        Kind::Light | Kind::Switch => vec![turn_on(), Command::TurnOff, Command::Toggle],
        Kind::Cover => vec![Command::Open, Command::Close, Command::Stop],
        Kind::Number => vec![Command::SetNumber {
            value: match current {
                Some(Scalar::Number(n)) => *n,
                _ => 0.0,
            },
        }],
        Kind::Button => vec![Command::Press],
        Kind::Climate => vec![Command::SetTemperature { temperature: 20.0 }],
        Kind::Generic => generic_controls(entity),
        _ => Vec::new(),
    }
}

/// One `control` command per announced control, with a sensible default value for its shape.
fn generic_controls(entity: &EntityView) -> Vec<Command> {
    let Some(caps) = entity.capabilities.generic.as_ref() else {
        return Vec::new();
    };
    let def = type_def(&entity.component);
    caps.controls
        .iter()
        .map(|name| {
            let ctl = def.and_then(|d| d.controls.get(name));
            let value = match ctl {
                Some(c) if c.press => None,
                Some(c) => match c.shape {
                    Shape::OnOff => Some(json!(true)),
                    Shape::Number => Some(json!(0.0)),
                    Shape::Choice => Some(json!(
                        c.field
                            .as_ref()
                            .and_then(|f| caps.options.get(f))
                            .and_then(|o| o.first())
                            .cloned()
                            .unwrap_or_default()
                    )),
                    Shape::Text => Some(json!("")),
                    _ => None,
                },
                None => None,
            };
            Command::Control {
                control: name.clone(),
                value,
            }
        })
        .collect()
}

/// The picker entries for `query`, ordered by [`Group`] then entity title. An empty query gives
/// only the generic cards; otherwise those whose label matches, plus When, And and Then cards for
/// up to eight primary entities whose title or device name holds every word of the query
/// (ignoring case and accents).
pub fn entries(query: &str, ctx: &Ctx) -> Vec<Entry> {
    let needle = fold_accents(query.trim());
    let generic = [
        (
            Group::When,
            NodeKind::TriggerSchedule(TriggerScheduleParams {
                cron: "0 * * * *".into(),
                ..Default::default()
            }),
            Some("Schedule"),
            "At a set time, or on a repeat",
        ),
        (
            Group::When,
            NodeKind::TriggerSun(sun_params_from_preset(SunWhich::Sunset, None)),
            Some("Sun event"),
            "Sunrise, sunset, dusk or dawn",
        ),
        (
            Group::And,
            NodeKind::If(IfParams {
                op: Op::IsDark,
                value: None,
                entity: None,
                missing: None,
                extra: Default::default(),
            }),
            Some("When it's dark"),
            "Between dusk and dawn",
        ),
        (
            Group::Wait,
            NodeKind::Delay(DelayParams {
                for_: "5m".into(),
                ..Default::default()
            }),
            Some("Wait"),
            "A fixed or random delay",
        ),
        (
            Group::Then,
            NodeKind::Notify(NotifyParams {
                title: "Notification".into(),
                ..Default::default()
            }),
            Some("Notify"),
            "A push to your phone",
        ),
        (
            Group::Flow,
            NodeKind::FlowEnable(FlowEnableParams {
                enabled: false,
                flow: None,
                ..Default::default()
            }),
            None,
            "Stops it from running again",
        ),
        (
            Group::Flow,
            NodeKind::FlowEnable(FlowEnableParams {
                enabled: true,
                flow: None,
                ..Default::default()
            }),
            Some("Enable a flow"),
            "Switches another flow on or off",
        ),
    ];
    let mut out: Vec<Entry> = generic
        .into_iter()
        .map(|(group, kind, label, sub)| {
            let mut e = entry(group, kind, ctx);
            e.sub = Some(sub.to_owned());
            // The sentence of a card with no target reads poorly; the picker names it.
            if let Some(label) = label {
                e.label = label.to_owned();
            }
            e
        })
        .filter(|e| needle.is_empty() || fold_accents(&e.label).contains(&needle))
        .collect();

    if !needle.is_empty() {
        let words: Vec<&str> = needle.split_whitespace().collect();
        let mut matches: Vec<(String, &_)> = ctx
            .entities
            .values()
            .filter(|e| e.is_primary())
            .map(|e| (entity_title(e), e))
            .filter(|(title, e)| {
                let hay = fold_accents(&format!(
                    "{title} {}",
                    e.device_name.as_deref().unwrap_or_default()
                ));
                words.iter().all(|w| hay.contains(w))
            })
            .collect();
        matches.sort_by(|a, b| a.0.cmp(&b.0));
        matches.truncate(MAX_ENTITIES);
        for (_, e) in matches {
            let first = out.len();
            let current = Scalar::of_state(&e.state.value);
            out.push(entry(
                Group::When,
                NodeKind::TriggerState(TriggerStateParams {
                    entity: e.id.clone(),
                    ..Default::default()
                }),
                ctx,
            ));
            out.push(entry(
                Group::And,
                NodeKind::If(IfParams {
                    entity: Some(e.id.clone()),
                    op: Op::Eq,
                    value: Some(current.as_ref().map_or(Value::Bool(true), Scalar::to_json)),
                    missing: None,
                    extra: Default::default(),
                }),
                ctx,
            ));
            if let Some(caps) = e
                .capabilities
                .generic
                .as_ref()
                .filter(|c| !c.signals.is_empty())
            {
                let signals = caps
                    .signals
                    .iter()
                    .map(|s| Some(s.clone()))
                    .chain(std::iter::once(None));
                for signal in signals {
                    out.push(entry(
                        Group::When,
                        NodeKind::TriggerSignal(TriggerSignalParams {
                            entity: e.id.clone(),
                            signal,
                            ..Default::default()
                        }),
                        ctx,
                    ));
                }
            }
            for c in commands(e, current.as_ref()) {
                out.push(entry(Group::Then, command(&e.id, c), ctx));
            }
            if is_group_id(&e.id) {
                for entry in &mut out[first..] {
                    entry.label.push_str(GROUP_SUFFIX);
                }
            }
        }
    }
    out.sort_by_key(|e| e.group);
    out
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use oikos_api::{AreaView, EntityView};
    use oikos_core::model::{Capabilities, EntityCategory, EntityState, StateValue, UiOverride};

    use super::*;

    fn ent(id: &str, name: &str, kind: Kind) -> EntityView {
        EntityView {
            id: id.into(),
            device_id: None,
            name: name.into(),
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
            area: "salon".into(),
            part_of: None,
            capabilities: Capabilities::default(),
            ui: UiOverride::default(),
            state: EntityState::default(),
        }
    }

    fn house(list: Vec<EntityView>) -> BTreeMap<String, EntityView> {
        list.into_iter().map(|e| (e.id.clone(), e)).collect()
    }

    fn run(query: &str, entities: &BTreeMap<String, EntityView>) -> Vec<Entry> {
        let areas: Vec<AreaView> = Vec::new();
        let ctx = Ctx {
            entities,
            areas: &areas,
            flows: &[],
        };
        entries(query, &ctx)
    }

    fn count(entries: &[Entry], group: Group) -> usize {
        entries.iter().filter(|e| e.group == group).count()
    }

    #[test]
    fn empty_query_has_generic_entries() {
        let entities = house(vec![ent("l1", "Lumière table", Kind::Light)]);
        let got = run("", &entities);
        let labels: Vec<_> = got.iter().map(|e| (e.group, e.label.as_str())).collect();
        assert_eq!(
            labels,
            [
                (Group::When, "Schedule"),
                (Group::When, "Sun event"),
                (Group::And, "When it's dark"),
                (Group::Then, "Notify"),
                (Group::Wait, "Wait"),
                (Group::Flow, "Disable this flow"),
                (Group::Flow, "Enable a flow"),
            ]
        );
        let subs: Vec<_> = got.iter().map(|e| e.sub.as_deref()).collect();
        assert_eq!(
            subs,
            [
                Some("At a set time, or on a repeat"),
                Some("Sunrise, sunset, dusk or dawn"),
                Some("Between dusk and dawn"),
                Some("A push to your phone"),
                Some("A fixed or random delay"),
                Some("Stops it from running again"),
                Some("Switches another flow on or off"),
            ]
        );
        assert!(matches!(
            &got[0].kind,
            NodeKind::TriggerSchedule(p) if p.cron == "0 * * * *"
        ));
        assert!(matches!(&got[3].kind, NodeKind::Notify(p) if p.title == "Notification"));
        assert!(matches!(&got[4].kind, NodeKind::Delay(p) if p.for_ == "5m"));
    }

    #[test]
    fn entity_entries_carry_the_subline() {
        let entities = house(vec![ent("l1", "Lumière table", Kind::Light)]);
        let areas = vec![AreaView {
            id: "salon".into(),
            name: "Salon".into(),
            icon: None,
            order: None,
            entity_count: 1,
            primary_count: 1,
            group_names: Default::default(),
            group_favorites: Default::default(),
        }];
        let ctx = Ctx {
            entities: &entities,
            areas: &areas,
            flows: &[],
        };
        let got = entries("table", &ctx);
        let entity: Vec<_> = got.iter().filter(|e| e.label.contains("Lumière")).collect();
        assert!(!entity.is_empty());
        for e in entity {
            assert_eq!(e.sub.as_deref(), Some("Salon"), "{}", e.label);
        }
    }

    #[test]
    fn picker_offers_lock_controls() {
        let mut lock = ent("k1", "Front door", Kind::Generic);
        lock.component = "lock".into();
        lock.capabilities.generic = Some(oikos_core::types::GenericCaps {
            controls: vec!["locked".into(), "open".into()],
            ..Default::default()
        });
        let cmds = commands(&lock, None);
        assert!(cmds.contains(&Command::Control {
            control: "locked".into(),
            value: Some(json!(true))
        }));
        assert!(cmds.contains(&Command::Control {
            control: "open".into(),
            value: None
        }));
    }

    #[test]
    fn picker_offers_remote_signals() {
        let mut remote = ent("r1", "Remote", Kind::Generic);
        remote.component = "device_automation".into();
        remote.capabilities.generic = Some(oikos_core::types::GenericCaps {
            signals: vec!["on".into(), "off".into()],
            ..Default::default()
        });
        let got = run("remote", &house(vec![remote]));
        let signals: Vec<Option<String>> = got
            .iter()
            .filter_map(|e| match &e.kind {
                NodeKind::TriggerSignal(p) => Some(p.signal.clone()),
                _ => None,
            })
            .collect();
        assert_eq!(signals, [Some("on".into()), Some("off".into()), None]);
    }

    #[test]
    fn table_query_lists_light_entries() {
        let mut light = ent("l1", "Lumière table", Kind::Light);
        light.state.value = StateValue::OnOff(true);
        let entities = house(vec![light, ent("l2", "Bedroom", Kind::Light)]);
        let got = run("table", &entities);
        assert_eq!(count(&got, Group::When), 1);
        assert_eq!(count(&got, Group::And), 1);
        assert_eq!(count(&got, Group::Then), 3);
        let label = |g| got.iter().find(|e| e.group == g).unwrap().label.clone();
        assert_eq!(label(Group::When), "When Lumière table changes");
        assert_eq!(label(Group::And), "Lumière table is on?");
        let then: Vec<_> = got
            .iter()
            .filter(|e| e.group == Group::Then)
            .map(|e| e.label.as_str())
            .collect();
        assert_eq!(
            then,
            [
                "Turn on Lumière table",
                "Turn off Lumière table",
                "Toggle Lumière table"
            ]
        );
        let groups: Vec<_> = got.iter().map(|e| e.group).collect();
        assert!(groups.windows(2).all(|w| w[0] <= w[1]));
    }

    #[test]
    fn accent_insensitive() {
        let entities = house(vec![ent("l1", "Lumière", Kind::Light)]);
        assert_eq!(count(&run("lumiere", &entities), Group::When), 1);
        assert_eq!(count(&run("LUMIÈRE", &entities), Group::When), 1);
    }

    #[test]
    fn device_name_and_words_match() {
        let weather = |id: &str, name: &str| {
            let mut e = ent(id, name, Kind::Sensor);
            e.device_name = Some("Weather".into());
            e
        };
        let entities = house(vec![
            weather("w1", "Weather Temperature"),
            weather("w2", "Weather Rain next hour"),
            ent("l1", "Lumière", Kind::Light),
        ]);
        assert_eq!(count(&run("weather", &entities), Group::When), 2);
        let rain = run("weather rain", &entities);
        assert_eq!(count(&rain, Group::When), 1);
        assert_eq!(
            rain.iter().find(|e| e.group == Group::When).unwrap().label,
            "When Rain next hour changes"
        );
    }

    #[test]
    fn caps_at_eight_entities() {
        let list = (0..12)
            .map(|i| ent(&format!("l{i}"), &format!("Lampe {i:02}"), Kind::Light))
            .collect();
        let got = run("lampe", &house(list));
        assert_eq!(count(&got, Group::When), 8);
        assert_eq!(count(&got, Group::Then), 24);
    }

    #[test]
    fn group_entries_carry_the_group_tag() {
        let entities = house(vec![
            ent("group_living", "Living lights", Kind::Light),
            ent("l1", "Living lamp", Kind::Light),
        ]);
        let got = run("living", &entities);
        let tagged = |e: &Entry| e.label.ends_with(" (Group)");
        let group_entries: Vec<_> = got
            .iter()
            .filter(|e| e.label.contains("Living lights"))
            .collect();
        assert!(!group_entries.is_empty() && group_entries.iter().all(|e| tagged(e)));
        assert!(
            got.iter()
                .filter(|e| e.label.contains("Living lamp"))
                .all(|e| !tagged(e))
        );
    }

    #[test]
    fn sensor_has_no_actions() {
        let entities = house(vec![ent("s1", "Entrée", Kind::Sensor)]);
        let got = run("entree", &entities);
        assert_eq!(count(&got, Group::When), 1);
        assert_eq!(count(&got, Group::And), 1);
        assert_eq!(count(&got, Group::Then), 0);
    }

    #[test]
    fn hidden_entities_excluded() {
        let mut hidden = ent("l1", "Lumière", Kind::Light);
        hidden.ui.hidden = true;
        let mut diagnostic = ent("l2", "Lumière diag", Kind::Light);
        diagnostic.category = EntityCategory::Diagnostic;
        let entities = house(vec![hidden, diagnostic]);
        assert_eq!(run("lumiere", &entities).len(), 0);
    }

    #[test]
    fn generic_entries_filter_by_query() {
        let got = run("wai", &BTreeMap::new());
        assert_eq!(got.len(), 1);
        assert_eq!(got[0].group, Group::Wait);
    }

    #[test]
    fn number_and_climate_defaults() {
        let mut n = ent("n1", "Seuil", Kind::Number);
        n.state.value = StateValue::Number(42.0);
        let entities = house(vec![n, ent("c1", "Clim", Kind::Climate)]);
        let got = run("seuil", &entities);
        let NodeKind::Command(p) = &got.last().unwrap().kind else {
            panic!()
        };
        assert_eq!(p.command, Command::SetNumber { value: 42.0 });
        let got = run("clim", &entities);
        let NodeKind::Command(p) = &got.last().unwrap().kind else {
            panic!()
        };
        assert_eq!(p.command, Command::SetTemperature { temperature: 20.0 });
    }
}
