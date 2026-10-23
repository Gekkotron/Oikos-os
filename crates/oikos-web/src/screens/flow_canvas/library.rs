//! The card library in the side panel while editing: a search box over the cards a flow can
//! gain, grouped by role, each row shaped like the card it adds.

use leptos::ev;
use leptos::prelude::*;
use oikos_core::flow::NodeKind;

use crate::flow_picker::{Entry, Group, entries};
use crate::flow_text::{Ctx, Role, role};
use crate::house::House;
use crate::icons::node_icon;

/// The id of the card just added from the library, which glows for a moment so it can be
/// found on the canvas. Provided by the canvas.
#[derive(Clone, Copy)]
pub struct Fresh(pub RwSignal<Option<String>>);

fn header(group: Group) -> &'static str {
    match group {
        Group::When => "WHEN · TRIGGER",
        Group::And => "AND · CHECK",
        Group::Then => "THEN · ACTION",
        Group::Wait => "WAIT",
        Group::Flow => "FLOW",
    }
}

/// The role colour of a section header.
fn header_role(group: Group) -> Role {
    match group {
        Group::When => Role::Trigger,
        Group::And => Role::Check,
        Group::Then | Group::Flow => Role::Action,
        Group::Wait => Role::Wait,
    }
}

fn role_suffix(role: Role) -> &'static str {
    match role {
        Role::Trigger => "trigger",
        Role::Check => "check",
        Role::Action => "action",
        Role::Wait => "wait",
    }
}

/// The library. Clicking a row, or Enter on the highlighted one while the search box has the
/// focus, calls `on_add`; the library stays as it is so several cards can go in a row. Esc in
/// the search box clears it.
#[component]
pub fn Library(on_add: Callback<NodeKind>) -> impl IntoView {
    let house = expect_context::<House>();
    let query = RwSignal::new(String::new());
    let highlight = RwSignal::new(0_usize);

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

    // A new query starts again from the top.
    Effect::watch(move || query.get(), move |_, _, _| highlight.set(0), false);

    let add = move |entry: Entry| on_add.run(entry.kind);
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
                if let Some(entry) =
                    results.with_untracked(|r| r.get(highlight.get_untracked()).cloned())
                {
                    add(entry);
                }
            }
            "Escape" => query.set(String::new()),
            _ => {}
        }
    };

    let list = move || {
        let mut last = None;
        results
            .get()
            .into_iter()
            .enumerate()
            .map(|(i, entry)| {
                let head = (last != Some(entry.group)).then(|| {
                    let class = format!(
                        "flow-library__header flow-library__header--{}",
                        role_suffix(header_role(entry.group))
                    );
                    view! { <li class=class role="presentation">{header(entry.group)}</li> }
                });
                last = Some(entry.group);
                let class = format!(
                    "flow-library__item flow-library__item--{}",
                    role_suffix(role(&entry.kind))
                );
                let icon = node_icon(&entry.kind);
                let label = entry.label.clone();
                let sub = entry.sub.clone();
                view! {
                    {head}
                    <li
                        class=class
                        class:flow-library__item--on=move || highlight.get() == i
                        role="option"
                        aria-selected=move || (highlight.get() == i).to_string()
                        on:pointermove=move |_| highlight.set(i)
                        on:click=move |_| add(entry.clone())
                    >
                        <span class="flow-library__icon" inner_html=icon></span>
                        <span class="flow-library__text">
                            <span class="flow-library__title">{label}</span>
                            {sub.map(|s| view! { <span class="flow-library__sub">{s}</span> })}
                        </span>
                        <span class="flow-library__grip" aria-hidden="true">"⋮⋮"</span>
                    </li>
                }
            })
            .collect_view()
    };

    view! {
        <section class="flow-library" aria-label="Add a card">
            <header class="flow-drawer__header">
                <h2 class="flow-drawer__title">"Add a card"</h2>
            </header>
            <input
                class="field flow-library__search"
                type="search"
                placeholder="Search a card or an entity"
                aria-label="Search cards"
                prop:value=move || query.get()
                on:input=move |ev| query.set(event_target_value(&ev))
                on:keydown=on_key
            />
            <ul class="flow-library__list" role="listbox">
                {list}
                <Show when=move || results.with(Vec::is_empty)>
                    <li class="flow-library__empty">"Nothing matches"</li>
                </Show>
            </ul>
        </section>
    }
}
