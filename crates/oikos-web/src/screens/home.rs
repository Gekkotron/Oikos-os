//! The Home screen: room chips on top, the selected room's cards below.

use leptos::prelude::*;
use oikos_api::{AreaView, EntityView};
use oikos_core::model::{Kind, UNASSIGNED_AREA};

use crate::cards::alarm::AlarmPanelCard;
use crate::cards::binary_sensor::BinarySensorCard;
use crate::cards::button::ButtonCard;
use crate::cards::climate::ClimateCard;
use crate::cards::cover::CoverCard;
use crate::cards::light::LightCard;
use crate::cards::number::NumberCard;
use crate::cards::select::SelectCard;
use crate::cards::sensor::SensorCard;
use crate::cards::switch::SwitchCard;
use crate::cards::vacuum::VacuumCard;
use crate::cards::water_heater::WaterHeaterCard;
use crate::components::area_chip::AreaChip;
use crate::house::House;

/// The id of the virtual "Favorites" chip.
const FAVORITES: &str = "__favorites__";

/// The chips in the order they are shown: Favorites first (when `favorites` is non-zero), then
/// the areas exactly as the hub sent them, except that "Unassigned" moves to the end.
fn chips(areas: &[AreaView], favorites: usize) -> Vec<AreaView> {
    let mut chips = Vec::with_capacity(areas.len() + 1);
    if favorites > 0 {
        chips.push(AreaView {
            id: FAVORITES.into(),
            name: "Favorites".into(),
            icon: None,
            order: None,
            entity_count: favorites,
            primary_count: favorites,
        });
    }
    chips.extend(areas.iter().filter(|a| a.id != UNASSIGNED_AREA).cloned());
    chips.extend(areas.iter().filter(|a| a.id == UNASSIGNED_AREA).cloned());
    chips
}

/// The ids of the primary entities to show under `chip`, in display order: the hub's `order`
/// override first, then by name.
fn entity_ids<'a>(entities: impl Iterator<Item = &'a EntityView>, chip: &str) -> Vec<String> {
    let mut shown: Vec<&EntityView> = entities
        .filter(|e| e.is_primary())
        .filter(|e| {
            if chip == FAVORITES {
                e.ui.favorite
            } else {
                e.area == chip
            }
        })
        .collect();
    shown.sort_by_cached_key(|e| (e.ui.order.unwrap_or(i32::MAX), e.name.to_lowercase()));
    shown.into_iter().map(|e| e.id.clone()).collect()
}

/// The card component for an entity's kind.
fn card_for(entity: EntityView) -> AnyView {
    match entity.kind {
        Kind::Light => view! { <LightCard entity=entity /> }.into_any(),
        Kind::Switch => view! { <SwitchCard entity=entity /> }.into_any(),
        Kind::Cover => view! { <CoverCard entity=entity /> }.into_any(),
        Kind::Climate => view! { <ClimateCard entity=entity /> }.into_any(),
        Kind::Sensor => view! { <SensorCard entity=entity /> }.into_any(),
        Kind::BinarySensor => view! { <BinarySensorCard entity=entity /> }.into_any(),
        Kind::Number => view! { <NumberCard entity=entity /> }.into_any(),
        Kind::Select => view! { <SelectCard entity=entity /> }.into_any(),
        Kind::Button => view! { <ButtonCard entity=entity /> }.into_any(),
        Kind::AlarmPanel => view! { <AlarmPanelCard entity=entity /> }.into_any(),
        Kind::Vacuum => view! { <VacuumCard entity=entity /> }.into_any(),
        Kind::WaterHeater => view! { <WaterHeaterCard entity=entity /> }.into_any(),
    }
}

#[component]
pub fn HomeScreen() -> impl IntoView {
    let house = expect_context::<House>();

    // Memos only notify when their value changes, so a state echo leaves chips and the card list
    // alone and repaints just the one card that changed.
    let favorites = Memo::new(move |_| {
        house.entities.with(|m| {
            m.values()
                .filter(|e| e.is_primary() && e.ui.favorite)
                .count()
        })
    });
    let chip_list = Memo::new(move |_| house.areas.with(|a| chips(a, favorites.get())));
    let selected = RwSignal::new(None::<String>);
    // The chosen chip, or the first one while nothing (valid) is chosen.
    let current = Memo::new(move |_| {
        chip_list.with(|list| {
            selected
                .get()
                .filter(|id| list.iter().any(|c| c.id == *id))
                .or_else(|| list.first().map(|c| c.id.clone()))
        })
    });
    let ids = Memo::new(move |_| {
        current.get().map_or_else(Vec::new, |chip| {
            house.entities.with(|m| entity_ids(m.values(), &chip))
        })
    });
    let loaded = Memo::new(move |_| {
        house.areas.with(|a| !a.is_empty())
            || house.entities.with(|m| !m.is_empty())
            || matches!(house.connection.get(), oikos_api::ConnectionStatus::Online)
    });
    let title = move || {
        chip_list.with(|list| {
            current
                .get()
                .and_then(|id| list.iter().find(|c| c.id == id).map(|c| c.name.clone()))
                .unwrap_or_default()
        })
    };

    let on_chip = Callback::new(move |id: String| selected.set(Some(id)));

    view! {
        <Show
            when=move || loaded.get()
            fallback=|| {
                view! {
                    <div class="grid" aria-busy="true">
                        <div class="card card--skeleton"></div>
                        <div class="card card--skeleton"></div>
                        <div class="card card--skeleton"></div>
                        <div class="card card--skeleton"></div>
                    </div>
                }
            }
        >
            <nav class="chips" aria-label="Rooms">
                <For
                    each=move || chip_list.get()
                    key=|c| (c.id.clone(), c.name.clone(), c.primary_count)
                    children=move |area| {
                        let id = area.id.clone();
                        let is_selected = Signal::derive(move || {
                            current.with(|c| c.as_deref() == Some(id.as_str()))
                        });
                        view! { <AreaChip area=area selected=is_selected on_click=on_chip /> }
                    }
                />
            </nav>
            <section class="room">
                <h1 class="room__title">{title}</h1>
                <Show
                    when=move || ids.with(|v| !v.is_empty())
                    fallback=|| view! { <p class="room__empty">"Nothing here yet."</p> }
                >
                    <div class="grid">
                        <For
                            each=move || ids.get()
                            key=|id| id.clone()
                            children=move |id| {
                                // One memo per card: it only changes when this entity does.
                                let entity = Memo::new(move |_| {
                                    house.entities.with(|m| m.get(&id).cloned())
                                });
                                view! { {move || entity.get().map(card_for)} }
                            }
                        />
                    </div>
                </Show>
            </section>
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use super::{FAVORITES, chips};
    use oikos_api::AreaView;

    fn area(id: &str) -> AreaView {
        AreaView {
            id: id.into(),
            name: id.into(),
            icon: None,
            order: None,
            entity_count: 1,
            primary_count: 1,
        }
    }

    fn ids(list: &[AreaView]) -> Vec<&str> {
        list.iter().map(|a| a.id.as_str()).collect()
    }

    #[test]
    fn keeps_hub_order_and_moves_unassigned_last() {
        let areas = [area("zeta"), area("Unassigned"), area("alpha")];
        assert_eq!(ids(&chips(&areas, 0)), ["zeta", "alpha", "Unassigned"]);
    }

    #[test]
    fn favorites_chip_comes_first_when_any() {
        let areas = [area("kitchen")];
        assert_eq!(ids(&chips(&areas, 2)), [FAVORITES, "kitchen"]);
        assert_eq!(ids(&chips(&areas, 0)), ["kitchen"]);
    }
}
