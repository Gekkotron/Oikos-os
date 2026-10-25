//! A room's page: its title and the cards Home shows there. The overview stacks these sections.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use oikos_core::model::UNASSIGNED_AREA;

use crate::api::encode_id;
use crate::components::card_place::{CardPlace, provide_card_place};
use crate::components::room_title::RoomTitle;
use crate::format::area_name;
use crate::groups::{HomeItem, home_items};
use crate::house::House;
use crate::i18n::settings::{SettingsText, settings_text};
use crate::i18n::{Key, use_lang, use_text};
use crate::nav::use_back_anchor;
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

/// One room (or the Favorites chip): a renamable title and its cards. On the overview
/// (`anchored`) the section carries the id `room-<encoded id>`, and a click inside it makes Back
/// from the sheet it opens return to this room rather than the top of the page.
#[component]
pub(crate) fn RoomSection(id: String, #[prop(optional)] anchored: bool) -> impl IntoView {
    let house = expect_context::<House>();
    let anchor = anchored.then(|| format!("room-{}", encode_id(&id)));
    let mark = use_back_anchor();
    let on_click = {
        let anchor = anchor.clone();
        // Runs before the back stack records the sheet: that waits for an effect.
        move |_| {
            if let Some(anchor) = &anchor {
                mark(anchor);
            }
        }
    };
    // Under its own heading a card does not repeat the room; Favorites mixes rooms.
    if id != FAVORITES {
        provide_card_place(CardPlace::Room);
    }
    let lang = use_lang();
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
                return settings_text(lang(), SettingsText::Favorites).to_owned();
            }
            house.areas.with(|a| {
                a.iter()
                    .find(|a| a.id == id)
                    .map(|a| area_name(lang(), &a.id, &a.name))
                    .unwrap_or_default()
            })
        })
    };
    // The room the title can rename: not Favorites, not Unassigned.
    let renamable = (id != FAVORITES && id != UNASSIGNED_AREA).then_some(id);

    view! {
        <section class="room" id=anchor on:click=on_click>
            <RoomTitle id=renamable name=title />
            <Show
                when=move || items.with(|v| !v.is_empty())
                fallback=move || {
                    view! {
                        <p class="room__empty">
                            {move || settings_text(lang(), SettingsText::NothingHere)}
                        </p>
                    }
                }
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
    // The room on screen: `Some(id)` for any area the hub knows (even one with nothing on Home)
    // or a pseudo-room with a chip (Favorites), `None` once it is gone. One memo carrying the id,
    // so a room-to-room link rebuilds the page (a bare known/gone flag stays `true` and the page
    // kept the first room), yet a card's state echo never does.
    let room = Memo::new(move |_| {
        let id = id.get();
        let known = house.areas.with(|a| a.iter().any(|a| a.id == id))
            || chips.with(|list| list.iter().any(|c| c.id == id));
        known.then_some(id)
    });

    view! {
        <Show when=move || loaded.get() fallback=|| view! { <RoomsSkeleton /> }>
            {move || match room.get() {
                Some(id) => view! { <RoomSection id=id /> }.into_any(),
                None => view! { <p class="room__gone">{move || t(Key::RoomGone)}</p> }.into_any(),
            }}
        </Show>
    }
}
