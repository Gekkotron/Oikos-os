//! The "＋ Add" popover: a search box over the cards a flow can gain, grouped by role.

use leptos::ev;
use leptos::html;
use leptos::prelude::*;
use oikos_core::flow::NodeKind;
use wasm_bindgen::JsCast;

use crate::flow_picker::{Entry, Group, entries};
use crate::flow_text::Ctx;
use crate::house::House;

fn header(group: Group) -> &'static str {
    match group {
        Group::When => "WHEN · TRIGGER",
        Group::And => "AND · CHECK",
        Group::Then => "THEN · ACTION",
        Group::Wait => "WAIT",
        Group::Flow => "FLOW",
    }
}

/// The popover under the ＋ Add button, shown while `open`. Picking an entry calls `on_pick` and
/// closes it; Esc or a press outside `.flow-picker-wrap` closes it too.
#[component]
pub fn Picker(open: RwSignal<bool>, on_pick: Callback<NodeKind>) -> impl IntoView {
    let house = expect_context::<House>();
    let query = RwSignal::new(String::new());
    let highlight = RwSignal::new(0_usize);
    let input: NodeRef<html::Input> = NodeRef::new();

    let results = Memo::new(move |_| {
        let query = query.get();
        let names: Vec<(String, String)> = house.flows.with(|flows| {
            flows
                .iter()
                .map(|f| (f.definition.id.clone(), f.definition.name.clone()))
                .collect()
        });
        house.entities.with(|entities| {
            house.areas.with(|areas| {
                entries(
                    &query,
                    &Ctx {
                        entities,
                        areas,
                        flows: &names,
                    },
                )
            })
        })
    });

    // A fresh search each time it opens, with the box focused.
    Effect::new(move |_| {
        if open.get() {
            query.set(String::new());
            highlight.set(0);
            if let Some(el) = input.get() {
                let _ = el.focus();
            }
        }
    });
    // A new query starts again from the top.
    Effect::watch(move || query.get(), move |_, _, _| highlight.set(0), false);

    let pick = move |entry: Entry| {
        open.set(false);
        on_pick.run(entry.kind);
    };
    let pick_highlighted = move || {
        if let Some(entry) = results.with_untracked(|r| r.get(highlight.get_untracked()).cloned()) {
            pick(entry);
        }
    };
    let on_key = move |ev: ev::KeyboardEvent| {
        let last = results.with_untracked(Vec::len).saturating_sub(1);
        match ev.key().as_str() {
            "ArrowDown" => {
                ev.prevent_default();
                highlight.update(|i| *i = (*i + 1).min(last));
            }
            "ArrowUp" => {
                ev.prevent_default();
                highlight.update(|i| *i = i.saturating_sub(1));
            }
            "Enter" => {
                ev.prevent_default();
                pick_highlighted();
            }
            _ => {}
        }
    };

    // Esc and outside presses are heard on the window, wherever the focus is.
    let esc = window_event_listener(ev::keydown, move |ev| {
        if open.get_untracked() && ev.key() == "Escape" {
            open.set(false);
        }
    });
    let outside = window_event_listener(ev::pointerdown, move |ev| {
        if !open.get_untracked() {
            return;
        }
        let inside = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .is_some_and(|el| el.closest(".flow-picker-wrap").ok().flatten().is_some());
        if !inside {
            open.set(false);
        }
    });
    on_cleanup(move || {
        esc.remove();
        outside.remove();
    });

    let list = move || {
        let mut last = None;
        results
            .get()
            .into_iter()
            .enumerate()
            .map(|(i, entry)| {
                let head = (last != Some(entry.group)).then(|| {
                    view! { <li class="flow-picker__header" role="presentation">{header(entry.group)}</li> }
                });
                last = Some(entry.group);
                let label = entry.label.clone();
                view! {
                    {head}
                    <li
                        class="flow-picker__item"
                        class:flow-picker__item--on=move || highlight.get() == i
                        role="option"
                        aria-selected=move || (highlight.get() == i).to_string()
                        on:pointermove=move |_| highlight.set(i)
                        on:click=move |_| pick(entry.clone())
                    >
                        {label}
                    </li>
                }
            })
            .collect_view()
    };

    view! {
        <Show when=move || open.get()>
            <div class="flow-picker" role="dialog" aria-label="Add a card">
                <input
                    class="field flow-picker__search"
                    type="search"
                    placeholder="Search a card or an entity"
                    aria-label="Search cards"
                    node_ref=input
                    prop:value=move || query.get()
                    on:input=move |ev| query.set(event_target_value(&ev))
                    on:keydown=on_key
                />
                <ul class="flow-picker__list" role="listbox">
                    {list}
                    <Show when=move || results.with(Vec::is_empty)>
                        <li class="flow-picker__empty">"Nothing matches"</li>
                    </Show>
                </ul>
            </div>
        </Show>
    }
}
