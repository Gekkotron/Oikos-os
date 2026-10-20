//! One flow drawn like n8n's canvas: read-only shows the last run lit on top of the layout,
//! edit mode lets each node be dragged to a new position and saved. Zoom controls scale the
//! canvas in both modes.

use std::collections::BTreeMap;

use leptos::html;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use oikos_api::{FlowView, RunView, StepOutcome};
use oikos_core::flow::{Flow, FlowMode, Missing, NodeKind, Op, Position};
use wasm_bindgen::JsCast;

use crate::alerts::relative_time;
use crate::api;
use crate::components::flow_switch::FlowSwitch;
use crate::components::now::Now;
use crate::components::toast::Toasts;
use crate::flow_layout::{
    NODE_H, PAD, Pt, bounds, out_anchor, positions, routes, with_resolved_positions,
};
use crate::flows::{
    clamp_zoom, highlight, node_detail, node_title, orphans, parked_deadline, parked_node,
    run_duration, status_class, status_word, step_offset, trigger_line,
};
use crate::format::format_timestamp;
use crate::house::House;
use crate::icons::{CHECK, LOUPE, PENCIL, PLAY, X_MARK, node_icon};
use crate::screens::flow_schedule::{SaveBlocked, ScheduleField};

fn type_class(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::TriggerState(_) => "flow-node--trigger",
        NodeKind::TriggerSchedule(_) => "flow-node--schedule",
        NodeKind::If(_) => "flow-node--if",
        NodeKind::Delay(_) => "flow-node--delay",
        NodeKind::Command(_) => "flow-node--command",
        NodeKind::FlowEnable(_) => "flow-node--flow",
    }
}

/// The height, from a card's top, of the pin an output `port` leaves by.
fn port_top(kind: &NodeKind, port: Option<&str>) -> f64 {
    out_anchor(kind, Pt { x: 0.0, y: 0.0 }, port).y
}

/// The scrolling canvas's size: every card plus the margin, measured from the origin.
fn canvas_size(pos: &BTreeMap<String, Pt>) -> (f64, f64) {
    let b = bounds(pos);
    (b.x + b.w + PAD, b.y + b.h + PAD)
}

/// `m:ss` of a number of seconds.
fn minutes_seconds(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

/// Where the pointer sits inside the untransformed canvas, from a client-coord pointer event
/// on an element inside the scaled canvas. `canvas` is the `.flow-canvas` div with the scale.
fn pointer_in_canvas(
    ev: &web_sys::PointerEvent,
    canvas: &web_sys::Element,
    zoom: f64,
) -> (f64, f64) {
    let rect = canvas.get_bounding_client_rect();
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

/// Handles Ctrl/⌘ + wheel on the scroll container: zooms around the cursor by rebalancing scroll
/// offsets so the point under the pointer stays put. Plain wheel is left to the browser so the
/// container keeps scrolling naturally.
fn wheel_zoom(ev: web_sys::WheelEvent, scroll: NodeRef<html::Div>, zoom: RwSignal<f64>) {
    if !(ev.ctrl_key() || ev.meta_key()) {
        return;
    }
    ev.prevent_default();
    let Some(scroll_el) = scroll.get_untracked() else {
        return;
    };
    let el: &web_sys::Element = scroll_el.as_ref();
    let rect = el.get_bounding_client_rect();
    let z1 = zoom.get_untracked();
    let factor = if ev.delta_y() > 0.0 { 0.9 } else { 1.0 / 0.9 };
    let z2 = clamp_zoom(z1 * factor);
    if (z2 - z1).abs() < f64::EPSILON {
        return;
    }
    let px = f64::from(ev.client_x()) - rect.left();
    let py = f64::from(ev.client_y()) - rect.top();
    let sl1 = f64::from(el.scroll_left());
    let st1 = f64::from(el.scroll_top());
    let z_div = if z1.abs() < f64::EPSILON { 1.0 } else { z1 };
    let cx = (px + sl1) / z_div;
    let cy = (py + st1) / z_div;
    zoom.set(z2);
    #[allow(clippy::cast_possible_truncation)]
    let new_sl = (cx * z2 - px).max(0.0).round() as i32;
    #[allow(clippy::cast_possible_truncation)]
    let new_st = (cy * z2 - py).max(0.0).round() as i32;
    el.set_scroll_left(new_sl);
    el.set_scroll_top(new_st);
}

/// Zoom presets the loupe cycles through, from most zoomed-out to most zoomed-in; a click
/// steps to the next and wraps.
const ZOOM_PRESETS: &[f64] = &[0.5, 0.75, 1.0, 1.25, 1.5, 2.0];

#[component]
fn ZoomLoupe(zoom: RwSignal<f64>) -> impl IntoView {
    let cycle = move |_: web_sys::MouseEvent| {
        zoom.update(|z| {
            let i = ZOOM_PRESETS
                .iter()
                .position(|p| (p - *z).abs() < 1e-3)
                .unwrap_or_else(|| {
                    ZOOM_PRESETS
                        .iter()
                        .enumerate()
                        .min_by(|(_, a), (_, b)| {
                            (**a - *z).abs().partial_cmp(&(**b - *z).abs()).unwrap()
                        })
                        .map(|(i, _)| i)
                        .unwrap_or(2)
                });
            *z = clamp_zoom(ZOOM_PRESETS[(i + 1) % ZOOM_PRESETS.len()]);
        });
    };
    let readout = move || format!("{}%", (zoom.get() * 100.0).round() as i64);
    view! {
        <button
            type="button"
            class="flow-loupe"
            aria-label="Zoom"
            on:click=cycle
        >
            <span class="flow-loupe__icon" inner_html=LOUPE></span>
            <span class="flow-loupe__readout">{readout}</span>
        </button>
    }
}

#[component]
fn EditControls(
    editing: RwSignal<bool>,
    draft: RwSignal<Flow>,
    saving: RwSignal<bool>,
    original: Flow,
) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let blocked = use_context::<SaveBlocked>();
    let is_blocked = move || blocked.is_some_and(|b| b.0.get());
    let cancel_flow = original.clone();
    let cancel = move |_| {
        if let Some(b) = blocked {
            b.0.set(false);
        }
        draft.set(cancel_flow.clone());
        editing.set(false);
    };
    let save = move |_| {
        if saving.get_untracked() || blocked.is_some_and(|b| b.0.get_untracked()) {
            return;
        }
        saving.set(true);
        let snapshot = draft.get_untracked();
        spawn_local(async move {
            match api::put_flow(&snapshot).await {
                Ok(view) => draft.set(view.definition),
                Err(e) => toasts.push(e.error),
            }
            saving.set(false);
        });
    };
    view! {
        <button
            type="button"
            class="btn btn--ghost btn--icon"
            aria-label="Cancel editing"
            title="Cancel"
            disabled=move || saving.get()
            on:click=cancel
            inner_html=X_MARK
        ></button>
        <button
            type="button"
            class="btn btn--icon"
            aria-label=move || if saving.get() { "Saving".to_string() } else { "Save".to_string() }
            title=move || if saving.get() { "Saving…".to_string() } else { "Save".to_string() }
            disabled=move || saving.get() || is_blocked()
            on:click=save
            inner_html=CHECK
        ></button>
    }
}

#[component]
fn ReadControls(
    flow: Flow,
    invalid: bool,
    busy: bool,
    editing: RwSignal<bool>,
    draft: RwSignal<Flow>,
) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let id = flow.id.clone();
    let name = flow.name.clone();
    let enabled = flow.enabled;
    let run_id = id.clone();
    let run_now = move |_| {
        let id = run_id.clone();
        spawn_local(async move {
            if let Err(e) = api::run_flow(&id).await {
                toasts.push(e.error);
            }
        });
    };
    let edit_flow = flow.clone();
    let enter_edit = move |_| {
        draft.set(with_resolved_positions(&edit_flow));
        editing.set(true);
    };
    view! {
        <FlowSwitch id=id name=name enabled=enabled invalid=invalid />
        <button
            type="button"
            class="btn btn--icon"
            aria-label="Run now"
            title="Run now"
            disabled=invalid || busy
            on:click=run_now
            inner_html=PLAY
        ></button>
        <button
            type="button"
            class="btn btn--ghost btn--icon"
            aria-label="Edit layout"
            title="Edit layout"
            disabled=invalid
            on:click=enter_edit
            inner_html=PENCIL
        ></button>
    }
}

#[component]
fn Header(
    view: FlowView,
    editing: RwSignal<bool>,
    draft: RwSignal<Flow>,
    saving: RwSignal<bool>,
) -> impl IntoView {
    let flow = view.definition.clone();
    let invalid = view.invalid.is_some();
    let busy = view.active_run.is_some();
    let name = flow.name.clone();
    let mode = match flow.mode {
        FlowMode::Single => "single",
        FlowMode::Restart => "restart",
    };
    let edit_flow = flow.clone();
    let read_flow = flow.clone();
    view! {
        <header class="flow-canvas__header">
            <A href="/flows" attr:class="sheet__back">
                "←"
                <span class="sheet__back-label">"Flows"</span>
            </A>
            <h1 class="sheet__title flow-canvas__title">
                {name}
                <span class="flow-canvas__mode">{mode}</span>
            </h1>
            <Show
                when=move || editing.get()
                fallback=move || {
                    view! {
                        <ReadControls
                            flow=read_flow.clone()
                            invalid=invalid
                            busy=busy
                            editing=editing
                            draft=draft
                        />
                    }
                }
            >
                <EditControls
                    editing=editing
                    draft=draft
                    saving=saving
                    original=edit_flow.clone()
                />
            </Show>
        </header>
    }
}

#[component]
fn Canvas(view: FlowView, zoom: RwSignal<f64>) -> impl IntoView {
    let house = expect_context::<House>();
    let now = expect_context::<Now>();
    let flow = view.definition.clone();
    let placed = positions(&flow);
    let (width, height) = canvas_size(&placed);
    let lit = highlight(&flow, view.runs.first());
    let orphaned = orphans(&flow);
    let parked = view
        .runs
        .first()
        .filter(|r| view.active_run.as_deref() == Some(r.id.as_str()))
        .and_then(|r| parked_node(&flow, r));
    let pos = |id: &str| placed.get(id).map(|p| (p.x, p.y));

    let wires = routes(&flow, &placed)
        .into_iter()
        .map(|(key, route)| {
            let is_lit = lit.edges.contains(&key);
            view! { <path class="flow-wire" class:flow-wire--lit=is_lit d=route.d /> }
        })
        .collect_view();

    let nodes = flow
        .nodes
        .iter()
        .filter_map(|node| {
            let (x, y) = pos(&node.id)?;
            let outcome = lit.nodes.get(&node.id).copied();
            let failed = outcome == Some(StepOutcome::Failed);
            let is_lit = outcome.is_some() && !failed;
            let deadline = parked
                .as_ref()
                .filter(|(id, _)| *id == node.id)
                .and_then(|(id, since)| parked_deadline(&flow, id, *since));
            let latest = view
                .runs
                .first()
                .and_then(|r| r.steps.iter().rev().find(|s| s.node == node.id));
            let is_if = matches!(node.kind, NodeKind::If(_));
            let is_trigger = matches!(node.kind, NodeKind::TriggerState(_));
            let trigger_text = (is_trigger && latest.is_some()).then(|| {
                let trigger = view.runs.first().and_then(|r| r.trigger.as_ref());
                house.entities.with(|e| trigger_line(trigger, e))
            });
            let step_detail = if is_if {
                None
            } else {
                trigger_text.or_else(|| latest.map(|s| s.detail.clone()).filter(|d| !d.is_empty()))
            };
            let taken = latest.filter(|_| is_if).map(|s| s.detail.clone());
            let step_line = move || match deadline {
                Some(at) => Some(format!(
                    "{} left",
                    minutes_seconds(at.saturating_sub(now.0.get()))
                )),
                None => step_detail.clone(),
            };
            let title = node_title(node);
            let detail = house.entities.with(|e| node_detail(node, e));
            let detail = (detail != title).then_some(detail);
            let ports = is_if.then(|| {
                ["true", "false"]
                    .into_iter()
                    .map(|port| {
                        let top = port_top(&node.kind, Some(port));
                        view! {
                            <span
                                class="flow-port"
                                class:flow-port--taken=taken.as_deref() == Some(port)
                                style=format!("top:{top}px")
                            >
                                {port}
                            </span>
                            <span class="flow-node__out" style=format!("top:{top}px")></span>
                        }
                    })
                    .collect_view()
            });
            let out_dot = (!is_if).then(|| {
                let top = port_top(&node.kind, None);
                view! { <span class="flow-node__out" style=format!("top:{top}px")></span> }
            });
            let in_dot = (!is_trigger).then(|| {
                let top = NODE_H / 2.0;
                view! { <span class="flow-node__in" style=format!("top:{top}px")></span> }
            });
            Some(view! {
                <div
                    class=format!("flow-node {}", type_class(&node.kind))
                    class:flow-node--lit=is_lit
                    class:flow-node--failed=failed
                    class:flow-node--pending=deadline.is_some()
                    class:flow-node--orphan=orphaned.contains(&node.id)
                    data-node=node.id.clone()
                    style=format!("left:{x}px;top:{y}px")
                >
                    <span class="flow-node__icon" inner_html=node_icon(&node.kind)></span>
                    <span class="flow-node__title">{title}</span>
                    {detail.map(|d| view! { <span class="flow-node__detail">{d}</span> })}
                    <span class="flow-node__step">{step_line}</span>
                    {in_dot}
                    {out_dot}
                    {ports}
                </div>
            })
        })
        .collect_view();

    let viewport_style = move || {
        format!(
            "width:{}px;height:{}px",
            width * zoom.get(),
            height * zoom.get()
        )
    };
    let canvas_style = move || {
        format!(
            "width:{width}px;height:{height}px;transform:scale({});transform-origin:0 0",
            zoom.get()
        )
    };
    let scroll_ref: NodeRef<html::Div> = NodeRef::new();
    let on_wheel = move |ev: web_sys::WheelEvent| wheel_zoom(ev, scroll_ref, zoom);
    view! {
        <div class="flow-canvas-scroll" node_ref=scroll_ref on:wheel=on_wheel>
            <div class="flow-canvas-viewport" style=viewport_style>
                <div class="flow-canvas" style=canvas_style>
                    <svg
                        class="flow-wires"
                        width=width.to_string()
                        height=height.to_string()
                        style="position:absolute;inset:0;pointer-events:none"
                    >
                        {wires}
                    </svg>
                    {nodes}
                </div>
            </div>
        </div>
    }
}

#[component]
fn EditCanvas(
    draft: RwSignal<Flow>,
    selected: RwSignal<Option<String>>,
    zoom: RwSignal<f64>,
) -> impl IntoView {
    let drag = RwSignal::new(None::<Drag>);
    let canvas_ref: NodeRef<html::Div> = NodeRef::new();

    let size = Memo::new(move |_| draft.with(|f| canvas_size(&positions(f))));
    let width = Memo::new(move |_| size.get().0);
    let height = Memo::new(move |_| size.get().1);

    let node_order = Memo::new(move |_| {
        draft.with(|f| f.nodes.iter().map(|n| n.id.clone()).collect::<Vec<_>>())
    });
    let wires = move || {
        draft.with(|f| {
            routes(f, &positions(f))
                .into_iter()
                .map(|(_, route)| view! { <path class="flow-wire" d=route.d /> })
                .collect_view()
        })
    };

    let viewport_style = move || {
        format!(
            "width:{}px;height:{}px",
            width.get() * zoom.get(),
            height.get() * zoom.get()
        )
    };
    let canvas_style = move || {
        format!(
            "width:{}px;height:{}px;transform:scale({});transform-origin:0 0",
            width.get(),
            height.get(),
            zoom.get()
        )
    };

    let scroll_ref: NodeRef<html::Div> = NodeRef::new();
    let on_wheel = move |ev: web_sys::WheelEvent| wheel_zoom(ev, scroll_ref, zoom);
    view! {
        <div
            class="flow-canvas-scroll flow-canvas-scroll--editing"
            node_ref=scroll_ref
            on:wheel=on_wheel
        >
            <div class="flow-canvas-viewport" style=viewport_style>
                <div
                    class="flow-canvas flow-canvas--editing"
                    node_ref=canvas_ref
                    style=canvas_style
                >
                    <svg
                        class="flow-wires"
                        width=move || width.get().to_string()
                        height=move || height.get().to_string()
                        style="position:absolute;inset:0;pointer-events:none"
                    >
                        {wires}
                    </svg>
                    <For
                        each=move || node_order.get()
                        key=|id| id.clone()
                        children=move |id| {
                            edit_node(id, draft, drag, selected, zoom, canvas_ref)
                        }
                    />
                </div>
            </div>
        </div>
    }
}

fn edit_node(
    id: String,
    draft: RwSignal<Flow>,
    drag: RwSignal<Option<Drag>>,
    selected: RwSignal<Option<String>>,
    zoom: RwSignal<f64>,
    canvas_ref: NodeRef<html::Div>,
) -> impl IntoView {
    let (kind_class, port_info) = draft.with_untracked(|f| {
        let node = f.node(&id).expect("node from draft");
        let is_if = matches!(node.kind, NodeKind::If(_));
        let is_trigger = matches!(node.kind, NodeKind::TriggerState(_));
        let in_top = (!is_trigger).then_some(NODE_H / 2.0);
        let out_top = (!is_if).then(|| port_top(&node.kind, None));
        let ports: Vec<(String, f64)> = if is_if {
            ["true", "false"]
                .into_iter()
                .map(|p| (p.to_owned(), port_top(&node.kind, Some(p))))
                .collect()
        } else {
            Vec::new()
        };
        (type_class(&node.kind).to_owned(), (in_top, out_top, ports))
    });
    let (in_top, out_top, ports) = port_info;

    let id_for_style = id.clone();
    let node_style = move || {
        draft.with(|f| {
            let (x, y) = f
                .node(&id_for_style)
                .and_then(|n| n.position)
                .map_or((0.0, 0.0), |p| (p.x, p.y));
            format!("left:{x}px;top:{y}px")
        })
    };
    let id_for_title = id.clone();
    let title_text = move || {
        draft
            .with(|f| f.node(&id_for_title).map(node_title))
            .unwrap_or_default()
    };
    let id_for_detail = id.clone();
    let detail_text = move || {
        draft
            .with(|f| {
                f.node(&id_for_detail).map(|n| {
                    let d = node_detail(n, &BTreeMap::new());
                    let t = node_title(n);
                    if d == t { String::new() } else { d }
                })
            })
            .unwrap_or_default()
    };

    let id_for_down = id.clone();
    let down = move |ev: web_sys::PointerEvent| {
        if ev.button() != 0 && ev.pointer_type() == "mouse" {
            return;
        }
        let Some(canvas) = canvas_ref.get_untracked() else {
            return;
        };
        let (px, py) = pointer_in_canvas(&ev, canvas.as_ref(), zoom.get_untracked());
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
        let Some(canvas) = canvas_ref.get_untracked() else {
            return;
        };
        let (px, py) = pointer_in_canvas(&ev, canvas.as_ref(), zoom.get_untracked());
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
            selected.set(Some(id_for_up.clone()));
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
    let id_for_sel = id.clone();
    let is_selected = move || selected.with(|s| s.as_deref() == Some(id_for_sel.as_str()));

    view! {
        <div
            class=format!("flow-node flow-node--editing {kind_class}")
            class:flow-node--dragging=is_dragging
            class:flow-node--selected=is_selected
            data-node=id.clone()
            style=node_style
            on:pointerdown=down
            on:pointermove=moved
            on:pointerup=up_on_up
            on:pointercancel=up_on_cancel
        >
            <span class="flow-node__title">{title_text}</span>
            <span class="flow-node__detail">{detail_text}</span>
            {in_top.map(|top| view! {
                <span class="flow-node__in" style=format!("top:{top}px")></span>
            })}
            {out_top.map(|top| view! {
                <span class="flow-node__out" style=format!("top:{top}px")></span>
            })}
            {ports.into_iter().map(|(port, top)| view! {
                <span class="flow-port" style=format!("top:{top}px")>{port}</span>
                <span class="flow-node__out" style=format!("top:{top}px")></span>
            }).collect_view()}
        </div>
    }
}

#[component]
fn RunItem(run: RunView, flow: Flow) -> impl IntoView {
    let house = expect_context::<House>();
    let now = expect_context::<Now>();
    let trigger = house
        .entities
        .with(|e| trigger_line(run.trigger.as_ref(), e));
    let started = run.started_at;
    let duration = run_duration(run.started_at, run.ended_at);
    let steps = run
        .steps
        .iter()
        .map(|s| {
            let node = flow
                .node(&s.node)
                .map_or_else(|| s.node.clone(), node_title);
            let outcome = match s.outcome {
                StepOutcome::Ok => "ok",
                StepOutcome::Failed => "failed",
                _ => "skipped",
            };
            let outcome_class = match s.outcome {
                StepOutcome::Ok => "step__outcome--ok",
                StepOutcome::Failed => "step__outcome--failed",
                StepOutcome::Skipped => "step__outcome--skipped",
            };
            let offset = step_offset(s.at, started);
            let detail = (!s.detail.is_empty()).then(|| s.detail.clone());
            view! {
                <li class="step">
                    <span class="step__at" title=format_timestamp(s.at)>{offset}</span>
                    <span class="step__node">{node}</span>
                    <span class=format!("step__outcome {outcome_class}")>{outcome}</span>
                    {detail.map(|d| view! { <span class="step__detail">{d}</span> })}
                </li>
            }
        })
        .collect_view();
    view! {
        <li class=format!("run {}", status_class(run.status))>
            <details>
                <summary>
                    <span class="run__status">{status_word(run.status)}</span>
                    <span class="run__time" title=format_timestamp(started)>
                        {move || relative_time(started, now.0.get())}
                    </span>
                    {duration.map(|d| view! {
                        <span class="run__duration" title="Duration">{format!("· {d}")}</span>
                    })}
                    <span class="run__trigger">{trigger}</span>
                </summary>
                <ol class="run__steps">{steps}</ol>
            </details>
        </li>
    }
}

fn op_str(op: Op) -> &'static str {
    match op {
        Op::Eq => "eq",
        Op::Ne => "ne",
        Op::Gt => "gt",
        Op::Lt => "lt",
        Op::Gte => "gte",
        Op::Lte => "lte",
    }
}

fn op_from(s: &str) -> Option<Op> {
    match s {
        "eq" => Some(Op::Eq),
        "ne" => Some(Op::Ne),
        "gt" => Some(Op::Gt),
        "lt" => Some(Op::Lt),
        "gte" => Some(Op::Gte),
        "lte" => Some(Op::Lte),
        _ => None,
    }
}

/// A text field whose value is a JSON scalar. The user types raw JSON (`true`, `21.5`, `"ON"`);
/// `None` means absent (an empty textbox clears an optional field). On a parse error the field
/// shows a hint and does not touch `write`.
fn json_text(initial: &serde_json::Value) -> String {
    match initial {
        serde_json::Value::String(s) => format!("\"{s}\""),
        other => other.to_string(),
    }
}

#[component]
fn JsonField(
    label: String,
    initial: serde_json::Value,
    optional: bool,
    write: Callback<Option<serde_json::Value>>,
) -> impl IntoView {
    let raw = RwSignal::new(json_text(&initial));
    let error = RwSignal::new(None::<String>);
    let on_input = move |ev: web_sys::Event| {
        let text = event_target_value(&ev);
        raw.set(text.clone());
        let trimmed = text.trim();
        if trimmed.is_empty() {
            if optional {
                error.set(None);
                write.run(None);
            } else {
                error.set(Some("required".into()));
            }
            return;
        }
        match serde_json::from_str::<serde_json::Value>(trimmed) {
            Ok(value) => {
                error.set(None);
                write.run(Some(value));
            }
            Err(e) => error.set(Some(e.to_string())),
        }
    };
    view! {
        <label class="node-editor__field">
            <span class="node-editor__label">{label}</span>
            <input
                type="text"
                class="field"
                spellcheck="false"
                autocomplete="off"
                prop:value=move || raw.get()
                on:input=on_input
                aria-invalid=move || error.with(|e| e.is_some()).then_some("true")
            />
            {move || error.get().map(|e| view! {
                <span class="node-editor__error">{e}</span>
            })}
        </label>
    }
}

#[component]
fn TextField(
    label: String,
    initial: String,
    placeholder: String,
    optional: bool,
    write: Callback<Option<String>>,
) -> impl IntoView {
    let raw = RwSignal::new(initial);
    let on_input = move |ev: web_sys::Event| {
        let text = event_target_value(&ev);
        raw.set(text.clone());
        let trimmed = text.trim();
        if trimmed.is_empty() {
            if optional {
                write.run(None);
            }
        } else {
            write.run(Some(trimmed.to_owned()));
        }
    };
    view! {
        <label class="node-editor__field">
            <span class="node-editor__label">{label}</span>
            <input
                type="text"
                class="field"
                placeholder=placeholder
                spellcheck="false"
                autocomplete="off"
                prop:value=move || raw.get()
                on:input=on_input
            />
        </label>
    }
}

#[component]
fn NodeEditor(draft: RwSignal<Flow>, selected: RwSignal<Option<String>>) -> impl IntoView {
    view! {
        <Show when=move || selected.with(Option::is_some)>
            {move || {
                let Some(id) = selected.get() else { return ().into_any(); };
                let Some(node) = draft.with_untracked(|f| f.node(&id).cloned()) else {
                    return ().into_any();
                };
                node_editor_body(draft, selected, id, node).into_any()
            }}
        </Show>
    }
}

fn update_node<F>(draft: RwSignal<Flow>, id: &str, mutate: F)
where
    F: FnOnce(&mut oikos_core::flow::Node),
{
    let id = id.to_owned();
    draft.update(|f| {
        if let Some(n) = f.nodes.iter_mut().find(|n| n.id == id) {
            mutate(n);
        }
    });
}

fn node_editor_body(
    draft: RwSignal<Flow>,
    selected: RwSignal<Option<String>>,
    id: String,
    node: oikos_core::flow::Node,
) -> impl IntoView {
    let kind_label = match &node.kind {
        NodeKind::TriggerState(_) => "Trigger",
        NodeKind::TriggerSchedule(_) => "Schedule",
        NodeKind::If(_) => "If",
        NodeKind::Delay(_) => "Delay",
        NodeKind::Command(_) => "Command",
        NodeKind::FlowEnable(_) => "Flow switch",
    };
    let id_display = id.clone();
    let close = move |_: web_sys::MouseEvent| selected.set(None);
    let initial_name = node.name.clone().unwrap_or_default();
    let name_id = id.clone();
    let name_write = Callback::new(move |value: Option<String>| {
        update_node(draft, &name_id, |n| n.name = value);
    });
    let kind_fields = kind_fields(draft, id.clone(), node.kind);
    view! {
        <section class="node-editor" aria-label="Node editor">
            <header class="node-editor__header">
                <div class="node-editor__title">
                    <span class="chip">{kind_label}</span>
                    <code class="node-editor__id">{id_display}</code>
                </div>
                <button
                    type="button"
                    class="btn btn--ghost"
                    aria-label="Close editor"
                    on:click=close
                >
                    "Close"
                </button>
            </header>
            <TextField
                label="Name".into()
                initial=initial_name
                placeholder="(auto)".into()
                optional=true
                write=name_write
            />
            {kind_fields}
        </section>
    }
}

/// The select value of an `if`'s missing-value choice; absent reads as `fail`.
fn missing_str(missing: Option<Missing>) -> &'static str {
    match missing {
        None | Some(Missing::Fail) => "fail",
        Some(Missing::True) => "true",
        Some(Missing::False) => "false",
    }
}

/// The choice behind a select value; `fail` is the default so it is stored as absent.
fn missing_from(value: &str) -> Option<Missing> {
    match value {
        "true" => Some(Missing::True),
        "false" => Some(Missing::False),
        _ => None,
    }
}

/// The `flow.enable` editor: which flow it switches ("This flow" or another by name) and
/// whether it enables or disables it.
fn flow_enable_fields(
    draft: RwSignal<Flow>,
    id: String,
    p: oikos_core::flow::FlowEnableParams,
) -> impl IntoView {
    let house = expect_context::<House>();
    let own_id = draft.with_untracked(|f| f.id.clone());
    let others: Vec<(String, String)> = house.flows.with_untracked(|flows| {
        flows
            .iter()
            .filter(|f| f.definition.id != own_id)
            .map(|f| (f.definition.id.clone(), f.definition.name.clone()))
            .collect()
    });
    let target = p.flow.clone();
    let known = target
        .as_ref()
        .is_none_or(|t| others.iter().any(|(id, _)| id == t));
    let target_id = id.clone();
    let on_target = move |ev: web_sys::Event| {
        let value = event_target_value(&ev);
        update_node(draft, &target_id, |n| {
            if let NodeKind::FlowEnable(ref mut f) = n.kind {
                f.flow = (!value.is_empty()).then_some(value);
            }
        });
    };
    let state_id = id;
    let on_state = move |ev: web_sys::Event| {
        let enabled = event_target_value(&ev) == "enable";
        update_node(draft, &state_id, |n| {
            if let NodeKind::FlowEnable(ref mut f) = n.kind {
                f.enabled = enabled;
            }
        });
    };
    let enabled = p.enabled;
    view! {
        <label class="node-editor__field">
            <span class="node-editor__label">"Flow"</span>
            <select class="field" on:change=on_target>
                <option value="" selected=target.is_none()>"This flow"</option>
                {(!known)
                    .then(|| {
                        let gone = target.clone().unwrap_or_default();
                        view! { <option value=gone.clone() selected=true>{gone.clone()}</option> }
                    })}
                {others
                    .into_iter()
                    .map(|(id, name)| {
                        let selected = target.as_deref() == Some(id.as_str());
                        view! { <option value=id selected=selected>{name}</option> }
                    })
                    .collect_view()}
            </select>
        </label>
        <label class="node-editor__field">
            <span class="node-editor__label">"Action"</span>
            <select class="field" on:change=on_state>
                <option value="enable" selected=enabled>"Enable"</option>
                <option value="disable" selected=!enabled>"Disable"</option>
            </select>
        </label>
    }
}

fn kind_fields(draft: RwSignal<Flow>, id: String, kind: NodeKind) -> impl IntoView {
    match kind {
        NodeKind::TriggerState(p) => {
            let entity_id = id.clone();
            let entity_write = Callback::new(move |value: Option<String>| {
                update_node(draft, &entity_id, |n| {
                    if let NodeKind::TriggerState(ref mut t) = n.kind {
                        t.entity = value.unwrap_or_default();
                    }
                });
            });
            let to_id = id.clone();
            let to_write = Callback::new(move |value: Option<serde_json::Value>| {
                update_node(draft, &to_id, |n| {
                    if let NodeKind::TriggerState(ref mut t) = n.kind {
                        t.to = value;
                    }
                });
            });
            let from_id = id;
            let from_write = Callback::new(move |value: Option<serde_json::Value>| {
                update_node(draft, &from_id, |n| {
                    if let NodeKind::TriggerState(ref mut t) = n.kind {
                        t.from = value;
                    }
                });
            });
            view! {
                <TextField
                    label="Entity".into()
                    initial=p.entity
                    placeholder="entity.id".into()
                    optional=false
                    write=entity_write
                />
                <JsonField
                    label="Fires when it becomes (JSON, optional)".into()
                    initial=p.to.unwrap_or(serde_json::Value::Null)
                    optional=true
                    write=to_write
                />
                <JsonField
                    label="Only after (previous, JSON, optional)".into()
                    initial=p.from.unwrap_or(serde_json::Value::Null)
                    optional=true
                    write=from_write
                />
            }
            .into_any()
        }
        NodeKind::If(p) => {
            let op_id = id.clone();
            let on_op = move |ev: web_sys::Event| {
                if let Some(op) = op_from(&event_target_value(&ev)) {
                    update_node(draft, &op_id, |n| {
                        if let NodeKind::If(ref mut i) = n.kind {
                            i.op = op;
                        }
                    });
                }
            };
            let current_op = p.op;
            let value_id = id.clone();
            let value_write = Callback::new(move |value: Option<serde_json::Value>| {
                if let Some(v) = value {
                    update_node(draft, &value_id, |n| {
                        if let NodeKind::If(ref mut i) = n.kind {
                            i.value = v;
                        }
                    });
                }
            });
            let missing_id = id.clone();
            let current_missing = p.missing;
            let on_missing = move |ev: web_sys::Event| {
                let missing = missing_from(&event_target_value(&ev));
                update_node(draft, &missing_id, |n| {
                    if let NodeKind::If(ref mut i) = n.kind {
                        i.missing = missing;
                    }
                });
            };
            let entity_id = id;
            let entity_write = Callback::new(move |value: Option<String>| {
                update_node(draft, &entity_id, |n| {
                    if let NodeKind::If(ref mut i) = n.kind {
                        i.entity = value;
                    }
                });
            });
            view! {
                <label class="node-editor__field">
                    <span class="node-editor__label">"Compare"</span>
                    <select class="field" on:change=on_op>
                        {["eq", "ne", "gt", "lt", "gte", "lte"]
                            .into_iter()
                            .map(|o| {
                                let selected = op_str(current_op) == o;
                                view! {
                                    <option value=o selected=selected>{o}</option>
                                }
                            })
                            .collect_view()}
                    </select>
                </label>
                <JsonField
                    label="Value (JSON)".into()
                    initial=p.value
                    optional=false
                    write=value_write
                />
                <TextField
                    label="Entity (defaults to trigger value)".into()
                    initial=p.entity.unwrap_or_default()
                    placeholder="entity.id".into()
                    optional=true
                    write=entity_write
                />
                <label class="node-editor__field">
                    <span class="node-editor__label">"When the value is unknown"</span>
                    <select class="field" on:change=on_missing>
                        {[
                            ("fail", "Fail"),
                            ("true", "Take Yes"),
                            ("false", "Take No"),
                        ]
                            .into_iter()
                            .map(|(value, label)| {
                                let selected = missing_str(current_missing) == value;
                                view! {
                                    <option value=value selected=selected>{label}</option>
                                }
                            })
                            .collect_view()}
                    </select>
                </label>
            }
            .into_any()
        }
        NodeKind::TriggerSchedule(p) => {
            view! { <ScheduleField draft=draft id=id initial=p.cron /> }.into_any()
        }
        NodeKind::FlowEnable(p) => flow_enable_fields(draft, id, p).into_any(),
        NodeKind::Delay(p) => {
            let for_id = id;
            let for_write = Callback::new(move |value: Option<String>| {
                update_node(draft, &for_id, |n| {
                    if let NodeKind::Delay(ref mut d) = n.kind {
                        d.for_ = value.unwrap_or_default();
                    }
                });
            });
            view! {
                <TextField
                    label="Duration".into()
                    initial=p.for_
                    placeholder="5m, 1h30m, 45s".into()
                    optional=false
                    write=for_write
                />
            }
            .into_any()
        }
        NodeKind::Command(p) => {
            let entity_id = id.clone();
            let entity_write = Callback::new(move |value: Option<String>| {
                update_node(draft, &entity_id, |n| {
                    if let NodeKind::Command(ref mut c) = n.kind {
                        c.entity = value.unwrap_or_default();
                    }
                });
            });
            let cmd_id = id;
            let command_json = serde_json::to_value(&p.command).unwrap_or(serde_json::Value::Null);
            let cmd_write = Callback::new(move |value: Option<serde_json::Value>| {
                if let Some(v) = value
                    && let Ok(cmd) = serde_json::from_value(v)
                {
                    update_node(draft, &cmd_id, |n| {
                        if let NodeKind::Command(ref mut c) = n.kind {
                            c.command = cmd;
                        }
                    });
                }
            });
            view! {
                <TextField
                    label="Entity".into()
                    initial=p.entity
                    placeholder="entity.id".into()
                    optional=false
                    write=entity_write
                />
                <JsonField
                    label=r#"Command (JSON, e.g. {"type":"turn_on"})"#.into()
                    initial=command_json
                    optional=false
                    write=cmd_write
                />
            }
            .into_any()
        }
    }
}

#[component]
fn FlowBody(view: FlowView) -> impl IntoView {
    let runs = view.runs.clone();
    let flow = view.definition.clone();
    let editing = RwSignal::new(false);
    let saving = RwSignal::new(false);
    let selected = RwSignal::new(None::<String>);
    provide_context(SaveBlocked(RwSignal::new(false)));
    let draft = RwSignal::new(flow.clone());
    let zoom = RwSignal::new(1.0_f64);
    // Leaving edit mode drops any selection.
    Effect::new(move |_| {
        if !editing.get() {
            selected.set(None);
        }
    });
    let body = match view.invalid.clone() {
        Some(reason) => view! {
            <div class="flow-invalid">
                <strong>"This flow cannot run"</strong>
                <p>{reason}</p>
            </div>
        }
        .into_any(),
        None => view! {
            <Show
                when=move || editing.get()
                fallback={
                    let view = view.clone();
                    move || view! { <Canvas view=view.clone() zoom=zoom /> }
                }
            >
                <EditCanvas draft=draft selected=selected zoom=zoom />
                <NodeEditor draft=draft selected=selected />
            </Show>
        }
        .into_any(),
    };
    let has_runs = !runs.is_empty();
    view! {
        <Header view=view editing=editing draft=draft saving=saving />
        <section class="flow-canvas-runs" aria-label="Recent runs">
            <h2 class="flow-canvas-runs__title">"Recent runs"</h2>
            {if has_runs {
                view! {
                    <ol class="run-list">
                        {runs
                            .into_iter()
                            .map(|run| view! { <RunItem run=run flow=flow.clone() /> })
                            .collect_view()}
                    </ol>
                }
                .into_any()
            } else {
                view! { <p class="flow-canvas-runs__empty">"No runs yet."</p> }.into_any()
            }}
        </section>
        <div class="flow-canvas-stage">
            {body}
            <ZoomLoupe zoom=zoom />
        </div>
    }
}

#[component]
pub fn FlowCanvasScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let params = use_params_map();
    let id = Memo::new(move |_| params.read().get("id").unwrap_or_default());
    let current = Memo::new(move |_| {
        let id = id.get();
        house
            .flows
            .with(|flows| flows.iter().find(|f| f.definition.id == id).cloned())
    });
    let loaded = Memo::new(move |_| {
        house.flows.with(|f| !f.is_empty())
            || matches!(house.connection.get(), oikos_api::ConnectionStatus::Online)
    });
    view! {
        <div class="flow-canvas-screen">
            {move || match current.get() {
                Some(flow) => view! { <FlowBody view=flow /> }.into_any(),
                None if loaded.get() => {
                    view! {
                        <div class="flow-invalid">
                            <strong>"Unknown flow"</strong>
                            <p>
                                <A href="/flows">"Back to flows"</A>
                            </p>
                        </div>
                    }
                        .into_any()
                }
                None => ().into_any(),
            }}
        </div>
    }
}
