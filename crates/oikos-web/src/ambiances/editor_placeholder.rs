//! Stands in for the ambiance editor on `/ambiances/new` and `/ambiances/:id` until it lands.

use leptos::prelude::*;

#[component]
pub fn AmbianceEditorPlaceholder() -> impl IntoView {
    view! { <div class="sheet"></div> }
}
