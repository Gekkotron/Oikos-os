//! Glowing dots travelling along the active wires of the circles diagram. The motion maths is
//! pure Rust (unit-tested natively); `run` drives the DOM from a requestAnimationFrame loop and
//! only exists on wasm.

use std::cell::RefCell;
use std::collections::BTreeMap;
use std::rc::Rc;

use crate::energy::level;
use crate::energy_circles::{Curve, Wire};

/// The motion state of one wire.
#[derive(Clone, Debug, PartialEq)]
pub struct DotWire {
    pub curve: Curve,
    pub len: f64,
    /// px per second.
    pub speed: f64,
    pub radius: f64,
    pub count: usize,
    /// Position of the first dot along the wire, 0..1.
    pub phase: f64,
    pub tone: &'static str,
}

pub fn dot_wire(w: &Wire, phase: f64) -> DotWire {
    let lvl = level(Some(w.watts));
    let len = w.curve.length();
    DotWire {
        curve: w.curve,
        len,
        speed: 30.0 + 90.0 * lvl,
        radius: 2.2 + 2.6 * lvl,
        count: ((len / 40.0).floor() as usize).max(2),
        phase,
        tone: w.tone,
    }
}

pub fn advance(phase: f64, dt_s: f64, speed: f64, len: f64) -> f64 {
    if len <= 0.0 {
        return phase;
    }
    (phase + dt_s * speed / len).rem_euclid(1.0)
}

/// One circle to draw.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Dot {
    pub x: f64,
    pub y: f64,
    pub r: f64,
    pub opacity: f64,
    pub halo: bool,
}

/// For each travelling dot: a halo, then the head and two trail dots. Dots fade in and out at
/// both ends of the wire.
pub fn dots(w: &DotWire) -> Vec<Dot> {
    const TRAIL_R: [f64; 3] = [1.0, 0.72, 0.44];
    const TRAIL_O: [f64; 3] = [1.0, 0.62, 0.24];
    let mut out = Vec::with_capacity(4 * w.count);
    for i in 0..w.count {
        let q = (w.phase + i as f64 / w.count as f64).rem_euclid(1.0);
        let env = (std::f64::consts::PI * q).sin();
        let (x, y) = w.curve.point(q);
        out.push(Dot {
            x,
            y,
            r: w.radius * 2.4,
            opacity: 0.25 * env,
            halo: true,
        });
        for k in 0..3 {
            let qk = (q - 0.018 * k as f64).clamp(0.0, 1.0);
            let (x, y) = w.curve.point(qk);
            out.push(Dot {
                x,
                y,
                r: w.radius * TRAIL_R[k],
                opacity: env * TRAIL_O[k],
                halo: false,
            });
        }
    }
    out
}

pub type DotStore = Rc<RefCell<BTreeMap<String, DotWire>>>;

/// FNV-1a over the id, mapped to [0, 1).
fn start_phase(id: &str) -> f64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in id.bytes() {
        h ^= u64::from(b);
        h = h.wrapping_mul(0x0000_0100_0000_01b3);
    }
    (h >> 11) as f64 / (1u64 << 53) as f64
}

/// Keeps only the active wires; survivors keep their phase, new ones start at a phase of their id.
pub fn sync(store: &DotStore, wires: &[Wire]) {
    let mut map = store.borrow_mut();
    let mut next = BTreeMap::new();
    for w in wires.iter().filter(|w| w.active) {
        let phase = map
            .get(&w.id)
            .map_or_else(|| start_phase(&w.id), |d| d.phase);
        next.insert(w.id.clone(), dot_wire(w, phase));
    }
    *map = next;
}

/// Whether the user asked the system for less motion. Always false off wasm.
pub fn reduced_motion() -> bool {
    #[cfg(target_arch = "wasm32")]
    {
        web_sys::window()
            .and_then(|w| {
                w.match_media(crate::i18n::settings::REDUCED_MOTION_QUERY)
                    .ok()
                    .flatten()
            })
            .is_some_and(|m| m.matches())
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        false
    }
}

#[cfg(target_arch = "wasm32")]
const SVG_NS: &str = "http://www.w3.org/2000/svg";

#[cfg(target_arch = "wasm32")]
fn num(v: f64) -> String {
    let s = format!("{v:.2}");
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s.is_empty() || s == "-" {
        "0".to_string()
    } else {
        s.to_string()
    }
}

#[cfg(target_arch = "wasm32")]
fn set_attr(el: &web_sys::Element, name: &str, value: &str) {
    if el.get_attribute(name).as_deref() != Some(value) {
        let _ = el.set_attribute(name, value);
    }
}

/// Starts the animation loop on `layer`. Dots stop (and the loop ends) once the returned flag is
/// set to `false`.
#[cfg(target_arch = "wasm32")]
pub fn run(layer: web_sys::Element, store: DotStore) -> Rc<std::cell::Cell<bool>> {
    use leptos::prelude::request_animation_frame;
    use std::cell::Cell;

    let alive = Rc::new(Cell::new(true));
    let last = Rc::new(Cell::new(js_sys::Date::now()));

    fn frame(layer: web_sys::Element, store: DotStore, alive: Rc<Cell<bool>>, last: Rc<Cell<f64>>) {
        if !alive.get() {
            return;
        }
        let now = js_sys::Date::now();
        let dt = ((now - last.get()) / 1000.0).clamp(0.0, 0.1);
        last.set(now);
        draw(&layer, &store, dt);
        let (l, s, a, t) = (layer.clone(), store.clone(), alive.clone(), last.clone());
        request_animation_frame(move || frame(l, s, a, t));
    }

    fn draw(layer: &web_sys::Element, store: &DotStore, dt: f64) {
        let Some(doc) = layer.owner_document() else {
            return;
        };
        let mut map = store.borrow_mut();
        // Drop groups of wires that are gone.
        let mut existing: BTreeMap<String, web_sys::Element> = BTreeMap::new();
        let mut child = layer.first_element_child();
        while let Some(el) = child {
            child = el.next_element_sibling();
            match el.get_attribute("data-k") {
                Some(k) if map.contains_key(&k) && !existing.contains_key(&k) => {
                    existing.insert(k, el);
                }
                _ => el.remove(),
            }
        }
        for (id, w) in map.iter_mut() {
            w.phase = advance(w.phase, dt, w.speed, w.len);
            let g = match existing.get(id) {
                Some(g) => g.clone(),
                None => {
                    let Ok(g) = doc.create_element_ns(Some(SVG_NS), "g") else {
                        continue;
                    };
                    let _ = g.set_attribute("data-k", id);
                    let _ = layer.append_child(&g);
                    g
                }
            };
            set_attr(
                &g,
                "class",
                &format!("energy-dots__wire energy-tone--{}", w.tone),
            );
            let list = dots(w);
            // Add missing circles, remove extras.
            let mut have = g.child_element_count() as usize;
            while have < list.len() {
                let Ok(c) = doc.create_element_ns(Some(SVG_NS), "circle") else {
                    break;
                };
                let _ = g.append_child(&c);
                have += 1;
            }
            let mut node = g.first_element_child();
            for d in &list {
                let Some(c) = node else { break };
                node = c.next_element_sibling();
                set_attr(&c, "cx", &num(d.x));
                set_attr(&c, "cy", &num(d.y));
                set_attr(&c, "r", &num(d.r));
                set_attr(&c, "style", &format!("opacity:{}", num(d.opacity)));
                set_attr(&c, "class", if d.halo { "energy-dots__halo" } else { "" });
            }
            while let Some(c) = node {
                node = c.next_element_sibling();
                c.remove();
            }
        }
    }

    frame(layer, store, alive.clone(), last);
    alive
}

#[cfg(test)]
mod tests {
    use super::*;

    fn wire(id: &str, watts: f64, active: bool) -> Wire {
        Wire {
            id: id.to_string(),
            from: id.to_string(),
            to: "house".to_string(),
            kind: oikos_core::energy::NodeKind::Solar,
            tone: "solar",
            from_tone: "solar",
            to_tone: "house",
            curve: Curve {
                from: (0.0, 0.0),
                ctrl: (50.0, 0.0),
                to: (100.0, 0.0),
            },
            watts,
            active,
            width: 2.0,
        }
    }

    fn close(a: f64, b: f64) -> bool {
        (a - b).abs() < 1e-9
    }

    #[test]
    fn advance_wraps_and_ignores_empty_curves() {
        assert!(close(advance(0.9, 1.0, 20.0, 100.0), 0.1));
        assert!(close(advance(0.3, 1.0, 50.0, 0.0), 0.3));
    }

    #[test]
    fn dots_fade_at_both_ends() {
        let mut w = dot_wire(&wire("a", 500.0, true), 0.0);
        w.count = 2;
        let d = dots(&w);
        assert_eq!(d.len(), 8);
        assert!(d[0].halo && !d[1].halo);
        assert!(close(d[1].x, w.curve.from.0) && close(d[1].y, w.curve.from.1));
        assert!(close(d[1].opacity, 0.0));
        assert!(close(d[5].opacity, 1.0));
    }

    #[test]
    fn faster_and_bigger_with_power() {
        let lo = dot_wire(&wire("a", 10.0, true), 0.0);
        let hi = dot_wire(&wire("a", 10_000.0, true), 0.0);
        assert!(close(lo.speed, 30.0) && close(hi.speed, 120.0));
        assert!(close(lo.radius, 2.2) && close(hi.radius, 4.8));
        assert!(lo.count >= 2 && hi.count >= 2);
    }

    #[test]
    fn sync_drops_gone_wires_and_keeps_phase() {
        let store: DotStore = Rc::default();
        sync(
            &store,
            &[
                wire("a", 100.0, true),
                wire("b", 100.0, true),
                wire("z", 0.0, false),
            ],
        );
        assert_eq!(store.borrow().len(), 2);
        store.borrow_mut().get_mut("a").unwrap().phase = 0.42;
        sync(&store, &[wire("a", 200.0, true), wire("c", 100.0, true)]);
        let keys: Vec<String> = store.borrow().keys().cloned().collect();
        assert_eq!(keys, ["a", "c"]);
        assert!(close(store.borrow()["a"].phase, 0.42));
        let other: DotStore = Rc::default();
        sync(&other, &[wire("c", 5.0, true)]);
        assert!(close(other.borrow()["c"].phase, store.borrow()["c"].phase));
        assert!((0.0..1.0).contains(&store.borrow()["c"].phase));
    }
}
