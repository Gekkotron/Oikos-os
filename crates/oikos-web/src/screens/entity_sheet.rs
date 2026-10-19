use leptos::prelude::*;
use leptos_router::hooks::use_params_map;

#[component]
pub fn EntitySheetScreen() -> impl IntoView {
    let params = use_params_map();
    let id = move || params.read().get("id").unwrap_or_default();
    view! {
        <h1>"Entity"</h1>
        <p>{id}</p>
    }
}
