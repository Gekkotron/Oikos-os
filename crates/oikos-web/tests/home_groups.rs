#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::Router;
use oikos_api::{AreaView, EntityView, WsEvent};
use oikos_core::model::{
    Availability, Capabilities, CoverState, EntityCategory, EntityState, GroupKey, GroupKind, Kind,
    LightState, StateValue, UiOverride,
};
use oikos_web::cards::group::{CoverGroupCard, LightGroupCard};
use oikos_web::commands::Pending;
use oikos_web::components::toast::Toasts;
use oikos_web::house::House;
use oikos_web::nav::HomeRoom;
use oikos_web::screens::home::HomeScreen;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

fn entity(id: &str, kind: Kind, value: StateValue, availability: Availability) -> EntityView {
    EntityView {
        id: id.into(),
        device_id: None,
        name: id.into(),
        device_name: None,
        label: None,
        kind,
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
            availability,
            ..Default::default()
        },
    }
}

fn host() -> web_sys::Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    host
}

fn salon(house: House) {
    house.apply(WsEvent::AreasChanged {
        areas: vec![AreaView {
            id: "salon".into(),
            name: "Salon".into(),
            icon: None,
            order: None,
            entity_count: 2,
            primary_count: 2,
            group_names: Default::default(),
            group_favorites: Default::default(),
        }],
    });
}

#[wasm_bindgen_test]
async fn two_lights_show_one_group_card() {
    let host = host();
    let house = House::new();
    for id in ["light.a", "light.b"] {
        house.apply(WsEvent::EntityUpserted(entity(
            id,
            Kind::Light,
            StateValue::Light(LightState::default()),
            Availability::Online,
        )));
    }
    salon(house);
    let _mounted = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(house);
        provide_context(Toasts::new());
        provide_context(Pending::new());
        provide_context(HomeRoom(RwSignal::new(None)));
        view! {
            <Router>
                <HomeScreen />
            </Router>
        }
    });
    tick().await;
    let cards = host.query_selector_all(".card--wide").unwrap();
    assert_eq!(cards.length(), 1);
    let text = host
        .query_selector(".card--wide")
        .unwrap()
        .unwrap()
        .text_content()
        .unwrap();
    assert!(text.contains("Lights"), "{text}");
    assert!(text.contains("×2"), "{text}");
    assert!(text.contains("All off"), "{text}");
    assert_eq!(host.query_selector_all(".card").unwrap().length(), 1);
}

#[wasm_bindgen_test]
async fn cover_group_buttons_reach_every_member() {
    let host = host();
    let house = House::new();
    let cover = |id: &str, availability| {
        entity(
            id,
            Kind::Cover,
            StateValue::Cover(CoverState {
                state: Some("open".into()),
                position: Some(100),
            }),
            availability,
        )
    };
    for (id, availability) in [
        ("cover.a", Availability::Online),
        ("cover.b", Availability::Online),
        ("cover.c", Availability::Offline),
    ] {
        house.apply(WsEvent::EntityUpserted(cover(id, availability)));
    }
    salon(house);
    let pending = Pending::new();
    let key = GroupKey {
        area: "salon".into(),
        kind: GroupKind::Cover,
    };
    let _mounted = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(house);
        provide_context(Toasts::new());
        provide_context(pending);
        view! {
            <Router>
                <CoverGroupCard
                    key=key.clone()
                    members=vec!["cover.a".to_owned(), "cover.b".to_owned(), "cover.c".to_owned()]
                />
            </Router>
        }
    });
    tick().await;
    let card = host.query_selector(".card--wide").unwrap().expect("card");
    assert_eq!(
        card.get_attribute("aria-label").as_deref(),
        Some("Shutters, Salon, 3 open, 0 closed")
    );
    let close = host
        .query_selector("[aria-label='Close all shutters']")
        .unwrap()
        .expect("close button");
    close.unchecked_into::<web_sys::HtmlElement>().click();
    tick().await;
    let held = pending.0.get_untracked();
    assert!(
        held.contains("cover.a") && held.contains("cover.b"),
        "{held:?}"
    );
    assert!(!held.contains("cover.c"), "{held:?}");
}

#[wasm_bindgen_test]
async fn offline_light_group_is_unavailable_and_disabled() {
    let host = host();
    let house = House::new();
    for id in ["light.a", "light.b"] {
        house.apply(WsEvent::EntityUpserted(entity(
            id,
            Kind::Light,
            StateValue::Light(LightState::default()),
            Availability::Offline,
        )));
    }
    salon(house);
    let key = GroupKey {
        area: "salon".into(),
        kind: GroupKind::Light,
    };
    let _mounted = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(house);
        provide_context(Toasts::new());
        provide_context(Pending::new());
        view! {
            <Router>
                <LightGroupCard
                    key=key.clone()
                    members=vec!["light.a".to_owned(), "light.b".to_owned()]
                />
            </Router>
        }
    });
    tick().await;
    let toggle = host
        .query_selector("[aria-label='All lights']")
        .unwrap()
        .expect("the switch is named for its purpose");
    assert!(toggle.has_attribute("disabled"));
    assert!(toggle.text_content().unwrap().contains("Unavailable"));
}

#[wasm_bindgen_test]
async fn favorites_group_card_holds_only_its_members() {
    let host = host();
    let house = House::new();
    for id in ["light.a", "light.b", "light.c"] {
        let mut e = entity(
            id,
            Kind::Light,
            StateValue::Light(LightState::default()),
            Availability::Online,
        );
        e.ui.favorite = id != "light.c";
        house.apply(WsEvent::EntityUpserted(e));
    }
    salon(house);
    let _mounted = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(house);
        provide_context(Toasts::new());
        provide_context(Pending::new());
        provide_context(HomeRoom(RwSignal::new(Some("__favorites__".to_owned()))));
        view! {
            <Router>
                <HomeScreen />
            </Router>
        }
    });
    tick().await;
    let text = host
        .query_selector(".card--wide")
        .unwrap()
        .expect("a group card")
        .text_content()
        .unwrap();
    assert!(text.contains("×2"), "{text}");
}
