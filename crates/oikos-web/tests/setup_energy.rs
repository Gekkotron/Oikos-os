#![cfg(target_arch = "wasm32")]

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::tick;
use oikos_api::EntityView;
use oikos_core::model::{Capabilities, EntityCategory, EntityState, Kind, UiOverride};
use oikos_web::house::House;
use oikos_web::i18n::Lang;
use oikos_web::setup::services::energy::EnergyForm;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

#[path = "support/fetch_stub.rs"]
mod fetch_stub;
use fetch_stub::FetchStub;

fn sensor(id: &str, device_class: &str, role: Option<&str>) -> EntityView {
    EntityView {
        id: id.into(),
        device_id: None,
        name: id.into(),
        device_name: None,
        label: None,
        kind: Kind::Sensor,
        component: Kind::Sensor.component().to_string(),
        protocol_kind: Kind::Sensor,
        device_class: Some(device_class.into()),
        unit: None,
        icon: None,
        category: EntityCategory::Primary,
        enabled_by_default: true,
        area: "salon".into(),
        part_of: None,
        capabilities: Capabilities::default(),
        ui: UiOverride {
            energy_role: role.map(Into::into),
            ..Default::default()
        },
        state: EntityState::default(),
    }
}

fn mount(house: House) -> web_sys::Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(RwSignal::new(Lang::En));
        provide_context(house);
        view! { <EnergyForm /> }
    })
    .forget();
    host
}

fn change(el: &web_sys::Element) {
    el.dispatch_event(
        &web_sys::Event::new_with_event_init_dict("change", &{
            let init = web_sys::EventInit::new();
            init.set_bubbles(true);
            init
        })
        .unwrap(),
    )
    .unwrap();
}

async fn settle() {
    for _ in 0..8 {
        TimeoutFuture::new(15).await;
        tick().await;
    }
}

fn house_with(entities: Vec<EntityView>) -> House {
    let house = House::new();
    house
        .entities
        .set(entities.into_iter().map(|e| (e.id.clone(), e)).collect());
    house
}

fn patches(stub: &FetchStub) -> Vec<(String, String)> {
    stub.requests()
        .into_iter()
        .filter(|r| r.method == "PATCH" && r.url.ends_with("/override"))
        .map(|r| (r.url, r.body))
        .collect()
}

#[wasm_bindgen_test]
async fn moving_a_role_clears_the_old_holder() {
    let stub = FetchStub::install();
    let a = sensor("meter_a", "power", Some("grid"));
    let b = sensor("meter_b", "energy", None);
    stub.reply("GET", "/api/v1/home/settings", 200, "{}");
    for e in [&a, &b] {
        stub.reply(
            "PATCH",
            &format!("/api/v1/entities/{}/override", e.id),
            200,
            &serde_json::to_string(e).unwrap(),
        );
    }
    let host = mount(house_with(vec![a, b]));
    tick().await;
    let select: web_sys::HtmlSelectElement = host
        .query_selector("select.energy__role")
        .unwrap()
        .expect("grid select")
        .unchecked_into();
    assert_eq!(select.value(), "meter_a");
    select.set_value("meter_b");
    change(&select);
    settle().await;
    let got = patches(&stub);
    assert_eq!(got.len(), 2, "{got:?}");
    assert_eq!(got[0].0, "/api/v1/entities/meter_a/override");
    assert!(got[0].1.contains("\"energyRole\":null"), "{}", got[0].1);
    assert_eq!(got[1].0, "/api/v1/entities/meter_b/override");
    assert!(got[1].1.contains("\"energyRole\":\"grid\""), "{}", got[1].1);
}

#[wasm_bindgen_test]
async fn a_failed_move_is_reported_and_the_select_keeps_the_real_holder() {
    let stub = FetchStub::install();
    let a = sensor("meter_a", "power", Some("grid"));
    let b = sensor("meter_b", "energy", None);
    stub.reply("GET", "/api/v1/home/settings", 200, "{}");
    stub.reply(
        "PATCH",
        "/api/v1/entities/meter_a/override",
        500,
        r#"{"error":"hub exploded"}"#,
    );
    let host = mount(house_with(vec![a, b]));
    tick().await;
    let select: web_sys::HtmlSelectElement = host
        .query_selector("select.energy__role")
        .unwrap()
        .unwrap()
        .unchecked_into();
    select.set_value("meter_b");
    change(&select);
    settle().await;
    let alert = host
        .query_selector("[role=alert]")
        .unwrap()
        .expect("alert line")
        .text_content()
        .unwrap();
    assert!(alert.contains("hub exploded"), "alert: {alert}");
    assert_eq!(select.value(), "meter_a");
    assert_eq!(patches(&stub).len(), 1);
}

#[wasm_bindgen_test]
async fn off_peak_from_the_card_keeps_the_rest_of_home_settings() {
    let stub = FetchStub::install();
    let doc = r#"{"eco_setpoint":17.5,"mode_entity":"virtual_mode","indoor_entity":null,"cold_below":12.0,"hot_above":28.0,"off_peak":[],"off_peak_history":[]}"#;
    stub.reply("GET", "/api/v1/home/settings", 200, doc);
    stub.reply("PUT", "/api/v1/home/settings", 200, doc);
    let host = mount(house_with(vec![]));
    settle().await;
    let add: web_sys::HtmlElement = host
        .query_selector(".home-settings__add")
        .unwrap()
        .expect("add button")
        .unchecked_into();
    add.click();
    tick().await;
    let times = host.query_selector_all("input[type=time]").unwrap();
    assert_eq!(times.length(), 2);
    for (i, v) in ["22:00", "06:00"].into_iter().enumerate() {
        let input: web_sys::HtmlInputElement = times.item(i as u32).unwrap().unchecked_into();
        input.set_value(v);
        change(&input);
    }
    let save: web_sys::HtmlElement = host
        .query_selector(".home-settings__save")
        .unwrap()
        .expect("save button")
        .unchecked_into();
    save.click();
    settle().await;
    let put = stub
        .requests()
        .into_iter()
        .find(|r| r.method == "PUT" && r.url == "/api/v1/home/settings")
        .expect("a PUT");
    let sent: serde_json::Value = serde_json::from_str(&put.body).unwrap();
    let mut expected: serde_json::Value = serde_json::from_str(doc).unwrap();
    expected["off_peak"] = serde_json::json!([{"start":"22:00","end":"06:00"}]);
    assert_eq!(sent, expected);
}
