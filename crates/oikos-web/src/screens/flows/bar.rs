//! The top bar of the Flows page: search, sort, grouping, view toggle and the filter chips.

use leptos::prelude::*;

use crate::flow_list::{
    FlowListPrefs, Grouping, Sort, StatusCounts, StatusFilter, TriggerKind, ViewMode,
};

/// A small segmented control: one button per option, the current one pressed.
#[component]
fn Segmented<T>(
    label: &'static str,
    options: Vec<(T, &'static str)>,
    current: Signal<T>,
    on_pick: Callback<T>,
) -> impl IntoView
where
    T: Copy + PartialEq + Send + Sync + 'static,
{
    let buttons = options
        .into_iter()
        .map(|(value, text)| {
            view! {
                <button
                    type="button"
                    class="seg__btn"
                    class:seg__btn--on=move || current.get() == value
                    aria-pressed=move || (current.get() == value).to_string()
                    on:click=move |_| on_pick.run(value)
                >
                    {text}
                </button>
            }
        })
        .collect_view();
    view! {
        <div class="seg" role="group" aria-label=label>
            {buttons}
        </div>
    }
}

const TRIGGER_CHIPS: [(TriggerKind, &str); 5] = [
    (TriggerKind::State, "Device state"),
    (TriggerKind::Schedule, "Schedule"),
    (TriggerKind::Sun, "Sun"),
    (TriggerKind::Signal, "Button / event"),
    (TriggerKind::Manual, "Manual"),
];

#[component]
pub fn FlowsBar(
    prefs: RwSignal<FlowListPrefs>,
    query: RwSignal<String>,
    counts: Signal<StatusCounts>,
) -> impl IntoView {
    let status_chip =
        move |status: StatusFilter, label: &'static str, count: fn(&StatusCounts) -> usize| {
            view! {
                <button
                    type="button"
                    class="chip"
                    class:chip--selected=move || prefs.with(|p| p.status == status)
                    aria-pressed=move || prefs.with(|p| p.status == status).to_string()
                    on:click=move |_| prefs.update(|p| p.status = status)
                >
                    {label}
                    <span class="chip-badge">{move || counts.with(count)}</span>
                </button>
            }
        };
    let trigger_chips = TRIGGER_CHIPS
        .into_iter()
        .map(|(kind, label)| {
            let on = move || prefs.with(|p| p.triggers.contains(&kind));
            view! {
                <button
                    type="button"
                    class="chip"
                    class:chip--selected=on
                    aria-pressed=move || on().to_string()
                    on:click=move |_| {
                        prefs.update(|p| {
                            if !p.triggers.remove(&kind) {
                                p.triggers.insert(kind);
                            }
                        });
                    }
                >
                    {label}
                </button>
            }
        })
        .collect_view();
    view! {
        <div class="flow-bar">
            <input
                type="search"
                class="flow-bar__search"
                placeholder="Search flows, devices, rooms…"
                aria-label="Search flows"
                prop:value=move || query.get()
                on:input=move |ev| query.set(event_target_value(&ev))
            />
            <span class="flow-bar__label">"Sort"</span>
            <Segmented
                label="Sort"
                options=vec![(Sort::Name, "Name"), (Sort::LastRun, "Last run")]
                current=Signal::derive(move || prefs.with(|p| p.sort))
                on_pick=Callback::new(move |v| prefs.update(|p| p.sort = v))
            />
            <span class="flow-bar__label">"Group"</span>
            <Segmented
                label="Group"
                options=vec![(Grouping::Room, "Room"), (Grouping::None, "None")]
                current=Signal::derive(move || prefs.with(|p| p.grouping))
                on_pick=Callback::new(move |v| prefs.update(|p| p.grouping = v))
            />
            <Segmented
                label="View"
                options=vec![(ViewMode::List, "List"), (ViewMode::Cards, "Cards")]
                current=Signal::derive(move || prefs.with(|p| p.view))
                on_pick=Callback::new(move |v| prefs.update(|p| p.view = v))
            />
        </div>
        <div class="flow-chips">
            <div class="flow-chips__set" role="group" aria-label="Status">
                {status_chip(StatusFilter::All, "All", |c| c.all)}
                {status_chip(StatusFilter::On, "On", |c| c.on)}
                {status_chip(StatusFilter::Off, "Off", |c| c.off)}
                {status_chip(StatusFilter::Attention, "⚠ Needs attention", |c| c.attention)}
            </div>
            <div class="flow-chips__set" role="group" aria-label="Trigger type">
                {trigger_chips}
            </div>
        </div>
    }
}
