//! One flow drawn read-only like n8n's canvas, with the last run lit, and its run history.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use oikos_api::{FlowView, RunView, StepOutcome};
use oikos_core::flow::{Flow, FlowMode, NodeKind};

use crate::alerts::relative_time;
use crate::api;
use crate::components::flow_switch::FlowSwitch;
use crate::components::now::Now;
use crate::components::toast::Toasts;
use crate::flows::{
    NODE_H, NODE_W, canvas_size, highlight, layout, node_detail, node_title, orphans,
    parked_deadline, parked_node, port_offset, status_class, status_word, trigger_line, wire_path,
};
use crate::format::format_timestamp;
use crate::house::House;
use crate::icons::node_icon;

fn type_class(kind: &NodeKind) -> &'static str {
    match kind {
        NodeKind::TriggerState(_) => "flow-node--trigger",
        NodeKind::If(_) => "flow-node--if",
        NodeKind::Delay(_) => "flow-node--delay",
        NodeKind::Command(_) => "flow-node--command",
    }
}

/// `m:ss` of a number of seconds.
fn minutes_seconds(secs: u64) -> String {
    format!("{}:{:02}", secs / 60, secs % 60)
}

#[component]
fn Header(view: FlowView) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let flow = &view.definition;
    let id = flow.id.clone();
    let invalid = view.invalid.is_some();
    let busy = view.active_run.is_some();
    let mode = match flow.mode {
        FlowMode::Single => "single",
        FlowMode::Restart => "restart",
    };
    let run_now = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            spawn_local(async move {
                if let Err(e) = api::run_flow(&id).await {
                    toasts.push(e.error);
                }
            });
        }
    };
    view! {
        <header class="flow-canvas__header">
            <A href="/flows" attr:class="sheet__back">
                "←"
                <span class="sheet__back-label">"Flows"</span>
            </A>
            <h1 class="sheet__title flow-canvas__title">{flow.name.clone()}</h1>
            <FlowSwitch id=id name=flow.name.clone() enabled=flow.enabled invalid=invalid />
            <button type="button" class="btn" disabled=invalid || busy on:click=run_now>
                "Run now"
            </button>
            <span class="chip">{mode}</span>
        </header>
    }
}

#[component]
fn Canvas(view: FlowView) -> impl IntoView {
    let house = expect_context::<House>();
    let now = expect_context::<Now>();
    let flow = view.definition.clone();
    let placed = layout(&flow);
    let (width, height) = canvas_size(&placed);
    let lit = highlight(&flow, view.runs.first());
    let orphaned = orphans(&flow);
    let parked = view
        .runs
        .first()
        .filter(|r| view.active_run.as_deref() == Some(r.id.as_str()))
        .and_then(|r| parked_node(&flow, r));
    let pos = |id: &str| placed.iter().find(|p| p.id == id).map(|p| (p.x, p.y));

    let wires = flow
        .edges
        .iter()
        .filter_map(|edge| {
            let (x1, y1) = pos(&edge.from)?;
            let (x2, y2) = pos(&edge.to)?;
            let from = flow.node(&edge.from)?;
            let d = wire_path(
                x1 + NODE_W,
                y1 + port_offset(&from.kind, edge.port.as_deref()),
                x2,
                y2 + NODE_H / 2.0,
            );
            let is_lit =
                lit.edges
                    .contains(&(edge.from.clone(), edge.port.clone(), edge.to.clone()));
            Some(view! { <path class="flow-wire" class:flow-wire--lit=is_lit d=d /> })
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
                        let top = port_offset(&node.kind, Some(port));
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
                let top = port_offset(&node.kind, None);
                view! { <span class="flow-node__out" style=format!("top:{top}px")></span> }
            });
            let in_dot = (!is_trigger).then(|| {
                let top = port_offset(&node.kind, None);
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

    view! {
        <div class="flow-canvas-scroll">
            <div class="flow-canvas" style=format!("width:{width}px;height:{height}px")>
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
            let line = if s.detail.is_empty() {
                format!("{node} · {outcome}")
            } else {
                format!("{node} · {outcome} · {}", s.detail)
            };
            view! { <li>{line}</li> }
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
                    <span class="run__trigger">{trigger}</span>
                </summary>
                <ol class="run__steps">{steps}</ol>
            </details>
        </li>
    }
}

#[component]
fn FlowBody(view: FlowView) -> impl IntoView {
    let runs = view.runs.clone();
    let flow = view.definition.clone();
    let body = match view.invalid.clone() {
        Some(reason) => view! {
            <div class="flow-invalid">
                <strong>"This flow cannot run"</strong>
                <p>{reason}</p>
            </div>
        }
        .into_any(),
        None => view! { <Canvas view=view.clone() /> }.into_any(),
    };
    view! {
        <Header view=view />
        {body}
        <ol class="run-list">
            {runs
                .into_iter()
                .map(|run| view! { <RunItem run=run flow=flow.clone() /> })
                .collect_view()}
        </ol>
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
