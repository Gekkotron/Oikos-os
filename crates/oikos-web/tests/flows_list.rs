#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::Router;
use oikos_api::FlowView;
use oikos_core::flow::Flow;
use oikos_web::components::toast::Toasts;
use oikos_web::house::House;
use oikos_web::screens::flows::FlowsScreen;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

const REFERENCE: &str = r#"{
  "id": "living_led_motion",
  "name": "Living LED on motion",
  "enabled": true,
  "mode": "restart",
  "nodes": [
    { "id": "motion",  "type": "trigger.state", "name": "Motion",
      "params": { "entity": "0x00158d00044c4d4c_occupancy_zigbee2mqtt" } },
    { "id": "moving", "type": "if",
      "params": { "op": "eq", "value": true } },
    { "id": "on",  "type": "command",
      "params": { "entity": "0x9035eafffec07012_light_zigbee2mqtt", "command": { "type": "turn_on" } } },
    { "id": "wait", "type": "delay", "params": { "for": "5m" } },
    { "id": "off", "type": "command",
      "params": { "entity": "0x9035eafffec07012_light_zigbee2mqtt", "command": { "type": "turn_off" } } }
  ],
  "edges": [
    { "from": "motion", "to": "moving" },
    { "from": "moving", "port": "true",  "to": "on" },
    { "from": "moving", "port": "false", "to": "wait" },
    { "from": "wait",   "to": "off" }
  ]
}"#;

fn host() -> web_sys::Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    host
}

fn mount(host: &web_sys::Element, flows: Vec<FlowView>) -> Box<dyn std::any::Any> {
    let house = House::new();
    house.flows.set(flows);
    Box::new(leptos::mount::mount_to(
        host.clone().unchecked_into(),
        move || {
            provide_context(house);
            provide_context(Toasts::new());
            provide_context(oikos_web::components::now::install_now());
            view! {
                <Router>
                    <FlowsScreen />
                </Router>
            }
        },
    ))
}

#[wasm_bindgen_test]
async fn lists_valid_and_invalid_flows() {
    let valid = FlowView {
        definition: Flow::decode(REFERENCE.as_bytes()).unwrap(),
        invalid: None,
        active_run: None,
        runs: vec![],
    };
    let broken = FlowView {
        definition: Flow {
            id: "broken".into(),
            name: "broken".into(),
            ..Default::default()
        },
        invalid: Some("missing port".into()),
        active_run: None,
        runs: vec![],
    };
    let host = host();
    let _mounted = mount(&host, vec![valid, broken]);
    tick().await;
    let html = host.inner_html();
    assert!(html.contains("Living LED on motion"), "{html}");
    assert!(html.contains("flow-row"), "{html}");
    assert!(
        html.contains("0x00158d00044c4d4c_occupancy_zigbee2mqtt"),
        "{html}"
    );
    assert!(html.contains("switch--on"), "{html}");
    assert!(html.contains("role=\"switch\""), "{html}");
    assert!(html.contains("badge--invalid"), "{html}");
    assert!(html.contains("missing port"), "{html}");
    assert!(html.contains("/flows/living_led_motion"), "{html}");
    assert!(html.contains("No runs yet"), "{html}");
}

#[wasm_bindgen_test]
async fn empty_list_shows_the_empty_state() {
    let host = host();
    let _mounted = mount(&host, vec![]);
    tick().await;
    let html = host.inner_html();
    assert!(
        html.contains("No flows yet. Ask the assistant to create one."),
        "{html}"
    );
}

#[wasm_bindgen_test]
async fn list_has_new_flow_button() {
    let host = host();
    let _mounted = mount(&host, vec![]);
    tick().await;
    let link = host
        .query_selector("a.flows-new[href=\"/flows/new\"]")
        .unwrap();
    assert!(link.is_some(), "{}", host.inner_html());
}

fn reference() -> FlowView {
    FlowView {
        definition: Flow::decode(REFERENCE.as_bytes()).unwrap(),
        invalid: None,
        active_run: None,
        runs: vec![],
    }
}

fn broken() -> FlowView {
    FlowView {
        definition: Flow {
            id: "broken".into(),
            name: "Broken one".into(),
            ..Default::default()
        },
        invalid: Some("missing port".into()),
        active_run: None,
        runs: vec![],
    }
}

fn count(host: &web_sys::Element, selector: &str) -> u32 {
    host.query_selector_all(selector).unwrap().length()
}

fn click(host: &web_sys::Element, selector: &str) {
    host.query_selector(selector)
        .unwrap()
        .unwrap_or_else(|| panic!("no {selector}: {}", host.inner_html()))
        .unchecked_into::<web_sys::HtmlElement>()
        .click();
}

/// Clicks the button inside `selector` whose text is `label`.
fn click_text(host: &web_sys::Element, selector: &str, label: &str) {
    let all = host.query_selector_all(selector).unwrap();
    (0..all.length())
        .filter_map(|i| all.item(i))
        .filter_map(|n| n.dyn_into::<web_sys::HtmlElement>().ok())
        .find(|el| el.text_content().unwrap_or_default().trim() == label)
        .unwrap_or_else(|| panic!("no {label} in {selector}: {}", host.inner_html()))
        .click();
}

#[wasm_bindgen_test]
async fn menu_offers_delete_with_confirmation() {
    let host = host();
    let _mounted = mount(&host, vec![reference()]);
    tick().await;
    click(&host, ".room-menu__button");
    tick().await;
    let delete = host
        .query_selector(".room-menu__item--danger")
        .unwrap()
        .expect("a delete item");
    assert_eq!(delete.text_content().unwrap(), "Delete flow…");
    click(&host, ".room-menu__item--danger");
    tick().await;
    let confirm = host
        .query_selector(".room-menu__pop [role=alertdialog]")
        .unwrap()
        .expect("a confirmation");
    let text = confirm.text_content().unwrap();
    assert!(text.contains("Delete “Living LED on motion”?"), "{text}");
    assert!(text.contains("This can't be undone."), "{text}");
    assert_eq!(count(&host, ".room-menu__head"), 0);
    click_text(&host, ".room-menu__confirm button", "Cancel");
    tick().await;
    assert_eq!(count(&host, ".room-menu__confirm"), 0);
    assert_eq!(count(&host, ".room-menu__head"), 1, "{}", host.inner_html());
}

/// The text of the focused element.
fn focused_text() -> String {
    web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .active_element()
        .and_then(|el| el.text_content())
        .unwrap_or_default()
}

#[wasm_bindgen_test]
async fn delete_confirm_keeps_the_keyboard_focus() {
    let host = host();
    let _mounted = mount(&host, vec![reference()]);
    tick().await;
    click(&host, ".room-menu__button");
    tick().await;
    click(&host, ".room-menu__item--danger");
    gloo_timers::future::TimeoutFuture::new(50).await;
    // The confirm opens on its safe choice.
    assert_eq!(focused_text(), "Cancel");
    click_text(&host, ".room-menu__confirm button", "Cancel");
    gloo_timers::future::TimeoutFuture::new(50).await;
    assert_eq!(focused_text(), "Delete flow…");
}

#[wasm_bindgen_test]
async fn failed_delete_keeps_the_row() {
    let host = host();
    let _mounted = mount(&host, vec![reference(), broken()]);
    tick().await;
    let rows = count(&host, ".room-menu__button");
    click(&host, ".room-menu__button");
    tick().await;
    click(&host, ".room-menu__item--danger");
    tick().await;
    // No hub answers in the test, so the delete fails.
    click_text(&host, ".room-menu__confirm button", "Delete");
    gloo_timers::future::TimeoutFuture::new(100).await;
    assert_eq!(count(&host, ".room-menu__pop"), 0);
    assert_eq!(count(&host, ".room-menu__button"), rows);
}

#[wasm_bindgen_test]
async fn invalid_flow_can_still_be_deleted() {
    let host = host();
    let _mounted = mount(&host, vec![broken()]);
    tick().await;
    click(&host, ".room-menu__button");
    tick().await;
    let delete = host
        .query_selector(".room-menu__item--danger")
        .unwrap()
        .expect("a delete item");
    assert!(!delete.has_attribute("disabled"));
}
