//! The detail sheet of one entity: controls, the settings that apply to it, and the device info
//! and raw state folded under "Device & diagnostics".

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use std::collections::BTreeMap;

use oikos_api::{DeviceView, EntityView, GroupView};
use oikos_core::model::{Availability, Command, EntityState, Hs, Kind, LightState, StateValue};

use crate::cards::Sender;
use crate::cards::light::has_colour;
use crate::components::hs_wheel::HsWheel;

use crate::components::card_place::InSheet;
use crate::format::{list_label, title_suffix, type_word, wrap_id};
use crate::house::House;
use crate::i18n::format::{date_time, local_date_time};
use crate::icons::entity_icon;
use crate::nav::BackLink;
use crate::override_editor::OverrideEditor;
use crate::screens::generic_panel::GenericPanel;
use crate::screens::home::card_for;

pub(crate) fn availability_class(availability: Availability) -> &'static str {
    match availability {
        Availability::Online => "dot dot--online",
        Availability::Offline => "dot dot--offline",
        Availability::Unknown => "dot dot--unknown",
    }
}

pub(crate) fn availability_word(availability: Availability) -> &'static str {
    match availability {
        Availability::Online => "Online",
        Availability::Offline => "Offline",
        Availability::Unknown => "Unknown",
    }
}

fn pretty<T: serde::Serialize>(value: &T) -> String {
    serde_json::to_string_pretty(value).unwrap_or_default()
}

/// A glass panel with a heading.
#[component]
fn Panel(title: &'static str, children: Children) -> impl IntoView {
    view! {
        <section class="sheet__panel">
            <h2 class="sheet__heading">{title}</h2>
            {children()}
        </section>
    }
}

/// The `(group id, group name)` of every group that lists `entity_id` as a member, in group-id
/// order.
pub(crate) fn groups_containing(
    groups: &BTreeMap<String, GroupView>,
    entity_id: &str,
) -> Vec<(String, String)> {
    groups
        .values()
        .filter(|g| g.members.iter().any(|m| m.id == entity_id))
        .map(|g| (g.id.clone(), g.name.clone()))
        .collect()
}

/// The colour of a light, when its state has one.
fn light_hs(entity: &EntityView) -> Option<Hs> {
    match &entity.state.value {
        StateValue::Light(l) => l.hs,
        _ => None,
    }
}

/// Pure white: what the White chip sends.
const WHITE: Hs = Hs { h: 0.0, s: 0.0 };

/// Whether a light's colour reads as white, as the app's White chip decides it: saturation
/// under 2 %. A light without a colour is not.
fn is_white(hs: Option<Hs>) -> bool {
    hs.is_some_and(|hs| hs.s < 2.0)
}

/// The HS colour wheel of a light that can show colour, and a White chip under it as in the
/// app. Built once per sheet: the wheel reads the light's colour through signals, so a state
/// echo moves its marker without rebuilding it.
#[component]
fn ColourPanel(id: String, entity: Memo<Option<EntityView>>) -> impl IntoView {
    let sender = Sender::from_context();
    let shows_colour = Memo::new(move |_| {
        entity.with(|e| {
            e.as_ref()
                .is_some_and(|e| e.kind == Kind::Light && has_colour(&e.capabilities.color_modes))
        })
    });
    let value = Signal::derive(move || entity.with(|e| e.as_ref().and_then(light_hs)));
    let on_pick = Callback::new(move |hs: Hs| {
        let guess = entity.with_untracked(|e| {
            let e = e.as_ref()?;
            let StateValue::Light(light) = &e.state.value else {
                return None;
            };
            Some(EntityState {
                value: StateValue::Light(LightState {
                    on: true,
                    hs: Some(hs),
                    ..light.clone()
                }),
                ..e.state.clone()
            })
        });
        sender.send(
            &id,
            Command::TurnOn {
                brightness: None,
                color: Some(hs),
                effect: None,
            },
            guess,
        );
    });
    view! {
        <Show when=move || shows_colour.get()>
            <Panel title="Colour">
                <HsWheel value=value on_pick=on_pick />
                <div class="colour__presets">
                    <button
                        type="button"
                        class="chip"
                        class:chip--selected=move || is_white(value.get())
                        aria-pressed=move || is_white(value.get()).to_string()
                        on:click=move |_| on_pick.run(WHITE)
                    >
                        <span class="colour__swatch colour__swatch--white"></span>
                        "White"
                    </button>
                </div>
            </Panel>
        </Show>
    }
}

#[component]
fn Row(label: &'static str, #[prop(into)] value: String) -> impl IntoView {
    view! {
        <div class="sheet__row">
            <dt>{label}</dt>
            <dd>{wrap_id(&value)}</dd>
        </div>
    }
}

fn device_panel(house: House, entity: &EntityView) -> Option<AnyView> {
    let device_id = entity.device_id.as_deref()?;
    let device: DeviceView = house
        .devices
        .with(|d| d.iter().find(|d| d.id == device_id).cloned())?;
    let siblings: Vec<(String, String)> = house.entities.with(|m| {
        device
            .entity_ids
            .iter()
            .filter(|id| **id != entity.id)
            .filter_map(|id| {
                m.get(id)
                    .map(|e| (e.id.clone(), list_label(crate::i18n::Lang::En, e)))
            })
            .collect()
    });
    let dash = || "—".to_owned();
    Some(
        view! {
            <h3 class="sheet__subheading">"Device"</h3>
            <dl class="sheet__rows">
                    <Row label="Name" value=device.friendly_name.clone() />
                    <Row label="Model" value=device.model.clone().unwrap_or_else(dash) />
                    <Row label="Vendor" value=device.vendor.clone().unwrap_or_else(dash) />
                    <Row label="IEEE address" value=device.ieee.clone() />
                    <Row label="Availability" value=availability_word(device.availability) />
                    <Row
                        label="Battery"
                        value=device.battery.map_or_else(dash, |b| format!("{b} %"))
                    />
                    <Row
                        label="Link quality"
                        value=device.linkquality.map_or_else(dash, |q| q.to_string())
                    />
                </dl>
                {(!siblings.is_empty())
                    .then(|| {
                        view! {
                            <h3 class="sheet__subheading">"Other entities"</h3>
                            <ul class="sheet__links">
                                {siblings
                                    .into_iter()
                                    .map(|(id, name)| {
                                        view! {
                                            <li>
                                                <A href=format!("/entity/{}", crate::api::encode_id(&id))>{wrap_id(&name)}</A>
                                            </li>
                                        }
                                    })
                                    .collect_view()}
                            </ul>
                        }
                    })}
        }
        .into_any(),
    )
}

fn state_panel(entity: &EntityView) -> impl IntoView + use<> {
    let value = pretty(&entity.state.value);
    let attributes = pretty(&entity.state.attributes);
    let lang = crate::i18n::use_lang();
    let last_update = entity.state.last_update;
    let updated = move || {
        last_update.map_or_else(
            || "—".to_owned(),
            |secs| date_time(lang(), local_date_time(secs)),
        )
    };
    view! {
        <h3 class="sheet__subheading">"Value"</h3>
        <pre class="sheet__json">{value}</pre>
        <h3 class="sheet__subheading">"Attributes"</h3>
        <pre class="sheet__json">{attributes}</pre>
        <p class="sheet__updated">"Last update: " {updated}</p>
    }
}

/// The sheet for entity `id`. The caller keys it on the id, so it is built once per entity and
/// nothing below it is rebuilt by a state echo unless it reads `entity` itself.
#[component]
pub fn SheetBody(id: String) -> impl IntoView {
    let house = expect_context::<House>();
    // What the override editor starts from. It follows outside changes on its own; handing it
    // the live entity would rebuild it (and drop unsaved typing) on every WebSocket echo.
    let initial = house.entities.with_untracked(|m| m.get(&id).cloned());
    let entity = {
        let id = id.clone();
        Memo::new(move |_| house.entities.with(|m| m.get(&id).cloned()))
    };

    let header = move || {
        entity.get().map(|e| {
            let area = house.areas.with(|areas| {
                areas
                    .iter()
                    .find(|a| a.id == e.area)
                    .map_or_else(|| e.area.clone(), |a| a.name.clone())
            });
            let availability = e.state.availability;
            let title = match title_suffix(crate::i18n::Lang::En, &e) {
                Some(suffix) => format!("{} · {suffix}", e.name),
                None => e.name.clone(),
            };
            view! {
                <span class="sheet__icon" inner_html=entity_icon(&e)></span>
                <div class="sheet__titles">
                    <h1 class="sheet__title">{wrap_id(&title)}</h1>
                    <p class="sheet__subtitle">
                        {area} " · " {type_word(crate::i18n::Lang::En, &e)} " · "
                        <span class=availability_class(availability)></span>
                        " "
                        {availability_word(availability)}
                    </p>
                </div>
            }
        })
    };

    let history_href = format!("/history/{}", crate::api::encode_id(&id));
    let id_for_groups = id.clone();
    view! {
        <header class="sheet__header">
            <BackLink />
            {header}
        </header>
        <Panel title="Controls">
            <div class="sheet__controls">
                <InSheet>{move || entity.get().map(card_for)}</InSheet>
            </div>
            <A href=history_href attr:class="chip sheet__history">
                "View history →"
            </A>
        </Panel>
        <ColourPanel id=id.clone() entity=entity />
        <GenericPanel entity=entity />
        <Panel title="Settings">
            {initial.map(|e| view! { <OverrideEditor entity=e /> })}
        </Panel>
        {move || {
            let list = house.groups.with(|g| groups_containing(g, &id_for_groups));
            (!list.is_empty())
                .then(|| {
                    view! {
                        <Panel title="In groups">
                            <ul class="sheet__links">
                                {list
                                    .into_iter()
                                    .map(|(gid, name)| {
                                        view! {
                                            <li>
                                                <A href=format!("/group/{}", crate::api::encode_id(&gid))>{wrap_id(&name)}</A>
                                            </li>
                                        }
                                    })
                                    .collect_view()}
                            </ul>
                        </Panel>
                    }
                })
        }}
        <details class="sheet__panel sheet__details">
            <summary class="sheet__heading sheet__summary">"Device & diagnostics"</summary>
            {move || entity.get().and_then(|e| device_panel(house, &e))}
            {move || entity.get().map(|e| state_panel(&e))}
        </details>
    }
}

#[component]
pub fn EntitySheetScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let params = use_params_map();
    let id = Memo::new(move |_| params.read().get("id").unwrap_or_default());
    // `Some(id)` while the house has the entity. One memo carrying the id, so a sheet-to-sheet
    // link rebuilds the body (a bare known flag stays `true` and the sheet kept the first
    // entity), yet a state echo never does.
    let shown = Memo::new(move |_| {
        let id = id.get();
        house.entities.with(|m| m.contains_key(&id)).then_some(id)
    });
    let loaded = Memo::new(move |_| {
        house.entities.with(|m| !m.is_empty())
            || matches!(house.connection.get(), oikos_api::ConnectionStatus::Online)
    });

    view! {
        <div class="sheet">
            {move || {
                if let Some(id) = shown.get() {
                    view! { <SheetBody id=id /> }.into_any()
                } else if loaded.get() {
                    view! {
                        <header class="sheet__header">
                            <BackLink />
                            <div class="sheet__titles">
                                <h1 class="sheet__title">"Unknown entity"</h1>
                                <p class="sheet__subtitle">{move || wrap_id(&id.get())}</p>
                            </div>
                        </header>
                    }
                        .into_any()
                } else {
                    view! { <div class="card card--skeleton" aria-busy="true"></div> }.into_any()
                }
            }}
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::{WHITE, groups_containing, is_white};
    use oikos_api::{EntityView, GroupMemberView, GroupView};
    use oikos_core::model::{Capabilities, EntityCategory, EntityState, Hs, Kind, UiOverride};
    use std::collections::BTreeMap;

    fn group(id: &str, name: &str, members: &[&str]) -> GroupView {
        GroupView {
            id: id.into(),
            name: name.into(),
            kind: Kind::Light,
            members: members
                .iter()
                .map(|m| GroupMemberView {
                    id: (*m).into(),
                    name: (*m).into(),
                    present: true,
                    mismatch: false,
                })
                .collect(),
            hide_members: true,
            entity: EntityView {
                id: format!("group_{id}"),
                device_id: None,
                name: name.into(),
                device_name: None,
                label: None,
                kind: Kind::Light,
                component: Kind::Light.component().to_string(),
                protocol_kind: Kind::Light,
                device_class: None,
                unit: None,
                icon: None,
                category: EntityCategory::Primary,
                enabled_by_default: true,
                area: "x".into(),
                part_of: None,
                capabilities: Capabilities::default(),
                ui: UiOverride::default(),
                state: EntityState::default(),
            },
            used_by_flows: vec![],
        }
    }

    #[test]
    fn lists_every_group_containing_the_entity() {
        let mut m = BTreeMap::new();
        for g in [
            group("hall", "Hallway lights", &["a", "b"]),
            group("living", "Living lights", &["a"]),
            group("other", "Other", &["c"]),
        ] {
            m.insert(g.id.clone(), g);
        }
        let got = groups_containing(&m, "a");
        let names: Vec<_> = got.iter().map(|(_, n)| n.as_str()).collect();
        assert_eq!(names, ["Hallway lights", "Living lights"]);
        assert!(groups_containing(&m, "zzz").is_empty());
    }

    #[test]
    fn white_is_near_zero_saturation() {
        assert!(is_white(Some(WHITE)));
        assert!(is_white(Some(Hs { h: 200.0, s: 1.5 })));
        assert!(!is_white(Some(Hs { h: 0.0, s: 2.0 })));
        assert!(!is_white(Some(Hs { h: 120.0, s: 80.0 })));
        assert!(!is_white(None));
    }
}
