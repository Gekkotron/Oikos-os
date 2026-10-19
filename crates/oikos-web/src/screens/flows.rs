//! The Flows page: one row per automation with its summary, last run and an enable switch.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use oikos_api::FlowView;

use crate::api;
use crate::components::toast::Toasts;
use crate::flows::{status_word, summary};
use crate::format::format_timestamp;
use crate::house::House;

/// The last run as "Done · <time>", or "No runs yet".
fn last_run(flow: &FlowView) -> String {
    flow.runs.first().map_or_else(
        || "No runs yet".to_owned(),
        |run| {
            format!(
                "{} · {}",
                status_word(run.status),
                format_timestamp(run.started_at)
            )
        },
    )
}

#[component]
fn FlowRow(flow: FlowView) -> impl IntoView {
    let toasts = expect_context::<Toasts>();
    let id = flow.definition.id.clone();
    let href = format!("/flows/{}", api::encode_id(&id));
    let name = flow.definition.name.clone();
    let enabled = flow.definition.enabled;
    let invalid = flow.invalid.clone();
    let run = last_run(&flow);
    let line = match &invalid {
        Some(reason) => reason.clone(),
        None => summary(&flow.definition),
    };
    let label = format!("Enable {name}");
    let toggle = {
        let id = id.clone();
        move |_| {
            let id = id.clone();
            spawn_local(async move {
                if let Err(e) = api::set_flow_enabled(&id, !enabled).await {
                    toasts.push(e.error);
                }
            });
        }
    };
    view! {
        <li class="flow-row">
            <div class="flow-row__main">
                <A href=href attr:class="flow-row__name">
                    {name.clone()}
                </A>
                <span class="flow-row__summary">{line}</span>
                <span class="flow-row__run">{run}</span>
            </div>
            {invalid
                .clone()
                .map(|reason| view! { <span class="badge badge--invalid" title=reason>"Invalid"</span> })}
            <button
                type="button"
                role="switch"
                class="switch"
                class:switch--on=enabled
                aria-checked=enabled.to_string()
                aria-label=label
                disabled=invalid.is_some()
                on:click=toggle
            >
                <span class="switch__knob"></span>
            </button>
        </li>
    }
}

#[component]
pub fn FlowsScreen() -> impl IntoView {
    let house = expect_context::<House>();
    view! {
        <div class="flows">
            <header class="sheet__header">
                <div class="sheet__titles">
                    <h1 class="sheet__title">"Flows"</h1>
                </div>
            </header>
            <Show
                when=move || house.flows.with(|f| !f.is_empty())
                fallback=|| {
                    view! {
                        <p class="flow-list__empty">
                            "No flows yet. Ask the assistant to create one."
                        </p>
                    }
                }
            >
                <ul class="flow-list">
                    <For
                        each=move || house.flows.get()
                        key=|f| serde_json::to_string(f).unwrap_or_else(|_| f.definition.id.clone())
                        children=|flow| view! { <FlowRow flow=flow /> }
                    />
                </ul>
            </Show>
        </div>
    }
}
