#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::Router;
use oikos_api::{DeviceView, UpdateView};
use oikos_core::model::Availability;
use oikos_web::house::House;
use oikos_web::screens::settings::SettingsScreen;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

fn device(name: &str, update: Option<UpdateView>) -> DeviceView {
    DeviceView {
        id: format!("id-{name}"),
        ieee: "0x1".into(),
        friendly_name: name.into(),
        model: None,
        vendor: None,
        description: None,
        supported: true,
        interview_state: "SUCCESSFUL".into(),
        power_source: None,
        entity_ids: vec![],
        availability: Availability::Online,
        battery: None,
        linkquality: None,
        update,
    }
}

async fn mount(devices: Vec<DeviceView>) -> web_sys::Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    std::mem::forget(leptos::mount::mount_to(
        host.clone().unchecked_into(),
        move || {
            let house = House::new();
            house.devices.set(devices.clone());
            provide_context(house);
            provide_context(RwSignal::new(oikos_web::theme::Theme::Auto));
            view! { <Router><SettingsScreen /></Router> }
        },
    ));
    tick().await;
    host
}

#[wasm_bindgen_test]
async fn settings_lists_devices_with_updates() {
    let up = UpdateView {
        installed: Some("1.0".into()),
        latest: Some("1.1".into()),
        available: true,
    };
    let host = mount(vec![
        device("Hall lamp", Some(up)),
        device("Quiet plug", None),
    ])
    .await;
    let text = host.text_content().unwrap();
    assert!(text.contains("Updates available"));
    assert!(text.contains("Hall lamp"));
    assert!(text.contains("1.0 → 1.1"));
    assert!(!text.contains("All devices are up to date"));
}

#[wasm_bindgen_test]
async fn settings_shows_up_to_date_when_none() {
    let host = mount(vec![device("Quiet plug", None)]).await;
    assert!(
        host.text_content()
            .unwrap()
            .contains("All devices are up to date")
    );
}
