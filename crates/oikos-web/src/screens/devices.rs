//! The Zigbee devices of the house, with the discovery topics the hub ignored.

use leptos::prelude::*;
use leptos_router::components::A;
use oikos_api::DeviceView;

use crate::format::{list_label, wrap_id};
use crate::house::House;
use crate::screens::entity_sheet::{availability_class, availability_word};

/// True when the device's name, model, vendor or IEEE address contains `query`, ignoring case.
/// A blank query matches everything.
fn matches(device: &DeviceView, query: &str) -> bool {
    let query = query.trim().to_lowercase();
    query.is_empty()
        || [
            Some(device.friendly_name.as_str()),
            device.model.as_deref(),
            device.vendor.as_deref(),
            Some(device.ieee.as_str()),
        ]
        .into_iter()
        .flatten()
        .any(|field| field.to_lowercase().contains(&query))
}

/// "Model · Vendor", whichever of the two is known.
fn model_line(device: &DeviceView) -> String {
    let parts: Vec<&str> = [device.model.as_deref(), device.vendor.as_deref()]
        .into_iter()
        .flatten()
        .collect();
    if parts.is_empty() {
        "—".to_owned()
    } else {
        parts.join(" · ")
    }
}

#[component]
fn DeviceRow(device: DeviceView, expanded: RwSignal<Option<String>>) -> impl IntoView {
    let house = expect_context::<House>();
    let id = device.id.clone();
    let open = {
        let id = id.clone();
        Memo::new(move |_| expanded.with(|e| e.as_deref() == Some(id.as_str())))
    };
    let toggle = move |_| {
        let id = id.clone();
        expanded.update(|e| {
            *e = if e.as_deref() == Some(id.as_str()) {
                None
            } else {
                Some(id)
            }
        });
    };
    let ids = device.entity_ids.clone();
    let links = move || {
        house.entities.with(|m| {
            ids.iter()
                .filter_map(|id| m.get(id).map(|e| (e.id.clone(), list_label(e))))
                .collect::<Vec<_>>()
        })
    };
    let entity_count = device.entity_ids.len();
    view! {
        <li class="device" class:device--open=move || open.get()>
            <button
                type="button"
                class="device__head"
                aria-expanded=move || open.get().to_string()
                on:click=toggle
            >
                <span
                    class=availability_class(device.availability)
                    title=availability_word(device.availability)
                ></span>
                <span class="device__titles">
                    <span class="device__name">{wrap_id(&device.friendly_name)}</span>
                    <span class="device__model">{model_line(&device)}</span>
                </span>
                <span class="device__badges">
                    {device
                        .update
                        .clone()
                        .filter(|u| u.available)
                        .map(|u| {
                            view! {
                                <span class="chip-badge device__update-badge" title="Firmware update available">
                                    {format!("v{}", u.latest.unwrap_or_default())}
                                </span>
                            }
                        })}
                    {device
                        .battery
                        .map(|b| view! { <span class="chip-badge">{format!("{b} %")}</span> })}
                    {device
                        .linkquality
                        .map(|q| view! { <span class="chip-badge">{format!("LQ {q}")}</span> })}
                    <span class="chip-badge" title="Entities">
                        {entity_count}
                    </span>
                </span>
            </button>
            <Show when=move || open.get()>
                <div class="device__body">
                    <ul class="sheet__links">
                        {links()
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
                </div>
            </Show>
        </li>
    }
}

#[component]
pub fn DevicesScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let query = RwSignal::new(String::new());
    let expanded = RwSignal::new(None::<String>);
    let filtered = Memo::new(move |_| {
        let query = query.get();
        let mut devices: Vec<DeviceView> = house
            .devices
            .with(|d| d.iter().filter(|d| matches(d, &query)).cloned().collect());
        devices.sort_by_key(|d| d.friendly_name.to_lowercase());
        devices
    });
    let total = Memo::new(move |_| house.devices.with(Vec::len));

    view! {
        <div class="devices">
            <h1 class="devices__title">"Devices"</h1>
            <input
                class="devices__search"
                type="search"
                placeholder="Search name, model, vendor or IEEE"
                aria-label="Search devices"
                prop:value=move || query.get()
                on:input=move |ev| query.set(event_target_value(&ev))
            />
            <p class="devices__count">
                {move || filtered.with(Vec::len)} " of " {move || total.get()} " devices"
            </p>
            <ul class="device-list">
                <For
                    each=move || filtered.get()
                    key=|d| d.id.clone()
                    children=move |device| view! { <DeviceRow device expanded /> }
                />
            </ul>
            <section class="sheet__panel">
                <h2 class="sheet__heading">
                    "Ignored discovery topics (" {move || house.ignored.with(Vec::len)} ")"
                </h2>
                <ul class="ignored-list">
                    <For
                        each=move || house.ignored.get()
                        key=|i| i.topic.clone()
                        children=|item| {
                            view! {
                                <li class="ignored-list__item">
                                    <code class="ignored-list__topic">{wrap_id(&item.topic)}</code>
                                    <span class="ignored-list__reason">{item.reason}</span>
                                </li>
                            }
                        }
                    />
                </ul>
            </section>
        </div>
    }
}

#[cfg(test)]
mod tests {
    use super::matches;
    use oikos_api::DeviceView;
    use oikos_core::model::Availability;

    fn device() -> DeviceView {
        DeviceView {
            id: "d1".into(),
            ieee: "0x00158D0001ABCDEF".into(),
            friendly_name: "Salon/Lamp".into(),
            model: Some("TS0601".into()),
            vendor: Some("Tuya".into()),
            description: None,
            supported: true,
            interview_state: "SUCCESSFUL".into(),
            power_source: None,
            entity_ids: vec![],
            availability: Availability::Online,
            battery: None,
            linkquality: None,
            update: None,
        }
    }

    #[test]
    fn filter_matches_name_model_vendor_and_ieee_ignoring_case() {
        let d = device();
        assert!(matches(&d, ""));
        assert!(matches(&d, "  "));
        assert!(matches(&d, "salon"));
        assert!(matches(&d, "ts06"));
        assert!(matches(&d, "TUYA"));
        assert!(matches(&d, "abcdef"));
        assert!(!matches(&d, "ikea"));
    }

    #[test]
    fn filter_tolerates_missing_model_and_vendor() {
        let d = DeviceView {
            model: None,
            vendor: None,
            ..device()
        };
        assert!(!matches(&d, "tuya"));
        assert!(matches(&d, "lamp"));
    }
}
