#![cfg(target_arch = "wasm32")]

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::tick;
use oikos_alarm::doc::AlarmSettings;
use oikos_alarm::view::AlarmView;
use oikos_api::SetupStep;
use oikos_web::i18n::Lang;
use oikos_web::setup::services::ServicesStep;
use oikos_web::setup::wizard::WizardNav;
use oikos_web::setup::{Applying, SetupState};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

#[path = "support/fetch_stub.rs"]
mod fetch_stub;
use fetch_stub::FetchStub;

const SKIPPED_KEY: &str = "oikos.setup.skipped";

struct Mounted {
    host: web_sys::Element,
    applying: Applying,
    _handle: Box<dyn std::any::Any>,
}

fn mount() -> Mounted {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let step = RwSignal::new(SetupStep::Services);
    let applying = Applying::new(|_| {});
    let handle = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(RwSignal::new(Lang::En));
        provide_context(WizardNav { step });
        provide_context(applying);
        provide_context(SetupState(RwSignal::new(None)));
        view! { <ServicesStep /> }
    });
    Mounted {
        host,
        applying,
        _handle: Box::new(handle),
    }
}

fn click(host: &web_sys::Element, selector: &str) {
    let el: web_sys::HtmlElement = host
        .query_selector(selector)
        .unwrap()
        .unwrap_or_else(|| panic!("no {selector}"))
        .unchecked_into();
    el.click();
}

async fn settle() {
    for _ in 0..8 {
        TimeoutFuture::new(15).await;
        tick().await;
    }
}

fn storage() -> web_sys::Storage {
    web_sys::window().unwrap().local_storage().unwrap().unwrap()
}

#[wasm_bindgen_test]
async fn climate_keeps_its_prefix() {
    let stub = FetchStub::install();
    stub.reply(
        "GET",
        "/api/v1/settings/climate",
        200,
        r#"{"enabled":false,"prefix":"house/climate"}"#,
    );
    stub.reply(
        "PUT",
        "/api/v1/settings/climate",
        200,
        r#"{"restarting":true}"#,
    );
    let m = mount();
    settle().await;
    click(&m.host, "#svc-climate .svc__head");
    settle().await;
    click(&m.host, "#svc-climate button.switch");
    tick().await;
    click(&m.host, "#svc-climate .svc__save");
    settle().await;
    let put = stub
        .requests()
        .into_iter()
        .find(|r| r.method == "PUT")
        .expect("a PUT");
    assert_eq!(put.url, "/api/v1/settings/climate");
    let body: serde_json::Value = serde_json::from_str(&put.body).unwrap();
    assert_eq!(
        body,
        serde_json::json!({"enabled":true,"prefix":"house/climate"})
    );
    assert_eq!(
        m.applying.target.get_untracked().as_deref(),
        Some("/setup?step=services")
    );
}

#[wasm_bindgen_test]
async fn alarm_switch_needs_no_restart() {
    let stub = FetchStub::install();
    let view = AlarmView {
        enabled: false,
        entity_id: "alarm_control_panel.oikos_alarm_house".into(),
        state: "disarmed".into(),
        mode: None,
        source: None,
        since: 0,
        deadline: None,
        cause: vec![],
        open_at_arm: vec![],
        settings: AlarmSettings::default(),
        settings_invalid: None,
        warnings: vec![],
    };
    stub.reply(
        "GET",
        "/api/v1/alarm",
        200,
        &serde_json::to_string(&view).unwrap(),
    );
    stub.reply("PUT", "/api/v1/alarm/settings", 200, "{}");
    let m = mount();
    settle().await;
    click(&m.host, "#svc-alarm .svc__head");
    settle().await;
    click(&m.host, "#svc-alarm button.switch");
    settle().await;
    let put = stub
        .requests()
        .into_iter()
        .find(|r| r.method == "PUT")
        .expect("a PUT");
    assert_eq!(put.url, "/api/v1/alarm/settings");
    let body: serde_json::Value = serde_json::from_str(&put.body).unwrap();
    assert_eq!(body["enabled"], serde_json::json!(true));
    assert_eq!(m.applying.target.get_untracked(), None);
    let pill = m
        .host
        .query_selector("#svc-alarm .svc__pill")
        .unwrap()
        .unwrap();
    assert_eq!(pill.text_content().unwrap(), "Set up");
}

#[wasm_bindgen_test]
async fn skip_marks_the_card() {
    let _stub = FetchStub::install();
    let storage = storage();
    let before = storage.get_item(SKIPPED_KEY).unwrap();
    storage.remove_item(SKIPPED_KEY).unwrap();
    let m = mount();
    settle().await;
    let pill = |host: &web_sys::Element| {
        host.query_selector("#svc-weather .svc__pill")
            .unwrap()
            .unwrap()
            .text_content()
            .unwrap()
    };
    assert_eq!(pill(&m.host), "Not set up");
    click(&m.host, "#svc-weather .svc__head");
    settle().await;
    click(&m.host, "#svc-weather .svc__skip");
    settle().await;
    assert_eq!(pill(&m.host), "Skipped");
    let again = mount();
    settle().await;
    assert_eq!(pill(&again.host), "Skipped");
    match before {
        Some(v) => storage.set_item(SKIPPED_KEY, &v).unwrap(),
        None => storage.remove_item(SKIPPED_KEY).unwrap(),
    }
}
