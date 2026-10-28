//! Geometry of the Sankey model: where each bar, band and label sits in an SVG frame, horizontal
//! on wide screens and vertical on phones. Pure; `screens::energy_flux` draws it.

use std::collections::HashMap;

use crate::energy_sankey::{Sankey, SankeyRole};

#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Frame {
    pub w: f64,
    pub h: f64,
    /// Columns are rows, bands flow downward.
    pub vertical: bool,
}

pub const WIDE: Frame = Frame {
    w: 980.0,
    h: 460.0,
    vertical: false,
};
pub const NARROW: Frame = Frame {
    w: 360.0,
    h: 620.0,
    vertical: true,
};
/// Bar thickness across the main axis.
pub const BAR: f64 = 16.0;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Anchor {
    Start,
    Middle,
    End,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Bar {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub w: f64,
    pub h: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Band {
    /// `"{from}>{to}"`.
    pub id: String,
    pub from: String,
    pub to: String,
    /// Always the same five commands so CSS can interpolate it between frames.
    pub path: String,
    /// Gradient axis: centre line from the source bar's edge to the target bar's edge.
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Label {
    pub id: String,
    pub x: f64,
    pub y: f64,
    pub anchor: Anchor,
}

#[derive(Clone, Debug, PartialEq, Default)]
pub struct Placed {
    pub bars: Vec<Bar>,
    pub bands: Vec<Band>,
    pub labels: Vec<Label>,
}

/// Where a column sits along the main axis, as a share of it.
fn column_share(column: u8, vertical: bool) -> f64 {
    let shares = if vertical {
        [0.06, 0.42, 0.78]
    } else {
        [0.17, 0.49, 0.79]
    };
    shares[(column as usize).min(2)]
}

/// A coordinate for an SVG path: one decimal at most, no trailing ".0", never "-0".
fn n(v: f64) -> String {
    let r = (v * 10.0).round() / 10.0;
    format!("{}", if r == 0.0 { 0.0 } else { r })
}

/// A bar's position on the cross axis and its length.
struct Slot {
    cross: f64,
    len: f64,
    main: f64,
}

pub fn place(s: &Sankey, frame: Frame) -> Placed {
    let v = frame.vertical;
    let (main_size, cross_size) = if v {
        (frame.h, frame.w)
    } else {
        (frame.w, frame.h)
    };
    let (margin, gap) = if v { (16.0, 12.0) } else { (30.0, 20.0) };
    let space = cross_size - 2.0 * margin;

    // One scale for every column so equal watts look equal.
    let mut k = f64::INFINITY;
    for c in 0..3u8 {
        let col: Vec<_> = s.nodes.iter().filter(|nd| nd.column == c).collect();
        let sum: f64 = col.iter().map(|nd| nd.size).sum();
        if col.is_empty() || sum <= 0.0 {
            continue;
        }
        k = k.min((space - gap * (col.len() as f64 - 1.0)) / sum);
    }
    if !k.is_finite() || k <= 0.0 {
        return Placed::default();
    }

    let mut slots: HashMap<&str, Slot> = HashMap::new();
    for c in 0..3u8 {
        let col: Vec<_> = s.nodes.iter().filter(|nd| nd.column == c).collect();
        if col.is_empty() {
            continue;
        }
        let lens: Vec<f64> = col.iter().map(|nd| (nd.size * k).max(3.0)).collect();
        let total = lens.iter().sum::<f64>() + gap * (col.len() as f64 - 1.0);
        let mut cross = margin + (space - total) / 2.0;
        let main = column_share(c, v) * main_size;
        for (nd, len) in col.iter().zip(lens) {
            slots.insert(nd.id.as_str(), Slot { cross, len, main });
            cross += len + gap;
        }
    }

    let mut out = Placed::default();
    for nd in &s.nodes {
        let Some(sl) = slots.get(nd.id.as_str()) else {
            continue;
        };
        let (x, y, w, h) = if v {
            (sl.cross, sl.main, sl.len, BAR)
        } else {
            (sl.main, sl.cross, BAR, sl.len)
        };
        out.bars.push(Bar {
            id: nd.id.clone(),
            x,
            y,
            w,
            h,
        });

        let house = nd.role == SankeyRole::House;
        let (lx, ly, anchor) = if v {
            let cx = x + w / 2.0;
            if house {
                (cx, y - 12.0, Anchor::Middle)
            } else {
                (cx, y + h + 18.0, Anchor::Middle)
            }
        } else if house {
            (x + w / 2.0, y - 12.0, Anchor::Middle)
        } else if nd.column == 0 {
            (x - 14.0, y + h / 2.0, Anchor::End)
        } else {
            (x + w + 14.0, y + h / 2.0, Anchor::Start)
        };
        out.labels.push(Label {
            id: nd.id.clone(),
            x: lx,
            y: ly,
            anchor,
        });
    }

    // Bands stack from the top (left) of each bar in link order.
    let mut leaving: HashMap<&str, f64> = HashMap::new();
    let mut entering: HashMap<&str, f64> = HashMap::new();
    for l in &s.links {
        let (Some(a), Some(b)) = (slots.get(l.from.as_str()), slots.get(l.to.as_str())) else {
            continue;
        };
        let t = (l.watts * k).max(1.5);
        let out_off = leaving.entry(l.from.as_str()).or_insert(0.0);
        let c0 = a.cross + *out_off;
        *out_off += t;
        let in_off = entering.entry(l.to.as_str()).or_insert(0.0);
        let c1 = b.cross + *in_off;
        *in_off += t;
        let (m0, m1) = (a.main + BAR, b.main);
        let mm = (m0 + m1) / 2.0;
        // Horizontal: (main, cross) = (x, y). Vertical swaps them.
        let pt = |m: f64, c: f64| {
            if v {
                format!("{} {}", n(c), n(m))
            } else {
                format!("{} {}", n(m), n(c))
            }
        };
        let path = format!(
            "M {} C {} {} {} L {} C {} {} {} Z",
            pt(m0, c0),
            pt(mm, c0),
            pt(mm, c1),
            pt(m1, c1),
            pt(m1, c1 + t),
            pt(mm, c1 + t),
            pt(mm, c0 + t),
            pt(m0, c0 + t),
        );
        let (x0, y0, x1, y1) = if v {
            (c0 + t / 2.0, m0, c1 + t / 2.0, m1)
        } else {
            (m0, c0 + t / 2.0, m1, c1 + t / 2.0)
        };
        out.bands.push(Band {
            id: format!("{}>{}", l.from, l.to),
            from: l.from.clone(),
            to: l.to.clone(),
            path,
            x0,
            y0,
            x1,
            y1,
        });
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::energy_sankey::{SankeyLink, SankeyNode};

    fn node(id: &str, role: SankeyRole, column: u8, size: f64) -> SankeyNode {
        SankeyNode {
            id: id.into(),
            role,
            column,
            name: None,
            watts: size,
            size,
            sensor: None,
            count: 0,
            slot: 0,
        }
    }

    fn link(from: &str, to: &str, watts: f64) -> SankeyLink {
        SankeyLink {
            from: from.into(),
            to: to.into(),
            watts,
        }
    }

    /// Solar and grid feed the house, which feeds an export sink and two consumers.
    fn sample() -> Sankey {
        Sankey {
            nodes: vec![
                node("solar", SankeyRole::Solar, 0, 3000.0),
                node("grid", SankeyRole::Grid, 0, 1000.0),
                node("house", SankeyRole::House, 1, 3500.0),
                node("export", SankeyRole::Export, 1, 500.0),
                node("oven", SankeyRole::Consumer, 2, 2000.0),
                node("heat-pump", SankeyRole::Consumer, 2, 1500.0),
            ],
            links: vec![
                link("solar", "house", 2500.0),
                link("solar", "export", 500.0),
                link("grid", "house", 1000.0),
                link("house", "oven", 2000.0),
                link("house", "heat-pump", 1500.0),
            ],
            ..Sankey::default()
        }
    }

    fn bar<'a>(p: &'a Placed, id: &str) -> &'a Bar {
        p.bars.iter().find(|b| b.id == id).unwrap()
    }

    fn label<'a>(p: &'a Placed, id: &str) -> &'a Label {
        p.labels.iter().find(|b| b.id == id).unwrap()
    }

    #[test]
    fn paths_keep_one_shape() {
        for f in [WIDE, NARROW] {
            let p = place(&sample(), f);
            assert_eq!(p.bands.len(), 5);
            for b in &p.bands {
                assert!(b.path.starts_with("M "), "{}", b.path);
                assert!(b.path.ends_with(" Z"), "{}", b.path);
                assert_eq!(b.path.matches("C ").count(), 2, "{}", b.path);
                assert_eq!(b.path.matches("L ").count(), 1, "{}", b.path);
            }
        }
    }

    #[test]
    fn bars_fit_the_frame() {
        for f in [WIDE, NARROW] {
            let p = place(&sample(), f);
            assert_eq!(p.bars.len(), 6);
            for b in &p.bars {
                assert!(b.x >= 0.0 && b.y >= 0.0, "{b:?}");
                assert!(b.x + b.w <= f.w && b.y + b.h <= f.h, "{b:?}");
            }
        }
    }

    #[test]
    fn bands_never_outgrow_their_bar() {
        for f in [WIDE, NARROW] {
            let s = sample();
            let p = place(&s, f);
            // Recover each band's thickness from the path: the L command's far edge minus the
            // M command's start, along the cross axis.
            let thickness = |path: &str| -> f64 {
                let nums: Vec<f64> = path
                    .split(|c: char| c.is_alphabetic() || c == ' ')
                    .filter(|t| !t.is_empty())
                    .map(|t| t.parse().unwrap())
                    .collect();
                // M(0,1) C(2..7) L(8,9): cross index is 0 (vertical) or 1 (horizontal).
                let i = usize::from(!f.vertical);
                (nums[8 + i] - nums[2 + 2 + i]).abs()
            };
            for b in &p.bars {
                let len = if f.vertical { b.w } else { b.h };
                let out: f64 = p
                    .bands
                    .iter()
                    .filter(|x| x.from == b.id)
                    .map(|x| thickness(&x.path))
                    .sum();
                let inn: f64 = p
                    .bands
                    .iter()
                    .filter(|x| x.to == b.id)
                    .map(|x| thickness(&x.path))
                    .sum();
                assert!(out <= len + 0.5, "{} leaves {out} > {len}", b.id);
                assert!(inn <= len + 0.5, "{} enters {inn} > {len}", b.id);
            }
        }
    }

    #[test]
    fn columns_sit_on_the_main_axis() {
        let p = place(&sample(), WIDE);
        assert_eq!(bar(&p, "solar").x, bar(&p, "grid").x);
        let p = place(&sample(), NARROW);
        assert_eq!(bar(&p, "solar").y, bar(&p, "grid").y);
        assert_eq!(bar(&p, "oven").y, bar(&p, "heat-pump").y);
    }

    #[test]
    fn vertical_swaps_axes() {
        let p = place(&sample(), NARROW);
        let h = bar(&p, "house");
        assert!(h.w > h.h, "{h:?}");
        let p = place(&sample(), WIDE);
        let h = bar(&p, "house");
        assert!(h.h > h.w, "{h:?}");
    }

    #[test]
    fn labels_follow_the_side_rules() {
        let p = place(&sample(), WIDE);
        let (solar, l) = (bar(&p, "solar"), label(&p, "solar"));
        assert_eq!(l.anchor, Anchor::End);
        assert!(l.x < solar.x);
        let (oven, l) = (bar(&p, "oven"), label(&p, "oven"));
        assert_eq!(l.anchor, Anchor::Start);
        assert!(l.x > oven.x + BAR);
        let (house, l) = (bar(&p, "house"), label(&p, "house"));
        assert_eq!(l.anchor, Anchor::Middle);
        assert!(l.y < house.y);
        let l = label(&p, "export");
        assert_eq!(l.anchor, Anchor::Start);

        let p = place(&sample(), NARROW);
        assert!(p.labels.iter().all(|l| l.anchor == Anchor::Middle));
        let (oven, l) = (bar(&p, "oven"), label(&p, "oven"));
        assert!(l.y > oven.y + oven.h);
        assert!(label(&p, "house").y < bar(&p, "house").y);
    }

    #[test]
    fn empty_model_places_nothing() {
        assert_eq!(place(&Sankey::default(), WIDE), Placed::default());
    }
}
