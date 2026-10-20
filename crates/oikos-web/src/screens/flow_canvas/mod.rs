//! One flow on a full-bleed canvas, React Flow style: pills and routed wires on a pannable,
//! zoomable surface, floating toolbars, zoom controls, a minimap and a runs drawer. A running
//! run lights its path live; clicking a past run replays it. Edit mode drags cards and edits
//! their settings, saved with `PUT /api/v1/flows/{id}`.

mod drawer;
mod graph;
mod settings;
mod toolbar;
mod viewport;

use std::collections::BTreeMap;

use leptos::ev;
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use oikos_api::{FlowView, RunView};
use oikos_core::flow::Flow;

pub use drawer::Drawer;
pub use graph::{Graph, Mode, Selection};
pub use viewport::{Camera, CanvasControls, Minimap, Viewport};

use crate::flow_layout::{bounds, positions};
use crate::flows::{highlight, parked_deadline, parked_node, step_offset};
use crate::house::House;
use crate::screens::flow_schedule::SaveBlocked;
use settings::NodeEditor;
use toolbar::Toolbar;

/// The flow being edited, provided while the canvas is mounted; edit-mode cards write their
/// new positions into it.
#[derive(Clone, Copy)]
pub struct Draft(pub RwSignal<Flow>);

/// Each node a run visited, with the time of its latest step since the run started.
fn step_times(run: &RunView) -> BTreeMap<String, String> {
    run.steps
        .iter()
        .map(|s| (s.node.clone(), step_offset(s.at, run.started_at)))
        .collect()
}

#[component]
fn FlowBody(view: Signal<FlowView>) -> impl IntoView {
    let flow = Memo::new(move |_| view.with(|v| v.definition.clone()));
    let runs = Memo::new(move |_| view.with(|v| v.runs.clone()));
    let invalid = Memo::new(move |_| view.with(|v| v.invalid.clone()));
    let editing = RwSignal::new(false);
    let saving = RwSignal::new(false);
    let selection = RwSignal::new(None::<Selection>);
    let drawer = RwSignal::new(false);
    let replay = RwSignal::new(None::<String>);
    provide_context(SaveBlocked(RwSignal::new(false)));
    let draft = RwSignal::new(flow.get_untracked());
    provide_context(Draft(draft));
    let camera = Camera::provide();

    // Leaving edit mode drops any selection; selecting a card while editing opens its
    // settings in the drawer.
    Effect::new(move |_| {
        if !editing.get() {
            selection.set(None);
        }
    });
    Effect::new(move |_| {
        let card = matches!(selection.get(), Some(Selection::Node(_)));
        if card && editing.get_untracked() {
            drawer.set(true);
        }
    });
    // Esc stops a replay.
    let keys = window_event_listener(ev::keydown, move |ev| {
        if ev.key() == "Escape" {
            replay.set(None);
        }
    });
    on_cleanup(move || keys.remove());

    let shown = Memo::new(move |_| {
        if editing.get() {
            draft.get()
        } else {
            flow.get()
        }
    });
    let content = Memo::new(move |_| shown.with(|f| bounds(&positions(f))));
    let find = move |id: Option<String>| {
        id.and_then(|id| runs.with(|r| r.iter().find(|r| r.id == id).cloned()))
    };
    let replayed = Memo::new(move |_| find(replay.get()));
    let active = Memo::new(move |_| find(view.with(|v| v.active_run.clone())));
    let lit = Memo::new(move |_| {
        shown.with(|f| match replayed.get() {
            Some(run) => highlight(f, Some(&run)),
            None => highlight(f, active.get().as_ref()),
        })
    });
    let dim = Memo::new(move |_| replayed.with(Option::is_some));
    let live = Memo::new(move |_| replayed.with(Option::is_none) && active.with(Option::is_some));
    let steps =
        Memo::new(move |_| replayed.with(|r| r.as_ref().map(step_times).unwrap_or_default()));
    let parked = Memo::new(move |_| {
        if replayed.with(Option::is_some) {
            return None;
        }
        let run = active.get()?;
        shown.with(|f| {
            let (node, since) = parked_node(f, &run)?;
            let deadline = parked_deadline(f, &node, since)?;
            Some((node, deadline))
        })
    });

    let canvas = move || match invalid.get() {
        Some(reason) => view! {
            <div class="flow-invalid">
                <strong>"This flow cannot run"</strong>
                <p>{reason}</p>
            </div>
        }
        .into_any(),
        None => view! {
            <Viewport view=camera.view content=content.into()>
                {move || {
                    let mode = if editing.get() { Mode::Edit } else { Mode::View };
                    view! {
                        <Graph
                            flow=shown.into()
                            mode=mode
                            lit=lit.into()
                            dim=dim.into()
                            steps=steps.into()
                            parked=parked.into()
                            live=live.into()
                            selection=selection
                        />
                    }
                }}
            </Viewport>
            <CanvasControls view=camera.view content=content.into() />
            <Minimap view=camera.view flow=shown.into() />
        }
        .into_any(),
    };
    view! {
        <div class="flow-stage" class:flow-stage--drawer=move || drawer.get()>
            {canvas}
            <Toolbar view=view editing=editing draft=draft saving=saving drawer=drawer />
            <Drawer open=drawer runs=runs.into() flow=flow.into() replay=replay>
                <Show when=move || editing.get()>
                    <NodeEditor draft=draft selected=selection />
                </Show>
            </Drawer>
        </div>
    }
}

/// Measures where the canvas starts: just under the sticky top bar.
fn top_offset() -> f64 {
    document()
        .query_selector(".topbar")
        .ok()
        .flatten()
        .map_or(0.0, |bar| bar.get_bounding_client_rect().bottom() + 8.0)
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
    let found = Memo::new(move |_| current.with(Option::is_some));
    let loaded = Memo::new(move |_| {
        house.flows.with(|f| !f.is_empty())
            || matches!(house.connection.get(), oikos_api::ConnectionStatus::Online)
    });
    let top = RwSignal::new(0.0_f64);
    Effect::new(move |_| top.set(top_offset()));
    let resize = window_event_listener(ev::resize, move |_| top.set(top_offset()));
    on_cleanup(move || resize.remove());
    view! {
        <div class="flow-canvas-screen" style=move || format!("--flow-top:{}px", top.get())>
            {move || {
                // A new id mounts a fresh body (and fits its flow); live updates of the same
                // flow flow through `view` without remounting.
                id.track();
                if found.get() {
                    let Some(initial) = current.get_untracked() else {
                        return ().into_any();
                    };
                    let view = Signal::derive(move || current.get().unwrap_or_else(|| initial.clone()));
                    return view! { <FlowBody view=view /> }.into_any();
                }
                if loaded.get() {
                    return view! {
                        <div class="flow-invalid">
                            <strong>"Unknown flow"</strong>
                            <p>
                                <A href="/flows">"Back to flows"</A>
                            </p>
                        </div>
                    }
                    .into_any();
                }
                ().into_any()
            }}
        </div>
    }
}
