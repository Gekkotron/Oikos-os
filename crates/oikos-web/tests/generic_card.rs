#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::Router;
use oikos_api::EntityView;
use oikos_core::model::{
    Capabilities, EntityCategory, EntityState, GenericState, GenericValue, Kind, StateValue,
    UiOverride,
};
use oikos_core::types::{self, GenericCaps};
use oikos_web::cards::generic::GenericCard;
use oikos_web::commands::Pending;
use oikos_web::components::toast::Toasts;
use oikos_web::house::House;
use oikos_web::screens::home::card_for;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

fn generic(component: &str, controls: &[&str], text: &str) -> EntityView {
    let mut g = GenericState::default();
    g.fields
        .insert("state".into(), GenericValue::Text(text.into()));
    EntityView {
        id: format!("{component}.x"),
        device_id: None,
        name: "Thing".into(),
        device_name: None,
        label: None,
        kind: Kind::Generic,
        component: component.into(),
        protocol_kind: Kind::Generic,
        device_class: None,
        unit: None,
        icon: None,
        category: EntityCategory::Primary,
        enabled_by_default: true,
        area: "salon".into(),
        part_of: None,
        capabilities: Capabilities {
            generic: Some(GenericCaps {
                controls: controls.iter().map(|s| s.to_string()).collect(),
                ..Default::default()
            }),
            ..Default::default()
        },
        ui: UiOverride::default(),
        state: EntityState {
            value: StateValue::Generic(g),
            ..Default::default()
        },
    }
}

fn mount(entity: EntityView, use_card_for: bool) -> web_sys::Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    std::mem::forget(leptos::mount::mount_to(
        host.clone().unchecked_into(),
        move || {
            provide_context(House::new());
            provide_context(Toasts::new());
            provide_context(Pending::new());
            let entity = entity.clone();
            view! {
                <Router>
                    {if use_card_for {
                        card_for(entity)
                    } else {
                        view! { <GenericCard entity=entity /> }.into_any()
                    }}
                </Router>
            }
        },
    ));
    host
}

#[wasm_bindgen_test]
async fn lock_card_shows_locked_and_unlock_button() {
    assert!(types::type_def("lock").is_some());
    let host = mount(generic("lock", &["locked"], "locked"), false);
    tick().await;
    let text = host.text_content().unwrap();
    assert!(text.contains("Locked"), "{text}");
    assert!(text.contains("Unlock"), "{text}");
}

#[wasm_bindgen_test]
async fn unlocked_lock_card_has_alert_class() {
    let host = mount(generic("lock", &["locked"], "unlocked"), false);
    tick().await;
    assert!(host.query_selector(".card--alert").unwrap().is_some());
}

#[wasm_bindgen_test]
async fn generic_card_without_main_control_shows_state_only() {
    let host = mount(generic("device_tracker", &[], "home"), false);
    tick().await;
    assert!(host.text_content().unwrap().contains("Home"));
    // The header is the only button.
    assert_eq!(
        host.query_selector_all("button:not(.card__header)")
            .unwrap()
            .length(),
        0
    );
}

#[wasm_bindgen_test]
async fn main_control_override_none_hides_the_control() {
    let mut e = generic("lock", &["locked"], "locked");
    e.ui.main_control = Some("none".into());
    let host = mount(e, false);
    tick().await;
    assert_eq!(
        host.query_selector_all("button:not(.card__header)")
            .unwrap()
            .length(),
        0
    );
}

#[wasm_bindgen_test]
async fn remote_has_no_home_card() {
    let host = mount(generic("device_automation", &[], "press"), true);
    tick().await;
    assert!(host.query_selector(".card").unwrap().is_none());
}
