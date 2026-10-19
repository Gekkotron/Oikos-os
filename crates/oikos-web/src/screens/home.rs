//! The Home screen: room chips on top, the selected room's cards below.

use leptos::prelude::*;
use oikos_api::{AreaView, EntityView};
use oikos_core::model::{Kind, Placement, UNASSIGNED_AREA};
use std::collections::{BTreeMap, HashSet};

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
use crate::components::alert_row::RecentAlerts;
use crate::components::area_chip::AreaChip;
use crate::components::room_title::RoomTitle;
use crate::house::House;
use crate::nav::HomeRoom;

/// The id of the virtual "Favorites" chip.
const FAVORITES: &str = "__favorites__";

/// True when Home shows `e`: a primary entity that the app's placement rule keeps on Home (an
/// entity with an energy role or shown on Security leaves Home unless it is pinned).
pub(crate) fn shows_on_home(e: &EntityView) -> bool {
    e.is_primary() && Placement::on_home(e.kind, e.device_class.as_deref(), &e.ui)
}

/// How many entities Home shows per area id.
fn home_counts<'a>(entities: impl Iterator<Item = &'a EntityView>) -> BTreeMap<String, usize> {
    let mut counts = BTreeMap::new();
    for e in entities.filter(|e| shows_on_home(e)) {
        *counts.entry(e.area.clone()).or_insert(0) += 1;
    }
    counts
}

/// The chips in the order they are shown: Favorites first (when `favorites` is non-zero), then
/// the areas exactly as the hub sent them, except that "Unassigned" moves to the end. Each chip
/// carries the count of entities Home shows there (`counts`), not the hub's primary count.
fn chips(areas: &[AreaView], favorites: usize, counts: &BTreeMap<String, usize>) -> Vec<AreaView> {
    let mut chips = Vec::with_capacity(areas.len() + 1);
    if favorites > 0 {
        chips.push(AreaView {
            id: FAVORITES.into(),
            name: "Favorites".into(),
            icon: None,
            order: None,
            entity_count: favorites,
            primary_count: favorites,
            group_names: Default::default(),
            group_favorites: Default::default(),
        });
    }
    let counted = |a: &AreaView| AreaView {
        primary_count: counts.get(&a.id).copied().unwrap_or(0),
        ..a.clone()
    };
    chips.extend(
        areas
            .iter()
            .filter(|a| a.id != UNASSIGNED_AREA)
            .map(counted),
    );
    chips.extend(
        areas
            .iter()
            .filter(|a| a.id == UNASSIGNED_AREA)
            .map(counted),
    );
    chips
}

/// The ids of the primary entities to show under `chip`, in display order: the hub's `order`
/// override first, then by name.
fn entity_ids<'a>(entities: impl Iterator<Item = &'a EntityView>, chip: &str) -> Vec<String> {
    let mut shown: Vec<&EntityView> = entities
        .filter(|e| shows_on_home(e))
        .filter(|e| {
            if chip == FAVORITES {
                e.ui.favorite
            } else {
                e.area == chip
            }
        })
        .collect();
    shown.sort_by_cached_key(|e| (e.ui.order.unwrap_or(i32::MAX), e.name.to_lowercase()));
    without_paired_humidity(shown)
        .into_iter()
        .map(|e| e.id.clone())
        .collect()
}

fn is_sensor_of_class(e: &EntityView, class: &str) -> bool {
    e.kind == Kind::Sensor && e.device_class.as_deref() == Some(class)
}

/// `shown` without the humidity sensors whose device's temperature sensor is also in `shown`:
/// the temperature card carries the humidity reading.
fn without_paired_humidity(mut shown: Vec<&EntityView>) -> Vec<&EntityView> {
    let with_temperature: HashSet<&str> = shown
        .iter()
        .filter(|e| is_sensor_of_class(e, "temperature"))
        .filter_map(|e| e.device_id.as_deref())
        .collect();
    shown.retain(|e| {
        !(is_sensor_of_class(e, "humidity")
            && e.device_id
                .as_deref()
                .is_some_and(|d| with_temperature.contains(d)))
    });
    shown
}

/// The card component for an entity's kind.
pub(crate) fn card_for(entity: EntityView) -> AnyView {
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
                .filter(|e| shows_on_home(e) && e.ui.favorite)
                .count()
        })
    });
    let counts = Memo::new(move |_| house.entities.with(|m| home_counts(m.values())));
    let chip_list = Memo::new(move |_| {
        house
            .areas
            .with(|a| counts.with(|c| chips(a, favorites.get(), c)))
    });
    // Kept above the router, so coming back from a sheet keeps the room.
    let selected = expect_context::<HomeRoom>().0;
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
    let title = Signal::derive(move || {
        chip_list.with(|list| {
            current
                .get()
                .and_then(|id| list.iter().find(|c| c.id == id).map(|c| c.name.clone()))
                .unwrap_or_default()
        })
    });
    // The room the title can rename: not Favorites, not Unassigned.
    let room = Signal::derive(move || {
        current
            .get()
            .filter(|id| id != FAVORITES && id != UNASSIGNED_AREA)
    });

    let on_chip = Callback::new(move |id: String| selected.set(Some(id)));

    view! {
        <Show
            when=move || loaded.get()
            fallback=|| {
                view! {
                    <div class="grid grid--home" aria-busy="true">
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
                <RoomTitle id=room name=title />
                <Show
                    when=move || ids.with(|v| !v.is_empty())
                    fallback=|| view! { <p class="room__empty">"Nothing here yet."</p> }
                >
                    <div class="grid grid--home">
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
            <RecentAlerts />
        </Show>
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeMap;

    use super::{FAVORITES, chips, entity_ids, home_counts};
    use oikos_api::{AreaView, EntityView};
    use oikos_core::model::{
        Capabilities, EntityCategory, EntityState, Kind, UNASSIGNED_AREA, UiOverride,
    };

    fn entity(id: &str, kind: Kind, device_class: Option<&str>, ui: UiOverride) -> EntityView {
        EntityView {
            id: id.into(),
            device_id: None,
            name: id.into(),
            device_name: None,
            label: None,
            kind,
            protocol_kind: kind,
            device_class: device_class.map(Into::into),
            unit: None,
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: "living".into(),
            part_of: None,
            capabilities: Capabilities::default(),
            ui,
            state: EntityState::default(),
        }
    }

    fn role(role: &str, pinned: bool, hidden: bool) -> UiOverride {
        UiOverride {
            energy_role: Some(role.into()),
            home_pinned: pinned,
            hidden,
            ..Default::default()
        }
    }

    #[test]
    fn energy_and_security_entities_leave_home() {
        let all = [
            entity("lamp", Kind::Light, None, UiOverride::default()),
            entity(
                "grid",
                Kind::Sensor,
                Some("power"),
                role("grid", false, false),
            ),
            entity(
                "solar",
                Kind::Sensor,
                Some("power"),
                role("solar", true, false),
            ),
            entity(
                "hidden",
                Kind::Sensor,
                Some("power"),
                role("grid", true, true),
            ),
            entity(
                "door",
                Kind::BinarySensor,
                Some("door"),
                UiOverride::default(),
            ),
            entity(
                "siren",
                Kind::Switch,
                None,
                UiOverride {
                    security_action: true,
                    ..Default::default()
                },
            ),
            entity("plug", Kind::Switch, None, UiOverride::default()),
        ];
        assert_eq!(entity_ids(all.iter(), "living"), ["lamp", "plug", "solar"]);
    }

    fn on_device(mut e: EntityView, device: &str) -> EntityView {
        e.device_id = Some(device.into());
        e
    }

    fn thermometer_parts(prefix: &str, device: &str) -> [EntityView; 2] {
        [
            on_device(
                entity(
                    &format!("{prefix}_t"),
                    Kind::Sensor,
                    Some("temperature"),
                    UiOverride::default(),
                ),
                device,
            ),
            on_device(
                entity(
                    &format!("{prefix}_h"),
                    Kind::Sensor,
                    Some("humidity"),
                    UiOverride::default(),
                ),
                device,
            ),
        ]
    }

    #[test]
    fn humidity_is_folded_into_its_temperature_sensor() {
        let [t, h] = thermometer_parts("th", "dev");
        let lamp = entity("lamp", Kind::Light, None, UiOverride::default());
        let all = [t, h, lamp];
        assert_eq!(entity_ids(all.iter(), "living"), ["lamp", "th_t"]);
    }

    #[test]
    fn humidity_stays_without_a_temperature_on_the_same_device() {
        let [t, _] = thermometer_parts("a", "dev_a");
        let [_, h] = thermometer_parts("b", "dev_b");
        let all = [t, h];
        assert_eq!(entity_ids(all.iter(), "living"), ["a_t", "b_h"]);
    }

    #[test]
    fn humidity_without_device_id_stays() {
        let t = entity(
            "t",
            Kind::Sensor,
            Some("temperature"),
            UiOverride::default(),
        );
        let h = entity("h", Kind::Sensor, Some("humidity"), UiOverride::default());
        let all = [t, h];
        assert_eq!(entity_ids(all.iter(), "living"), ["h", "t"]);
    }

    #[test]
    fn favorite_humidity_stays_when_its_temperature_is_not_a_favorite() {
        let [t, mut h] = thermometer_parts("th", "dev");
        h.ui.favorite = true;
        let all = [t, h];
        assert_eq!(entity_ids(all.iter(), FAVORITES), ["th_h"]);
    }

    #[test]
    fn chip_counts_follow_home_rule() {
        let mut outside = entity(
            "door",
            Kind::BinarySensor,
            Some("door"),
            UiOverride::default(),
        );
        outside.area = UNASSIGNED_AREA.into();
        let all = [
            entity("lamp", Kind::Light, None, UiOverride::default()),
            entity(
                "grid",
                Kind::Sensor,
                Some("power"),
                role("grid", false, false),
            ),
            outside,
        ];
        let counts = home_counts(all.iter());
        assert_eq!(counts.get("living"), Some(&1));
        assert_eq!(counts.get(UNASSIGNED_AREA), None);
        let areas = [area("living"), area(UNASSIGNED_AREA)];
        let list = chips(&areas, 0, &counts);
        assert_eq!(list[0].primary_count, 1);
        assert_eq!(list[1].primary_count, 0);
    }

    fn area(id: &str) -> AreaView {
        AreaView {
            id: id.into(),
            name: id.into(),
            icon: None,
            order: None,
            entity_count: 1,
            primary_count: 1,
            group_names: Default::default(),
            group_favorites: Default::default(),
        }
    }

    fn ids(list: &[AreaView]) -> Vec<&str> {
        list.iter().map(|a| a.id.as_str()).collect()
    }

    #[test]
    fn keeps_hub_order_and_moves_unassigned_last() {
        let areas = [area("zeta"), area("Unassigned"), area("alpha")];
        assert_eq!(
            ids(&chips(&areas, 0, &BTreeMap::new())),
            ["zeta", "alpha", "Unassigned"]
        );
    }

    #[test]
    fn favorites_chip_comes_first_when_any() {
        let areas = [area("kitchen")];
        let none = BTreeMap::new();
        assert_eq!(ids(&chips(&areas, 2, &none)), [FAVORITES, "kitchen"]);
        assert_eq!(ids(&chips(&areas, 0, &none)), ["kitchen"]);
    }
}
