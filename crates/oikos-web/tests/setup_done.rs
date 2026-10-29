#![cfg(target_arch = "wasm32")]

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::Router;
use oikos_web::house::House;
use oikos_web::i18n::Lang;
use oikos_web::screens::settings::SettingsScreen;
use oikos_web::setup::SetupState;
use oikos_web::setup::done::DoneStep;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

#[path = "support/fetch_stub.rs"]
mod fetch_stub;
use fetch_stub::FetchStub;

const SKIPPED_KEY: &str = "oikos.setup.skipped";

const STATUS: &str = r#"{"mode":"running","step":"done","finished":false,"bundled":false,
"location":{"city":"Bordeaux","country":"FR","latitude":44.84,"longitude":-0.58,"timezone":"Europe/Paris"},
"mqtt":{"host":"192.0.2.10","port":1883,"user":null,"password":{"set":false},"client_id":null,"z2m_base":null},
"database":{"host":"192.0.2.11","port":5432,"database":"oikos","user":"oikos","password":{"set":false}},
"restarting":false}"#;

fn mount_done() -> web_sys::Element {
    let status = serde_json::from_str(STATUS).unwrap();
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    std::mem::forget(leptos::mount::mount_to(
        host.clone().unchecked_into(),
        move || {
            provide_context(RwSignal::new(Lang::En));
            provide_context(House::new());
            provide_context(SetupState(RwSignal::new(Some(status))));
            view! { <Router><DoneStep /></Router> }
        },
    ));
    host
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

fn pathname() -> String {
    web_sys::window().unwrap().location().pathname().unwrap()
}

#[wasm_bindgen_test]
async fn open_my_house_finishes_and_leaves() {
    let stub = FetchStub::install();
    stub.reply(
        "POST",
        "/api/v1/setup/finish",
        200,
        &STATUS.replace(r#""finished":false"#, r#""finished":true"#),
    );
    let window = web_sys::window().unwrap();
    let history = window.history().unwrap();
    let start = window.location().href().unwrap();
    let host = mount_done();
    settle().await;
    let open: web_sys::HtmlElement = host
        .query_selector(".done__open")
        .unwrap()
        .expect("Open my house")
        .unchecked_into();
    // Two quick clicks must send one request.
    open.click();
    open.click();
    settle().await;
    let posts = stub
        .requests()
        .into_iter()
        .filter(|r| r.method == "POST" && r.url == "/api/v1/setup/finish")
        .count();
    let path = pathname();
    history
        .replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(&start))
        .unwrap();
    assert_eq!(posts, 1);
    assert_eq!(path, "/dashboards/house");
}

#[wasm_bindgen_test]
async fn finish_error_is_shown() {
    let stub = FetchStub::install();
    stub.reply(
        "POST",
        "/api/v1/setup/finish",
        500,
        r#"{"error":"disk full"}"#,
    );
    let host = mount_done();
    settle().await;
    let open: web_sys::HtmlElement = host
        .query_selector(".done__open")
        .unwrap()
        .unwrap()
        .unchecked_into();
    open.click();
    settle().await;
    let problem = host.query_selector(".loc__problem").unwrap().unwrap();
    assert!(problem.text_content().unwrap().contains("disk full"));
}

#[wasm_bindgen_test]
async fn summary_shows_skipped_services() {
    let _stub = FetchStub::install();
    storage().set_item(SKIPPED_KEY, r#"["cameras"]"#).unwrap();
    let host = mount_done();
    settle().await;
    storage().remove_item(SKIPPED_KEY).unwrap();
    let text = |id: &str| {
        host.query_selector(&format!("#done-{id}"))
            .unwrap()
            .unwrap_or_else(|| panic!("no row {id}"))
            .text_content()
            .unwrap()
    };
    assert!(text("cameras").contains("Skipped"), "{}", text("cameras"));
    assert!(text("weather").contains("Not set up"));
    assert!(text("location").contains("Bordeaux, FR"));
    assert!(text("mqtt").contains("192.0.2.10:1883"));
    assert!(text("database").contains("192.0.2.11:5432 / oikos"));
}

#[wasm_bindgen_test]
async fn run_setup_again_is_listed() {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    std::mem::forget(leptos::mount::mount_to(
        host.clone().unchecked_into(),
        move || {
            provide_context(RwSignal::new(Lang::En));
            provide_context(House::new());
            provide_context(RwSignal::new(oikos_web::theme::Theme::Auto));
            view! { <Router><SettingsScreen /></Router> }
        },
    ));
    tick().await;
    assert!(host.query_selector("a[href=\"/setup\"]").unwrap().is_some());
}
