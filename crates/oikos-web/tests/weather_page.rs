#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::Router;
use oikos_api::WeatherView;
use oikos_web::house::House;
use oikos_web::i18n::Lang;
use oikos_web::screens::weather::WeatherScreen;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

fn full_weather() -> WeatherView {
    let day = |d: u32| {
        serde_json::json!({
            "date": format!("2026-10-{d}"), "condition": "rain",
            "min": 9.0, "max": 13.0, "precipitation_probability": 80.0
        })
    };
    serde_json::from_value(serde_json::json!({
        "place": {"name": "X", "admin": "Y"},
        "fetched_at": (js_sys::Date::now() / 1000.0) as u64, "stale": false,
        "current": {
            "temperature": 9.4, "apparent_temperature": 5.3, "humidity": 70.0,
            "cloud_cover": 81.0, "pressure": 1015.0, "wind_speed": 19.0,
            "wind_gusts": 48.0, "wind_direction": 246.0, "uv_index": 0.0,
            "condition": "showers", "daylight": true
        },
        "today": {
            "min": 9.0, "max": 14.0, "precipitation": 6.0,
            "precipitation_probability": 93.0, "wind_gusts_max": 73.0
        },
        "rain_next_hour": 0.0, "tonight_min": 9.0,
        "days": [day(24), day(25), day(26), day(27), day(28)]
    }))
    .expect("weather fixture")
}

async fn mount(weather: Option<WeatherView>, loaded: bool) -> web_sys::Element {
    mount_in(RwSignal::new(Lang::En), weather, loaded).await
}

async fn mount_in(
    lang: RwSignal<Lang>,
    weather: Option<WeatherView>,
    loaded: bool,
) -> web_sys::Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let house = House::new();
    house.weather.set(weather);
    house.weather_loaded.set(loaded);
    std::mem::forget(leptos::mount::mount_to(
        host.clone().unchecked_into(),
        move || {
            oikos_web::nav::install_nav();
            provide_context(house);
            provide_context(lang);
            view! { <Router><WeatherScreen /></Router> }
        },
    ));
    tick().await;
    host
}

#[wasm_bindgen_test]
async fn weather_page_shows_tiles_and_days() {
    let host = mount(Some(full_weather()), true).await;
    assert_eq!(
        host.query_selector_all(".weather-tile").unwrap().length(),
        8
    );
    assert_eq!(host.query_selector_all(".weather-day").unwrap().length(), 5);
    assert!(host.query_selector(".sheet__back").unwrap().is_some());
    let text = host.text_content().unwrap_or_default();
    assert!(text.contains("Open-Meteo"), "{text}");
    assert!(text.contains("No rain within the hour"), "{text}");
}

#[wasm_bindgen_test]
async fn weather_page_unavailable() {
    let host = mount(None, true).await;
    assert_eq!(
        host.query_selector_all(".weather-tile").unwrap().length(),
        0
    );
    let text = host.text_content().unwrap_or_default();
    assert!(text.contains("The weather is unavailable."), "{text}");
}

#[wasm_bindgen_test]
async fn weather_page_is_empty_before_the_first_fetch() {
    let host = mount(None, false).await;
    let text = host.text_content().unwrap_or_default();
    assert!(!text.contains("unavailable"), "{text}");
    assert!(host.query_selector(".sheet__back").unwrap().is_some());
}

#[wasm_bindgen_test]
async fn weather_page_back_link_is_compact() {
    let host = mount(Some(full_weather()), true).await;
    let back = host.query_selector(".sheet__back").unwrap().unwrap();
    let page = host.query_selector(".weather-page").unwrap().unwrap();
    assert!(back.get_bounding_client_rect().width() < page.get_bounding_client_rect().width());
}

/// Units follow a no-break space in both languages, and the page flips language in place.
#[wasm_bindgen_test]
async fn the_tiles_use_no_break_units_and_follow_the_language() {
    let lang = RwSignal::new(Lang::Fr);
    let host = mount_in(lang, Some(full_weather()), true).await;
    let fr = host.text_content().unwrap_or_default();
    for unit in ["19\u{A0}km/h", "70\u{A0}%", "1015\u{A0}hPa", "6,0\u{A0}mm"] {
        assert!(fr.contains(unit), "{unit} in {fr}");
    }
    for ascii in ["19 km/h", "70 %", "6,0 mm"] {
        assert!(!fr.contains(ascii), "{ascii} in {fr}");
    }
    assert!(fr.contains("Vent"), "{fr}");
    assert!(fr.contains("risque 93\u{A0}%"), "{fr}");

    lang.set(Lang::En);
    tick().await;
    let en = host.text_content().unwrap_or_default();
    assert!(en.contains("Wind"), "{en}");
    assert!(en.contains("6.0\u{A0}mm"), "{en}");
    assert!(en.contains("93\u{A0}% chance"), "{en}");
    assert!(!en.contains("Vent"), "{en}");
}
