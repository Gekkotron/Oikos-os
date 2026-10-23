#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;
use oikos_api::{AreaView, EntityView, WsEvent};
use oikos_core::model::{Capabilities, EntityCategory, EntityState, Kind, UiOverride};
use oikos_web::components::toast::Toasts;
use oikos_web::house::House;
use oikos_web::screens::group_wizard::GroupWizardScreen;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

fn entity(id: &str, kind: Kind, protocol_kind: Kind, area: &str) -> EntityView {
    EntityView {
        id: id.into(),
        device_id: None,
        name: id.into(),
        device_name: None,
        label: None,
        kind,
        component: kind.component().to_string(),
        protocol_kind,
        device_class: None,
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
        entity_count: 1,
        primary_count: 1,
        group_names: Default::default(),
        group_favorites: Default::default(),
    }
}

fn mount() -> web_sys::Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    web_sys::window()
        .unwrap()
        .history()
        .unwrap()
        .replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some("/groups/new"))
        .unwrap();
    let house = House::new();
    for e in [
        entity("lamp", Kind::Light, Kind::Light, "salon"),
        entity("plug", Kind::Light, Kind::Switch, "cuisine"),
        entity("fan", Kind::Switch, Kind::Switch, "salon"),
    ] {
        house.apply(WsEvent::EntityUpserted(e));
    }
    house.apply(WsEvent::AreasChanged {
        areas: vec![area("salon", "Salon"), area("cuisine", "Cuisine")],
    });
    let mounted = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(house);
        provide_context(Toasts::new());
        oikos_web::nav::install_nav();
        view! {
            <Router>
                <Routes fallback=|| "fallback">
                    <Route path=path!("/groups/new") view=GroupWizardScreen />
                </Routes>
            </Router>
        }
    });
    mounted.forget();
    host
}

async fn click(host: &web_sys::Element, selector: &str, text: &str) {
    let buttons = host.query_selector_all(selector).unwrap();
    for i in 0..buttons.length() {
        let el: web_sys::HtmlElement = buttons.item(i).unwrap().unchecked_into();
        if el.text_content().unwrap().contains(text) {
            el.click();
            tick().await;
            return;
        }
    }
    panic!("no {selector} reading {text:?}");
}

#[wasm_bindgen_test]
async fn step_one_lists_controllable_kinds_only() {
    let host = mount();
    tick().await;
    let text = host.text_content().unwrap();
    for kind in [
        "light",
        "switch",
        "cover",
        "climate",
        "number",
        "select",
        "button",
        "vacuum",
        "water heater",
    ] {
        assert!(text.contains(kind), "{kind} missing from {text}");
    }
    for kind in [
        "sensor",
        "binary sensor",
        "alarm control panel",
        "alarm panel",
    ] {
        assert!(!text.contains(kind), "{kind} shown in {text}");
    }
}

#[wasm_bindgen_test]
async fn step_two_filters_by_effective_kind_and_groups_by_room() {
    let host = mount();
    tick().await;
    click(&host, "button.card", "light").await;
    click(&host, "button.btn", "Next").await;
    let text = host.text_content().unwrap();
    assert!(text.contains("Salon") && text.contains("Cuisine"), "{text}");
    assert_eq!(host.query_selector_all("button.card").unwrap().length(), 2);
    assert!(!text.contains("fan"), "{text}");
}

#[wasm_bindgen_test]
async fn step_three_hides_members_by_default() {
    let host = mount();
    tick().await;
    click(&host, "button.card", "light").await;
    click(&host, "button.btn", "Next").await;
    click(&host, "button.card", "lamp").await;
    click(&host, "button.btn", "Next").await;
    let switches = host.query_selector_all("[role=switch]").unwrap();
    let toggle = (0..switches.length())
        .map(|i| {
            switches
                .item(i)
                .unwrap()
                .unchecked_into::<web_sys::Element>()
        })
        .find(|el| el.text_content().unwrap().contains("Hide members on Home"))
        .unwrap_or_else(|| panic!("no Hide members switch in {}", host.inner_html()));
    assert_eq!(
        toggle.get_attribute("aria-checked").as_deref(),
        Some("true")
    );
}
