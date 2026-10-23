#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::Router;
use oikos_api::{EntityView, WeatherView, WsEvent};
use oikos_core::model::{
    Capabilities, EntityCategory, EntityState, Kind, LightState, StateValue, UiOverride,
};
use oikos_web::commands::Pending;
use oikos_web::components::toast::Toasts;
use oikos_web::home_header::view::HomeHeader;
use oikos_web::house::House;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

fn entity(id: &str, kind: Kind, device_class: Option<&str>, value: StateValue) -> EntityView {
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
            value,
            ..Default::default()
        },
    }
}

fn weather(temperature: f64, fetched_at: u64) -> WeatherView {
    serde_json::from_value(serde_json::json!({
        "place": {"name": "X"},
        "fetched_at": fetched_at, "stale": false,
        "current": {"temperature": temperature, "condition": "showers", "daylight": true},
        "today": {"min": 5.0, "max": 14.0},
        "rain_next_hour": null, "tonight_min": null, "days": []
    }))
    .expect("weather fixture")
}

async fn mount(weather: Option<WeatherView>) -> web_sys::Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let house = House::new();
    house.apply(WsEvent::EntityUpserted(entity(
        "light.lamp",
        Kind::Light,
        None,
        StateValue::Light(LightState {
            on: true,
            ..Default::default()
        }),
    )));
    house.apply(WsEvent::EntityUpserted(entity(
        "sensor.temp",
        Kind::Sensor,
        Some("temperature"),
        StateValue::Number(20.0),
    )));
    house.weather.set(weather);
    std::mem::forget(leptos::mount::mount_to(
        host.clone().unchecked_into(),
        move || {
            provide_context(house);
            provide_context(Toasts::new());
            provide_context(Pending::new());
            view! { <Router><HomeHeader /></Router> }
        },
    ));
    tick().await;
    host
}

#[wasm_bindgen_test]
async fn header_shows_sentence_and_chips() {
    let host = mount(None).await;
    let text = host.text_content().unwrap_or_default();
    assert!(
        text.contains("The house is awake, it's comfortable."),
        "{text}"
    );
    let chip = host
        .query_selector(".home-chip--lit")
        .unwrap()
        .expect("lights chip");
    assert!(chip.text_content().unwrap().contains('1'));
    assert!(
        host.query_selector(".home-header__weather")
            .unwrap()
            .is_none()
    );
}

#[wasm_bindgen_test]
async fn header_shows_weather_card() {
    let host = mount(Some(weather(9.4, (js_sys::Date::now() / 1000.0) as u64))).await;
    let card = host
        .query_selector(".home-header__weather")
        .unwrap()
        .expect("weather card");
    assert_eq!(card.get_attribute("href").as_deref(), Some("/weather"));
    assert!(card.text_content().unwrap().contains("9°"));
}
