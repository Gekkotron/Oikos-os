//! Where the flow canvas draws things: pill geometry, the layered auto layout, the pins wires
//! leave and enter by, and wires routed around cards. Pure, tested natively.

use std::collections::BTreeMap;

use oikos_core::flow::{Flow, NodeKind, Position};

/// Width of a card (pill) on the canvas.
pub const NODE_W: f64 = 232.0;
/// Height of a card (pill) on the canvas.
pub const NODE_H: f64 = 52.0;
/// Horizontal space between two columns.
pub const COL_GAP: f64 = 96.0;
/// Vertical space between two cards of a column.
pub const ROW_GAP: f64 = 28.0;
/// Margin the auto layout leaves above and left of the graph.
pub const PAD: f64 = 40.0;

/// A point in canvas (world) coordinates.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Pt {
    pub x: f64,
    pub y: f64,
}

/// An axis-aligned rectangle in canvas coordinates: top-left corner and size.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Rect {
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

/// One drawn wire: its SVG path, where its "yes"/"no" label sits and where its ✕ goes.
#[derive(Clone, Debug, PartialEq)]
pub struct Route {
    pub d: String,
    pub label_at: Pt,
    pub mid: Pt,
}

/// An edge as `(from, port, to)`.
type Key = (String, Option<String>, String);

/// How far around a card a wire must stay clear.
const CLEARANCE: f64 = 6.0;
/// Points sampled along a wire segment to test it against the cards.
const SAMPLES: usize = 24;
/// Widest horizontal run of the curve that takes a wire onto, or off, its lane; kept inside
/// the gap between two columns.
const RAMP: f64 = COL_GAP * 0.75;
/// Distance from a wire's start to its "yes"/"no" label.
const LABEL_AT: f64 = 28.0;

/// Where each card goes: the stored positions when every node has one (a flow arranged by hand
/// keeps its look), else [`auto_layout`].
pub fn positions(flow: &Flow) -> BTreeMap<String, Pt> {
    if !flow.nodes.is_empty() && flow.nodes.iter().all(|n| n.position.is_some()) {
        return flow
            .nodes
            .iter()
            .filter_map(|n| n.position.map(|p| (n.id.clone(), Pt { x: p.x, y: p.y })))
            .collect();
    }
    auto_layout(flow)
}

/// A place in a column: a card, or a point a long edge passes through on its way across.
struct Slot {
    node: Option<String>,
    col: usize,
    h: f64,
}

/// The layered layout, ignoring stored positions. Columns come from [`Flow::layers`]; an edge
/// that skips columns reserves a half-height slot in each column it crosses. Each column is
/// ordered by four down-and-up barycenter sweeps (fewer crossings), then stacked and centred on
/// its predecessors, never above `PAD`.
pub fn auto_layout(flow: &Flow) -> BTreeMap<String, Pt> {
    let mut slots: Vec<Slot> = Vec::new();
    let mut columns: Vec<Vec<usize>> = Vec::new();
    let mut slot_of: BTreeMap<String, usize> = BTreeMap::new();
    for (col, ids) in flow.layers().into_iter().enumerate() {
        let mut column = Vec::new();
        for id in ids {
            let s = slots.len();
            slot_of.entry(id.clone()).or_insert(s);
            slots.push(Slot {
                node: Some(id),
                col,
                h: NODE_H,
            });
            column.push(s);
        }
        columns.push(column);
    }
    // Links between slots of consecutive columns, dummies included.
    let mut left: Vec<Vec<usize>> = vec![Vec::new(); slots.len()];
    let mut right: Vec<Vec<usize>> = vec![Vec::new(); slots.len()];
    for edge in &flow.edges {
        let (Some(&a), Some(&b)) = (slot_of.get(&edge.from), slot_of.get(&edge.to)) else {
            continue;
        };
        let (ca, cb) = (slots[a].col, slots[b].col);
        if cb <= ca {
            continue;
        }
        let mut chain = vec![a];
        for (col, column) in columns.iter_mut().enumerate().take(cb).skip(ca + 1) {
            column.push(slots.len());
            chain.push(slots.len());
            slots.push(Slot {
                node: None,
                col,
                h: NODE_H / 2.0,
            });
            left.push(Vec::new());
            right.push(Vec::new());
        }
        chain.push(b);
        for pair in chain.windows(2) {
            right[pair[0]].push(pair[1]);
            left[pair[1]].push(pair[0]);
        }
    }

    let mut row = vec![0usize; slots.len()];
    for column in &columns {
        for (i, &s) in column.iter().enumerate() {
            row[s] = i;
        }
    }
    for _ in 0..4 {
        for column in columns.iter_mut().skip(1) {
            order_by_barycenter(column, &left, &mut row);
        }
        let below_last = columns.len().saturating_sub(1);
        for column in columns.iter_mut().take(below_last).rev() {
            order_by_barycenter(column, &right, &mut row);
        }
    }

    let mut top = vec![0.0; slots.len()];
    for column in &columns {
        let mut offsets = Vec::with_capacity(column.len());
        let mut height = 0.0;
        for &s in column {
            offsets.push(height);
            height += slots[s].h + ROW_GAP;
        }
        height = (height - ROW_GAP).max(0.0);
        let mut preds: Vec<usize> = column.iter().flat_map(|&s| left[s].clone()).collect();
        preds.sort_unstable();
        preds.dedup();
        let start = if preds.is_empty() {
            PAD
        } else {
            let centre = preds
                .iter()
                .map(|&p| top[p] + slots[p].h / 2.0)
                .sum::<f64>()
                / preds.len() as f64;
            (centre - height / 2.0).round().max(PAD)
        };
        for (&s, offset) in column.iter().zip(offsets) {
            top[s] = start + offset;
        }
    }

    slots
        .iter()
        .zip(top)
        .filter_map(|(slot, y)| {
            let id = slot.node.clone()?;
            let x = PAD + slot.col as f64 * (NODE_W + COL_GAP);
            Some((id, Pt { x, y }))
        })
        .collect()
}

/// Sorts `column` by the mean row of each slot's `neighbours` (a slot without any keeps its own
/// row); the sort is stable, so ties keep the current order. Updates `row` to match.
fn order_by_barycenter(column: &mut Vec<usize>, neighbours: &[Vec<usize>], row: &mut [usize]) {
    let mut keyed: Vec<(usize, f64)> = column
        .iter()
        .map(|&s| {
            let ns = &neighbours[s];
            let at = if ns.is_empty() {
                row[s] as f64
            } else {
                ns.iter().map(|&n| row[n] as f64).sum::<f64>() / ns.len() as f64
            };
            (s, at)
        })
        .collect();
    keyed.sort_by(|a, b| a.1.total_cmp(&b.1));
    *column = keyed.into_iter().map(|(s, _)| s).collect();
    for (i, &s) in column.iter().enumerate() {
        row[s] = i;
    }
}

/// `flow` with every node's `position` replaced by [`auto_layout`]'s: what **Tidy** does.
pub fn tidy(flow: &Flow) -> Flow {
    let auto = auto_layout(flow);
    let mut out = flow.clone();
    for node in &mut out.nodes {
        node.position = auto.get(&node.id).map(|p| Position { x: p.x, y: p.y });
    }
    out
}

/// `flow` with the missing positions filled from [`auto_layout`]; stored ones are kept. Used to
/// start edit mode from coordinates every node has.
pub fn with_resolved_positions(flow: &Flow) -> Flow {
    let auto = auto_layout(flow);
    let mut out = flow.clone();
    for node in &mut out.nodes {
        if node.position.is_none() {
            node.position = auto.get(&node.id).map(|p| Position { x: p.x, y: p.y });
        }
    }
    out
}

/// The rectangle a card at `at` covers.
fn card(at: Pt) -> Rect {
    Rect {
        x: at.x,
        y: at.y,
        w: NODE_W,
        h: NODE_H,
    }
}

/// The smallest rectangle holding every card, with no margin; all zero when there is none.
pub fn bounds(pos: &BTreeMap<String, Pt>) -> Rect {
    if pos.is_empty() {
        return Rect {
            x: 0.0,
            y: 0.0,
            w: 0.0,
            h: 0.0,
        };
    }
    let (mut x0, mut y0) = (f64::INFINITY, f64::INFINITY);
    let (mut x1, mut y1) = (f64::NEG_INFINITY, f64::NEG_INFINITY);
    for at in pos.values() {
        x0 = x0.min(at.x);
        y0 = y0.min(at.y);
        x1 = x1.max(at.x + NODE_W);
        y1 = y1.max(at.y + NODE_H);
    }
    Rect {
        x: x0,
        y: y0,
        w: x1 - x0,
        h: y1 - y0,
    }
}

/// Where a wire leaves a card at `at`: its right edge, halfway down, or for an `if` 30 % down
/// for `true` and 70 % down for `false`.
pub fn out_anchor(kind: &NodeKind, at: Pt, port: Option<&str>) -> Pt {
    let down = match (kind, port) {
        (NodeKind::If(_), Some("true")) => 0.3,
        (NodeKind::If(_), Some("false")) => 0.7,
        _ => 0.5,
    };
    Pt {
        x: at.x + NODE_W,
        y: at.y + NODE_H * down,
    }
}

/// Where each wire enters its target, by edge: on the card's left edge, the wires into one
/// card sorted by the height they leave at and spread over the middle 60 % (a lone wire enters
/// halfway down), so they do not pile onto one point.
pub fn in_anchors(flow: &Flow, pos: &BTreeMap<String, Pt>) -> BTreeMap<Key, Pt> {
    let mut incoming: BTreeMap<&str, Vec<(Key, f64)>> = BTreeMap::new();
    for edge in &flow.edges {
        let (Some(from), Some(&at), true) = (
            flow.node(&edge.from),
            pos.get(&edge.from),
            pos.contains_key(&edge.to),
        ) else {
            continue;
        };
        let leave = out_anchor(&from.kind, at, edge.port.as_deref()).y;
        let key = (edge.from.clone(), edge.port.clone(), edge.to.clone());
        incoming.entry(&edge.to).or_default().push((key, leave));
    }
    let mut out = BTreeMap::new();
    for (to, mut wires) in incoming {
        wires.sort_by(|a, b| a.1.total_cmp(&b.1));
        let at = pos[to];
        let last = wires.len().saturating_sub(1);
        for (i, (key, _)) in wires.into_iter().enumerate() {
            let down = if last == 0 {
                0.5
            } else {
                0.2 + 0.6 * i as f64 / last as f64
            };
            out.insert(
                key,
                Pt {
                    x: at.x,
                    y: at.y + NODE_H * down,
                },
            );
        }
    }
    out
}

/// A piece of a wire.
enum Seg {
    Cubic([Pt; 4]),
    Line([Pt; 2]),
}

impl Seg {
    fn at(&self, t: f64) -> Pt {
        let lerp = |a: Pt, b: Pt, t: f64| Pt {
            x: a.x + (b.x - a.x) * t,
            y: a.y + (b.y - a.y) * t,
        };
        match self {
            Seg::Line([a, b]) => lerp(*a, *b, t),
            Seg::Cubic([p0, p1, p2, p3]) => {
                let a = lerp(lerp(*p0, *p1, t), lerp(*p1, *p2, t), t);
                let b = lerp(lerp(*p1, *p2, t), lerp(*p2, *p3, t), t);
                lerp(a, b, t)
            }
        }
    }
}

/// A cubic from `a` to `b` that leaves and arrives horizontally, its handles `k` long.
fn curve(a: Pt, b: Pt, k: f64) -> Seg {
    Seg::Cubic([a, Pt { x: a.x + k, y: a.y }, Pt { x: b.x - k, y: b.y }, b])
}

/// Whether any of [`SAMPLES`] points of any segment lies within [`CLEARANCE`] of a rect.
fn blocked(path: &[Seg], obstacles: &[Rect]) -> bool {
    path.iter().any(|seg| {
        (0..SAMPLES).any(|i| {
            let p = seg.at(i as f64 / (SAMPLES - 1) as f64);
            obstacles.iter().any(|r| {
                p.x > r.x - CLEARANCE
                    && p.x < r.x + r.w + CLEARANCE
                    && p.y > r.y - CLEARANCE
                    && p.y < r.y + r.h + CLEARANCE
            })
        })
    })
}

/// The detour of a blocked wire: onto a horizontal lane just above or below one of the cards
/// in its way, the lane closest to both ends that keeps the whole wire clear.
fn lane(from: Pt, to: Pt, obstacles: &[Rect]) -> Option<Vec<Seg>> {
    let (lo, hi) = (from.x.min(to.x), from.x.max(to.x));
    let mut lanes: Vec<f64> = obstacles
        .iter()
        .filter(|r| r.x < hi && r.x + r.w > lo)
        .flat_map(|r| [r.y - ROW_GAP / 2.0, r.y + r.h + ROW_GAP / 2.0])
        .collect();
    let cost = |y: f64| (y - from.y).abs() + (y - to.y).abs();
    lanes.sort_by(|a, b| cost(*a).total_cmp(&cost(*b)));
    let ramp = RAMP.min((to.x - from.x).abs() / 2.0);
    lanes.into_iter().find_map(|y| {
        let on = Pt {
            x: from.x + ramp,
            y,
        };
        let off = Pt { x: to.x - ramp, y };
        let path = vec![
            curve(from, on, ramp / 2.0),
            Seg::Line([on, off]),
            curve(off, to, ramp / 2.0),
        ];
        (!blocked(&path, obstacles)).then_some(path)
    })
}

/// The wire from `from` to `to`: a horizontal-handled cubic, or, when that passes within
/// [`CLEARANCE`] of an obstacle, a detour along a lane beside the cards in the way.
pub fn route(from: Pt, to: Pt, obstacles: &[Rect]) -> Route {
    let k = ((to.x - from.x).abs() / 2.0).max(40.0);
    let direct = vec![curve(from, to, k)];
    let path = if blocked(&direct, obstacles) {
        lane(from, to, obstacles).unwrap_or(direct)
    } else {
        direct
    };
    finish(from, &path)
}

/// The SVG path of `path`, plus the points at [`LABEL_AT`] along it and halfway along it.
fn finish(from: Pt, path: &[Seg]) -> Route {
    let mut d = format!("M{:.1} {:.1}", from.x, from.y);
    for seg in path {
        match seg {
            Seg::Cubic([_, a, b, c]) => d.push_str(&format!(
                " C{:.1} {:.1}, {:.1} {:.1}, {:.1} {:.1}",
                a.x, a.y, b.x, b.y, c.x, c.y
            )),
            Seg::Line([_, b]) => d.push_str(&format!(" L{:.1} {:.1}", b.x, b.y)),
        }
    }
    // A polyline dense enough to measure lengths along the wire.
    let points: Vec<Pt> = std::iter::once(from)
        .chain(
            path.iter()
                .flat_map(|seg| (1..=64).map(move |i| seg.at(f64::from(i) / 64.0))),
        )
        .collect();
    let mut walked = vec![0.0];
    for pair in points.windows(2) {
        let step = (pair[1].x - pair[0].x).hypot(pair[1].y - pair[0].y);
        walked.push(walked.last().copied().unwrap_or(0.0) + step);
    }
    let total = walked.last().copied().unwrap_or(0.0);
    let along = |len: f64| {
        let i = walked
            .partition_point(|&w| w < len)
            .clamp(1, points.len() - 1);
        let span = walked[i] - walked[i - 1];
        let t = if span > 0.0 {
            (len - walked[i - 1]) / span
        } else {
            0.0
        };
        Pt {
            x: points[i - 1].x + (points[i].x - points[i - 1].x) * t,
            y: points[i - 1].y + (points[i].y - points[i - 1].y) * t,
        }
    };
    Route {
        d,
        label_at: along(LABEL_AT.min(total)),
        mid: along(total / 2.0),
    }
}

/// Every edge's wire, in edge order, keyed by `(from, port, to)`: from its [`out_anchor`] to
/// its [`in_anchors`] entry, routed around every card but its own two ends. Edges whose ends
/// have no position are left out.
pub fn routes(flow: &Flow, pos: &BTreeMap<String, Pt>) -> Vec<(Key, Route)> {
    let anchors = in_anchors(flow, pos);
    flow.edges
        .iter()
        .filter_map(|edge| {
            let from = flow.node(&edge.from)?;
            let start = out_anchor(&from.kind, *pos.get(&edge.from)?, edge.port.as_deref());
            let key = (edge.from.clone(), edge.port.clone(), edge.to.clone());
            let end = *anchors.get(&key)?;
            let obstacles: Vec<Rect> = pos
                .iter()
                .filter(|(id, _)| **id != edge.from && **id != edge.to)
                .map(|(_, at)| card(*at))
                .collect();
            Some((key, route(start, end, &obstacles)))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn led() -> Flow {
        Flow::decode(include_str!("../tests/fixtures/living_led_motion.json").as_bytes()).unwrap()
    }

    fn intersects(a: Rect, b: Rect) -> bool {
        a.x < b.x + b.w && b.x < a.x + a.w && a.y < b.y + b.h && b.y < a.y + a.h
    }

    fn inside(p: Pt, r: Rect) -> bool {
        p.x > r.x && p.x < r.x + r.w && p.y > r.y && p.y < r.y + r.h
    }

    fn lerp(a: Pt, b: Pt, t: f64) -> Pt {
        Pt {
            x: a.x + (b.x - a.x) * t,
            y: a.y + (b.y - a.y) * t,
        }
    }

    /// `n` points along an SVG path of `M`, `C` and `L` commands, spread evenly over its
    /// segments (by parameter, not by length).
    fn route_samples(d: &str, n: usize) -> Vec<Pt> {
        let mut spaced = String::new();
        for ch in d.chars() {
            if ch.is_ascii_alphabetic() {
                spaced.push(' ');
                spaced.push(ch);
                spaced.push(' ');
            } else if ch == ',' {
                spaced.push(' ');
            } else {
                spaced.push(ch);
            }
        }
        let mut commands: Vec<(char, Vec<f64>)> = Vec::new();
        for token in spaced.split_whitespace() {
            match token.parse::<f64>() {
                Ok(v) => commands
                    .last_mut()
                    .expect("path starts with a command")
                    .1
                    .push(v),
                Err(_) => commands.push((token.chars().next().unwrap(), Vec::new())),
            }
        }
        let pt = |v: &[f64], i: usize| Pt {
            x: v[2 * i],
            y: v[2 * i + 1],
        };
        let mut segments: Vec<Vec<Pt>> = Vec::new();
        let mut cur = Pt { x: 0.0, y: 0.0 };
        for (c, v) in &commands {
            match c {
                'M' => cur = pt(v, 0),
                'L' => {
                    segments.push(vec![cur, pt(v, 0)]);
                    cur = pt(v, 0);
                }
                'C' => {
                    segments.push(vec![cur, pt(v, 0), pt(v, 1), pt(v, 2)]);
                    cur = pt(v, 2);
                }
                other => panic!("unexpected path command {other} in {d}"),
            }
        }
        (0..n)
            .map(|i| {
                let g = i as f64 / (n - 1) as f64 * segments.len() as f64;
                let s = (g.floor() as usize).min(segments.len() - 1);
                let t = g - s as f64;
                let p = &segments[s];
                if p.len() == 2 {
                    lerp(p[0], p[1], t)
                } else {
                    let a = lerp(lerp(p[0], p[1], t), lerp(p[1], p[2], t), t);
                    let b = lerp(lerp(p[1], p[2], t), lerp(p[2], p[3], t), t);
                    lerp(a, b, t)
                }
            })
            .collect()
    }

    #[test]
    fn led_layout_has_no_overlaps() {
        let pos = auto_layout(&led());
        assert_eq!(pos.len(), 23);
        let cards: Vec<_> = pos.iter().collect();
        for (i, (a, pa)) in cards.iter().enumerate() {
            assert!(pa.x >= PAD && pa.y >= PAD, "{a} at {pa:?}");
            for (b, pb) in &cards[i + 1..] {
                assert!(!intersects(card(**pa), card(**pb)), "{a} overlaps {b}");
            }
        }
    }

    #[test]
    fn led_routes_avoid_cards() {
        let flow = led();
        let pos = auto_layout(&flow);
        let all = routes(&flow, &pos);
        assert_eq!(all.len(), flow.edges.len());
        for ((from, port, to), route) in &all {
            for p in route_samples(&route.d, 48) {
                for (id, at) in &pos {
                    if id == from || id == to {
                        continue;
                    }
                    assert!(
                        !inside(p, card(*at)),
                        "{from} -{port:?}-> {to} crosses {id} at {p:?}: {}",
                        route.d
                    );
                }
            }
        }
    }

    #[test]
    fn led_triggers_spread() {
        let flow = led();
        let pos = auto_layout(&flow);
        let tv = pos["tv_on"];
        let mut ys: Vec<f64> = in_anchors(&flow, &pos)
            .into_iter()
            .filter(|((_, _, to), _)| to == "tv_on")
            .map(|(_, at)| {
                assert_eq!(at.x, tv.x);
                assert!(at.y > tv.y && at.y < tv.y + NODE_H, "{at:?} off {tv:?}");
                at.y
            })
            .collect();
        assert_eq!(ys.len(), 5);
        ys.sort_by(f64::total_cmp);
        ys.dedup();
        assert_eq!(ys.len(), 5, "{ys:?}");
    }

    #[test]
    fn single_wire_enters_at_the_middle() {
        let flow = led();
        let pos = auto_layout(&flow);
        let anchors = in_anchors(&flow, &pos);
        let key = ("wait".to_owned(), None, "off".to_owned());
        assert_eq!(anchors[&key].y, pos["off"].y + NODE_H / 2.0);
    }

    #[test]
    fn if_ports() {
        let flow = led();
        let kind = &flow.node("tv_on").unwrap().kind;
        let origin = Pt { x: 0.0, y: 0.0 };
        assert_eq!(out_anchor(kind, origin, Some("true")).y, NODE_H * 0.3);
        assert_eq!(out_anchor(kind, origin, Some("false")).y, NODE_H * 0.7);
        assert_eq!(out_anchor(kind, origin, Some("true")).x, NODE_W);
        let wait = &flow.node("wait").unwrap().kind;
        assert_eq!(
            out_anchor(wait, Pt { x: 10.0, y: 20.0 }, None),
            Pt {
                x: 10.0 + NODE_W,
                y: 20.0 + NODE_H / 2.0
            }
        );
    }

    fn placed(flow: &mut Flow) {
        for (i, n) in flow.nodes.iter_mut().enumerate() {
            n.position = Some(Position {
                x: 10.0 * i as f64,
                y: 5.0,
            });
        }
    }

    #[test]
    fn stored_positions_win() {
        let mut flow = led();
        placed(&mut flow);
        let pos = positions(&flow);
        assert_eq!(pos.len(), 23);
        for (i, n) in flow.nodes.iter().enumerate() {
            assert_eq!(
                pos[&n.id],
                Pt {
                    x: 10.0 * i as f64,
                    y: 5.0
                }
            );
        }
    }

    #[test]
    fn partial_positions_autolayout() {
        let mut flow = led();
        placed(&mut flow);
        flow.nodes[3].position = None;
        assert_eq!(positions(&flow), auto_layout(&flow));
    }

    #[test]
    fn tidy_overwrites_positions() {
        let mut flow = led();
        placed(&mut flow);
        let auto = auto_layout(&flow);
        let tidied = tidy(&flow);
        for n in &tidied.nodes {
            let p = n.position.expect("tidy sets every position");
            assert_eq!(Pt { x: p.x, y: p.y }, auto[&n.id], "{}", n.id);
        }
    }

    #[test]
    fn resolve_positions_fills_only_the_missing_ones() {
        let mut flow = led();
        flow.nodes[0].position = Some(Position { x: 10.0, y: 20.0 });
        let auto = auto_layout(&flow);
        let resolved = with_resolved_positions(&flow);
        assert_eq!(
            resolved.nodes[0].position,
            Some(Position { x: 10.0, y: 20.0 })
        );
        for n in &resolved.nodes[1..] {
            let p = n.position.expect("filled");
            assert_eq!(Pt { x: p.x, y: p.y }, auto[&n.id], "{}", n.id);
        }
    }

    #[test]
    fn crossings_reduced() {
        let flow = Flow::decode(
            br#"{
          "id": "x", "name": "X",
          "nodes": [
            { "id": "a", "type": "trigger.state", "params": { "entity": "m" } },
            { "id": "b", "type": "trigger.state", "params": { "entity": "n" } },
            { "id": "c", "type": "command", "params": { "entity": "l", "command": { "type": "turn_on" } } },
            { "id": "d", "type": "command", "params": { "entity": "l", "command": { "type": "turn_off" } } }
          ],
          "edges": [ { "from": "a", "to": "d" }, { "from": "b", "to": "c" } ]
        }"#,
        )
        .unwrap();
        let pos = auto_layout(&flow);
        assert!(pos["a"].y < pos["b"].y);
        assert!(pos["d"].y < pos["c"].y, "{pos:?}");
        assert_eq!(pos["a"].x, PAD);
        assert_eq!(pos["c"].x, PAD + NODE_W + COL_GAP);
    }

    #[test]
    fn bounds_cover_every_card() {
        let pos = BTreeMap::from([
            ("a".to_owned(), Pt { x: 40.0, y: 100.0 }),
            ("b".to_owned(), Pt { x: 400.0, y: 10.0 }),
        ]);
        assert_eq!(
            bounds(&pos),
            Rect {
                x: 40.0,
                y: 10.0,
                w: 360.0 + NODE_W,
                h: 90.0 + NODE_H
            }
        );
        assert_eq!(
            bounds(&BTreeMap::new()),
            Rect {
                x: 0.0,
                y: 0.0,
                w: 0.0,
                h: 0.0
            }
        );
    }

    #[test]
    fn clear_route_is_one_cubic_with_label_and_middle() {
        let r = route(Pt { x: 0.0, y: 0.0 }, Pt { x: 400.0, y: 0.0 }, &[]);
        assert_eq!(r.d, "M0.0 0.0 C200.0 0.0, 200.0 0.0, 400.0 0.0");
        assert!((r.label_at.x - 28.0).abs() < 1.0, "{:?}", r.label_at);
        assert!((r.mid.x - 200.0).abs() < 1.0, "{:?}", r.mid);
    }

    #[test]
    fn blocked_route_takes_a_lane() {
        let block = Rect {
            x: 150.0,
            y: 74.0,
            w: NODE_W,
            h: NODE_H,
        };
        let from = Pt { x: 0.0, y: 100.0 };
        let to = Pt { x: 600.0, y: 100.0 };
        let r = route(from, to, &[block]);
        assert!(r.d.contains('L'), "{}", r.d);
        for p in route_samples(&r.d, 48) {
            assert!(!inside(p, block), "{p:?} in {}", r.d);
        }
        // Lane at the top, ROW_GAP/2 above the card (both lanes cost the same; top comes first).
        assert!(
            r.d.contains(&format!("{:.1}", block.y - ROW_GAP / 2.0)),
            "{}",
            r.d
        );
    }
}
