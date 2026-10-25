//! Side sheets for edit mode: per-widget config and per-dashboard settings.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::hooks::use_navigate;
use oikos_api::HOUSE_ID;
use oikos_core::dashboard::{Dashboard, Span, Widget, WidgetConfig, WidgetKind, allowed_spans};

use crate::api;
use crate::components::confirm::{ConfirmBar, Confirming};
use crate::components::toast::Toasts;
use crate::dashboards::edit::EditState;
use crate::house::House;
use crate::i18n::{DashText, Key, dash_text, delete_question, text, use_lang, widget_sheet_title};

/// Per-widget sheet: title override, size and delete. Swapping the widget's target
/// entity is deferred; delete + re-add covers it for v1.
#[component]
pub fn WidgetConfigSheet(state: EditState, index: usize, on_close: Callback<()>) -> impl IntoView {
    let lang = use_lang();
    // Kind and span are tracked separately so typing in the title does not rebuild the
    // inputs (which would drop focus on every keystroke).
    let kind = Memo::new(move |_| state.draft.with(|d| d.widgets.get(index).map(|w| w.kind)));
    let span = Memo::new(move |_| state.draft.with(|d| d.widgets.get(index).map(|w| w.span)));
    let initial_title = state
        .draft
        .with_untracked(|d| d.widgets.get(index).and_then(title_of))
        .unwrap_or_default();

    // The index goes stale once the widget is removed; close rather than render nothing.
    Effect::new(move |_| {
        if kind.get().is_none() {
            on_close.run(());
        }
    });

    view! {
        <aside class="widget-config-sheet">
            <h3>{move || widget_sheet_title(lang(), kind.get().unwrap_or(WidgetKind::Light))}</h3>
            <label class="sheet-form__field">
                <span class="sheet-form__label">{move || dash_text(lang(), DashText::Title)}</span>
                <input
                    type="text"
                    class="field"
                    value=initial_title
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        state.draft.update(|d| {
                            if let Some(w) = d.widgets.get_mut(index) {
                                set_title(w, if v.is_empty() { None } else { Some(v) });
                            }
                        });
                    }
                />
            </label>
            <label class="sheet-form__field">
                <span class="sheet-form__label">{move || dash_text(lang(), DashText::Size)}</span>
                <select
                    class="select"
                    on:change=move |ev| {
                        let v = event_target_value(&ev);
                        state.draft.update(|d| {
                            if let (Some(w), Some(s)) = (d.widgets.get_mut(index), parse_span(&v)) {
                                w.span = s;
                            }
                        });
                    }
                >
                    {move || {
                        let cur = span.get();
                        let k = kind.get().unwrap_or(WidgetKind::Light);
                        allowed_spans(k)
                            .iter()
                            .map(|s| {
                                let label = s.as_str();
                                view! { <option value=label selected={Some(*s) == cur}>{label}</option> }
                            })
                            .collect::<Vec<_>>()
                    }}
                </select>
            </label>
            <button
                type="button"
                class="btn btn--danger"
                on:click=move |_| {
                    state.draft.update(|d| remove_widget(d, index));
                    on_close.run(());
                }
            >
                {move || dash_text(lang(), DashText::DeleteWidget)}
            </button>
            <button type="button" class="btn" on:click=move |_| on_close.run(())>
                {move || dash_text(lang(), DashText::Close)}
            </button>
        </aside>
    }
}

/// History widgets carry no title in the model, so the override is a no-op for them.
fn title_of(w: &Widget) -> Option<String> {
    match &w.config {
        WidgetConfig::Entity { title, .. }
        | WidgetConfig::Area { title, .. }
        | WidgetConfig::Flow { title, .. }
        | WidgetConfig::Energy { title, .. } => title.clone(),
        WidgetConfig::History { .. } => None,
    }
}

fn set_title(w: &mut Widget, new: Option<String>) {
    match &mut w.config {
        WidgetConfig::Entity { title, .. }
        | WidgetConfig::Area { title, .. }
        | WidgetConfig::Flow { title, .. }
        | WidgetConfig::Energy { title, .. } => *title = new,
        WidgetConfig::History { .. } => {}
    }
}

fn parse_span(s: &str) -> Option<Span> {
    match s {
        "1x1" => Some(Span::X1x1),
        "2x1" => Some(Span::X2x1),
        "2x2" => Some(Span::X2x2),
        "1x2" => Some(Span::X1x2),
        _ => None,
    }
}

fn remove_widget(d: &mut Dashboard, index: usize) {
    if index < d.widgets.len() {
        d.widgets.remove(index);
    }
}

/// Swaps `id` with its neighbour (`delta` -1 up, +1 down); unchanged at the ends or when
/// `id` is absent.
fn shifted_order(mut ids: Vec<String>, id: &str, delta: isize) -> Vec<String> {
    if let Some(pos) = ids.iter().position(|i| i == id) {
        let target = pos as isize + delta;
        if target >= 0 && (target as usize) < ids.len() {
            ids.swap(pos, target as usize);
        }
    }
    ids
}

/// Persists a reorder and mirrors it into the in-memory list, since the hub does not
/// push dashboard structure. House stays first; it is not part of the reorder payload.
fn move_dashboard(house: House, toasts: Toasts, id: String, delta: isize) {
    let ids: Vec<String> = house
        .dashboards
        .get_untracked()
        .iter()
        .map(|dv| dv.dashboard.id.clone())
        .filter(|i| i != HOUSE_ID)
        .collect();
    let ids = shifted_order(ids, &id, delta);
    spawn_local(async move {
        match api::reorder_dashboards(&ids).await {
            Ok(_) => house.dashboards.update(|v| {
                v.sort_by_key(|dv| {
                    ids.iter()
                        .position(|i| *i == dv.dashboard.id)
                        .map_or(0, |p| p + 1)
                });
            }),
            Err(e) => toasts.push(e.error),
        }
    });
}

/// Per-dashboard sheet: name, header, delete and move up/down in the sidebar list.
#[component]
pub fn DashboardSettingsSheet(state: EditState, on_close: Callback<()>) -> impl IntoView {
    let house = expect_context::<House>();
    let toasts = expect_context::<Toasts>();
    let confirming = expect_context::<Confirming>();
    let lang = use_lang();
    let navigate = use_navigate();
    let initial_name = state.draft.with_untracked(|d| d.name.clone());
    let show_header = move || state.draft.with(|d| d.show_header);

    view! {
        <aside class="dashboard-settings-sheet">
            <h3>{move || dash_text(lang(), DashText::DashboardSettings)}</h3>
            <label class="sheet-form__field">
                <span class="sheet-form__label">{move || dash_text(lang(), DashText::Name)}</span>
                <input
                    type="text"
                    class="field"
                    value=initial_name
                    on:input=move |ev| {
                        let v = event_target_value(&ev);
                        state.draft.update(|d| d.name = v);
                    }
                />
            </label>
            <label class="sheet-form__flag">
                <span class="sheet-form__label">{move || dash_text(lang(), DashText::ShowHeader)}</span>
                <button
                    type="button"
                    role="switch"
                    class="switch"
                    class:switch--on=show_header
                    aria-checked=move || show_header().to_string()
                    on:click=move |_| state.draft.update(|d| d.show_header = !d.show_header)
                >
                    <span class="switch__knob"></span>
                </button>
            </label>
            <div class="dashboard-settings-sheet__reorder">
                <button
                    type="button"
                    class="btn"
                    on:click=move |_| {
                        move_dashboard(house, toasts, state.draft.get_untracked().id, -1)
                    }
                >
                    {move || text(lang(), Key::MoveUp)}
                </button>
                <button
                    type="button"
                    class="btn"
                    on:click=move |_| {
                        move_dashboard(house, toasts, state.draft.get_untracked().id, 1)
                    }
                >
                    {move || text(lang(), Key::MoveDown)}
                </button>
            </div>
            <Show
                when=move || confirming.is_asking(&state.draft.with(|d| d.id.clone()))
                fallback=move || {
                    view! {
                        <button
                            type="button"
                            class="btn btn--danger"
                            on:click=move |_| confirming.ask(&state.draft.get_untracked().id)
                        >
                            {move || dash_text(lang(), DashText::DeleteDashboard)}
                        </button>
                    }
                }
            >
                <ConfirmBar
                    question=Signal::derive(move || {
                        delete_question(lang(), &state.draft.with(|d| d.name.clone()))
                    })
                    yes=Signal::derive(move || text(lang(), Key::Delete).to_owned())
                    on_yes=Callback::new({
                        let navigate = navigate.clone();
                        move |()| {
                            confirming.clear();
                            let id = state.draft.get_untracked().id;
                            let navigate = navigate.clone();
                            spawn_local(async move {
                                match api::delete_dashboard(&id).await {
                                    Ok(_) => {
                                        house
                                            .dashboards
                                            .update(|v| v.retain(|dv| dv.dashboard.id != id));
                                        navigate(
                                            &format!("/dashboards/{HOUSE_ID}"),
                                            Default::default(),
                                        );
                                    }
                                    Err(e) => toasts.push(e.error),
                                }
                            });
                        }
                    })
                    on_cancel=Callback::new(move |()| confirming.clear())
                />
            </Show>
            <button type="button" class="btn" on:click=move |_| on_close.run(())>
                {move || dash_text(lang(), DashText::Close)}
            </button>
        </aside>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn widget(id: &str) -> Widget {
        Widget {
            id: id.into(),
            kind: WidgetKind::Light,
            span: Span::X1x1,
            config: WidgetConfig::Entity {
                entity_id: "light.a".into(),
                title: None,
            },
        }
    }

    #[test]
    fn config_sheet_delete_removes_widget_from_draft() {
        let s = EditState::new(
            Dashboard {
                id: "d".into(),
                name: "D".into(),
                order: 0,
                widgets: vec![widget("a"), widget("b")],
                show_header: false,
            },
            true,
        );
        s.draft.update(|d| remove_widget(d, 0));
        let d = s.draft.get();
        assert_eq!(d.widgets.len(), 1);
        assert_eq!(d.widgets[0].id, "b");
    }

    #[test]
    fn move_up_calculates_adjacent_swap() {
        let ids = vec!["a".to_string(), "b".into(), "c".into()];
        assert_eq!(shifted_order(ids.clone(), "b", -1), vec!["b", "a", "c"]);
        assert_eq!(shifted_order(ids.clone(), "a", -1), vec!["a", "b", "c"]);
        assert_eq!(shifted_order(ids, "b", 1), vec!["a", "c", "b"]);
    }
}
