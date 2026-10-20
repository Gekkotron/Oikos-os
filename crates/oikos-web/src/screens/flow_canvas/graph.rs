//! The flow itself, in world coordinates: one pill per node (role colour, icon, sentence title,
//! sub-line, pins) and the routed wires between them, with "yes"/"no" labels and the classes a
//! run lights, dims or animates. Used in view and edit mode alike.

use std::collections::{BTreeMap, BTreeSet};
use std::time::Duration;

use leptos::html;
use leptos::prelude::*;
use oikos_api::StepOutcome;
use oikos_core::flow::{Flow, NodeKind, Position};
use wasm_bindgen::JsCast;

use super::Draft;
use super::viewport::Camera;
use crate::components::now::Now;
use crate::flow_edit::EdgeKey;
use crate::flow_layout::{NODE_H, Pt, out_anchor, positions, routes};
use crate::flow_text::{Ctx, Role, role, subline, title};
use crate::flows::{Highlight, orphans};
use crate::house::House;
use crate::icons::node_icon;

/// Whether the graph is only looked at or edited (cards drag, settings open).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    View,
    Edit,
}

/// What is selected on the canvas: a card by id, or a wire by `(from, port, to)`.
#[derive(Clone, Debug, PartialEq)]
pub enum Selection {
    Node(String),
    Edge(EdgeKey),
}

/// `m:ss` of a number of seconds.
fn minutes_seconds(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// `yes` for a wire leaving an `if` by `true`, `no` for `false`, nothing for other wires.
fn tone(key: &EdgeKey) -> Option<&'static str> {
    match key.1.as_deref() {
        Some("true") => Some("yes"),
        Some("false") => Some("no"),
        _ => None,
    }
}

fn role_class(role: Role) -> &'static str {
    match role {
        Role::Trigger => "flow-node--trigger",
        Role::Check => "flow-node--check",
        Role::Action => "flow-node--action",
        Role::Wait => "flow-node--wait",
    }
}

/// The height, from a card's top, of the pin an output `port` leaves by.
fn port_top(kind: &NodeKind, port: Option<&str>) -> f64 {
    out_anchor(kind, Pt { x: 0.0, y: 0.0 }, port).y
}

/// Unix seconds, for the countdown of a parked delay.
fn unix_now() -> u64 {
    (js_sys::Date::now() / 1000.0) as u64
}

/// Where the pointer sits in world coordinates, from a client-coord pointer event. `layer` is
/// the `.flow-graph` element, whose top-left is the world origin on screen.
fn pointer_in_canvas(
    ev: &web_sys::PointerEvent,
    layer: &web_sys::Element,
    zoom: f64,
) -> (f64, f64) {
    let rect = layer.get_bounding_client_rect();
    let z = if zoom.abs() < f64::EPSILON { 1.0 } else { zoom };
    (
        (f64::from(ev.client_x()) - rect.left()) / z,
        (f64::from(ev.client_y()) - rect.top()) / z,
    )
}

/// What drag is in progress: the id of the node under the finger, the offset of its top-left
/// from the pointer's canvas-coord position at pick-up, and whether the pointer has actually
/// moved far enough to count as a drag (a pointer-up with `moved = false` is a click).
#[derive(Clone, Debug, PartialEq)]
struct Drag {
    node: String,
    offset_x: f64,
    offset_y: f64,
    moved: bool,
}

/// Pointer travel in canvas coords above which a pointerdown counts as a drag; below, it is a
/// click that selects the node.
const DRAG_THRESHOLD: f64 = 3.0;

/// The signals every card reads.
#[derive(Clone, Copy)]
struct Shared {
    flow: Signal<Flow>,
    placed: Memo<BTreeMap<String, Pt>>,
    lit: Signal<Highlight>,
    dim: Signal<bool>,
    steps: Signal<BTreeMap<String, String>>,
    parked: Signal<Option<(String, u64)>>,
    clock: RwSignal<u64>,
    orphaned: Memo<BTreeSet<String>>,
    names: Memo<Vec<(String, String)>>,
    selection: RwSignal<Option<Selection>>,
    mode: Mode,
    drag: RwSignal<Option<Drag>>,
    layer: NodeRef<html::Div>,
}

/// The flow as pills and wires. `lit` is the run shown (its cards and wires light up), `dim`
/// fades everything it did not visit, `live` animates its wires, `steps` is the time label of
/// each visited card and `parked` the delay a running run waits on, with the unix second the
/// wait ends at (it shows a countdown).
#[component]
pub fn Graph(
    flow: Signal<Flow>,
    mode: Mode,
    lit: Signal<Highlight>,
    dim: Signal<bool>,
    steps: Signal<BTreeMap<String, String>>,
    parked: Signal<Option<(String, u64)>>,
    live: Signal<bool>,
    selection: RwSignal<Option<Selection>>,
) -> impl IntoView {
    let house = expect_context::<House>();
    let now = expect_context::<Now>();
    let placed = Memo::new(move |_| flow.with(positions));
    let wires = Memo::new(move |_| flow.with(|f| placed.with(|p| routes(f, p))));
    let ids =
        Memo::new(move |_| flow.with(|f| f.nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>()));
    let names = Memo::new(move |_| {
        house.flows.with(|flows| {
            flows
                .iter()
                .map(|f| (f.definition.id.clone(), f.definition.name.clone()))
                .collect::<Vec<_>>()
        })
    });

    // A one-second clock for the countdown, ticking only while a delay is parked.
    let clock = RwSignal::new(now.0.get_untracked());
    let ticker = StoredValue::new(None::<IntervalHandle>);
    let parked_some = Memo::new(move |_| parked.with(Option::is_some));
    Effect::new(move |_| {
        if let Some(handle) = ticker.get_value() {
            handle.clear();
            ticker.set_value(None);
        }
        if parked_some.get() {
            clock.set(unix_now());
            let handle =
                set_interval_with_handle(move || clock.set(unix_now()), Duration::from_secs(1));
            ticker.set_value(handle.ok());
        }
    });
    on_cleanup(move || {
        if let Some(handle) = ticker.try_get_value().flatten() {
            handle.clear();
        }
    });

    let shared = Shared {
        flow,
        placed,
        lit,
        dim,
        steps,
        parked,
        clock,
        orphaned: Memo::new(move |_| flow.with(orphans)),
        names,
        selection,
        mode,
        drag: RwSignal::new(None),
        layer: NodeRef::new(),
    };

    let wire_paths = move || {
        let lit = lit.get();
        let (dim, live) = (dim.get(), live.get());
        let selected = selection.get();
        wires.with(|w| {
            w.iter()
                .map(|(key, route)| {
                    let is_lit = lit.edges.contains(key);
                    let mut class = String::from("flow-wire");
                    if let Some(t) = tone(key) {
                        class.push_str(&format!(" flow-wire--{t}"));
                    }
                    if is_lit {
                        class.push_str(" flow-wire--lit");
                        if live {
                            class.push_str(" flow-wire--live");
                        }
                    } else if dim {
                        class.push_str(" flow-wire--dim");
                    }
                    if matches!(&selected, Some(Selection::Edge(k)) if k == key) {
                        class.push_str(" flow-wire--selected");
                    }
                    view! { <path class=class d=route.d.clone() /> }
                })
                .collect_view()
        })
    };
    let labels = move || {
        let lit = lit.get();
        let dim = dim.get();
        wires.with(|w| {
            w.iter()
                .filter_map(|(key, route)| {
                    let t = tone(key)?;
                    let faded = dim && !lit.edges.contains(key);
                    Some(view! {
                        <span
                            class=format!("flow-wire-label flow-wire-label--{t}")
                            class:flow-wire-label--dim=faded
                            style=format!("left:{}px;top:{}px", route.label_at.x, route.label_at.y)
                        >
                            {t}
                        </span>
                    })
                })
                .collect_view()
        })
    };

    view! {
        <div
            class="flow-graph"
            class:flow-graph--editing=mode == Mode::Edit
            node_ref=shared.layer
        >
            <svg class="flow-wires" width="1" height="1" aria-hidden="true">
                {wire_paths}
            </svg>
            {labels}
            <For
                each=move || ids.get()
                key=|id| id.clone()
                children=move |id| card(id, shared, house)
            />
        </div>
    }
}

/// One pill: reads its node, position and run state from `c` by id, so a card survives (and
/// keeps its pointer capture) while the flow around it changes.
fn card(id: String, c: Shared, house: House) -> impl IntoView {
    let node_id = id.clone();
    let kind = Memo::new(move |_| c.flow.with(|f| f.node(&node_id).map(|n| n.kind.clone())));
    let node_id = id.clone();
    let texts = Memo::new(move |_| {
        let names = c.names.get();
        c.flow.with(|f| {
            let node = f.node(&node_id)?;
            house.entities.with(|entities| {
                house.areas.with(|areas| {
                    let ctx = Ctx {
                        entities,
                        areas,
                        flows: &names,
                    };
                    Some((title(node, &ctx), subline(node, &ctx)))
                })
            })
        })
    });
    let title_text = move || texts.with(|t| t.as_ref().map(|t| t.0.clone()));
    let sub_text = move || texts.with(|t| t.as_ref().and_then(|t| t.1.clone()));

    let class = move || {
        kind.with(|k| {
            k.as_ref().map_or_else(String::new, |k| {
                let mut class = format!("flow-node {}", role_class(role(k)));
                if c.mode == Mode::Edit {
                    class.push_str(" flow-node--editing");
                }
                class
            })
        })
    };
    let icon = move || kind.with(|k| k.as_ref().map(node_icon).unwrap_or_default());

    let node_id = id.clone();
    let style = move || {
        c.placed.with(|p| {
            p.get(&node_id)
                .map(|at| format!("left:{}px;top:{}px", at.x, at.y))
                .unwrap_or_default()
        })
    };

    let pins = move || {
        kind.get().map(|k| {
            let input = (role(&k) != Role::Trigger).then(|| {
                view! { <span class="flow-node__in" style=format!("top:{}px", NODE_H / 2.0)></span> }
            });
            let ports: &[Option<&'static str>] = if matches!(k, NodeKind::If(_)) {
                &[Some("true"), Some("false")]
            } else {
                &[None]
            };
            let outputs = ports
                .iter()
                .map(|port| {
                    view! {
                        <span
                            class="flow-node__out"
                            data-port=*port
                            style=format!("top:{}px", port_top(&k, *port))
                        ></span>
                    }
                })
                .collect_view();
            view! {
                {input}
                {outputs}
            }
        })
    };

    let outcome_id = id.clone();
    let outcome = Memo::new(move |_| c.lit.with(|l| l.nodes.get(&outcome_id).copied()));
    let is_lit = move || outcome.get().is_some_and(|o| o != StepOutcome::Failed);
    let failed = move || outcome.get() == Some(StepOutcome::Failed);
    let dim = move || c.dim.get() && outcome.get().is_none();
    let pending_id = id.clone();
    let pending = move || {
        c.parked
            .with(|p| p.as_ref().is_some_and(|(n, _)| *n == pending_id))
    };
    let orphan_id = id.clone();
    let orphan = move || c.orphaned.with(|o| o.contains(&orphan_id));
    let selected_id = id.clone();
    let selected = move || {
        c.selection
            .with(|s| matches!(s, Some(Selection::Node(n)) if *n == selected_id))
    };
    let step_id = id.clone();
    let step = move || match c.parked.get() {
        Some((node, deadline)) if node == step_id => format!(
            "{} left",
            minutes_seconds(deadline.saturating_sub(c.clock.get()))
        ),
        _ => c
            .steps
            .with(|s| s.get(&step_id).cloned().unwrap_or_default()),
    };

    // Edit mode: the drag code of the first editor, kept until the React Flow-style editor.
    let drag = c.drag;
    let draft = use_context::<Draft>().map(|d| d.0);
    let camera = expect_context::<Camera>();
    let id_for_down = id.clone();
    let down = move |ev: web_sys::PointerEvent| {
        if c.mode != Mode::Edit {
            return;
        }
        if ev.button() != 0 && ev.pointer_type() == "mouse" {
            return;
        }
        let (Some(layer), Some(draft)) = (c.layer.get_untracked(), draft) else {
            return;
        };
        let (px, py) = pointer_in_canvas(&ev, layer.as_ref(), camera.view.get_untracked().zoom);
        let (nx, ny) = draft.with_untracked(|f| {
            f.node(&id_for_down)
                .and_then(|n| n.position)
                .map_or((0.0, 0.0), |p| (p.x, p.y))
        });
        if let Some(target) = ev
            .current_target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
        {
            let _ = target.set_pointer_capture(ev.pointer_id());
        }
        ev.prevent_default();
        // The surface below would pan otherwise.
        ev.stop_propagation();
        drag.set(Some(Drag {
            node: id_for_down.clone(),
            offset_x: px - nx,
            offset_y: py - ny,
            moved: false,
        }));
    };
    let id_for_move = id.clone();
    let moved = move |ev: web_sys::PointerEvent| {
        let Some(d) = drag.get_untracked() else {
            return;
        };
        if d.node != id_for_move {
            return;
        }
        let (Some(layer), Some(draft)) = (c.layer.get_untracked(), draft) else {
            return;
        };
        ev.stop_propagation();
        let (px, py) = pointer_in_canvas(&ev, layer.as_ref(), camera.view.get_untracked().zoom);
        let new_x = (px - d.offset_x).max(0.0);
        let new_y = (py - d.offset_y).max(0.0);
        let (nx, ny) = draft.with_untracked(|f| {
            f.node(&id_for_move)
                .and_then(|n| n.position)
                .map_or((new_x, new_y), |p| (p.x, p.y))
        });
        if !d.moved && (new_x - nx).hypot(new_y - ny) < DRAG_THRESHOLD {
            return;
        }
        drag.update(|cur| {
            if let Some(c) = cur {
                c.moved = true;
            }
        });
        let id = id_for_move.clone();
        draft.update(|f| {
            if let Some(n) = f.nodes.iter_mut().find(|n| n.id == id) {
                n.position = Some(Position { x: new_x, y: new_y });
            }
        });
    };
    let id_for_up = id.clone();
    let up = std::rc::Rc::new(move |_ev: web_sys::PointerEvent| {
        let was_click = drag.with_untracked(
            |d| matches!(d, Some(Drag { node, moved: false, .. }) if node == &id_for_up),
        );
        drag.set(None);
        if was_click {
            c.selection.set(Some(Selection::Node(id_for_up.clone())));
        }
    });
    let up_on_up = {
        let up = up.clone();
        move |ev: web_sys::PointerEvent| up(ev)
    };
    let up_on_cancel = move |ev: web_sys::PointerEvent| up(ev);
    let id_for_class = id.clone();
    let is_dragging = move || {
        drag.with(|d| matches!(d, Some(Drag { node, moved: true, .. }) if node == &id_for_class))
    };

    view! {
        <div
            class=class
            class:flow-node--lit=is_lit
            class:flow-node--failed=failed
            class:flow-node--dim=dim
            class:flow-node--pending=pending
            class:flow-node--orphan=orphan
            class:flow-node--selected=selected
            class:flow-node--dragging=is_dragging
            data-node=id.clone()
            style=style
            title=title_text
            on:pointerdown=down
            on:pointermove=moved
            on:pointerup=up_on_up
            on:pointercancel=up_on_cancel
        >
            <span class="flow-node__icon" inner_html=icon></span>
            <span class="flow-node__text">
                <span class="flow-node__title">{title_text}</span>
                {move || sub_text().map(|s| view! { <span class="flow-node__sub">{s}</span> })}
            </span>
            <span class="flow-node__step">{step}</span>
            {pins}
        </div>
    }
}
