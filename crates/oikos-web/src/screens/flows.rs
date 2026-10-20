//! The Flows page: one card per automation with its trigger, mode, last run and an enable switch.

use leptos::prelude::*;
use leptos_router::components::A;
use oikos_api::FlowView;
use oikos_core::flow::FlowMode;

use crate::alerts::relative_time;
use crate::api;
use crate::components::flow_switch::FlowSwitch;
use crate::components::now::Now;
use crate::flows::{status_word, trigger_summary};
use crate::format::format_timestamp;
use crate::house::House;

/// The last run as "Done · 2 min ago", or "No runs yet".
fn last_run(flow: &FlowView, now: u64) -> String {
    flow.runs.first().map_or_else(
        || "No runs yet".to_owned(),
        |run| {
            format!(
                "{} · {}",
                status_word(run.status),
                relative_time(run.started_at, now)
            )
        },
    )
}

fn mode_word(mode: FlowMode) -> &'static str {
    match mode {
        FlowMode::Single => "Single",
        FlowMode::Restart => "Restart",
    }
}

#[component]
fn FlowRow(flow: FlowView) -> impl IntoView {
    let house = expect_context::<House>();
    let id = flow.definition.id.clone();
    let href = format!("/flows/{}", api::encode_id(&id));
    let name = flow.definition.name.clone();
    let enabled = flow.definition.enabled;
    let invalid = flow.invalid.clone();
    let mode = mode_word(flow.definition.mode);
    let now = expect_context::<Now>();
    let started = flow
        .runs
        .first()
        .map(|run| format_timestamp(run.started_at));
    let trigger_text = house
        .entities
        .with(|e| trigger_summary(&flow.definition, e));
    let name_for_switch = name.clone();
    let invalid_badge = invalid.clone().map(|reason| {
        view! {
            <span class="badge badge--invalid" title=reason>"Invalid"</span>
        }
    });
    view! {
        <li class="flow-card" class:flow-card--invalid=invalid.is_some()>
            <A href=href attr:class="flow-card__title-link">
                <div class="flow-card__header">
                    <h2 class="flow-card__title">
                        {name.clone()}
                        <span class="flow-card__mode">{mode.to_lowercase()}</span>
                    </h2>
                    {invalid_badge}
                </div>
                <dl class="flow-card__meta">
                    <div class="flow-card__meta-row">
                        <dt>"Trigger"</dt>
                        <dd>{trigger_text}</dd>
                    </div>
                    <div class="flow-card__meta-row">
                        <dt>"Last run"</dt>
                        <dd title=started>
                            {move || last_run(&flow, now.0.get())}
                        </dd>
                    </div>
                </dl>
            </A>
            <div class="flow-card__switch">
                <FlowSwitch id=id name=name_for_switch enabled=enabled invalid=invalid.is_some() />
            </div>
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
