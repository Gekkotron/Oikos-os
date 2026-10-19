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
    let button = NodeRef::<leptos::html::Button>::new();
    // Bring a chip the user just picked to the middle of the scrolling rail. Only a change to
    // selected counts: a chip that is selected when it is first built (page load, or a rebuild
    // after its count changed) must not make the rail jump.
    Effect::new(move |previous: Option<bool>| {
        let now = selected.get();
        if previous == Some(false)
            && now
            && let Some(button) = button.get()
        {
            let options = web_sys::ScrollIntoViewOptions::new();
            options.set_inline(web_sys::ScrollLogicalPosition::Center);
            options.set_block(web_sys::ScrollLogicalPosition::Nearest);
            options.set_behavior(web_sys::ScrollBehavior::Smooth);
            button.scroll_into_view_with_scroll_into_view_options(&options);
        }
        now
    });
    view! {
        <button
            node_ref=button
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
