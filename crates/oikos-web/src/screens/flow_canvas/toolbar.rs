//! The floating bars over the canvas: top-left the back link, the flow's name (a text field
//! while editing) and its mode;
//! top-right the enable switch, Run, Edit and Runs (＋ Add, Tidy, Undo, Cancel and Save while
//! editing).

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::NavigateOptions;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;
use oikos_api::FlowView;
use oikos_core::flow::{Flow, FlowMode, NodeKind};

use super::picker::Picker;
use super::{Camera, Editor, Selection};
use crate::api;
use crate::components::flow_switch::FlowSwitch;
use crate::components::toast::Toasts;
use crate::flow_edit::{add_node, slug_id};
use crate::flow_layout::{NODE_H, NODE_W, Pt, tidy, with_resolved_positions};
use crate::flow_viewport::visible_world;
use crate::house::House;
use crate::icons::{CHECK, PENCIL, PLAY, X_MARK};
use crate::screens::flow_schedule::SaveBlocked;

#[component]
fn EditControls(
    editing: RwSignal<bool>,
    editor: Editor,
    saving: RwSignal<bool>,
    original: Signal<Flow>,
    selection: RwSignal<Option<Selection>>,
) -> impl IntoView {
    let camera = expect_context::<Camera>();
    let picker = RwSignal::new(false);
    // A picked card lands at the middle of what is on screen, and is selected (which opens its
    // settings).
    let add = Callback::new(move |kind: NodeKind| {
        let (w, h) = camera.size.get_untracked();
        let seen = visible_world(&camera.view.get_untracked(), w, h);
        let at = Pt {
            x: seen.x + (seen.w - NODE_W) / 2.0,
            y: seen.y + (seen.h - NODE_H) / 2.0,
        };
        let id = editor.edit(|f| add_node(f, kind, at));
        selection.set(Some(Selection::Node(id)));
    });
    let toasts = expect_context::<Toasts>();
    let blocked = use_context::<SaveBlocked>();
    let is_blocked = move || blocked.is_some_and(|b| b.0.get());
    let house = expect_context::<House>();
    let navigate = use_navigate();
    let go_back = navigate.clone();
    // An empty id marks a flow that was never saved: there is nothing to go back to.
    let cancel = move |_| {
        if let Some(b) = blocked {
            b.0.set(false);
        }
        let original = original.get_untracked();
        if original.id.is_empty() {
            go_back("/flows", NavigateOptions::default());
            return;
        }
        editor.reset(original);
        editing.set(false);
    };
    let save = move |_| {
        if saving.get_untracked() || blocked.is_some_and(|b| b.0.get_untracked()) {
            return;
        }
        saving.set(true);
        let mut snapshot = editor.draft.get_untracked();
        let is_new = snapshot.id.is_empty();
        if is_new {
            let taken = house
                .flows
                .with_untracked(|flows| flows.iter().map(|f| f.definition.id.clone()).collect());
            snapshot.id = slug_id(&snapshot.name, &taken);
        }
        let navigate = navigate.clone();
        spawn_local(async move {
            match api::put_flow(&snapshot).await {
                // Saved: back to the view, which the hub's echo updates. A new flow moves to
                // its own address, and the screen there finds it in the house.
                Ok((view, warning)) => {
                    if is_new {
                        let saved = view.clone();
                        house.flows.update(|flows| {
                            if !flows.iter().any(|f| f.definition.id == saved.definition.id) {
                                flows.push(saved);
                            }
                        });
                        navigate(
                            &format!("/flows/{}", api::encode_id(&view.definition.id)),
                            NavigateOptions {
                                replace: true,
                                ..Default::default()
                            },
                        );
                    } else {
                        editor.reset(view.definition);
                        editing.set(false);
                    }
                    if let Some(warning) = warning {
                        toasts.push(warning);
                    }
                    house.refresh_flows_config();
                }
                Err(e) => toasts.push(e.error),
            }
            saving.set(false);
        });
    };
    view! {
        <div class="flow-picker-wrap">
            <button
                type="button"
                class="btn btn--ghost"
                aria-label="Add card"
                title="Add card"
                aria-expanded=move || picker.get().to_string()
                on:click=move |_| picker.update(|open| *open = !*open)
            >
                "＋ Add"
            </button>
            <Picker open=picker on_pick=add />
        </div>
        <button
            type="button"
            class="btn btn--ghost"
            aria-label="Tidy"
            title="Arrange the cards automatically"
            on:click=move |_| editor.edit(|f| *f = tidy(f))
        >
            "Tidy"
        </button>
        <button
            type="button"
            class="btn btn--ghost btn--icon flow-toolbar__undo"
            aria-label="Undo"
            title="Undo"
            disabled=move || !editor.can_undo()
            on:click=move |_| {
                editor.step(false);
            }
        >
            "↶"
        </button>
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
fn ReadControls(view: Signal<FlowView>, editing: RwSignal<bool>, editor: Editor) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let invalid = Memo::new(move |_| view.with(|v| v.invalid.is_some()));
    let busy = Memo::new(move |_| view.with(|v| v.active_run.is_some()));
    // The switch takes plain values: it is rebuilt when one of them changes.
    let switch = Memo::new(move |_| {
        view.with(|v| {
            (
                v.definition.id.clone(),
                v.definition.name.clone(),
                v.definition.enabled,
                v.invalid.is_some(),
            )
        })
    });
    let run_now = move |_| {
        let id = view.with_untracked(|v| v.definition.id.clone());
        spawn_local(async move {
            if let Err(e) = api::run_flow(&id).await {
                toasts.push(e.error);
            }
        });
    };
    let enter_edit = move |_| {
        editor.reset(view.with_untracked(|v| with_resolved_positions(&v.definition)));
        editing.set(true);
    };
    view! {
        {move || {
            let (id, name, enabled, invalid) = switch.get();
            view! { <FlowSwitch id=id name=name enabled=enabled invalid=invalid /> }
        }}
        <button
            type="button"
            class="btn btn--icon"
            aria-label="Run now"
            title="Run now"
            disabled=move || invalid.get() || busy.get()
            on:click=run_now
            inner_html=PLAY
        ></button>
        <button
            type="button"
            class="btn btn--ghost btn--icon flow-toolbar__edit"
            aria-label="Edit layout"
            title="Edit layout"
            disabled=move || invalid.get()
            on:click=enter_edit
            inner_html=PENCIL
        ></button>
    }
}

/// Both floating bars. `drawer` is the runs drawer the Runs button toggles.
#[component]
pub fn Toolbar(
    view: Signal<FlowView>,
    editing: RwSignal<bool>,
    editor: Editor,
    saving: RwSignal<bool>,
    drawer: RwSignal<bool>,
    selection: RwSignal<Option<Selection>>,
) -> impl IntoView {
    let name = move || view.with(|v| v.definition.name.clone());
    // The first keystroke after focusing the field records one undo step; the rest join it.
    let armed = StoredValue::new(false);
    let rename = move |ev: web_sys::Event| {
        let text = event_target_value(&ev);
        if armed.get_value() {
            armed.set_value(false);
            editor
                .history
                .update_value(|h| h.record(editor.draft.get_untracked()));
        }
        editor.draft.update(|f| f.name = text);
    };
    let mode = move || {
        view.with(|v| match v.definition.mode {
            FlowMode::Single => "single",
            FlowMode::Restart => "restart",
        })
    };
    let original = Signal::derive(move || view.with(|v| v.definition.clone()));
    view! {
        <div class="flow-toolbar flow-toolbar--title">
            <A href="/flows" attr:class="flow-toolbar__back" attr:aria-label="Back to flows">
                "←"
            </A>
            <Show
                when=move || editing.get()
                fallback=move || view! { <h1 class="flow-toolbar__name">{name}</h1> }
            >
                <input
                    type="text"
                    class="flow-toolbar__name flow-toolbar__name--input"
                    aria-label="Flow name"
                    spellcheck="false"
                    autocomplete="off"
                    prop:value=move || editor.draft.with(|f| f.name.clone())
                    on:focus=move |_| armed.set_value(true)
                    on:input=rename
                />
            </Show>
            <span class="flow-toolbar__mode">{mode}</span>
        </div>
        <div class="flow-toolbar flow-toolbar--actions">
            <Show
                when=move || editing.get()
                fallback=move || view! { <ReadControls view=view editing=editing editor=editor /> }
            >
                <EditControls
                    editing=editing
                    editor=editor
                    saving=saving
                    original=original
                    selection=selection
                />
            </Show>
            <button
                type="button"
                class="btn btn--ghost flow-toolbar__runs"
                class:flow-toolbar__runs--open=move || drawer.get()
                aria-expanded=move || drawer.get().to_string()
                on:click=move |_| drawer.update(|open| *open = !*open)
            >
                "Runs"
            </button>
        </div>
    }
}
