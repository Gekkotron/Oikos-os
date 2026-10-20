//! The right drawer over the canvas: the recent runs, or what `children` holds (the selected
//! card's settings while editing). Clicking a run replays it on the canvas.

use leptos::prelude::*;
use oikos_api::{RunView, StepOutcome};
use oikos_core::flow::Flow;
use wasm_bindgen::JsCast;

use crate::alerts::relative_time;
use crate::components::now::Now;
use crate::flow_text::{Ctx, title};
use crate::flows::{run_duration, status_class, status_word, step_offset, trigger_line};
use crate::format::format_timestamp;
use crate::house::House;
use crate::icons::X_MARK;

#[component]
fn RunItem(run: RunView, flow: Flow, replay: RwSignal<Option<String>>) -> impl IntoView {
    let house = expect_context::<House>();
    let now = expect_context::<Now>();
    let trigger = house
        .entities
        .with(|e| trigger_line(run.trigger.as_ref(), e));
    let started = run.started_at;
    let duration = run_duration(run.started_at, run.ended_at);
    let names: Vec<(String, String)> = house.flows.with_untracked(|flows| {
        flows
            .iter()
            .map(|f| (f.definition.id.clone(), f.definition.name.clone()))
            .collect()
    });
    let steps = house.entities.with(|entities| {
        house.areas.with(|areas| {
            let ctx = Ctx {
                entities,
                areas,
                flows: &names,
            };
            run.steps
                .iter()
                .map(|s| {
                    let node = flow
                        .node(&s.node)
                        .map_or_else(|| s.node.clone(), |n| title(n, &ctx));
                    let outcome = match s.outcome {
                        StepOutcome::Ok => "ok",
                        StepOutcome::Failed => "failed",
                        _ => "skipped",
                    };
                    let outcome_class = match s.outcome {
                        StepOutcome::Ok => "step__outcome--ok",
                        StepOutcome::Failed => "step__outcome--failed",
                        StepOutcome::Skipped => "step__outcome--skipped",
                    };
                    let offset = step_offset(s.at, started);
                    let detail = (!s.detail.is_empty()).then(|| s.detail.clone());
                    view! {
                        <li class="step">
                            <span class="step__at" title=format_timestamp(s.at)>{offset}</span>
                            <span class="step__node">{node}</span>
                            <span class=format!("step__outcome {outcome_class}")>{outcome}</span>
                            {detail.map(|d| view! { <span class="step__detail">{d}</span> })}
                        </li>
                    }
                })
                .collect_view()
        })
    });
    let id = run.id.clone();
    let replayed = {
        let id = id.clone();
        move || replay.with(|r| r.as_deref() == Some(id.as_str()))
    };
    // A click on the run (not inside its step list) replays it, or stops the replay.
    let toggle = move |ev: web_sys::MouseEvent| {
        let in_steps = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .and_then(|el| el.closest(".run__steps").ok().flatten())
            .is_some();
        if !in_steps {
            replay.update(|r| {
                *r = if r.as_deref() == Some(id.as_str()) {
                    None
                } else {
                    Some(id.clone())
                };
            });
        }
    };
    view! {
        <li
            class=format!("run {}", status_class(run.status))
            class:run--replayed=replayed
            on:click=toggle
        >
            <details>
                <summary>
                    <span class="run__status">{status_word(run.status)}</span>
                    <span class="run__time" title=format_timestamp(started)>
                        {move || relative_time(started, now.0.get())}
                    </span>
                    {duration.map(|d| view! {
                        <span class="run__duration" title="Duration">{format!("· {d}")}</span>
                    })}
                    <span class="run__trigger">{trigger}</span>
                </summary>
                <ol class="run__steps">{steps}</ol>
            </details>
        </li>
    }
}

/// The drawer, closed by default; it overlays the canvas rather than pushing it. `replay` is
/// the run whose path the canvas shows. While `settings` holds, it shows `children` instead
/// of the runs.
#[component]
pub fn Drawer(
    open: RwSignal<bool>,
    runs: Signal<Vec<RunView>>,
    flow: Signal<Flow>,
    replay: RwSignal<Option<String>>,
    settings: Signal<bool>,
    children: ChildrenFn,
) -> impl IntoView {
    let list = move || {
        let runs = runs.get();
        if runs.is_empty() {
            return view! { <p class="flow-drawer__empty">"No runs yet."</p> }.into_any();
        }
        let flow = flow.get();
        view! {
            <ol class="run-list">
                {runs
                    .into_iter()
                    .map(|run| view! { <RunItem run=run flow=flow.clone() replay=replay /> })
                    .collect_view()}
            </ol>
        }
        .into_any()
    };
    view! {
        <aside
            class="flow-drawer"
            class:flow-drawer--open=move || open.get()
            aria-label=move || if settings.get() { "Card settings" } else { "Recent runs" }
            inert=move || !open.get()
        >
            {children()}
            <Show when=move || !settings.get()>
                <header class="flow-drawer__header">
                    <h2 class="flow-drawer__title">"Recent runs"</h2>
                    <button
                        type="button"
                        class="btn btn--ghost btn--icon flow-drawer__close"
                        aria-label="Close runs"
                        title="Close"
                        on:click=move |_| open.set(false)
                        inner_html=X_MARK
                    ></button>
                </header>
                {list}
            </Show>
        </aside>
    }
}
