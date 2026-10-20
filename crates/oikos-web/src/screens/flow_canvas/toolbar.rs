//! The floating bars over the canvas: top-left the back link, the flow's name and its mode;
//! top-right the enable switch, Run, Edit and Runs (＋ Add, Tidy, Undo, Cancel and Save while
//! editing).

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use oikos_api::FlowView;
use oikos_core::flow::{Flow, FlowMode};

use super::Editor;
use crate::api;
use crate::components::flow_switch::FlowSwitch;
use crate::components::toast::Toasts;
use crate::flow_layout::{tidy, with_resolved_positions};
use crate::icons::{CHECK, PENCIL, PLAY, X_MARK};
use crate::screens::flow_schedule::SaveBlocked;

#[component]
fn EditControls(
    editing: RwSignal<bool>,
    editor: Editor,
    saving: RwSignal<bool>,
    original: Signal<Flow>,
) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let blocked = use_context::<SaveBlocked>();
    let is_blocked = move || blocked.is_some_and(|b| b.0.get());
    let cancel = move |_| {
        if let Some(b) = blocked {
            b.0.set(false);
        }
        editor.reset(original.get_untracked());
        editing.set(false);
    };
    let save = move |_| {
        if saving.get_untracked() || blocked.is_some_and(|b| b.0.get_untracked()) {
            return;
        }
        saving.set(true);
        let snapshot = editor.draft.get_untracked();
        spawn_local(async move {
            match api::put_flow(&snapshot).await {
                // Saved: back to the view, which the hub's echo updates.
                Ok(view) => {
                    editor.reset(view.definition);
                    editing.set(false);
                }
                Err(e) => toasts.push(e.error),
            }
            saving.set(false);
        });
    };
    view! {
        // The picker that adds cards comes with the next step of the editor.
        <button type="button" class="btn btn--ghost" aria-label="Add card" title="Add card" disabled>
            "＋ Add"
        </button>
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
) -> impl IntoView {
    let name = move || {
        if editing.get() {
            editor.draft.with(|f| f.name.clone())
        } else {
            view.with(|v| v.definition.name.clone())
        }
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
            <h1 class="flow-toolbar__name">{name}</h1>
            <span class="flow-toolbar__mode">{mode}</span>
        </div>
        <div class="flow-toolbar flow-toolbar--actions">
            <Show
                when=move || editing.get()
                fallback=move || view! { <ReadControls view=view editing=editing editor=editor /> }
            >
                <EditControls editing=editing editor=editor saving=saving original=original />
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
