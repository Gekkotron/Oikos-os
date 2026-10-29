#![cfg(target_arch = "wasm32")]

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::tick;
use oikos_api::{AreaView, DeviceView, EntityView, SetupStep};
use oikos_core::model::{Capabilities, EntityCategory, EntityState, Kind, UiOverride};
use oikos_web::house::House;
use oikos_web::i18n::Lang;
use oikos_web::setup::devices::DevicesStep;
use oikos_web::setup::wizard::WizardNav;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

#[path = "support/fetch_stub.rs"]
mod fetch_stub;
use fetch_stub::FetchStub;

fn device(id: &str, name: &str, entity_ids: &[&str]) -> DeviceView {
    serde_json::from_value(serde_json::json!({
        "id": id, "ieee": id, "friendly_name": name, "model": "E1743", "vendor": "Acme",
        "description": null, "supported": true, "interview_state": "SUCCESSFUL",
        "power_source": null, "entity_ids": entity_ids, "availability": "online",
        "battery": null, "linkquality": null
    }))
    .unwrap()
}

fn entity(id: &str, device_id: &str) -> EntityView {
    EntityView {
        id: id.into(),
        device_id: Some(device_id.into()),
        name: id.into(),
        device_name: None,
        label: None,
        kind: Kind::Sensor,
        component: Kind::Sensor.component().to_string(),
        protocol_kind: Kind::Sensor,
        device_class: None,
        unit: None,
        icon: None,
        category: EntityCategory::Primary,
        enabled_by_default: true,
        area: "unassigned".into(),
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

fn mount(house: House, empty_after_ms: u32) -> (web_sys::Element, RwSignal<SetupStep>) {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let step = RwSignal::new(SetupStep::Devices);
    leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(RwSignal::new(Lang::En));
        provide_context(WizardNav { step });
        provide_context(house);
        view! { <DevicesStep z2m_base=String::new() empty_after_ms=empty_after_ms /> }
    })
    .forget();
    (host, step)
}

#[wasm_bindgen_test]
async fn no_devices_points_back_to_the_broker() {
    let _stub = FetchStub::install();
    let (host, step) = mount(House::new(), 10);
    tick().await;
    TimeoutFuture::new(50).await;
    tick().await;
    assert!(host.text_content().unwrap().contains("under zigbee2mqtt"));
    let back: web_sys::HtmlButtonElement = host
        .query_selector(".devices__back")
        .unwrap()
        .expect("back button")
        .unchecked_into();
    back.click();
    tick().await;
    assert_eq!(step.get_untracked(), SetupStep::Mqtt);
}

#[wasm_bindgen_test]
async fn choosing_a_room_moves_every_entity_of_the_device() {
    let stub = FetchStub::install();
    let house = House::new();
    house.devices.set(vec![device("d1", "Lamp", &["e1", "e2"])]);
    house.entities.set(
        [("e1", "d1"), ("e2", "d1"), ("e3", "other")]
            .into_iter()
            .map(|(id, dev)| (id.to_owned(), entity(id, dev)))
            .collect(),
    );
    house.areas.set(vec![area("kitchen", "Kitchen")]);
    let (host, _step) = mount(house, 5000);
    tick().await;
    let select: web_sys::HtmlSelectElement = host
        .query_selector("select.devices__room")
        .unwrap()
        .expect("room select")
        .unchecked_into();
    select.set_value("kitchen");
    select
        .dispatch_event(
            &web_sys::Event::new_with_event_init_dict("change", &{
                let init = web_sys::EventInit::new();
                init.set_bubbles(true);
                init
            })
            .unwrap(),
        )
        .unwrap();
    for _ in 0..6 {
        TimeoutFuture::new(15).await;
        tick().await;
    }
    let puts: Vec<_> = stub
        .requests()
        .into_iter()
        .filter(|r| r.method == "PATCH" && r.url.ends_with("/override"))
        .collect();
    let mut urls: Vec<&str> = puts.iter().map(|r| r.url.as_str()).collect();
    urls.sort();
    assert_eq!(
        urls,
        [
            "/api/v1/entities/e1/override",
            "/api/v1/entities/e2/override"
        ]
    );
    assert!(puts.iter().all(|r| r.body.contains("\"area\":\"kitchen\"")));
}
