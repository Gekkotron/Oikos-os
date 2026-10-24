//! Entity pickers for the climate settings: one entity, or a list of them.

use std::collections::BTreeMap;

use leptos::prelude::*;

use oikos_api::{AreaView, EntityView};
use oikos_core::model::Kind;

use crate::format::{list_label, room_name};
use crate::house::House;
use crate::i18n::Lang;
use crate::i18n::climate::{ClimateText, automatic_with, climate_text};
use crate::i18n::use_lang;

/// Every entity as `(id, label)`, sorted by label.
fn options(house: House, lang: Lang) -> Vec<(String, String)> {
    let mut list: Vec<(String, String)> = house.entities.with(|m| {
        m.values()
            .map(|e| (e.id.clone(), list_label(lang, e)))
            .collect()
    });
    list.sort_by_key(|(_, label)| label.to_lowercase());
    list
}

/// Whether `entity` reads a temperature.
fn is_thermometer(entity: &EntityView) -> bool {
    entity.kind == Kind::Sensor && entity.device_class.as_deref() == Some("temperature")
}

/// Options as `(id, label)` under the name of their room, `None` outside any room.
type RoomGroup = (Option<String>, Vec<(String, String)>);

/// The thermometers grouped by room: rooms by name, the ones outside any room last, each
/// group sorted by label.
fn thermometers_by_room(
    lang: Lang,
    entities: &BTreeMap<String, EntityView>,
    areas: &[AreaView],
) -> Vec<RoomGroup> {
    let mut groups: BTreeMap<(bool, String), RoomGroup> = BTreeMap::new();
    for e in entities.values().filter(|e| is_thermometer(e)) {
        let room = room_name(areas, &e.area);
        let key = (
            room.is_none(),
            room.as_deref().unwrap_or_default().to_lowercase(),
        );
        groups
            .entry(key)
            .or_insert_with(|| (room, Vec::new()))
            .1
            .push((e.id.clone(), list_label(lang, e)));
    }
    groups
        .into_values()
        .map(|(room, mut list)| {
            list.sort_by_key(|(_, label)| label.to_lowercase());
            (room, list)
        })
        .collect()
}

/// The label of `id` followed by its room, or the id itself when the house no longer has it.
fn label_with_room(house: House, lang: Lang, id: &str) -> String {
    house.entities.with(|m| match m.get(id) {
        Some(e) => match house.areas.with(|a| room_name(a, &e.area)) {
            Some(room) => format!("{} ({room})", list_label(lang, e)),
            None => list_label(lang, e),
        },
        None => id.to_owned(),
    })
}

/// The label of `id`, or the id itself when the house no longer has it.
fn label_of(house: House, lang: Lang, id: &str) -> String {
    house
        .entities
        .with(|m| m.get(id).map(|e| list_label(lang, e)))
        .unwrap_or_else(|| id.to_owned())
}

/// A select over the house's entities with a "None" choice. An id the house lost stays shown.
#[component]
pub fn EntityField(
    #[prop(into)] label: Signal<String>,
    value: Signal<Option<String>>,
    on_change: Callback<Option<String>>,
) -> impl IntoView {
    let house = expect_context::<House>();
    let lang = use_lang();
    view! {
        <label class="climate-field">
            <span class="climate-field__label">{move || label.get()}</span>
            <select
                class="field"
                on:change=move |ev| {
                    let v = event_target_value(&ev);
                    on_change.run((!v.is_empty()).then_some(v));
                }
            >
                <option value="" selected=move || value.get().is_none()>
                    {move || climate_text(lang(), ClimateText::None)}
                </option>
                {move || {
                    let current = value.get();
                    let mut list = options(house, lang());
                    if let Some(id) = current.as_ref()
                        && !list.iter().any(|(i, _)| i == id)
                    {
                        list.insert(0, (id.clone(), id.clone()));
                    }
                    list.into_iter()
                        .map(|(id, text)| {
                            let selected = current.as_deref() == Some(id.as_str());
                            view! { <option value=id selected=selected>{text}</option> }
                        })
                        .collect_view()
                }}
            </select>
        </label>
    }
}

/// A list of entities as removable chips, with a select that adds one.
#[component]
pub fn EntityListField(
    #[prop(into)] label: Signal<String>,
    value: Signal<Vec<String>>,
    on_change: Callback<Vec<String>>,
) -> impl IntoView {
    let house = expect_context::<House>();
    let lang = use_lang();
    view! {
        <div class="climate-field">
            <span class="climate-field__label">{move || label.get()}</span>
            <Show when=move || value.with(|v| !v.is_empty())>
                <div class="chips climate-chips">
                {move || {
                    let shown = lang();
                    value
                        .get()
                        .into_iter()
                        .map(|id| {
                            let text = label_of(house, shown, &id);
                            let gone = id.clone();
                            view! {
                                <button
                                    type="button"
                                    class="chip"
                                    title=move || climate_text(lang(), ClimateText::Remove)
                                    on:click=move |_| {
                                        let mut list = value.get_untracked();
                                        list.retain(|i| i != &gone);
                                        on_change.run(list);
                                    }
                                >
                                    {text}
                                    " ×"
                                </button>
                            }
                        })
                        .collect_view()
                }}
                </div>
            </Show>
            <select
                class="field"
                aria-label=move || label.get()
                on:change=move |ev| {
                    let v = event_target_value(&ev);
                    if !v.is_empty() {
                        let mut list = value.get_untracked();
                        if !list.contains(&v) {
                            list.push(v);
                            on_change.run(list);
                        }
                    }
                }
            >
                <option value="" selected=true>
                    {move || climate_text(lang(), ClimateText::AddEllipsis)}
                </option>
                {move || {
                    let taken = value.get();
                    options(house, lang())
                        .into_iter()
                        .filter(|(id, _)| !taken.contains(id))
                        .map(|(id, text)| view! { <option value=id>{text}</option> })
                        .collect_view()
                }}
            </select>
        </div>
    }
}

/// The thermometer that reads a room: "Automatic" first, then the house's temperature sensors
/// grouped by room.
/// `auto` is what Automatic resolves to now; none means the climate devices' own reading.
/// `label` and `auto_text` replace the climate screen's wording ("Room thermometer",
/// "Automatic · …") when given.
#[component]
pub fn ThermometerField(
    value: Signal<Option<String>>,
    auto: Signal<Option<String>>,
    on_change: Callback<Option<String>>,
    #[prop(strip_option, into, default = None)] label: Option<Signal<String>>,
    #[prop(strip_option, into, default = None)] auto_text: Option<Signal<String>>,
) -> impl IntoView {
    let house = expect_context::<House>();
    let lang = use_lang();
    let label = move || {
        label.map_or_else(
            || climate_text(lang(), ClimateText::RoomThermometer).to_owned(),
            |l| l.get(),
        )
    };
    let auto_text = move || {
        if let Some(text) = auto_text {
            return text.get();
        }
        match auto.get() {
            Some(id) => automatic_with(lang(), &label_with_room(house, lang(), &id)),
            None => climate_text(lang(), ClimateText::AutomaticClimateDevice).to_owned(),
        }
    };
    view! {
        <label class="climate-field">
            <span class="climate-field__label">{label}</span>
            <select
                class="field"
                on:change=move |ev| {
                    let v = event_target_value(&ev);
                    on_change.run((!v.is_empty()).then_some(v));
                }
            >
                <option value="" selected=move || value.get().is_none()>{auto_text}</option>
                {move || {
                    let current = value.get();
                    let groups = house.entities.with(|m| house.areas.with(|a| thermometers_by_room(lang(), m, a)));
                    let lost = current
                        .clone()
                        .filter(|id| !groups.iter().any(|(_, list)| list.iter().any(|(i, _)| i == id)))
                        .map(|id| {
                            let text = label_with_room(house, lang(), &id);
                            view! { <option value=id selected=true>{text}</option> }
                        });
                    let groups = groups
                        .into_iter()
                        .map(|(room, list)| {
                            let options = list
                                .into_iter()
                                .map(|(id, text)| {
                                    let selected = current.as_deref() == Some(id.as_str());
                                    view! { <option value=id selected=selected>{text}</option> }
                                })
                                .collect_view();
                            view! {
                                <optgroup label=room.unwrap_or_else(|| {
                                    climate_text(lang(), ClimateText::NoRoom).to_owned()
                                })>
                                    {options}
                                </optgroup>
                            }
                        })
                        .collect_view();
                    (lost, groups)
                }}
            </select>
        </label>
    }
}

#[cfg(test)]
mod tests {
    use super::thermometers_by_room;
    use crate::i18n::Lang;
    use oikos_api::{AreaView, EntityView};
    use oikos_core::model::{
        Capabilities, EntityCategory, EntityState, Kind, UNASSIGNED_AREA, UiOverride,
    };
    use std::collections::BTreeMap;

    fn sensor(id: &str, area: &str, class: &str) -> EntityView {
        EntityView {
            id: id.into(),
            device_id: None,
            name: id.into(),
            device_name: None,
            label: None,
            kind: Kind::Sensor,
            component: "sensor".into(),
            protocol_kind: Kind::Sensor,
            device_class: Some(class.into()),
            unit: None,
            icon: None,
            category: EntityCategory::Primary,
            enabled_by_default: true,
            area: area.into(),
            part_of: None,
            capabilities: Capabilities::default(),
            ui: UiOverride::default(),
            state: EntityState::default(),
        }
    }

    fn area(id: &str, name: &str) -> AreaView {
        AreaView {
            id: id.into(),
            name: name.into(),
            icon: None,
            order: None,
            entity_count: 0,
            primary_count: 0,
            group_names: Default::default(),
            group_favorites: Default::default(),
        }
    }

    #[test]
    fn thermometers_group_by_room_name_with_unassigned_last() {
        let entities: BTreeMap<String, EntityView> = [
            sensor("t_loose", UNASSIGNED_AREA, "temperature"),
            sensor("t_office", "office", "temperature"),
            sensor("t_bed_b", "bed", "temperature"),
            sensor("t_bed_a", "bed", "temperature"),
            sensor("h_bed", "bed", "humidity"),
        ]
        .into_iter()
        .map(|e| (e.id.clone(), e))
        .collect();
        let areas = [area("office", "Office"), area("bed", "Bedroom")];
        let groups = thermometers_by_room(Lang::En, &entities, &areas);
        let shape: Vec<(Option<&str>, Vec<&str>)> = groups
            .iter()
            .map(|(room, list)| {
                (
                    room.as_deref(),
                    list.iter().map(|(id, _)| id.as_str()).collect(),
                )
            })
            .collect();
        assert_eq!(
            shape,
            vec![
                (Some("Bedroom"), vec!["t_bed_a", "t_bed_b"]),
                (Some("Office"), vec!["t_office"]),
                (None, vec!["t_loose"]),
            ]
        );
    }
}
