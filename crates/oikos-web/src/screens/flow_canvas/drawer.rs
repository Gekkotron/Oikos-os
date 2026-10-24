//! The right drawer over the canvas: the recent runs, or what `children` holds (the card
//! library or the selected card's settings while editing). Clicking a run replays it on the
//! canvas.

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
    let trigger = house.entities.with(|e| trigger_line(&run, &flow, e));
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
                lang: crate::i18n::current_lang(),
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

/// What the side panel shows.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Panel {
    Closed,
    /// The cards to add (edit mode).
    Library,
    /// The selected card's settings (edit mode).
    Settings,
    Runs,
}

/// What the side panel shows: a selected card's settings beat the runs, which beat the
/// library; outside edit mode only the runs open it.
pub fn panel(editing: bool, card_selected: bool, runs_open: bool) -> Panel {
    if editing && card_selected {
        Panel::Settings
    } else if runs_open {
        Panel::Runs
    } else if editing {
        Panel::Library
    } else {
        Panel::Closed
    }
}

/// What the drawer holds: the open panel or, while closed, the one it showed last (the runs
/// if it never opened), so a closing panel slides out with its own content.
pub fn held(last: Option<Panel>, now: Panel) -> Panel {
    match now {
        Panel::Closed => last.unwrap_or(Panel::Runs),
        open => open,
    }
}

/// The drawer, open unless `panel` is [`Panel::Closed`]; it overlays the canvas rather than
/// pushing it. `replay` is the run whose path the canvas shows. When `held` is [`Panel::Runs`]
/// it lists the runs, whose close button clears `runs_open`; otherwise it shows `children`.
#[component]
pub fn Drawer(
    panel: Signal<Panel>,
    held: Signal<Panel>,
    runs_open: RwSignal<bool>,
    runs: Signal<Vec<RunView>>,
    flow: Signal<Flow>,
    replay: RwSignal<Option<String>>,
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
            class:flow-drawer--open=move || panel.get() != Panel::Closed
            aria-label=move || match held.get() {
                Panel::Library => "Card library",
                Panel::Settings => "Card settings",
                Panel::Runs | Panel::Closed => "Recent runs",
            }
            inert=move || panel.get() == Panel::Closed
        >
            {children()}
            <Show when=move || held.get() == Panel::Runs>
                <header class="flow-drawer__header">
                    <h2 class="flow-drawer__title">"Recent runs"</h2>
                    <button
                        type="button"
                        class="btn btn--ghost btn--icon flow-drawer__close"
                        aria-label="Close runs"
                        title="Close"
                        on:click=move |_| runs_open.set(false)
                        inner_html=X_MARK
                    ></button>
                </header>
                {list}
            </Show>
        </aside>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn panel_priorities() {
        assert_eq!(panel(false, false, false), Panel::Closed);
        assert_eq!(panel(false, true, false), Panel::Closed);
        assert_eq!(panel(false, false, true), Panel::Runs);
        assert_eq!(panel(true, false, false), Panel::Library);
        assert_eq!(panel(true, false, true), Panel::Runs);
        assert_eq!(panel(true, true, true), Panel::Settings);
    }

    #[test]
    fn held_keeps_the_last_open_panel_while_closed() {
        assert_eq!(held(None, Panel::Closed), Panel::Runs);
        assert_eq!(held(Some(Panel::Runs), Panel::Library), Panel::Library);
        assert_eq!(held(Some(Panel::Library), Panel::Closed), Panel::Library);
        assert_eq!(held(Some(Panel::Settings), Panel::Closed), Panel::Settings);
        assert_eq!(held(Some(Panel::Library), Panel::Runs), Panel::Runs);
    }
}
