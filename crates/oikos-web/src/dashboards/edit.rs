//! Dashboard edit mode: draft/baseline state plus the pencil / Discard / Save toolbar.

use leptos::prelude::*;
use leptos::task::spawn_local;
use oikos_core::dashboard::{Dashboard, Widget, allowed_spans};

use crate::api;
use crate::house::House;
use crate::icons::{CHECK, GEAR, PENCIL, X_MARK};

/// Reactive state for the edit mode: active flag, the editable draft, and the saved baseline
/// for Discard. House never gets one of these.
#[derive(Clone, Copy)]
pub struct EditState {
    pub active: RwSignal<bool>,
    pub draft: RwSignal<Dashboard>,
    /// Last-saved baseline; Save replaces it with the hub's reply.
    pub original: RwSignal<Dashboard>,
    /// Index of the widget whose config sheet is open.
    pub selected_widget: RwSignal<Option<usize>>,
    pub settings_open: RwSignal<bool>,
    /// The id of the widget just added from the library, which glows for a moment.
    pub fresh: RwSignal<Option<String>>,
}

impl EditState {
    pub fn new(initial: Dashboard, active: bool) -> Self {
        Self {
            active: RwSignal::new(active),
            draft: RwSignal::new(initial.clone()),
            original: RwSignal::new(initial),
            selected_widget: RwSignal::new(None),
            settings_open: RwSignal::new(false),
            fresh: RwSignal::new(None),
        }
    }

    pub fn dirty(&self) -> Memo<bool> {
        let draft = self.draft;
        let original = self.original;
        Memo::new(move |_| draft.get() != original.get())
    }

    /// Moves the widget at `from` so it ends up at index `to`.
    pub fn reorder(&self, from: usize, to: usize) {
        // Selection is positional, so any reorder invalidates it.
        self.selected_widget.set(None);
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
        self.selected_widget.set(None);
        self.draft.set(self.original.get_untracked());
    }
}

/// The selection after a save: kept only while its index still points at the same widget id
/// in the saved dashboard, since the hub may have normalized the widget order.
fn kept_selection(selected: Option<usize>, before: &Dashboard, after: &Dashboard) -> Option<usize> {
    let index = selected?;
    let id = &before.widgets.get(index)?.id;
    (after.widgets.get(index).map(|w| &w.id) == Some(id)).then_some(index)
}

/// The pencil's accessible name; it says so when switching edit mode off left an unsaved draft.
fn pencil_label(active: bool, dirty: bool) -> &'static str {
    if dirty && !active {
        "Toggle edit mode (unsaved changes)"
    } else {
        "Toggle edit mode"
    }
}

/// How long a widget added from the library glows.
#[cfg(target_arch = "wasm32")]
const FRESH_MS: u32 = 1200;

/// Marks widget `id` as just added, and unmarks it after [`FRESH_MS`] unless another widget
/// took the mark meanwhile.
#[cfg(target_arch = "wasm32")]
fn flash(fresh: RwSignal<Option<String>>, id: String) {
    fresh.set(Some(id.clone()));
    gloo_timers::callback::Timeout::new(FRESH_MS, move || {
        if fresh.try_get_untracked().flatten().as_deref() == Some(id.as_str()) {
            fresh.try_set(None);
        }
    })
    .forget();
}

/// Appends a picked widget to the draft and marks it fresh; the hub validates it on Save.
pub fn add_widget_handler(state: EditState) -> Callback<Widget> {
    Callback::new(move |widget: Widget| {
        let id = widget.id.clone();
        state.draft.update(|d| d.widgets.push(widget));
        #[cfg(target_arch = "wasm32")]
        flash(state.fresh, id);
        #[cfg(not(target_arch = "wasm32"))]
        state.fresh.set(Some(id));
    })
}

/// The pencil / Discard / Save bar shown on authored dashboards. Pencil toggles edit mode and
/// lights amber when on; in edit mode a gear opens the dashboard settings sheet, and Discard /
/// Save book-end the row with Save as the primary action.
#[component]
pub fn EditToolbar(state: EditState) -> impl IntoView {
    let house = expect_context::<House>();
    let dirty = state.dirty();
    // True while a Save request is in flight; blocks a second PUT and a Discard mid-save.
    let saving = RwSignal::new(false);
    view! {
        <div class="dashboard-edit-toolbar">
            <button
                type="button"
                class="btn btn--round dashboard-edit-toolbar__pencil"
                class:btn--active=move || state.active.get()
                class:dashboard-edit-toolbar__pencil--dirty=move || {
                    !state.active.get() && dirty.get()
                }
                aria-label=move || pencil_label(state.active.get(), dirty.get())
                aria-pressed=move || state.active.get().to_string()
                on:click=move |_| {
                    let now_active = !state.active.get_untracked();
                    state.active.set(now_active);
                    if !now_active {
                        state.selected_widget.set(None);
                        state.settings_open.set(false);
                    }
                }
            >
                <span class="btn__glyph" inner_html=PENCIL></span>
            </button>
            <Show when=move || state.active.get()>
                <button
                    type="button"
                    class="btn btn--round dashboard-edit-toolbar__gear"
                    aria-label="Dashboard settings"
                    on:click=move |_| state.settings_open.update(|o| *o = !*o)
                >
                    <span class="btn__glyph" inner_html=GEAR></span>
                </button>
                <button
                    type="button"
                    class="btn btn--with-icon dashboard-edit-toolbar__discard"
                    on:click=move |_| state.discard()
                    disabled=move || !dirty.get() || saving.get()
                >
                    <span class="btn__glyph" inner_html=X_MARK></span>
                    "Discard"
                </button>
                <button
                    type="button"
                    class="btn btn--with-icon btn--primary dashboard-edit-toolbar__save"
                    on:click=move |_| {
                        if saving.get_untracked() {
                            return;
                        }
                        saving.set(true);
                        let d = state.draft.get_untracked();
                        spawn_local(async move {
                            if let Ok(saved) = api::put_dashboard(&d).await {
                                // The hub may normalize widget order, invalidating the selection.
                                let current = state.draft.get_untracked();
                                let kept = kept_selection(
                                    state.selected_widget.get_untracked(),
                                    &current,
                                    &saved,
                                );
                                state.selected_widget.set(kept);
                                state.original.set(saved.clone());
                                state.draft.set(saved.clone());
                                house.store_dashboard(saved);
                            }
                            saving.set(false);
                        });
                    }
                    disabled=move || !dirty.get() || saving.get()
                >
                    <span class="btn__glyph" inner_html=CHECK></span>
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
            config: WidgetConfig::Entity {
                entity_id: "light.a".into(),
                title: None,
            },
        }
    }

    fn state(widgets: Vec<Widget>) -> EditState {
        EditState::new(
            Dashboard {
                id: "d".into(),
                name: "D".into(),
                order: 0,
                widgets,
                show_header: false,
            },
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
    fn selection_kept_only_when_widget_id_unchanged() {
        let dash = |ids: &[&str]| Dashboard {
            id: "d".into(),
            name: "D".into(),
            order: 0,
            widgets: ids
                .iter()
                .map(|id| widget(id, WidgetKind::Light, Span::X1x1))
                .collect(),
            show_header: false,
        };
        let before = dash(&["a", "b"]);
        assert_eq!(
            kept_selection(Some(1), &before, &dash(&["a", "b"])),
            Some(1)
        );
        assert_eq!(kept_selection(Some(1), &before, &dash(&["b", "a"])), None);
        assert_eq!(kept_selection(Some(1), &before, &dash(&["a"])), None);
        assert_eq!(kept_selection(Some(5), &before, &before), None);
        assert_eq!(kept_selection(None, &before, &before), None);
    }

    #[test]
    fn pencil_label_flags_unsaved_changes_only_when_inactive() {
        assert_eq!(
            pencil_label(false, true),
            "Toggle edit mode (unsaved changes)"
        );
        assert_eq!(pencil_label(true, true), "Toggle edit mode");
        assert_eq!(pencil_label(false, false), "Toggle edit mode");
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
