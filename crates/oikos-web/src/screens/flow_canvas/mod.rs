//! One flow on a full-bleed canvas, React Flow style: pills and routed wires on a pannable,
//! zoomable surface, floating toolbars, zoom controls, a minimap and a runs drawer. A running
//! run lights its path live; clicking a past run replays it. Edit mode works on a local draft:
//! select, drag, connect, cut, delete, duplicate, tidy and undo, then Save sends it with
//! `PUT /api/v1/flows/{id}`.

mod drawer;
mod graph;
mod picker;
mod settings;
mod toolbar;
mod viewport;

use std::collections::BTreeMap;

use leptos::ev;
use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use oikos_api::{FlowView, RunView};
use oikos_core::flow::{Flow, FlowMode};
use wasm_bindgen::JsCast;

pub use drawer::Drawer;
pub use graph::{Graph, Mode, Selection};
pub use viewport::{Camera, CanvasControls, Minimap, Viewport};

use crate::flow_edit::{History, duplicate_node, remove_edge, remove_node};
use crate::flow_layout::{bounds, positions};
use crate::flows::{highlight, parked_deadline, parked_node, step_offset};
use crate::house::House;
use crate::screens::flow_schedule::SaveBlocked;
use settings::NodeEditor;
use toolbar::Toolbar;

/// Records the draft as it is now in `history`, then applies `f` to it: one undo step. Call it
/// once per user gesture (a drag records itself once, when it starts moving).
pub fn edit<R>(
    draft: RwSignal<Flow>,
    history: StoredValue<History>,
    f: impl FnOnce(&mut Flow) -> R,
) -> R {
    history.update_value(|h| h.record(draft.get_untracked()));
    let mut out = None;
    draft.update(|flow| out = Some(f(flow)));
    out.expect("draft.update runs its closure")
}

/// The flow being edited and its undo history, provided while the canvas is mounted.
#[derive(Clone, Copy)]
pub struct Editor {
    pub draft: RwSignal<Flow>,
    pub history: StoredValue<History>,
    /// Bumped when undo or redo replace the draft, so the settings form rebuilds from it.
    pub restored: RwSignal<u32>,
}

impl Editor {
    fn new(flow: Flow) -> Self {
        Self {
            draft: RwSignal::new(flow),
            history: StoredValue::new(History::new()),
            restored: RwSignal::new(0),
        }
    }

    /// [`edit`] on this draft.
    pub fn edit<R>(self, f: impl FnOnce(&mut Flow) -> R) -> R {
        edit(self.draft, self.history, f)
    }

    /// Replaces the draft with `flow` and forgets the history (entering edit mode, Save, Cancel).
    pub fn reset(self, flow: Flow) {
        self.history.update_value(History::clear);
        self.draft.set(flow);
    }

    /// Whether there is a step to undo; tracks the draft, which every step changes.
    pub fn can_undo(self) -> bool {
        self.draft.track();
        self.history.with_value(History::can_undo)
    }

    /// Steps back (or forward again with `redo`); whether anything changed.
    pub fn step(self, redo: bool) -> bool {
        let current = self.draft.get_untracked();
        let mut restored = None;
        self.history.update_value(|h| {
            restored = if redo {
                h.redo(current)
            } else {
                h.undo(current)
            };
        });
        let Some(flow) = restored else {
            return false;
        };
        self.draft.set(flow);
        self.restored.update(|n| *n += 1);
        true
    }
}

/// Whether a key event comes from a form field (typing there must not delete a card).
fn typing(ev: &web_sys::KeyboardEvent) -> bool {
    let Some(el) = ev
        .target()
        .and_then(|t| t.dyn_into::<web_sys::HtmlElement>().ok())
    else {
        return false;
    };
    matches!(el.tag_name().as_str(), "INPUT" | "TEXTAREA" | "SELECT") || el.is_content_editable()
}

/// Each node a run visited, with the time of its latest step since the run started.
fn step_times(run: &RunView) -> BTreeMap<String, String> {
    run.steps
        .iter()
        .map(|s| (s.node.clone(), step_offset(s.at, run.started_at)))
        .collect()
}

#[component]
fn FlowBody(
    view: Signal<FlowView>,
    /// Starts in edit mode (a flow that was just created).
    #[prop(optional)]
    start_editing: bool,
) -> impl IntoView {
    let flow = Memo::new(move |_| view.with(|v| v.definition.clone()));
    let runs = Memo::new(move |_| view.with(|v| v.runs.clone()));
    let invalid = Memo::new(move |_| view.with(|v| v.invalid.clone()));
    let editing = RwSignal::new(start_editing);
    let saving = RwSignal::new(false);
    let selection = RwSignal::new(None::<Selection>);
    let drawer = RwSignal::new(false);
    let replay = RwSignal::new(None::<String>);
    provide_context(SaveBlocked(RwSignal::new(false)));
    let editor = Editor::new(flow.get_untracked());
    let draft = editor.draft;
    provide_context(editor);
    let camera = Camera::provide();

    // Entering or leaving edit mode drops the selection.
    Effect::watch(
        move || editing.get(),
        move |_, _, _| selection.set(None),
        false,
    );
    // Selecting a card while editing opens its settings in the drawer.
    Effect::new(move |_| {
        let card = matches!(selection.get(), Some(Selection::Node(_)));
        if card && editing.get_untracked() {
            drawer.set(true);
        }
    });
    // Closing the drawer stops a replay (nothing would show why the canvas is dimmed) and,
    // while editing, deselects the card whose settings it showed.
    Effect::new(move |_| {
        if !drawer.get() {
            replay.set(None);
            if editing.get_untracked()
                && matches!(selection.get_untracked(), Some(Selection::Node(_)))
            {
                selection.set(None);
            }
        }
    });
    // A selection the draft no longer has (undone, deleted) is dropped.
    Effect::new(move |_| {
        let stale = draft.with(|f| match selection.get_untracked() {
            Some(Selection::Node(id)) => f.node(&id).is_none(),
            Some(Selection::Edge((from, port, to))) => !f
                .edges
                .iter()
                .any(|e| e.from == from && e.port == port && e.to == to),
            None => false,
        });
        if stale && editing.get_untracked() {
            selection.set(None);
        }
    });

    let delete_selected = move || {
        match selection.get_untracked() {
            Some(Selection::Node(id)) => editor.edit(|f| remove_node(f, &id)),
            Some(Selection::Edge(key)) => editor.edit(|f| remove_edge(f, &key)),
            None => return,
        };
        selection.set(None);
    };
    // Esc stops a replay and drops the selection; while editing, ⌫/Delete removes the
    // selection, Mod-D duplicates the selected card, Mod-Z undoes and Shift-Mod-Z redoes.
    let keys = window_event_listener(ev::keydown, move |ev| {
        if typing(&ev) {
            return;
        }
        let key = ev.key();
        if key == "Escape" {
            replay.set(None);
            selection.set(None);
            return;
        }
        if !editing.get_untracked() {
            return;
        }
        let modifier = ev.meta_key() || ev.ctrl_key();
        match (key.as_str(), modifier) {
            ("Backspace" | "Delete", false) => {
                ev.prevent_default();
                delete_selected();
            }
            ("d" | "D", true) => {
                ev.prevent_default();
                if let Some(Selection::Node(id)) = selection.get_untracked()
                    && draft.with_untracked(|f| f.node(&id).is_some())
                    && let Some(copy) = editor.edit(|f| duplicate_node(f, &id))
                {
                    selection.set(Some(Selection::Node(copy)));
                }
            }
            ("z" | "Z", true) => {
                ev.prevent_default();
                editor.step(ev.shift_key());
            }
            _ => {}
        }
    });
    on_cleanup(move || keys.remove());

    // A tap on the canvas: in view mode a card only gets the selection ring; on empty canvas
    // (edit-mode cards and wires take their own clicks) it clears the selection.
    let on_tap = Callback::new(move |target: Option<web_sys::Element>| {
        let card = target
            .and_then(|t| t.closest("[data-node]").ok().flatten())
            .and_then(|card| card.get_attribute("data-node"));
        selection.set(card.map(Selection::Node));
    });

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
            <Viewport view=camera.view content=content.into() on_tap=on_tap>
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
            <Toolbar view=view editing=editing editor=editor saving=saving drawer=drawer selection=selection />
            <Drawer
                open=drawer
                runs=runs.into()
                flow=flow.into()
                replay=replay
                settings=Signal::derive(move || {
                    editing.get() && matches!(selection.get(), Some(Selection::Node(_)))
                })
            >
                <Show when=move || editing.get()>
                    <NodeEditor editor=editor selected=selection />
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

/// A flow that does not exist yet, opened in edit mode. Its empty id marks it unsaved: the first
/// Save derives the id from the name, and success moves to the flow's own page.
#[component]
pub fn NewFlowScreen() -> impl IntoView {
    let blank = FlowView {
        definition: Flow {
            id: String::new(),
            name: "New flow".into(),
            enabled: false,
            mode: FlowMode::Single,
            ..Default::default()
        },
        invalid: None,
        active_run: None,
        runs: Vec::new(),
    };
    let top = RwSignal::new(0.0_f64);
    Effect::new(move |_| top.set(top_offset()));
    let resize = window_event_listener(ev::resize, move |_| top.set(top_offset()));
    on_cleanup(move || resize.remove());
    view! {
        <div class="flow-canvas-screen" style=move || format!("--flow-top:{}px", top.get())>
            <FlowBody view=Signal::stored(blank) start_editing=true />
        </div>
    }
}
