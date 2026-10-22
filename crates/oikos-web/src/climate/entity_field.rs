//! Entity pickers for the climate settings: one entity, or a list of them.

use leptos::prelude::*;

use crate::format::list_label;
use crate::house::House;

/// Every entity as `(id, label)`, sorted by label.
fn options(house: House) -> Vec<(String, String)> {
    let mut list: Vec<(String, String)> = house
        .entities
        .with(|m| m.values().map(|e| (e.id.clone(), list_label(e))).collect());
    list.sort_by_key(|(_, label)| label.to_lowercase());
    list
}

/// The label of `id`, or the id itself when the house no longer has it.
fn label_of(house: House, id: &str) -> String {
    house
        .entities
        .with(|m| m.get(id).map(list_label))
        .unwrap_or_else(|| id.to_owned())
}

/// A select over the house's entities with a "None" choice. An id the house lost stays shown.
#[component]
pub fn EntityField(
    label: &'static str,
    value: Signal<Option<String>>,
    on_change: Callback<Option<String>>,
) -> impl IntoView {
    let house = expect_context::<House>();
    view! {
        <label class="climate-field">
            <span class="climate-field__label">{label}</span>
            <select
                class="field"
                on:change=move |ev| {
                    let v = event_target_value(&ev);
                    on_change.run((!v.is_empty()).then_some(v));
                }
            >
                <option value="" selected=move || value.get().is_none()>"None"</option>
                {move || {
                    let current = value.get();
                    let mut list = options(house);
                    if let Some(id) = current.as_ref()
                        && !list.iter().any(|(i, _)| i == id)
                    {
                        list.insert(0, (id.clone(), id.clone()));
                    }
                    list.into_iter()
                        .map(|(id, text)| {
                            let selected = current.as_deref() == Some(id.as_str());
                            view! { <option value=id selected=selected>{text}</option> }
                        })
                        .collect_view()
                }}
            </select>
        </label>
    }
}

/// A list of entities as removable chips, with a select that adds one.
#[component]
pub fn EntityListField(
    label: &'static str,
    value: Signal<Vec<String>>,
    on_change: Callback<Vec<String>>,
) -> impl IntoView {
    let house = expect_context::<House>();
    view! {
        <div class="climate-field">
            <span class="climate-field__label">{label}</span>
            <div class="chips climate-chips">
                {move || {
                    value
                        .get()
                        .into_iter()
                        .map(|id| {
                            let text = label_of(house, &id);
                            let gone = id.clone();
                            view! {
                                <button
                                    type="button"
                                    class="chip"
                                    title="Remove"
                                    on:click=move |_| {
                                        let mut list = value.get_untracked();
                                        list.retain(|i| i != &gone);
                                        on_change.run(list);
                                    }
                                >
                                    {text}
                                    " ×"
                                </button>
                            }
                        })
                        .collect_view()
                }}
            </div>
            <select
                class="field"
                aria-label=label
                on:change=move |ev| {
                    let v = event_target_value(&ev);
                    if !v.is_empty() {
                        let mut list = value.get_untracked();
                        if !list.contains(&v) {
                            list.push(v);
                            on_change.run(list);
                        }
                    }
                }
            >
                <option value="" selected=true>"Add…"</option>
                {move || {
                    let taken = value.get();
                    options(house)
                        .into_iter()
                        .filter(|(id, _)| !taken.contains(id))
                        .map(|(id, text)| view! { <option value=id>{text}</option> })
                        .collect_view()
                }}
            </select>
        </div>
    }
}
