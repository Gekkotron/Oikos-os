//! Pure presentation of the Energy tab: entity views → energy inputs, diagram geometry, labels.
//! Geometry and wording are ported from the app's `FlowLayout.kt`, `EnergyFormat.kt` and
//! `EnergyDescription.kt`.

use std::collections::BTreeMap;

use oikos_api::EntityView;
use oikos_core::energy::{
    ARC_SLOTS, EnergyEntity, EnergyNode, EnergyRole, EnergyUi, Flow, IDLE_W, NodeKind, NodeStatus,
    PowerUnits,
};
use oikos_core::model::StateValue;

/// The diagram's viewBox, in px.
pub const VIEW_W: f64 = 260.0;
pub const VIEW_H: f64 = 250.0;
/// A node circle's radius.
pub const NODE_R: f64 = 22.0;
/// How far an edge stays from a node's centre.
pub const CLEARANCE: f64 = 26.0;

const SOLAR: (f64, f64) = (0.5, 0.12);
const HOUSE: (f64, f64) = (0.5, 0.52);
const GRID: (f64, f64) = (0.12, 0.52);
const BOTTOM_LEFT: (f64, f64) = (0.15, 0.86);

/// The arc slots right of the house, by how many consumer nodes are drawn (1..=3).
fn arc_slots(n: usize) -> &'static [(f64, f64)] {
    match n.clamp(1, ARC_SLOTS) {
        1 => &[(0.86, 0.52)],
        2 => &[(0.86, 0.34), (0.86, 0.72)],
        _ => &[(0.82, 0.2), (0.88, 0.52), (0.82, 0.84)],
    }
}

pub use oikos_api::energy_entity;

pub fn energy_entities(entities: &BTreeMap<String, EntityView>) -> Vec<EnergyEntity> {
    oikos_api::energy_entities(entities.values())
}

/// True when the solar roles together produce at least 10 W, so the Energy tab glyph lights up.
pub fn any_solar_active<'a>(entities: impl Iterator<Item = &'a EntityView>) -> bool {
    let watts: f64 = entities
        .filter(|e| {
            e.ui.energy_role.as_deref().and_then(EnergyRole::parse) == Some(EnergyRole::Solar)
        })
        .filter_map(
            |e| match (&e.state.value, PowerUnits::factor(e.unit.as_deref())) {
                (StateValue::Number(n), Some(f)) => Some(*n * f),
                _ => None,
            },
        )
        .sum();
    watts >= IDLE_W
}

/// Node centres in viewBox px, by node id.
pub fn positions(diagram: &[EnergyNode]) -> BTreeMap<String, (f64, f64)> {
    let px = |(x, y): (f64, f64)| (x * VIEW_W, y * VIEW_H);
    let arc: Vec<&EnergyNode> = diagram
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Consumer | NodeKind::More))
        .collect();
    let mut out = BTreeMap::new();
    for n in diagram {
        let at = match n.kind {
            NodeKind::Solar => SOLAR,
            NodeKind::Grid => GRID,
            NodeKind::House => HOUSE,
            NodeKind::Battery | NodeKind::Water => BOTTOM_LEFT,
            NodeKind::Consumer | NodeKind::More => continue,
        };
        out.insert(n.id.clone(), px(at));
    }
    let slots = arc_slots(arc.len());
    for (n, slot) in arc.iter().zip(slots) {
        out.insert(n.id.clone(), px(*slot));
    }
    out
}

/// A cubic edge, node to house.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Edge {
    pub start: (f64, f64),
    pub c1: (f64, f64),
    pub c2: (f64, f64),
    pub end: (f64, f64),
}

impl Edge {
    pub fn reversed(&self) -> Edge {
        Edge {
            start: self.end,
            c1: self.c2,
            c2: self.c1,
            end: self.start,
        }
    }

    /// The SVG path: `M x y C x1 y1, x2 y2, x y`.
    pub fn path(&self) -> String {
        format!(
            "M {} {} C {} {}, {} {}, {} {}",
            self.start.0,
            self.start.1,
            self.c1.0,
            self.c1.1,
            self.c2.0,
            self.c2.1,
            self.end.0,
            self.end.1
        )
    }
}

/// The edge between `node` and the house `hub`, kept `clearance` px from both centres. A side
/// node joins the house side to side at circle-centre height; a node above (solar) passes
/// `below`, the bottom of its labels, as its start height instead.
pub fn edge(node: (f64, f64), hub: (f64, f64), clearance: f64, below: Option<f64>) -> Edge {
    if let Some(below) = below {
        let start = (node.0, below);
        let end = (hub.0, hub.1 - clearance);
        let mid_y = (start.1 + end.1) / 2.0;
        return Edge {
            start,
            c1: (start.0, mid_y),
            c2: (end.0, mid_y),
            end,
        };
    }
    let side = if node.0 < hub.0 { 1.0 } else { -1.0 };
    let start = (node.0 + side * clearance, node.1);
    let end = (hub.0 - side * clearance, hub.1);
    let mid_x = (start.0 + end.0) / 2.0;
    Edge {
        start,
        c1: (mid_x, start.1),
        c2: (mid_x, end.1),
        end,
    }
}

/// 0 at 10 W, 1 at 10 kW and above, on a log scale.
pub fn level(watts: Option<f64>) -> f64 {
    match watts {
        Some(w) if w >= 10.0 => ((w.log10() - 1.0) / 3.0).clamp(0.0, 1.0),
        _ => 0.0,
    }
}

pub fn stroke(watts: Option<f64>, flow: Flow) -> f64 {
    if flow == Flow::Idle {
        1.0
    } else {
        1.0 + 5.0 * level(watts)
    }
}

/// Dash cycles per second: 0.5 at 10 W to 2 at 10 kW.
pub fn speed(watts: Option<f64>) -> f64 {
    0.5 + 1.5 * level(watts)
}

const THIN_SPACE: char = '\u{2009}';

/// `value` with `min..=max` fraction digits and the integer part grouped by thin spaces.
fn number(value: f64, min: usize, max: usize) -> String {
    let fixed = format!("{:.max$}", value.abs());
    let (int, frac) = fixed.split_once('.').unwrap_or((fixed.as_str(), ""));
    let mut frac = frac.to_string();
    while frac.len() > min && frac.ends_with('0') {
        frac.pop();
    }
    let mut grouped = String::new();
    for (i, c) in int.chars().enumerate() {
        if i > 0 && (int.len() - i) % 3 == 0 {
            grouped.push(THIN_SPACE);
        }
        grouped.push(c);
    }
    let sign = if value < 0.0 && fixed.chars().any(|c| c.is_ascii_digit() && c != '0') {
        "-"
    } else {
        ""
    };
    if frac.is_empty() {
        format!("{sign}{grouped}")
    } else {
        format!("{sign}{grouped}.{frac}")
    }
}

/// "3.2 kW" from 1000 W, whole watts below, "—" when unknown.
pub fn power_text(watts: Option<f64>) -> String {
    match watts {
        None => "—".to_string(),
        Some(w) if w >= 1_000.0 => format!("{} kW", number(w / 1_000.0, 1, 1)),
        Some(w) => format!("{} W", number(w, 0, 0)),
    }
}

/// A lifetime energy counter: "6 584 kWh", "32.7 kWh", "—" when unknown.
pub fn energy_text(kwh: Option<f64>) -> String {
    kwh.map_or_else(|| "—".to_string(), |v| format!("{} kWh", number(v, 0, 1)))
}

/// A reading with its unit, one decimal at most: "12.5 L".
pub fn amount_text(value: f64, unit: Option<&str>) -> String {
    match unit.map(str::trim).filter(|u| !u.is_empty()) {
        Some(u) => format!("{} {u}", number(value, 0, 1)),
        None => number(value, 0, 1),
    }
}

/// A meter index to three decimals: "601.359 m³".
pub fn meter_text(value: f64, unit: Option<&str>) -> String {
    match unit.map(str::trim).filter(|u| !u.is_empty()) {
        Some(u) => format!("{} {u}", number(value, 0, 3)),
        None => number(value, 0, 3),
    }
}

/// The node's power, led by ↙ (into the house) or ↗ (out of it) for the grid and the battery.
pub fn grid_text(node: &EnergyNode) -> String {
    let arrow = match (node.kind, node.flow) {
        (NodeKind::Grid | NodeKind::Battery, Flow::In) => "↙ ",
        (NodeKind::Grid | NodeKind::Battery, Flow::Out) => "↗ ",
        _ => "",
    };
    format!("{arrow}{}", power_text(node.value))
}

/// What a node shows under its circle: "Unavailable", "—", the water reading or the power.
pub fn node_value_text(node: &EnergyNode) -> String {
    if node.status == NodeStatus::Unavailable {
        return "Unavailable".to_string();
    }
    match (node.kind, node.value) {
        (_, None) => "—".to_string(),
        (NodeKind::Water, Some(v)) => amount_text(v, node.unit.as_deref()),
        _ => grid_text(node),
    }
}

/// A node's name: the consumer's own, the role's, or "+N" for the folded consumers.
pub fn node_label(node: &EnergyNode) -> String {
    if node.kind == NodeKind::More {
        return format!("+{}", node.count);
    }
    if let Some(name) = &node.name {
        return name.clone();
    }
    match node.kind {
        NodeKind::Solar => "Solar",
        NodeKind::Grid => "Grid",
        NodeKind::Battery => "Battery",
        NodeKind::House => "House",
        NodeKind::Water => "Water",
        NodeKind::Consumer | NodeKind::More => "Consumer",
    }
    .to_string()
}

/// "3.2 kilowatts" from 1000 W, else "400 watts".
fn spoken(watts: f64) -> String {
    if watts >= 1_000.0 {
        format!("{} kilowatts", number(watts / 1_000.0, 1, 1))
    } else {
        format!("{} watts", number(watts, 0, 0))
    }
}

/// The diagram as one sentence, in the app's order: the role nodes as drawn, every consumer,
/// then water. Unknown values and idle grid or battery are left out.
pub fn aria_sentence(ui: &EnergyUi) -> String {
    let part = |n: &EnergyNode| -> Option<String> {
        let v = n.value?;
        match (n.kind, n.flow) {
            (NodeKind::Solar, _) => Some(format!("Solar {} to the house.", spoken(v))),
            (NodeKind::House, _) => Some(format!("House uses {}.", spoken(v))),
            (NodeKind::Grid, Flow::In) => Some(format!("Importing {} from the grid.", spoken(v))),
            (NodeKind::Grid, Flow::Out) => Some(format!("Exporting {} to the grid.", spoken(v))),
            (NodeKind::Battery, Flow::In) => Some(format!("Battery supplies {}.", spoken(v))),
            (NodeKind::Battery, Flow::Out) => Some(format!("Battery charging at {}.", spoken(v))),
            (NodeKind::Grid | NodeKind::Battery, Flow::Idle) => None,
            (NodeKind::Consumer, _) => Some(format!(
                "{} {}.",
                n.name.as_deref().unwrap_or("Consumer"),
                spoken(v)
            )),
            (NodeKind::Water, _) => Some(format!("Water {}.", amount_text(v, n.unit.as_deref()))),
            (NodeKind::More, _) => None,
        }
    };
    let roles = ui.diagram.iter().filter(|n| {
        !matches!(
            n.kind,
            NodeKind::Consumer | NodeKind::More | NodeKind::Water
        )
    });
    let water = ui.cards.iter().find(|n| n.kind == NodeKind::Water);
    roles
        .chain(ui.consumers.iter())
        .chain(water)
        .filter_map(part)
        .collect::<Vec<_>>()
        .join(" ")
}

/// Unix milliseconds: the consumer order throttle's clock.
pub fn now_ms() -> u64 {
    #[cfg(target_arch = "wasm32")]
    {
        js_sys::Date::now() as u64
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis() as u64)
    }
}

// --- History tiles (kWh today) ---

/// Per-counter kWh-today values for the derived house, split into additions and subtractions.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct HouseTodayParts {
    pub plus: Vec<Option<f64>>,
    pub minus: Vec<Option<f64>>,
}

/// Sum `plus - minus` from the derived-house parts. Returns `None` if any part is missing: a
/// partial sum would understate consumption and mislead the viewer.
pub fn derived_house_today(parts: &HouseTodayParts) -> Option<f64> {
    let mut acc = 0.0;
    for v in parts.plus.iter().chain(parts.minus.iter()) {
        v.as_ref()?;
    }
    for v in &parts.plus {
        acc += v.unwrap_or(0.0);
    }
    for v in &parts.minus {
        acc -= v.unwrap_or(0.0);
    }
    Some(acc)
}

/// Returns `interval_ms` when the tab is visible, `None` when it is hidden — so the caller
/// doesn't schedule another timer while nobody is watching.
pub fn next_fire_delay(hidden: bool, interval_ms: u32) -> Option<u32> {
    if hidden { None } else { Some(interval_ms) }
}

/// Whether a mount-edge Effect should fire this run. Returns true only on the first run where
/// `has_now` is true and the previous run (if any) was `false` — the transition from "empty" to
/// "loaded", once. Steady-state re-runs (entity map tick, same loaded-ness) are no-ops.
pub fn should_fire_mount(prev: Option<bool>, has_now: bool) -> bool {
    has_now && !prev.unwrap_or(false)
}

/// The derived house's `plus` and `minus` counter ids. `None` when the house has no derived
/// total (no solar or grid, or the inputs are not fully meter-paired).
pub fn plus_minus_ids(
    by_role: &BTreeMap<EnergyRole, Vec<&EnergyEntity>>,
    all: &[EnergyEntity],
) -> Option<(Vec<String>, Vec<String>)> {
    let m = oikos_core::energy::EnergyMeters::house_meters(by_role, all)?;
    Some((
        m.plus.into_iter().map(|e| e.id.clone()).collect(),
        m.minus.into_iter().map(|e| e.id.clone()).collect(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_core::energy::EnergyFlow;

    #[test]
    fn group_entities_never_reach_the_energy_list() {
        use oikos_core::model::{Capabilities, EntityCategory, EntityState, Kind, UiOverride};
        let view = |id: &str| EntityView {
            id: id.into(),
            device_id: None,
            name: id.into(),
            device_name: None,
            label: None,
            kind: Kind::Sensor,
            component: Kind::Sensor.component().to_string(),
            protocol_kind: Kind::Sensor,
            device_class: None,
            unit: None,
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: "x".into(),
            part_of: None,
            capabilities: Capabilities::default(),
            ui: UiOverride::default(),
            state: EntityState::default(),
        };
        let map: BTreeMap<String, EntityView> = ["power", "group_all"]
            .into_iter()
            .map(|id| (id.to_owned(), view(id)))
            .collect();
        let ids: Vec<_> = energy_entities(&map).into_iter().map(|e| e.id).collect();
        assert_eq!(ids, ["power"]);
    }

    fn ent(id: &str, name: &str, role: EnergyRole, unit: &str, value: f64) -> EnergyEntity {
        EnergyEntity {
            id: id.to_string(),
            device_id: None,
            name: name.to_string(),
            label: None,
            unit: Some(unit.to_string()),
            value: Some(value),
            available: true,
            role: Some(role),
            invert: false,
            order: None,
            icon: None,
            meter: None,
        }
    }

    fn consumer(n: usize) -> EnergyEntity {
        ent(
            &format!("c{n}"),
            &format!("C{n}"),
            EnergyRole::Consumer,
            "W",
            100.0 * (n as f64 + 1.0),
        )
    }

    #[test]
    fn positions_follow_layout_b() {
        let mut es = vec![
            ent("s", "S", EnergyRole::Solar, "W", 100.0),
            ent("g", "G", EnergyRole::Grid, "W", 100.0),
            ent("w", "Water", EnergyRole::Water, "L", 5.0),
        ];
        es.extend((0..5).map(consumer));
        let ui = EnergyFlow::build(&es, 0, None);
        let p = positions(&ui.diagram);
        assert_eq!(p["solar"], (130.0, 30.0));
        assert_eq!(p["house"], (130.0, 130.0));
        assert_eq!(p["grid"], (31.2, 130.0));
        assert_eq!(p["water"], (39.0, 215.0));
        let arc: Vec<(f64, f64)> = ui
            .diagram
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Consumer | NodeKind::More))
            .map(|n| p[&n.id])
            .collect();
        assert_eq!(arc, vec![(213.2, 50.0), (228.8, 130.0), (213.2, 210.0)]);

        es.push(ent("b", "B", EnergyRole::Battery, "W", 5.0));
        let ui = EnergyFlow::build(&es, 0, None);
        let p = positions(&ui.diagram);
        assert_eq!(p["battery"], (39.0, 215.0));
        assert!(!p.contains_key("water"));
    }

    #[test]
    fn edge_keeps_clearance() {
        let grid = (31.2, 130.0);
        let hub = (130.0, 130.0);
        let e = edge(grid, hub, CLEARANCE, None);
        assert_eq!(e.start, (grid.0 + CLEARANCE, 130.0));
        assert_eq!(e.end, (hub.0 - CLEARANCE, 130.0));
        let consumer = edge((228.8, 130.0), hub, CLEARANCE, None);
        assert_eq!(consumer.start.0, 228.8 - CLEARANCE);
        assert_eq!(consumer.end.0, 130.0 + CLEARANCE);
        let solar = edge((130.0, 30.0), hub, CLEARANCE, Some(80.0));
        assert_eq!(solar.start, (130.0, 80.0));
        assert_eq!(solar.end, (130.0, 130.0 - CLEARANCE));
        assert_eq!(e.reversed().start, e.end);
        assert_eq!(e.reversed().reversed(), e);
        assert_eq!(
            edge((0.0, 0.0), (100.0, 0.0), 26.0, None).path(),
            "M 26 0 C 50 0, 50 0, 74 0"
        );
    }

    #[test]
    fn stroke_and_speed() {
        assert_eq!(stroke(Some(10.0), Flow::In), 1.0);
        assert_eq!(speed(Some(10.0)), 0.5);
        assert_eq!(stroke(Some(10_000.0), Flow::In), 6.0);
        assert_eq!(speed(Some(10_000.0)), 2.0);
        assert_eq!(stroke(None, Flow::Idle), 1.0);
        assert_eq!(speed(None), 0.5);
        assert_eq!(level(Some(1_000_000.0)), 1.0);
    }

    #[test]
    fn texts() {
        assert_eq!(power_text(Some(3200.0)), "3.2 kW");
        assert_eq!(power_text(Some(253.3)), "253 W");
        assert_eq!(power_text(None), "—");
        assert_eq!(energy_text(Some(6584.04)), "6\u{2009}584 kWh");
        assert_eq!(energy_text(Some(32.7)), "32.7 kWh");
        assert_eq!(energy_text(None), "—");
        assert_eq!(amount_text(12.5, Some("L")), "12.5 L");
        assert_eq!(meter_text(601.359, Some("m³")), "601.359 m³");
    }

    #[test]
    fn aria_sentence_follows_the_app() {
        // The expectation is written from EnergyDescription.kt, not from the spec's rough
        // example: role nodes in diagram order (solar, grid, battery, house), then every
        // consumer by power, kilowatts only from 1000 W and watts below.
        let es = vec![
            ent("s", "S", EnergyRole::Solar, "W", 3200.0),
            ent("g", "G", EnergyRole::Grid, "W", -400.0),
            ent("a", "Water heater", EnergyRole::Consumer, "W", 1700.0),
            ent("b", "AC", EnergyRole::Consumer, "W", 600.0),
        ];
        let ui = EnergyFlow::build(&es, 0, None);
        assert_eq!(
            aria_sentence(&ui),
            "Solar 3.2 kilowatts to the house. Exporting 400 watts to the grid. \
             House uses 2.8 kilowatts. Water heater 1.7 kilowatts. AC 600 watts."
        );
    }

    #[test]
    fn labels_and_values() {
        let es = vec![
            ent("g", "G", EnergyRole::Grid, "W", -400.0),
            ent("w", "Water", EnergyRole::Water, "L", 5.0),
        ];
        let ui = EnergyFlow::build(&es, 0, None);
        let grid = ui.node("grid").unwrap();
        assert_eq!(grid_text(grid), "↗ 400 W");
        assert_eq!(node_label(grid), "Grid");
        assert_eq!(node_value_text(ui.node("water").unwrap()), "5 L");
    }

    #[test]
    fn solar_glyph_lights_with_units() {
        use oikos_core::model::{Capabilities, EntityCategory, EntityState, Kind, UiOverride};
        let view = |unit: &str, v: f64| EntityView {
            id: "s".into(),
            device_id: None,
            name: "s".into(),
            device_name: None,
            label: None,
            kind: Kind::Sensor,
            component: "sensor".to_string(),
            protocol_kind: Kind::Sensor,
            device_class: None,
            unit: Some(unit.into()),
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: "a".into(),
            part_of: None,
            capabilities: Capabilities::default(),
            ui: UiOverride {
                energy_role: Some("solar".into()),
                ..Default::default()
            },
            state: EntityState {
                value: StateValue::Number(v),
                ..Default::default()
            },
        };
        assert!(any_solar_active([view("kW", 0.5)].iter()));
        assert!(!any_solar_active([view("W", 9.0)].iter()));
        assert!(!any_solar_active([view("Wh", 900.0)].iter()));
    }

    #[test]
    fn next_fire_delay_pauses_when_hidden() {
        assert_eq!(next_fire_delay(false, 300_000), Some(300_000));
        assert_eq!(next_fire_delay(true, 300_000), None);
    }

    #[test]
    fn should_fire_mount_fires_on_loaded_edge_only() {
        assert!(should_fire_mount(None, true));
        assert!(!should_fire_mount(None, false));
        assert!(!should_fire_mount(Some(true), true));
        assert!(should_fire_mount(Some(false), true));
    }

    #[test]
    fn derived_house_today_sums_plus_minus_minus() {
        let m = HouseTodayParts {
            plus: vec![Some(10.0), Some(4.0)],
            minus: vec![Some(1.5)],
        };
        assert_eq!(derived_house_today(&m), Some(12.5));
    }

    #[test]
    fn derived_house_today_is_none_when_any_part_is_missing() {
        let partial = HouseTodayParts {
            plus: vec![Some(10.0), None],
            minus: vec![Some(1.0)],
        };
        assert_eq!(derived_house_today(&partial), None);
        let partial = HouseTodayParts {
            plus: vec![Some(10.0)],
            minus: vec![None],
        };
        assert_eq!(derived_house_today(&partial), None);
    }

    #[test]
    fn derived_house_today_handles_empty_parts() {
        assert_eq!(derived_house_today(&HouseTodayParts::default()), Some(0.0));
    }
}
