//! The Devices step: what the hub found, grouped by room. The room picker writes the same
//! `area` override the device sheet does, on every entity of the device.

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use oikos_api::{DeviceView, OverridePatch, SetupStep};
use serde_json::json;

use super::wizard::WizardNav;
use crate::api::setup::put_step;
use crate::format::room_name;
use crate::house::House;
use crate::i18n::devices::{DeviceText, use_device_text};
use crate::i18n::setup::{SetupText, setup_text, use_setup_text};
use crate::i18n::use_lang;
use crate::screens::devices::device_rooms;

/// The base topic Zigbee2MQTT publishes under when none is configured.
const DEFAULT_BASE: &str = "zigbee2mqtt";

/// Devices grouped under their first room. Rooms are sorted by name, the room-less group
/// comes last, and devices inside a group are sorted by name.
pub fn group_by_room(
    devices: &[DeviceView],
    rooms_of: impl Fn(&DeviceView) -> Vec<String>,
) -> Vec<(Option<String>, Vec<DeviceView>)> {
    let mut groups: Vec<(Option<String>, Vec<DeviceView>)> = Vec::new();
    for device in devices {
        let room = rooms_of(device).into_iter().next();
        match groups.iter_mut().find(|(r, _)| *r == room) {
            Some((_, list)) => list.push(device.clone()),
            None => groups.push((room, vec![device.clone()])),
        }
    }
    for (_, list) in &mut groups {
        list.sort_by(|a, b| a.friendly_name.cmp(&b.friendly_name));
    }
    // `None` sorts before `Some`, so compare on "has no room" first.
    groups.sort_by(|a, b| (a.0.is_none(), &a.0).cmp(&(b.0.is_none(), &b.0)));
    groups
}

/// The one-field patch that moves an entity to `area`; `None` clears the override.
fn area_patch(area: Option<&str>) -> OverridePatch {
    let mut patch = OverridePatch::new();
    patch.insert("area".to_owned(), json!(area));
    patch
}

/// One device: name, model and a room select. Changing the room moves each of its entities.
#[component]
fn DeviceLine(device: DeviceView, failure: RwSignal<Option<String>>) -> impl IntoView {
    let house = expect_context::<House>();
    let t = use_device_text();
    let model = [device.model.as_deref(), device.vendor.as_deref()]
        .into_iter()
        .flatten()
        .collect::<Vec<_>>()
        .join(" · ");
    let device_id = device.id.clone();
    let current = {
        let device = device.clone();
        Memo::new(move |_| {
            house.entities.with(|m| {
                device
                    .entity_ids
                    .iter()
                    .filter_map(|id| m.get(id))
                    .map(|e| e.area.clone())
                    .find(|a| room_name(&house.areas.get_untracked(), a).is_some())
                    .unwrap_or_default()
            })
        })
    };
    let select = NodeRef::<leptos::html::Select>::new();
    let lang = use_lang();
    let on_change = move |ev: leptos::ev::Event| {
        let value = event_target_value(&ev);
        let ids: Vec<String> = house.entities.with_untracked(|m| {
            m.values()
                .filter(|e| e.device_id.as_deref() == Some(device_id.as_str()))
                .map(|e| e.id.clone())
                .collect()
        });
        failure.set(None);
        leptos::task::spawn_local(async move {
            // Every entity is tried; the rows already moved stay moved.
            let mut first_error = None;
            for id in ids {
                let area = (!value.is_empty()).then_some(value.as_str());
                if let Err(e) = crate::api::set_override(&id, area_patch(area)).await {
                    first_error.get_or_insert(e.error);
                }
            }
            if let Some(error) = first_error {
                failure.try_set(Some(
                    setup_text(lang(), SetupText::RoomMoveIncomplete).replace("{error}", &error),
                ));
                if let Some(el) = select.get_untracked() {
                    el.set_value(&current.get_untracked());
                }
            }
        });
    };
    view! {
        <li class="devices__row">
            <span class="devices__name">{device.friendly_name.clone()}</span>
            <span class="devices__model">{model}</span>
            <select node_ref=select class="select devices__room" on:change=on_change>
                <option value="" prop:selected=move || current.with(String::is_empty)>
                    {move || t(DeviceText::NoRoom)}
                </option>
                <For
                    each=move || house.areas.get()
                    key=|a| (a.id.clone(), a.name.clone())
                    children=move |a| {
                        let id = a.id.clone();
                        let value = a.id.clone();
                        view! {
                            <option value=value prop:selected=move || current.get() == id>
                                {a.name}
                            </option>
                        }
                    }
                />
            </select>
        </li>
    }
}

/// The Devices step. Needs [`House`] and [`WizardNav`] in context. With no device after
/// `empty_after_ms` it points the user back to the broker settings.
#[component]
pub fn DevicesStep(z2m_base: String, #[prop(default = 5000)] empty_after_ms: u32) -> impl IntoView {
    let house = expect_context::<House>();
    let nav = expect_context::<WizardNav>();
    let t = use_setup_text();
    let lang = use_lang();
    let waited = RwSignal::new(false);
    let failure = RwSignal::new(None::<String>);
    leptos::task::spawn_local(async move {
        TimeoutFuture::new(empty_after_ms).await;
        waited.try_set(true);
    });
    let base = if z2m_base.trim().is_empty() {
        DEFAULT_BASE.to_owned()
    } else {
        z2m_base
    };
    let advancing = RwSignal::new(false);
    let go_next = move |_| {
        if advancing.get_untracked() {
            return;
        }
        advancing.set(true);
        leptos::task::spawn_local(async move {
            match put_step(SetupStep::Services).await {
                Ok(_) => {
                    nav.step.try_set(SetupStep::Services);
                }
                Err(e) => {
                    failure.try_set(Some(e.message(lang())));
                }
            }
            advancing.try_set(false);
        });
    };
    let groups = Memo::new(move |_| {
        let areas = house.areas.get();
        house.entities.with(|m| {
            house
                .devices
                .with(|d| group_by_room(d, |dev| device_rooms(dev, m, &areas)))
        })
    });
    let count = move || {
        let n = house.devices.with(Vec::len);
        if n == 1 {
            t(SetupText::DeviceFoundOne).to_owned()
        } else {
            t(SetupText::DevicesFound).replace("{n}", &n.to_string())
        }
    };
    view! {
        <div class="setup__body">
            <h1 class="setup__title">{move || t(SetupText::DevicesTitle)}</h1>
            <p class="setup__lead">{move || t(SetupText::DevicesLead)}</p>
            {move || {
                if house.devices.with(|d| !d.is_empty()) {
                    view! {
                        <p class="devices__count" role="status">{count}</p>
                        <div class="devices__groups">
                            {move || {
                                groups
                                    .get()
                                    .into_iter()
                                    .map(|(room, devices)| {
                                        let title = room
                                            .unwrap_or_else(|| {
                                                crate::i18n::devices::device_text(lang(), DeviceText::NoRoom)
                                                    .to_owned()
                                            });
                                        view! {
                                            <section class="devices__group">
                                                <h2 class="devices__room-title">{title}</h2>
                                                <ul class="devices__list">
                                                    {devices
                                                        .into_iter()
                                                        .map(|d| view! { <DeviceLine device=d failure=failure /> })
                                                        .collect_view()}
                                                </ul>
                                            </section>
                                        }
                                    })
                                    .collect_view()
                            }}
                        </div>
                    }
                        .into_any()
                } else if waited.get() {
                    let text = setup_text(lang(), SetupText::NoDevicesYet).replace("{base}", &base);
                    view! {
                        <p class="settings__hint" role="status">{text}</p>
                        <button
                            type="button"
                            class="btn devices__back"
                            on:click=move |_| nav.step.set(SetupStep::Mqtt)
                        >
                            {move || t(SetupText::BackToMqtt)}
                        </button>
                    }
                        .into_any()
                } else {
                    ().into_any()
                }
            }}
            <p class="loc__problem" role="alert">{move || failure.get()}</p>
            <div class="loc__actions devices__actions">
                <button type="button" class="btn" prop:disabled=move || advancing.get() on:click=go_next>
                    {move || t(SetupText::Skip)}
                </button>
                <button type="button" class="btn btn--primary loc__save" prop:disabled=move || advancing.get() on:click=go_next>
                    {move || t(SetupText::Next)}
                </button>
            </div>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn device(id: &str, name: &str) -> DeviceView {
        serde_json::from_value(serde_json::json!({
            "id": id, "ieee": id, "friendly_name": name, "model": null, "vendor": null,
            "description": null, "supported": true, "interview_state": "SUCCESSFUL",
            "power_source": null, "entity_ids": [], "availability": "online",
            "battery": null, "linkquality": null
        }))
        .unwrap()
    }

    #[test]
    fn rooms_sort_with_no_room_last() {
        let devices = vec![
            device("d1", "Lamp"),
            device("d2", "Bulb"),
            device("d3", "Sensor"),
            device("d4", "Plug"),
        ];
        let groups = group_by_room(&devices, |d| match d.id.as_str() {
            "d1" => vec!["Living room".into()],
            "d2" => vec!["Kitchen".into()],
            "d4" => vec!["Kitchen".into(), "Living room".into()],
            _ => vec![],
        });
        let names: Vec<Option<&str>> = groups.iter().map(|(r, _)| r.as_deref()).collect();
        assert_eq!(names, [Some("Kitchen"), Some("Living room"), None]);
        let kitchen: Vec<&str> = groups[0]
            .1
            .iter()
            .map(|d| d.friendly_name.as_str())
            .collect();
        assert_eq!(kitchen, ["Bulb", "Plug"]);
        assert_eq!(groups[2].1[0].friendly_name, "Sensor");
    }
}
