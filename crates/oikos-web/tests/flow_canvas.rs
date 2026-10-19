#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;
use oikos_api::{FlowView, RunStatus, RunView, StepOutcome, StepView};
use oikos_core::flow::Flow;
use oikos_web::components::now::install_now;
use oikos_web::components::toast::Toasts;
use oikos_web::house::House;
use oikos_web::screens::flow_canvas::FlowCanvasScreen;
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

fn mount(host: &web_sys::Element, flows: Vec<FlowView>, path: &str) -> Box<dyn std::any::Any> {
    web_sys::window()
        .unwrap()
        .history()
        .unwrap()
        .replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path))
        .unwrap();
    let house = House::new();
    house.flows.set(flows);
    house.connection.set(oikos_api::ConnectionStatus::Online);
    Box::new(leptos::mount::mount_to(
        host.clone().unchecked_into(),
        move || {
            provide_context(house);
            provide_context(Toasts::new());
            provide_context(install_now());
            view! {
                <Router>
                    <Routes fallback=|| "fallback">
                        <Route path=path!("/flows/:id") view=FlowCanvasScreen />
                    </Routes>
                </Router>
            }
        },
    ))
}

fn step(node: &str, detail: &str) -> StepView {
    StepView {
        node: node.into(),
        at: 1,
        outcome: StepOutcome::Ok,
        detail: detail.into(),
    }
}

fn count(host: &web_sys::Element, selector: &str) -> u32 {
    host.query_selector_all(selector).unwrap().length()
}

#[wasm_bindgen_test]
async fn canvas_lights_the_last_run() {
    let run = RunView {
        id: "r1".into(),
        flow_id: "living_led_motion".into(),
        status: RunStatus::Done,
        started_at: 1_700_000_000,
        ended_at: Some(1_700_000_300),
        trigger: None,
        steps: vec![
            step("motion", "occupancy = true"),
            step("moving", "false"),
            step("wait", "waited 5m"),
            step("off", "accepted"),
        ],
    };
    let flow = FlowView {
        definition: Flow::decode(REFERENCE.as_bytes()).unwrap(),
        invalid: None,
        active_run: None,
        runs: vec![run],
    };
    let host = host();
    let _mounted = mount(&host, vec![flow], "/flows/living_led_motion");
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-node"), 5, "{html}");
    assert_eq!(count(&host, ".flow-wire"), 4, "{html}");
    assert_eq!(count(&host, ".flow-wire--lit"), 3, "{html}");
    assert_eq!(count(&host, "[data-node=wait].flow-node--lit"), 1, "{html}");
    assert_eq!(count(&host, "[data-node=on].flow-node--lit"), 0, "{html}");
    assert_eq!(count(&host, ".flow-port"), 2, "{html}");
    assert!(
        host.query_selector(".run-list")
            .unwrap()
            .unwrap()
            .text_content()
            .unwrap()
            .contains("Done"),
        "{html}"
    );
}

#[wasm_bindgen_test]
async fn invalid_flow_shows_the_reason_and_no_canvas() {
    let flow = FlowView {
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
    let _mounted = mount(&host, vec![flow], "/flows/broken");
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-invalid"), 1, "{html}");
    assert!(html.contains("missing port"), "{html}");
    assert_eq!(count(&host, ".flow-canvas"), 0, "{html}");
}

#[wasm_bindgen_test]
async fn unknown_flow_links_back() {
    let host = host();
    let _mounted = mount(&host, vec![], "/flows/nope");
    tick().await;
    let html = host.inner_html();
    assert!(html.contains("Unknown flow"), "{html}");
    assert!(html.contains("href=\"/flows\""), "{html}");
}

#[wasm_bindgen_test]
async fn parked_delay_counts_down() {
    // The shape the engine really produces while it waits: the delay step is only recorded
    // after the wait, so the last step is the `if` that took the false branch.
    let now = (js_sys::Date::now() / 1000.0) as u64;
    let mut moving = step("moving", "false");
    moving.at = now;
    let run = RunView {
        id: "r1".into(),
        flow_id: "living_led_motion".into(),
        status: RunStatus::Running,
        started_at: now,
        ended_at: None,
        trigger: None,
        steps: vec![step("motion", "occupancy = false"), moving],
    };
    let flow = FlowView {
        definition: Flow::decode(REFERENCE.as_bytes()).unwrap(),
        invalid: None,
        active_run: Some("r1".into()),
        runs: vec![run],
    };
    let host = host();
    let _mounted = mount(&host, vec![flow], "/flows/living_led_motion");
    tick().await;
    let html = host.inner_html();
    assert_eq!(
        count(&host, "[data-node=wait].flow-node--pending"),
        1,
        "{html}"
    );
    assert_eq!(count(&host, ".flow-node--pending"), 1, "{html}");
    assert!(html.contains(" left"), "{html}");
    assert!(html.contains("4:5") || html.contains("5:00"), "{html}");
    assert!(html.contains("disabled"), "{html}");
    // The wire into the parked delay is lit.
    assert_eq!(count(&host, ".flow-wire--lit"), 2, "{html}");
    // The if card lights the port it took instead of repeating it on a second line.
    assert_eq!(count(&host, ".flow-port--taken"), 1, "{html}");
}

#[wasm_bindgen_test]
async fn cards_have_handles_and_orphans_fade() {
    let mut definition = Flow::decode(REFERENCE.as_bytes()).unwrap();
    definition.edges.retain(|e| e.to != "off");
    let flow = FlowView {
        definition,
        invalid: None,
        active_run: None,
        runs: vec![],
    };
    let host = host();
    let _mounted = mount(&host, vec![flow], "/flows/living_led_motion");
    tick().await;
    let html = host.inner_html();
    // Every node but the trigger has an input; every node has an output (two for the if).
    assert_eq!(count(&host, ".flow-node__in"), 4, "{html}");
    assert_eq!(count(&host, ".flow-node__out"), 6, "{html}");
    assert_eq!(count(&host, ".flow-node--orphan"), 1, "{html}");
    assert_eq!(
        count(&host, "[data-node=off].flow-node--orphan"),
        1,
        "{html}"
    );
}
