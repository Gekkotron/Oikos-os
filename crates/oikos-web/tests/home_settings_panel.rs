#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use oikos_api::{EntityView, HomeSettings, WsEvent};
use oikos_core::model::{Capabilities, EntityCategory, EntityState, Kind, StateValue, UiOverride};
use oikos_web::components::toast::Toasts;
use oikos_web::house::House;
use oikos_web::screens::home_settings_panel::HomeSettingsPanel;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

fn entity(id: &str, kind: Kind, device_class: Option<&str>) -> EntityView {
    EntityView {
        id: id.into(),
        device_id: None,
        name: id.into(),
        device_name: None,
        label: None,
        kind,
        protocol_kind: kind,
        component: kind.component().to_string(),
        device_class: device_class.map(Into::into),
        unit: None,
        icon: None,
        category: EntityCategory::Primary,
        enabled_by_default: true,
        area: "salon".into(),
        part_of: None,
        capabilities: Capabilities::default(),
        ui: UiOverride::default(),
        state: EntityState {
            value: StateValue::Unknown,
            ..Default::default()
        },
    }
}

async fn mount() -> (web_sys::Element, House) {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let house = House::new();
    for e in [
        entity("alpha_select", Kind::Select, None),
        entity("virtual_mode", Kind::Select, None),
        entity("t_hall", Kind::Sensor, Some("temperature")),
    ] {
        house.apply(WsEvent::EntityUpserted(e));
    }
    std::mem::forget(leptos::mount::mount_to(
        host.clone().unchecked_into(),
        move || {
            provide_context(house);
            provide_context(Toasts::new());
            view! { <HomeSettingsPanel /> }
        },
    ));
    tick().await;
    (host, house)
}

fn input(host: &web_sys::Element, id: &str) -> web_sys::HtmlInputElement {
    host.query_selector(&format!("#{id}"))
        .unwrap()
        .unwrap()
        .unchecked_into()
}

#[wasm_bindgen_test]
async fn panel_lists_selects_and_saves_draft() {
    let (host, house) = mount().await;
    let values: Vec<String> = {
        let options = host.query_selector_all("#home-Mode option").unwrap();
        (0..options.length())
            .map(|i| {
                options
                    .item(i)
                    .unwrap()
                    .unchecked_into::<web_sys::HtmlOptionElement>()
                    .value()
            })
            .collect()
    };
    // None, then the hub's virtual select, then the others; the sensor is no mode.
    assert_eq!(values, ["", "virtual_mode", "alpha_select"]);
    assert_eq!(input(&host, "home-Cold").placeholder(), "15");
    assert_eq!(input(&host, "home-Hot").placeholder(), "26");

    house.home_settings.set(HomeSettings {
        cold_below: Some(12.0),
        ..Default::default()
    });
    tick().await;
    tick().await;
    assert_eq!(input(&host, "home-Cold").value(), "12");
}

fn mode_values(host: &web_sys::Element) -> Vec<String> {
    let options = host.query_selector_all("#home-Mode option").unwrap();
    (0..options.length())
        .map(|i| {
            options
                .item(i)
                .unwrap()
                .unchecked_into::<web_sys::HtmlOptionElement>()
                .value()
        })
        .collect()
}

#[wasm_bindgen_test]
async fn mode_list_hides_non_primary_selects_but_keeps_the_chosen_one() {
    let (host, house) = mount().await;
    let mut diag = entity("diag_select", Kind::Select, None);
    diag.category = EntityCategory::Diagnostic;
    house.apply(WsEvent::EntityUpserted(diag));
    tick().await;
    assert!(!mode_values(&host).contains(&"diag_select".to_owned()));
    house.home_settings.set(HomeSettings {
        mode_entity: Some("diag_select".into()),
        ..Default::default()
    });
    tick().await;
    tick().await;
    assert!(mode_values(&host).contains(&"diag_select".to_owned()));
}
