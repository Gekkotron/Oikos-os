//! A room's page: its title and the cards Home shows there. The overview stacks these sections.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use oikos_core::model::UNASSIGNED_AREA;

use crate::components::room_title::RoomTitle;
use crate::groups::{HomeItem, home_items};
use crate::house::House;
use crate::i18n::{Key, use_text};
use crate::rooms::{FAVORITES, use_group_refresh, use_room_chips};
use crate::screens::home::card_for;

/// The placeholder cards shown until the house has said anything.
#[component]
pub(crate) fn RoomsSkeleton() -> impl IntoView {
    view! {
        <div class="grid grid--home" aria-busy="true">
            <div class="card card--skeleton"></div>
            <div class="card card--skeleton"></div>
            <div class="card card--skeleton"></div>
            <div class="card card--skeleton"></div>
        </div>
    }
}

/// One room (or the Favorites chip): a renamable title and its cards.
#[component]
pub(crate) fn RoomSection(id: String) -> impl IntoView {
    let house = expect_context::<House>();
    let chip = id.clone();
    let items = Memo::new(move |_| {
        house.areas.with(|a| {
            house
                .entities
                .with(|m| house.groups.with(|g| home_items(m.values(), a, &chip, g)))
        })
    });
    let title = {
        let id = id.clone();
        Signal::derive(move || {
            if id == FAVORITES {
                return "Favorites".to_owned();
            }
            house.areas.with(|a| {
                a.iter()
                    .find(|a| a.id == id)
                    .map(|a| a.name.clone())
                    .unwrap_or_default()
            })
        })
    };
    // The room the title can rename: not Favorites, not Unassigned.
    let renamable = (id != FAVORITES && id != UNASSIGNED_AREA).then_some(id);

    view! {
        <section class="room">
            <RoomTitle id=renamable name=title />
            <Show
                when=move || items.with(|v| !v.is_empty())
                fallback=|| view! { <p class="room__empty">"Nothing here yet."</p> }
            >
                <div class="grid grid--home">
                    <For
                        each=move || items.get()
                        key=|item| item.clone()
                        children=move |item| match item {
                            HomeItem::Entity(id) => {
                                // One memo per card: it only changes when this entity does.
                                let entity = Memo::new(move |_| {
                                    house.entities.with(|m| m.get(&id).cloned())
                                });
                                view! { {move || entity.get().map(card_for)} }.into_any()
                            }
                        }
                    />
                </div>
            </Show>
        </section>
    }
}

/// `/rooms/:id`: one room, or a note that it is gone once the areas have loaded.
#[component]
pub fn RoomScreen() -> impl IntoView {
    let t = use_text();
    let params = use_params_map();
    let id = Memo::new(move |_| params.read().get("id").unwrap_or_default());
    let chips = use_room_chips();
    let house = expect_context::<House>();
    // The room pages wait for the areas: until they arrive, no id can be called gone.
    let loaded = Memo::new(move |_| house.areas.with(|a| !a.is_empty()));
    use_group_refresh();
    // A room is any area the hub knows (even one with nothing on Home) or a pseudo-room with a
    // chip (Favorites). Only flips when it appears or goes, so a state echo never rebuilds the page.
    let known = Memo::new(move |_| {
        id.with(|id| {
            house.areas.with(|a| a.iter().any(|a| a.id == *id))
                || chips.with(|list| list.iter().any(|c| c.id == *id))
        })
    });

    view! {
        <Show when=move || loaded.get() fallback=|| view! { <RoomsSkeleton /> }>
            {move || {
                if known.get() {
                    view! { <RoomSection id=id.get() /> }.into_any()
                } else {
                    view! { <p class="room__gone">{move || t(Key::RoomGone)}</p> }.into_any()
                }
            }}
        </Show>
    }
}
