#![cfg(target_arch = "wasm32")]

#[path = "support/fetch_stub.rs"]
mod fetch_stub;

use fetch_stub::FetchStub;
use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::Router;
use oikos_api::{AlertChannel, AlertSeverity, AlertView, CameraInfo, CamerasView, Span};
use oikos_web::cameras::CameraList;
use oikos_web::components::now::Now;
use oikos_web::dashboards::camera_widget::CameraWidget;
use oikos_web::house::House;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

const NOW: u64 = 1_800_000_000;
const SNAPSHOT: &str = "/api/v1/media?url=snap";

fn alert() -> AlertView {
    AlertView {
        id: "frigate-1".into(),
        channel: AlertChannel::Camera,
        severity: AlertSeverity::Warning,
        title: "Front Door".into(),
        body: "Car detected".into(),
        image_url: Some(SNAPSHOT.into()),
        video_url: None,
        entity_id: None,
        camera: Some("front_door".into()),
        actions: vec![],
        source: Some("frigate".into()),
        silent: false,
        resolved: false,
        sent_at: NOW - 600,
    }
}

fn camera_list(live: bool) -> CamerasView {
    CamerasView {
        live,
        cameras: vec![
            CameraInfo {
                id: "front_door".into(),
                name: "Front Door".into(),
            },
            CameraInfo {
                id: "garden".into(),
                name: "Garden".into(),
            },
        ],
    }
}

fn mount(cameras: &[&str], span: Span, live: bool) -> web_sys::Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let cameras: Vec<String> = cameras.iter().map(|c| (*c).to_owned()).collect();
    std::mem::forget(leptos::mount::mount_to(
        host.clone().unchecked_into(),
        move || {
            let house = House::new();
            house.alerts.set(vec![alert()]);
            provide_context(house);
            provide_context(Now(RwSignal::new(NOW)));
            provide_context(CameraList(RwSignal::new(camera_list(live))));
            let cameras = cameras.clone();
            view! { <Router><CameraWidget cameras=cameras span=span /></Router> }
        },
    ));
    host
}

fn all(host: &web_sys::Element, selector: &str) -> u32 {
    host.query_selector_all(selector).unwrap().length()
}

fn one(host: &web_sys::Element, selector: &str) -> web_sys::HtmlElement {
    host.query_selector(selector)
        .unwrap()
        .expect(selector)
        .unchecked_into()
}

fn text(host: &web_sys::Element, selector: &str) -> String {
    one(host, selector).text_content().unwrap_or_default()
}

async fn sleep(ms: i32) {
    let promise = js_sys::Promise::new(&mut |resolve, _| {
        web_sys::window()
            .unwrap()
            .set_timeout_with_callback_and_timeout_and_arguments_0(&resolve, ms)
            .unwrap();
    });
    wasm_bindgen_futures::JsFuture::from(promise).await.unwrap();
}

#[wasm_bindgen_test]
async fn wide_tile_shows_one_camera_with_arrows() {
    let host = mount(&["front_door", "garden"], Span::X2x1, true);
    tick().await;
    assert_eq!(all(&host, "img"), 1);
    assert_eq!(all(&host, ".camera-widget__arrow"), 2);
    assert_eq!(text(&host, ".camera-frame__name"), "Front Door");
    one(&host, ".camera-widget__arrow--next").click();
    tick().await;
    assert_eq!(text(&host, ".camera-frame__name"), "Garden");
}

#[wasm_bindgen_test]
async fn big_tile_with_three_cameras_is_a_grid_with_an_empty_square() {
    let host = mount(&["front_door", "garden", "garden"], Span::X2x2, true);
    tick().await;
    assert_eq!(all(&host, ".camera-widget--grid .camera-widget__cell"), 4);
    assert_eq!(all(&host, ".camera-widget__cell--empty"), 1);
}

#[wasm_bindgen_test]
async fn badge_links_to_the_alert() {
    let host = mount(&["front_door"], Span::X2x1, true);
    tick().await;
    let badge = one(&host, ".camera-frame__badge");
    assert_eq!(
        badge.get_attribute("href").as_deref(),
        Some("/alerts/frigate-1")
    );
    assert!(badge.text_content().unwrap().starts_with("Car detected · "));
}

#[wasm_bindgen_test]
async fn live_off_shows_the_alert_snapshot() {
    let host = mount(&["front_door"], Span::X2x1, false);
    tick().await;
    assert_eq!(
        one(&host, "img").get_attribute("src").as_deref(),
        Some(SNAPSHOT)
    );
    assert_eq!(text(&host, ".camera-frame__note"), "Live view off");
    assert_eq!(all(&host, ".camera-frame__live"), 0);
}

#[wasm_bindgen_test]
async fn live_off_without_an_alert_says_no_events() {
    let host = mount(&["garden"], Span::X2x1, false);
    tick().await;
    assert_eq!(all(&host, "img"), 0);
    assert_eq!(text(&host, ".camera-frame__note"), "No events yet");
}

#[wasm_bindgen_test]
async fn missing_camera_shows_not_found() {
    // The stub answers 404 to every frame request. `porch` is this test's own camera: the
    // widgets earlier tests left mounted poll theirs.
    let stub = FetchStub::install();
    let host = mount(&["porch"], Span::X2x1, true);
    let mut waited = 0;
    while !host.inner_html().contains("Camera not found") && waited < 3000 {
        sleep(50).await;
        waited += 50;
    }
    assert_eq!(text(&host, ".camera-frame__note"), "Camera not found");
    sleep(1500).await;
    let probes = stub
        .requests()
        .into_iter()
        .filter(|r| r.url.starts_with("/api/v1/cameras/porch/latest.jpg"))
        .count();
    assert_eq!(probes, 1);
}

#[wasm_bindgen_test]
async fn viewer_streams_and_closes_on_escape() {
    let host = mount(&["front_door", "garden"], Span::X2x1, true);
    tick().await;
    one(&host, ".camera-frame").click();
    tick().await;
    let src = one(&host, ".camera-viewer img")
        .get_attribute("src")
        .unwrap_or_default();
    assert!(
        src.contains("/api/v1/cameras/front_door/stream?h="),
        "{src}"
    );
    let init = web_sys::KeyboardEventInit::new();
    init.set_key("Escape");
    let event =
        web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).unwrap();
    web_sys::window().unwrap().dispatch_event(&event).unwrap();
    tick().await;
    assert_eq!(all(&host, ".camera-viewer"), 0);
}
