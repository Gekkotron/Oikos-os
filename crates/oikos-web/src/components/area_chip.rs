//! A room chip: the area name and how many entities it shows.

use leptos::prelude::*;
use oikos_api::AreaView;

#[component]
pub fn AreaChip(
    area: AreaView,
    #[prop(into)] selected: Signal<bool>,
    on_click: Callback<String>,
) -> impl IntoView {
    let id = area.id;
    view! {
        <button
            type="button"
            class="chip"
            class:chip--selected=move || selected.get()
            aria-pressed=move || selected.get().to_string()
            on:click=move |_| on_click.run(id.clone())
        >
            <span class="chip-name">{area.name}</span>
            <span class="chip-badge">{area.primary_count}</span>
        </button>
    }
}
