//! The detail sheet of one entity: controls, device info, raw state and the override editor.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use oikos_api::{DeviceView, EntityView};
use oikos_core::model::{Availability, Command, EntityState, Hs, Kind, LightState, StateValue};

use crate::cards::Sender;
use crate::cards::light::has_colour;
use crate::components::hs_wheel::HsWheel;

use crate::format::{format_timestamp, kind_word, list_label, title_suffix, wrap_id};
use crate::house::House;
use crate::icons::kind_icon;
use crate::nav::ReturnPath;
use crate::override_editor::OverrideEditor;
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

/// The colour of a light, when its state has one.
fn light_hs(entity: &EntityView) -> Option<Hs> {
    match &entity.state.value {
        StateValue::Light(l) => l.hs,
        _ => None,
    }
}

/// The HS colour wheel of a light that can show colour. Built once per sheet: the wheel reads the
/// light's colour through signals, so a state echo moves its marker without rebuilding it.
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
            .filter_map(|id| m.get(id).map(|e| (e.id.clone(), list_label(e))))
            .collect()
    });
    let dash = || "—".to_owned();
    Some(
        view! {
            <Panel title="Device">
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
            </Panel>
        }
        .into_any(),
    )
}

fn state_panel(entity: &EntityView) -> impl IntoView + use<> {
    let value = pretty(&entity.state.value);
    let attributes = pretty(&entity.state.attributes);
    let updated = entity
        .state
        .last_update
        .map_or_else(|| "—".to_owned(), format_timestamp);
    view! {
        <Panel title="State">
            <h3 class="sheet__subheading">"Value"</h3>
            <pre class="sheet__json">{value}</pre>
            <h3 class="sheet__subheading">"Attributes"</h3>
            <pre class="sheet__json">{attributes}</pre>
            <p class="sheet__updated">"Last update: " {updated}</p>
        </Panel>
    }
}

/// The sheet for entity `id`. The caller keys it on the id, so it is built once per entity and
/// nothing below it is rebuilt by a state echo unless it reads `entity` itself.
#[component]
pub fn SheetBody(id: String) -> impl IntoView {
    let house = expect_context::<House>();
    let return_path = expect_context::<ReturnPath>();
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
            let title = match title_suffix(&e) {
                Some(suffix) => format!("{} · {suffix}", e.name),
                None => e.name.clone(),
            };
            view! {
                <span class="sheet__icon" inner_html=kind_icon(e.kind, e.device_class.as_deref())></span>
                <div class="sheet__titles">
                    <h1 class="sheet__title">{wrap_id(&title)}</h1>
                    <p class="sheet__subtitle">
                        {area} " · " {kind_word(e.kind)} " · "
                        <span class=availability_class(availability)></span>
                        " "
                        {availability_word(availability)}
                    </p>
                </div>
            }
        })
    };

    view! {
        <header class="sheet__header">
            <A href=move || return_path.0.get() attr:class="sheet__back">
                "←"
                <span class="sheet__back-label">"Back"</span>
            </A>
            {header}
        </header>
        <Panel title="Controls">
            <div class="sheet__controls">{move || entity.get().map(card_for)}</div>
        </Panel>
        <ColourPanel id=id.clone() entity=entity />
        {move || entity.get().and_then(|e| device_panel(house, &e))}
        {move || entity.get().map(|e| state_panel(&e))}
        <Panel title="Override">
            {initial.map(|e| view! { <OverrideEditor entity=e /> })}
        </Panel>
    }
}

#[component]
pub fn EntitySheetScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let return_path = expect_context::<ReturnPath>();
    let params = use_params_map();
    let id = Memo::new(move |_| params.read().get("id").unwrap_or_default());
    let known = Memo::new(move |_| house.entities.with(|m| m.contains_key(&id.get())));
    let loaded = Memo::new(move |_| {
        house.entities.with(|m| !m.is_empty())
            || matches!(house.connection.get(), oikos_api::ConnectionStatus::Online)
    });

    view! {
        <div class="sheet">
            {move || {
                if known.get() {
                    view! { <SheetBody id=id.get() /> }.into_any()
                } else if loaded.get() {
                    view! {
                        <header class="sheet__header">
                            <A href=move || return_path.0.get() attr:class="sheet__back">
                                "←"
                                <span class="sheet__back-label">"Back"</span>
                            </A>
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
