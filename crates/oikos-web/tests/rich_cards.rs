#![cfg(target_arch = "wasm32")]
//! The rich cards: what each shows on a tile and what its sheet adds.

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::Router;
use oikos_api::EntityView;
use oikos_core::model::{
    Capabilities, EntityCategory, EntityState, Kind, StateValue, UiOverride, WaterHeaterState,
};
use oikos_web::commands::Pending;
use oikos_web::components::card_place::{CardPlace, provide_card_place};
use oikos_web::components::toast::Toasts;
use oikos_web::house::House;
use oikos_web::screens::home::card_for;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

fn entity(id: &str, kind: Kind, value: StateValue) -> EntityView {
    EntityView {
        id: id.into(),
        device_id: None,
        name: "Thing".into(),
        device_name: None,
        label: None,
        kind,
        component: kind.component().to_string(),
        protocol_kind: kind,
        device_class: None,
        unit: None,
        icon: None,
        category: EntityCategory::Primary,
        enabled_by_default: true,
        area: "salon".into(),
        part_of: None,
        capabilities: Capabilities::default(),
        ui: UiOverride::default(),
        state: EntityState {
            value,
            ..Default::default()
        },
    }
}

/// Mounts the card for `entity` in `place` (the default, Overview, when `None`).
fn mount(entity: EntityView, place: Option<CardPlace>) -> web_sys::Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    std::mem::forget(leptos::mount::mount_to(
        host.clone().unchecked_into(),
        move || {
            provide_context(House::new());
            provide_context(Toasts::new());
            provide_context(Pending::new());
            if let Some(place) = place {
                provide_card_place(place);
            }
            view! { <Router>{card_for(entity)}</Router> }
        },
    ));
    host
}

fn has(host: &web_sys::Element, selector: &str) -> bool {
    host.query_selector(selector).unwrap().is_some()
}

fn water_heater() -> EntityView {
    let mut e = entity(
        "water_heater.tank",
        Kind::WaterHeater,
        StateValue::WaterHeater(WaterHeaterState {
            mode: Some("heat".into()),
            current: Some(15.0),
            target: Some(55.0),
        }),
    );
    e.capabilities.hvac_modes = vec!["heat".into(), "off".into()];
    e
}

#[wasm_bindgen_test]
async fn water_heater_tile_has_menu_not_stepper() {
    let host = mount(water_heater(), None);
    tick().await;
    assert!(has(
        &host,
        ".card__trailing button[aria-label=\"More options\"]"
    ));
    assert!(!has(&host, ".stepper"));
    assert!(!has(&host, "select"));
}

#[wasm_bindgen_test]
async fn water_heater_sheet_has_full_controls() {
    let host = mount(water_heater(), Some(CardPlace::Sheet));
    tick().await;
    assert!(has(&host, ".stepper"));
    assert!(has(&host, "select"));
}
