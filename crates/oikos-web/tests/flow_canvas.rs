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

/// The run that went motion → moving (false) → wait → off.
fn done_run() -> RunView {
    RunView {
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
    }
}

fn reference_view(active_run: Option<&str>, runs: Vec<RunView>) -> FlowView {
    FlowView {
        definition: Flow::decode(REFERENCE.as_bytes()).unwrap(),
        invalid: None,
        active_run: active_run.map(Into::into),
        runs,
    }
}

fn text(host: &web_sys::Element, selector: &str) -> String {
    host.query_selector(selector)
        .unwrap()
        .and_then(|e| e.text_content())
        .unwrap_or_default()
}

fn click(host: &web_sys::Element, selector: &str) {
    host.query_selector(selector)
        .unwrap()
        .unwrap_or_else(|| panic!("no {selector}"))
        .unchecked_into::<web_sys::HtmlElement>()
        .click();
}

#[wasm_bindgen_test]
async fn canvas_lights_the_active_run() {
    let host = host();
    let _mounted = mount(
        &host,
        vec![reference_view(Some("r1"), vec![done_run()])],
        "/flows/living_led_motion",
    );
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-node"), 5, "{html}");
    assert_eq!(count(&host, ".flow-wire"), 4, "{html}");
    assert_eq!(count(&host, ".flow-wire--lit"), 3, "{html}");
    assert_eq!(count(&host, ".flow-wire--live"), 3, "{html}");
    assert_eq!(count(&host, "[data-node=wait].flow-node--lit"), 1, "{html}");
    assert_eq!(count(&host, "[data-node=on].flow-node--lit"), 0, "{html}");
    assert_eq!(count(&host, ".flow-node--dim"), 0, "{html}");
    assert!(text(&host, ".run-list").contains("Done"), "{html}");
    drop(_mounted);

    // A finished run is not lit by default.
    let host = self::host();
    let _mounted = mount(
        &host,
        vec![reference_view(None, vec![done_run()])],
        "/flows/living_led_motion",
    );
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-wire--lit"), 0, "{html}");
    assert_eq!(count(&host, ".flow-node--lit"), 0, "{html}");
}

#[wasm_bindgen_test]
async fn replay_dims_the_rest() {
    let host = host();
    let _mounted = mount(
        &host,
        vec![reference_view(None, vec![done_run()])],
        "/flows/living_led_motion",
    );
    tick().await;
    assert_eq!(text(&host, "[data-node=wait] .flow-node__step"), "");
    assert_eq!(count(&host, ".flow-drawer--open"), 0);
    click(&host, ".flow-toolbar__runs");
    tick().await;
    assert_eq!(count(&host, ".flow-drawer--open"), 1);
    click(&host, ".run-list li:first-child");
    tick().await;
    let html = host.inner_html();
    // Four of the five cards were visited; `on` is dimmed.
    assert_eq!(count(&host, ".flow-node--dim"), 1, "{html}");
    assert_eq!(count(&host, "[data-node=on].flow-node--dim"), 1, "{html}");
    assert_eq!(count(&host, ".flow-wire--lit"), 3, "{html}");
    assert_eq!(count(&host, ".flow-wire--live"), 0, "{html}");
    assert_eq!(
        text(&host, "[data-node=wait] .flow-node__step"),
        "+0s",
        "{html}"
    );
    click(&host, ".run-list li:first-child");
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-node--dim"), 0, "{html}");
    assert_eq!(count(&host, ".flow-wire--lit"), 0, "{html}");
    assert_eq!(
        text(&host, "[data-node=wait] .flow-node__step"),
        "",
        "{html}"
    );
    // Esc stops a replay too.
    click(&host, ".run-list li:first-child");
    tick().await;
    assert_eq!(count(&host, ".flow-node--dim"), 1);
    let init = web_sys::KeyboardEventInit::new();
    init.set_key("Escape");
    let escape =
        web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).unwrap();
    web_sys::window().unwrap().dispatch_event(&escape).unwrap();
    tick().await;
    assert_eq!(count(&host, ".flow-node--dim"), 0, "{}", host.inner_html());
}

#[wasm_bindgen_test]
async fn pills_read_as_sentences() {
    let host = host();
    let _mounted = mount(
        &host,
        vec![reference_view(None, vec![])],
        "/flows/living_led_motion",
    );
    tick().await;
    let html = host.inner_html();
    assert_eq!(
        text(&host, "[data-node=wait] .flow-node__title"),
        "Wait 5 min",
        "{html}"
    );
    assert_eq!(count(&host, ".flow-node--wait"), 1, "{html}");
    assert_eq!(count(&host, ".flow-node--check"), 1, "{html}");
    assert_eq!(count(&host, ".flow-node--trigger"), 1, "{html}");
    assert_eq!(count(&host, ".flow-node--action"), 2, "{html}");
}

#[wasm_bindgen_test]
async fn yes_no_labels_on_wires() {
    let host = host();
    let _mounted = mount(
        &host,
        vec![reference_view(None, vec![])],
        "/flows/living_led_motion",
    );
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-wire-label"), 2, "{html}");
    assert_eq!(count(&host, ".flow-wire--yes"), 1, "{html}");
    assert_eq!(count(&host, ".flow-wire--no"), 1, "{html}");
    let labels: Vec<String> = (0..2)
        .map(|i| {
            host.query_selector_all(".flow-wire-label")
                .unwrap()
                .item(i)
                .and_then(|n| n.text_content())
                .unwrap_or_default()
        })
        .collect();
    assert!(labels.contains(&"yes".to_owned()), "{html}");
    assert!(labels.contains(&"no".to_owned()), "{html}");
}

#[wasm_bindgen_test]
async fn controls_and_minimap_present() {
    let host = host();
    let _mounted = mount(
        &host,
        vec![reference_view(None, vec![])],
        "/flows/living_led_motion",
    );
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-controls button"), 3, "{html}");
    assert_eq!(count(&host, ".flow-minimap"), 1, "{html}");
    assert_eq!(count(&host, ".flow-toolbar__edit"), 1, "{html}");
    // One minimap rect per card.
    assert_eq!(count(&host, ".flow-minimap__node"), 5, "{html}");
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
    // The branch the if took is the lit one.
    assert_eq!(count(&host, ".flow-wire--lit.flow-wire--no"), 1, "{html}");
    assert_eq!(count(&host, ".flow-wire--lit.flow-wire--yes"), 0, "{html}");
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
    // Every node but the trigger has an input; every node has one output, the if two.
    assert_eq!(count(&host, ".flow-node__in"), 4, "{html}");
    assert_eq!(count(&host, ".flow-node__out"), 6, "{html}");
    assert_eq!(count(&host, ".flow-node--orphan"), 1, "{html}");
    assert_eq!(
        count(&host, "[data-node=off].flow-node--orphan"),
        1,
        "{html}"
    );
}

fn pointer(host: &web_sys::Element, selector: &str, kind: &str) {
    let init = web_sys::PointerEventInit::new();
    init.set_bubbles(true);
    init.set_pointer_id(1);
    init.set_pointer_type("mouse");
    host.query_selector(selector)
        .unwrap()
        .unwrap_or_else(|| panic!("no {selector}"))
        .dispatch_event(&web_sys::PointerEvent::new_with_event_init_dict(kind, &init).unwrap())
        .unwrap();
}

#[wasm_bindgen_test]
async fn edit_mode_selects_a_card_and_opens_its_settings() {
    let host = host();
    let _mounted = mount(
        &host,
        vec![reference_view(None, vec![])],
        "/flows/living_led_motion",
    );
    // Let the page settle in view mode first, as it does in the browser.
    gloo_timers::future::TimeoutFuture::new(50).await;
    click(&host, ".flow-toolbar__edit");
    tick().await;
    pointer(&host, "[data-node=moving]", "pointerdown");
    pointer(&host, "[data-node=moving]", "pointerup");
    tick().await;
    let html = host.inner_html();
    assert_eq!(
        count(&host, "[data-node=moving].flow-node--selected"),
        1,
        "{html}"
    );
    assert_eq!(count(&host, ".flow-drawer--open"), 1, "{html}");
    assert_eq!(count(&host, ".flow-drawer .node-editor"), 1, "{html}");
}

/// Answers every `fetch` with `body` as a 200 JSON response, until the returned guard drops.
struct FakeFetch(wasm_bindgen::JsValue);

impl FakeFetch {
    fn answer(body: &str) -> Self {
        let global = js_sys::global();
        let real = js_sys::Reflect::get(&global, &"fetch".into()).unwrap();
        let script = format!(
            "return Promise.resolve(new Response({}, {{ status: 200, headers: {{ 'content-type': 'application/json' }} }}))",
            serde_json::to_string(body).unwrap()
        );
        let fake = js_sys::Function::new_no_args(&script);
        js_sys::Reflect::set(&global, &"fetch".into(), &fake).unwrap();
        Self(real)
    }
}

impl Drop for FakeFetch {
    fn drop(&mut self) {
        js_sys::Reflect::set(&js_sys::global(), &"fetch".into(), &self.0).unwrap();
    }
}

#[wasm_bindgen_test]
async fn save_leaves_edit_mode() {
    let saved = reference_view(None, vec![]);
    let host = host();
    let _mounted = mount(&host, vec![saved.clone()], "/flows/living_led_motion");
    tick().await;
    click(&host, ".flow-toolbar__edit");
    tick().await;
    assert_eq!(count(&host, ".flow-graph--editing"), 1);
    let _fetch = FakeFetch::answer(&serde_json::to_string(&saved).unwrap());
    click(&host, ".flow-toolbar button[aria-label=Save]");
    gloo_timers::future::TimeoutFuture::new(50).await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-graph--editing"), 0, "{html}");
    assert_eq!(count(&host, ".flow-toolbar__edit"), 1, "{html}");
}
