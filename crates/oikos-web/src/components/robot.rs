//! The robot vacuum drawn from above: rings around a round body with its top bump. Generic on
//! purpose, no brand. The activity, as a class, lets the stylesheet animate or tint it.

use leptos::prelude::*;

#[component]
pub fn Robot(activity: Option<String>) -> impl IntoView {
    let class = format!("robot robot--{}", activity.as_deref().unwrap_or("unknown"));
    view! {
        <svg class=class viewBox="0 0 150 150" aria-hidden="true">
            <circle class="robot__ring robot__ring--outer" cx="75" cy="75" r="72"></circle>
            <circle class="robot__ring" cx="75" cy="75" r="60"></circle>
            <circle class="robot__body" cx="75" cy="75" r="48"></circle>
            <circle class="robot__top" cx="75" cy="64" r="15"></circle>
            <rect class="robot__sensor" x="71" y="35" width="8" height="2" rx="1"></rect>
        </svg>
    }
}
