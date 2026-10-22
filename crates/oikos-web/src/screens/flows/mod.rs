//! The Flows page: a top bar (search, sort, grouping, view, filter chips) over the flows as a
//! list or cards, grouped in foldable room sections.

mod bar;
mod card;
mod room_menu;
mod row;

use leptos::prelude::*;
use leptos_router::components::A;

use crate::flow_list::{
    self, Filters, FlowListPrefs, Grouping, ListRow, Section, ViewMode, decode_prefs, encode_prefs,
};
use crate::house::House;
use bar::FlowsBar;
use card::FlowCard;
use room_menu::RoomMenu;
use row::FlowRow;

/// Storage key for the remembered view, grouping, sort, filters and folded sections.
#[cfg(target_arch = "wasm32")]
const STORAGE_KEY: &str = "oikos.flowList";

/// `localStorage`, or `None` when it is missing, blocked or throws.
#[cfg(target_arch = "wasm32")]
fn storage() -> Option<web_sys::Storage> {
    web_sys::window().and_then(|w| w.local_storage().ok().flatten())
}

fn load_prefs() -> FlowListPrefs {
    #[cfg(target_arch = "wasm32")]
    {
        storage()
            .and_then(|s| s.get_item(STORAGE_KEY).ok().flatten())
            .map(|raw| decode_prefs(&raw))
            .unwrap_or_default()
    }
    #[cfg(not(target_arch = "wasm32"))]
    {
        let _ = decode_prefs;
        FlowListPrefs::default()
    }
}

#[cfg_attr(not(target_arch = "wasm32"), allow(unused_variables))]
fn save_prefs(prefs: &FlowListPrefs) {
    #[cfg(target_arch = "wasm32")]
    if let Some(s) = storage() {
        let _ = s.set_item(STORAGE_KEY, &encode_prefs(prefs));
    }
    #[cfg(not(target_arch = "wasm32"))]
    let _ = encode_prefs;
}

/// The ⋯ menu of a row: open the flow, or move it to a room.
fn menu_for(row: &ListRow) -> AnyView {
    let id = row.view.definition.id.clone();
    let href = format!("/flows/{}", crate::api::encode_id(&id));
    view! {
        <RoomMenu
            id=id
            href=href
            current=row.room.clone()
            invalid=row.view.invalid.is_some()
        />
    }
    .into_any()
}

/// The rows of one block, as a list panel or a card grid.
fn block(rows: Vec<ListRow>, mode: ViewMode) -> AnyView {
    match mode {
        ViewMode::List => view! {
            <ul class="flow-list">
                {rows.into_iter()
                    .map(|row| {
                        let menu = menu_for(&row);
                        view! { <FlowRow row=row menu=menu /> }
                    })
                    .collect_view()}
            </ul>
        }
        .into_any(),
        ViewMode::Cards => view! {
            <ul class="flow-grid">
                {rows.into_iter()
                    .map(|row| {
                        let menu = menu_for(&row);
                        view! { <FlowCard row=row menu=menu /> }
                    })
                    .collect_view()}
            </ul>
        }
        .into_any(),
    }
}

/// A foldable room section: header button (title, count, ⚠) over its block.
fn section_view(section: Section, mode: ViewMode, prefs: RwSignal<FlowListPrefs>) -> impl IntoView {
    let key = section.key.clone();
    let folded = {
        let key = key.clone();
        move || prefs.with(|p| p.folded.contains(&key))
    };
    let toggle = move |_| {
        prefs.update(|p| {
            if !p.folded.remove(&key) {
                p.folded.insert(key.clone());
            }
        });
    };
    let title = section.title.to_uppercase();
    let count = section.rows.len();
    let attention = section.attention;
    let rows = section.rows;
    let folded_for_hdr = folded.clone();
    let folded_for_warn = folded.clone();
    view! {
        <section class="flow-section">
            <button
                type="button"
                class="flow-section__head"
                aria-expanded=move || (!folded_for_hdr()).to_string()
                on:click=toggle
            >
                <span class="flow-section__caret" class:flow-section__caret--folded=folded.clone()>
                    "▾"
                </span>
                <span class="flow-section__title">{title}</span>
                <span class="flow-section__count">{count}</span>
                {move || {
                    (attention && folded_for_warn())
                        .then(|| {
                            view! {
                                <span
                                    class="flow-section__warn"
                                    title="A flow in this room needs attention"
                                >
                                    "⚠"
                                </span>
                            }
                        })
                }}
            </button>
            <Show when=move || !folded()>{block(rows.clone(), mode)}</Show>
        </section>
    }
}

#[component]
pub fn FlowsScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let prefs = RwSignal::new(load_prefs());
    // The search text is never remembered.
    let query = RwSignal::new(String::new());
    Effect::new(move |_| prefs.with(save_prefs));

    let all_rows = Memo::new(move |_| {
        house.flows.with(|flows| {
            house.areas.with(|areas| {
                house
                    .entities
                    .with(|entities| flow_list::rows(flows, areas, entities))
            })
        })
    });
    let counts = Signal::derive(move || {
        all_rows.with(|rows| {
            query.with(|q| prefs.with(|p| flow_list::status_counts(rows, q, &p.triggers)))
        })
    });
    let shown = Memo::new(move |_| {
        let filters = Filters {
            query: query.get(),
            status: prefs.with(|p| p.status),
            triggers: prefs.with(|p| p.triggers.clone()),
        };
        let mut rows = all_rows.with(|rows| flow_list::filter(rows, &filters));
        flow_list::sort_rows(&mut rows, prefs.with(|p| p.sort));
        rows
    });
    let clear_filters = move |_| {
        query.set(String::new());
        prefs.update(|p| {
            p.status = flow_list::StatusFilter::All;
            p.triggers.clear();
        });
    };
    let body = move || {
        let rows = shown.get();
        let (mode, grouping) = prefs.with(|p| (p.view, p.grouping));
        if rows.is_empty() {
            return view! {
                <div class="flow-list__empty">
                    <p>"No flows match"</p>
                    <button type="button" class="btn" on:click=clear_filters>
                        "Clear filters"
                    </button>
                </div>
            }
            .into_any();
        }
        match grouping {
            Grouping::None => block(rows, mode),
            Grouping::Room => {
                let sections = house.areas.with(|areas| flow_list::sections(rows, areas));
                sections
                    .into_iter()
                    .map(|s| section_view(s, mode, prefs))
                    .collect_view()
                    .into_any()
            }
        }
    };
    view! {
        <div class="flows">
            <header class="sheet__header">
                <div class="sheet__titles">
                    <h1 class="sheet__title">"Flows"</h1>
                </div>
                <A href="/flows/new" attr:class="btn flows-new">
                    "New flow"
                </A>
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
                <FlowsBar prefs=prefs query=query counts=counts />
                {body}
            </Show>
        </div>
    }
}
