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
    assert!(html.contains("Motion → If"), "{html}");
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
