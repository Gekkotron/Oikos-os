//! Geometry of the "glowing circles" Classic energy diagram: circle layouts, node placement and
//! the quadratic curves of the wires. Pure Rust, no DOM, so it is unit-tested natively.

use std::collections::BTreeMap;

use oikos_core::energy::{EnergyNode, NodeKind};

/// One viewBox's constants. Positions are in viewBox units (px).
#[derive(Debug, PartialEq)]
pub struct CirclesLayout {
    pub view_w: f64,
    pub view_h: f64,
    pub r_big: f64,
    pub r_house: f64,
    pub r_small: f64,
    pub house: (f64, f64),
    /// Solar / grid without a battery or water.
    pub solar: (f64, f64),
    pub grid: (f64, f64),
    /// Solar / grid when the side slot is taken.
    pub solar_side: (f64, f64),
    pub grid_side: (f64, f64),
    /// Battery or water.
    pub side: (f64, f64),
    pub one: [(f64, f64); 1],
    pub two: [(f64, f64); 2],
    pub three: [(f64, f64); 3],
    pub bend_solar: f64,
    pub bend_grid: f64,
    pub bend_battery: f64,
    pub bend_three: [f64; 3],
}

impl CirclesLayout {
    pub const WIDE: CirclesLayout = CirclesLayout {
        view_w: 760.0,
        view_h: 380.0,
        r_big: 46.0,
        r_house: 54.0,
        r_small: 30.0,
        house: (400.0, 214.0),
        solar: (260.0, 74.0),
        grid: (96.0, 214.0),
        solar_side: (260.0, 74.0),
        grid_side: (96.0, 214.0),
        side: (190.0, 330.0),
        one: [(650.0, 214.0)],
        two: [(640.0, 140.0), (640.0, 288.0)],
        three: [(630.0, 80.0), (655.0, 214.0), (630.0, 348.0)],
        bend_solar: 26.0,
        bend_grid: -22.0,
        bend_battery: 0.0,
        bend_three: [-18.0, 0.0, 18.0],
    };

    pub const NARROW: CirclesLayout = CirclesLayout {
        view_w: 360.0,
        view_h: 470.0,
        r_big: 44.0,
        r_house: 54.0,
        r_small: 30.0,
        house: (180.0, 240.0),
        solar: (84.0, 74.0),
        grid: (276.0, 74.0),
        solar_side: (60.0, 74.0),
        grid_side: (300.0, 74.0),
        side: (180.0, 66.0),
        one: [(180.0, 404.0)],
        two: [(110.0, 404.0), (250.0, 404.0)],
        three: [(56.0, 400.0), (180.0, 412.0), (304.0, 400.0)],
        bend_solar: 22.0,
        bend_grid: -22.0,
        bend_battery: 0.0,
        bend_three: [16.0, 0.0, -16.0],
    };
}

pub fn for_width(narrow: bool) -> &'static CirclesLayout {
    if narrow {
        &CirclesLayout::NARROW
    } else {
        &CirclesLayout::WIDE
    }
}

/// A placed circle: its centre and radius.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Placed {
    pub at: (f64, f64),
    pub r: f64,
}

/// Places every node of the diagram, keyed by node id.
pub fn place(diagram: &[EnergyNode], layout: &CirclesLayout) -> BTreeMap<String, Placed> {
    let side_taken = diagram
        .iter()
        .any(|n| matches!(n.kind, NodeKind::Battery | NodeKind::Water));
    let slots = diagram
        .iter()
        .filter(|n| matches!(n.kind, NodeKind::Consumer | NodeKind::More))
        .count();
    let mut arc = 0usize;
    let mut out = BTreeMap::new();
    for n in diagram {
        let (at, r) = match n.kind {
            NodeKind::Solar => (
                if side_taken {
                    layout.solar_side
                } else {
                    layout.solar
                },
                layout.r_big,
            ),
            NodeKind::Grid => (
                if side_taken {
                    layout.grid_side
                } else {
                    layout.grid
                },
                layout.r_big,
            ),
            NodeKind::Battery => (layout.side, layout.r_big),
            NodeKind::Water => (layout.side, layout.r_small),
            NodeKind::House => (layout.house, layout.r_house),
            NodeKind::Consumer | NodeKind::More => {
                let i = arc;
                arc += 1;
                let at = match slots {
                    0 | 1 => layout.one[0],
                    2 => layout.two[i.min(1)],
                    _ => layout.three[i.min(2)],
                };
                (at, layout.r_small)
            }
        };
        out.insert(n.id.clone(), Placed { at, r });
    }
    out
}

/// A quadratic bezier wire.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Curve {
    pub from: (f64, f64),
    pub ctrl: (f64, f64),
    pub to: (f64, f64),
}

impl Curve {
    /// SVG path data, `M x y Q cx cy x y`.
    pub fn path(&self) -> String {
        format!(
            "M {} {} Q {} {} {} {}",
            self.from.0, self.from.1, self.ctrl.0, self.ctrl.1, self.to.0, self.to.1
        )
    }

    /// The point at `t` in 0..=1.
    pub fn point(&self, t: f64) -> (f64, f64) {
        let u = 1.0 - t;
        let (a, b, c) = (u * u, 2.0 * u * t, t * t);
        (
            a * self.from.0 + b * self.ctrl.0 + c * self.to.0,
            a * self.from.1 + b * self.ctrl.1 + c * self.to.1,
        )
    }

    /// Length, as a 16-segment polyline.
    pub fn length(&self) -> f64 {
        let mut prev = self.point(0.0);
        let mut sum = 0.0;
        for i in 1..=16 {
            let p = self.point(i as f64 / 16.0);
            sum += (p.0 - prev.0).hypot(p.1 - prev.1);
            prev = p;
        }
        sum
    }

    /// The same wire run the other way.
    pub fn reversed(&self) -> Curve {
        Curve {
            from: self.to,
            ctrl: self.ctrl,
            to: self.from,
        }
    }
}

/// The wire from a node's rim to the house's rim along the centre line; the control point is the
/// chord midpoint pushed `bend` px along the left normal of node -> house. Coincident centres
/// give a finite curve (no direction), never `NaN`.
pub fn rim_curve(node: Placed, house: Placed, bend: f64) -> Curve {
    let (dx, dy) = (house.at.0 - node.at.0, house.at.1 - node.at.1);
    let len = dx.hypot(dy);
    let (ux, uy) = if len > 1e-9 {
        (dx / len, dy / len)
    } else {
        (0.0, 0.0)
    };
    let from = (node.at.0 + ux * node.r, node.at.1 + uy * node.r);
    let to = (house.at.0 - ux * house.r, house.at.1 - uy * house.r);
    // Left normal on screen (y down) of the direction (ux, uy).
    let (nx, ny) = (uy, -ux);
    let mid = ((from.0 + to.0) / 2.0, (from.1 + to.1) / 2.0);
    Curve {
        from,
        ctrl: (mid.0 + nx * bend, mid.1 + ny * bend),
        to,
    }
}

/// The bend of a node's wire. `slot` is the consumer's index in the arc of `slots` consumers.
pub fn bend_for(
    layout: &CirclesLayout,
    node: &EnergyNode,
    slot: Option<usize>,
    slots: usize,
) -> f64 {
    match node.kind {
        NodeKind::Solar => layout.bend_solar,
        NodeKind::Grid => layout.bend_grid,
        NodeKind::Battery => layout.bend_battery,
        NodeKind::Consumer | NodeKind::More if slots == 3 => slot
            .and_then(|i| layout.bend_three.get(i).copied())
            .unwrap_or(0.0),
        _ => 0.0,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_core::energy::{EnergyEntity, EnergyFlow, EnergyRole};

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

    fn full() -> Vec<EnergyEntity> {
        let mut es = vec![
            ent("s", "S", EnergyRole::Solar, "W", 100.0),
            ent("g", "G", EnergyRole::Grid, "W", 100.0),
            ent("w", "Water", EnergyRole::Water, "L", 5.0),
        ];
        es.extend((0..5).map(consumer));
        es
    }

    fn arc_of(diagram: &[EnergyNode], p: &BTreeMap<String, Placed>) -> Vec<(f64, f64)> {
        diagram
            .iter()
            .filter(|n| matches!(n.kind, NodeKind::Consumer | NodeKind::More))
            .map(|n| p[&n.id].at)
            .collect()
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn wide_places_the_triangle_and_the_arc() {
        let ui = EnergyFlow::build(&full(), 0, None);
        let p = place(&ui.diagram, &CirclesLayout::WIDE);
        assert_eq!(p["solar"].at, (260.0, 74.0));
        assert_eq!(p["grid"].at, (96.0, 214.0));
        assert_eq!(p["house"].at, (400.0, 214.0));
        assert_eq!(p["house"].r, 54.0);
        assert_eq!(p["solar"].r, 46.0);
        assert_eq!(p["water"].at, (190.0, 330.0));
        assert_eq!(p["water"].r, 30.0);
        assert_eq!(
            arc_of(&ui.diagram, &p),
            vec![(630.0, 80.0), (655.0, 214.0), (630.0, 348.0)]
        );
    }

    #[test]
    fn narrow_runs_top_to_bottom() {
        let ui = EnergyFlow::build(&full(), 0, None);
        let p = place(&ui.diagram, &CirclesLayout::NARROW);
        assert_eq!(p["solar"].at, (60.0, 74.0));
        assert_eq!(p["grid"].at, (300.0, 74.0));
        assert_eq!(p["water"].at, (180.0, 66.0));
        assert_eq!(p["house"].at, (180.0, 240.0));
        let arc = arc_of(&ui.diagram, &p);
        assert_eq!(arc.len(), 3);
        assert!(arc.iter().all(|a| a.1 >= 400.0));
    }

    #[test]
    fn battery_or_water_takes_the_side_slot() {
        let mut es = full();
        es.push(ent("b", "B", EnergyRole::Battery, "W", 50.0));
        let ui = EnergyFlow::build(&es, 0, None);
        let p = place(&ui.diagram, &CirclesLayout::WIDE);
        assert_eq!(p["battery"].at, (190.0, 330.0));
        assert_eq!(p["battery"].r, 46.0);
        assert!(!p.contains_key("water"));

        let es = vec![
            ent("s", "S", EnergyRole::Solar, "W", 100.0),
            ent("g", "G", EnergyRole::Grid, "W", 100.0),
            consumer(0),
        ];
        let ui = EnergyFlow::build(&es, 0, None);
        let p = place(&ui.diagram, &CirclesLayout::NARROW);
        assert_eq!(p["solar"].at, (84.0, 74.0));
        assert_eq!(p["grid"].at, (276.0, 74.0));
        assert_eq!(arc_of(&ui.diagram, &p), vec![(180.0, 404.0)]);
    }

    #[test]
    fn bends_follow_the_table() {
        let ui = EnergyFlow::build(&full(), 0, None);
        let l = &CirclesLayout::WIDE;
        let node = |k: NodeKind| ui.diagram.iter().find(|n| n.kind == k).unwrap();
        assert_eq!(bend_for(l, node(NodeKind::Solar), None, 3), 26.0);
        assert_eq!(bend_for(l, node(NodeKind::Grid), None, 3), -22.0);
        assert_eq!(bend_for(l, node(NodeKind::House), None, 3), 0.0);
        let c = node(NodeKind::Consumer);
        assert_eq!(bend_for(l, c, Some(0), 3), -18.0);
        assert_eq!(bend_for(l, c, Some(2), 3), 18.0);
        assert_eq!(bend_for(l, c, Some(0), 2), 0.0);
        assert_eq!(bend_for(&CirclesLayout::NARROW, c, Some(0), 3), 16.0);
    }

    #[test]
    fn rim_curve_leaves_and_meets_the_rims() {
        let node = Placed {
            at: (260.0, 74.0),
            r: 46.0,
        };
        let house = Placed {
            at: (400.0, 214.0),
            r: 54.0,
        };
        let c = rim_curve(node, house, 0.0);
        let d = |a: (f64, f64), b: (f64, f64)| (a.0 - b.0).hypot(a.1 - b.1);
        assert!(close(d(c.from, node.at), 46.0));
        assert!(close(d(c.to, house.at), 54.0));
        let mid = ((c.from.0 + c.to.0) / 2.0, (c.from.1 + c.to.1) / 2.0);
        assert!(close(c.ctrl.0, mid.0) && close(c.ctrl.1, mid.1));
        let b = rim_curve(node, house, 20.0);
        assert!(close(d(b.ctrl, mid), 20.0));
        assert_eq!(b.point(0.0), b.from);
        assert_eq!(b.point(1.0), b.to);
        let r = b.reversed();
        assert_eq!((r.from, r.to, r.ctrl), (b.to, b.from, b.ctrl));
        assert!(b.path().starts_with("M ") && b.path().contains(" Q "));
        assert!(b.length() >= d(b.from, b.to));
    }

    #[test]
    fn rim_curve_survives_coincident_centres() {
        let p = Placed {
            at: (100.0, 100.0),
            r: 40.0,
        };
        let c = rim_curve(p, p, 22.0);
        for v in [c.from.0, c.from.1, c.ctrl.0, c.ctrl.1, c.to.0, c.to.1] {
            assert!(v.is_finite());
        }
        let l = c.length();
        assert!(l.is_finite() && l >= 0.0);
        assert!(!c.path().contains("NaN"));
    }
}
