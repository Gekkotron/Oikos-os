//! Dashboard edit mode: draft/baseline state plus the pencil / Discard / Save toolbar.

use leptos::prelude::*;
use leptos::task::spawn_local;
use oikos_core::dashboard::{allowed_spans, Dashboard, Widget};

use crate::api;

/// Reactive state for the edit mode: active flag, the editable draft, and the saved baseline
/// for Discard. House never gets one of these.
#[derive(Clone, Copy)]
pub struct EditState {
    pub active: RwSignal<bool>,
    pub draft: RwSignal<Dashboard>,
    /// Last-saved baseline; Save replaces it with the hub's reply.
    pub original: RwSignal<Dashboard>,
}

impl EditState {
    pub fn new(initial: Dashboard, active: bool) -> Self {
        Self {
            active: RwSignal::new(active),
            draft: RwSignal::new(initial.clone()),
            original: RwSignal::new(initial),
        }
    }

    pub fn dirty(&self) -> Memo<bool> {
        let draft = self.draft;
        let original = self.original;
        Memo::new(move |_| draft.get() != original.get())
    }

    /// Moves the widget at `from` so it ends up at index `to`.
    pub fn reorder(&self, from: usize, to: usize) {
        self.draft.update(|d| {
            if from < d.widgets.len() && to < d.widgets.len() && from != to {
                let w = d.widgets.remove(from);
                d.widgets.insert(to, w);
            }
        });
    }

    /// Advances widget `index` to the next span its kind allows, wrapping around, so the
    /// editor can never produce a span the hub would reject.
    pub fn cycle_span(&self, index: usize) {
        self.draft.update(|d| {
            if let Some(w) = d.widgets.get_mut(index) {
                let allowed = allowed_spans(w.kind);
                let cur = allowed.iter().position(|s| *s == w.span).unwrap_or(0);
                w.span = allowed[(cur + 1) % allowed.len()];
            }
        });
    }

    pub fn discard(&self) {
        self.draft.set(self.original.get_untracked());
    }
}

/// Appends a picked widget to the draft; the hub validates it on Save.
pub fn add_widget_handler(state: EditState) -> Callback<Widget> {
    Callback::new(move |widget: Widget| {
        state.draft.update(|d| d.widgets.push(widget));
    })
}

/// The pencil / Discard / Save bar shown on authored dashboards.
#[component]
pub fn EditToolbar(state: EditState) -> impl IntoView {
    let dirty = state.dirty();
    view! {
        <div class="dashboard-edit-toolbar">
            <button
                type="button"
                class="dashboard-edit-toolbar__pencil"
                aria-pressed=move || state.active.get().to_string()
                on:click=move |_| state.active.update(|a| *a = !*a)
            >
                "✎"
            </button>
            <Show when=move || state.active.get()>
                <button
                    type="button"
                    class="dashboard-edit-toolbar__discard"
                    on:click=move |_| state.discard()
                    disabled=move || !dirty.get()
                >
                    "Discard"
                </button>
                <button
                    type="button"
                    class="dashboard-edit-toolbar__save"
                    on:click=move |_| {
                        let d = state.draft.get_untracked();
                        spawn_local(async move {
                            if let Ok(saved) = api::put_dashboard(&d).await {
                                state.original.set(saved.clone());
                                state.draft.set(saved);
                            }
                        });
                    }
                    disabled=move || !dirty.get()
                >
                    "Save"
                </button>
            </Show>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use oikos_core::dashboard::{Span, Widget, WidgetConfig, WidgetKind};

    fn widget(id: &str, kind: WidgetKind, span: Span) -> Widget {
        Widget {
            id: id.into(),
            kind,
            span,
            config: WidgetConfig::Entity { entity_id: "light.a".into(), title: None },
        }
    }

    fn state(widgets: Vec<Widget>) -> EditState {
        EditState::new(
            Dashboard { id: "d".into(), name: "D".into(), order: 0, widgets },
            true,
        )
    }

    #[test]
    fn dirty_memo_true_after_reorder() {
        let s = state(vec![
            widget("a", WidgetKind::Light, Span::X1x1),
            widget("b", WidgetKind::Light, Span::X1x1),
        ]);
        let dirty = s.dirty();
        assert!(!dirty.get());
        s.reorder(0, 1);
        assert!(dirty.get());
        assert_eq!(s.draft.get().widgets[0].id, "b");
    }

    #[test]
    fn resize_cycles_only_allowed_spans_for_kind() {
        let s = state(vec![widget("a", WidgetKind::Light, Span::X1x1)]);
        s.cycle_span(0);
        assert_eq!(s.draft.get().widgets[0].span, Span::X2x1);
        s.cycle_span(0);
        assert_eq!(s.draft.get().widgets[0].span, Span::X1x1);
    }
}
