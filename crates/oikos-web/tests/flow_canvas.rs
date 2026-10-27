#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;
use oikos_api::{FlowView, FlowWriteOutcome, RunStatus, RunView, StepOutcome, StepView};
use oikos_core::flow::{Flow, FlowMode, Position};
use oikos_web::components::now::install_now;
use oikos_web::components::toast::Toasts;
use oikos_web::house::House;
use oikos_web::i18n::Lang;
use oikos_web::screens::flow_canvas::{FlowCanvasScreen, NewFlowScreen};
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
    mount_house(host, flows, path).0
}

/// Mounts the canvas and also returns its house, for tests that push live updates.
fn mount_house(
    host: &web_sys::Element,
    flows: Vec<FlowView>,
    path: &str,
) -> (Box<dyn std::any::Any>, House) {
    mount_house_lang(host, flows, path, RwSignal::new(Lang::En))
}

/// [`mount_house`] with the language signal the test flips.
fn mount_house_lang(
    host: &web_sys::Element,
    flows: Vec<FlowView>,
    path: &str,
    lang: RwSignal<Lang>,
) -> (Box<dyn std::any::Any>, House) {
    web_sys::window()
        .unwrap()
        .history()
        .unwrap()
        .replace_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path))
        .unwrap();
    let house = House::new();
    house.flows.set(flows);
    house.connection.set(oikos_api::ConnectionStatus::Online);
    let mounted = Box::new(leptos::mount::mount_to(
        host.clone().unchecked_into(),
        move || {
            provide_context(house);
            provide_context(Toasts::new());
            provide_context(install_now());
            provide_context(lang);
            oikos_web::nav::install_nav();
            view! {
                <Router>
                    <Routes fallback=|| "fallback">
                        <Route path=path!("/flows/new") view=NewFlowScreen />
                        <Route path=path!("/flows/:id") view=FlowCanvasScreen />
                    </Routes>
                </Router>
            }
        },
    ));
    (mounted, house)
}

fn step(node: &str, detail: &str) -> StepView {
    StepView {
        node: node.into(),
        at: 1,
        outcome: StepOutcome::Ok,
        detail: detail.into(),
        ports: None,
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
        manual: false,
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
        .unwrap_or_else(|| panic!("no {selector}: {}", host.inner_html()))
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
        "+0\u{a0}s",
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
        "Wait 5\u{a0}min",
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
        manual: false,
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

/// Answers every `fetch` with `body` as a 200 JSON response, until the returned guard drops.
struct FakeFetch(wasm_bindgen::JsValue);

impl FakeFetch {
    /// Also records every request as `{url, method, body}` for [`FakeFetch::requests`].
    fn answer(body: &str) -> Self {
        let global = js_sys::global();
        let real = js_sys::Reflect::get(&global, &"fetch".into()).unwrap();
        let script = format!(
            "return (async () => {{
                const req = input instanceof Request ? input : null;
                const url = req ? req.url : String(input);
                const method = req ? req.method : ((init && init.method) || 'GET');
                const text = req ? await req.clone().text() : ((init && init.body) || '');
                (globalThis.__fetches = globalThis.__fetches || []).push({{ url, method, body: text }});
                return new Response({}, {{ status: 200, headers: {{ 'content-type': 'application/json' }} }});
            }})()",
            serde_json::to_string(body).unwrap()
        );
        js_sys::Reflect::set(&global, &"__fetches".into(), &js_sys::Array::new()).unwrap();
        let fake = js_sys::Function::new_with_args("input, init", &script);
        js_sys::Reflect::set(&global, &"fetch".into(), &fake).unwrap();
        Self(real)
    }

    /// The (method, path, body) of every request made since `answer`.
    fn requests(&self) -> Vec<(String, String, String)> {
        let log: js_sys::Array = js_sys::Reflect::get(&js_sys::global(), &"__fetches".into())
            .unwrap()
            .unchecked_into();
        let field = |entry: &wasm_bindgen::JsValue, key: &str| {
            js_sys::Reflect::get(entry, &key.into())
                .unwrap()
                .as_string()
                .unwrap_or_default()
        };
        log.iter()
            .map(|e| {
                let url = field(&e, "url");
                let path = url
                    .find("/api/")
                    .map_or(url.clone(), |i| url[i..].to_owned());
                (field(&e, "method"), path, field(&e, "body"))
            })
            .collect()
    }
}

/// The hub's answer to a flow PUT that stored `flow` with no warning.
fn write_outcome(flow: &FlowView) -> String {
    serde_json::to_string(&FlowWriteOutcome {
        flow: flow.clone(),
        warning: None,
    })
    .unwrap()
}

/// The PUTs `fetch` saw; a save also refetches `/api/v1/config/flows` for the fallback badge.
fn puts(fetch: &FakeFetch) -> Vec<(String, String, String)> {
    fetch
        .requests()
        .into_iter()
        .filter(|(method, _, _)| method == "PUT")
        .collect()
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
    let _fetch = FakeFetch::answer(&write_outcome(&saved));
    click(&host, ".flow-toolbar button[aria-label=Save]");
    gloo_timers::future::TimeoutFuture::new(50).await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-graph--editing"), 0, "{html}");
    assert_eq!(count(&host, ".flow-toolbar__edit"), 1, "{html}");
}

// ---- Editing --------------------------------------------------------------------------------

fn element(host: &web_sys::Element, selector: &str) -> web_sys::Element {
    host.query_selector(selector)
        .unwrap()
        .unwrap_or_else(|| panic!("no {selector}: {}", host.inner_html()))
}

/// The centre of `el` on screen, scrolled into view first so a hit test can find it.
fn centre(el: &web_sys::Element) -> (i32, i32) {
    el.scroll_into_view();
    let rect = el.get_bounding_client_rect();
    (
        (rect.left() + rect.width() / 2.0) as i32,
        (rect.top() + rect.height() / 2.0) as i32,
    )
}

/// A mouse pointer event of `kind` at client point `at`, dispatched on `target`.
fn pointer_at(target: &web_sys::Element, kind: &str, at: (i32, i32)) {
    let init = web_sys::PointerEventInit::new();
    init.set_bubbles(true);
    init.set_pointer_id(1);
    init.set_pointer_type("mouse");
    init.set_client_x(at.0);
    init.set_client_y(at.1);
    target
        .dispatch_event(&web_sys::PointerEvent::new_with_event_init_dict(kind, &init).unwrap())
        .unwrap();
}

/// A bubbling `keydown` of `key` on `target`; `ctrl` stands for the platform modifier.
fn press(target: &web_sys::EventTarget, key: &str, ctrl: bool, shift: bool) {
    let init = web_sys::KeyboardEventInit::new();
    init.set_key(key);
    init.set_ctrl_key(ctrl);
    init.set_shift_key(shift);
    init.set_bubbles(true);
    init.set_cancelable(true);
    target
        .dispatch_event(
            &web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).unwrap(),
        )
        .unwrap();
}

fn body() -> web_sys::EventTarget {
    web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .body()
        .unwrap()
        .into()
}

/// A bubbling `click` on `selector` (it works on SVG paths too).
fn click_any(host: &web_sys::Element, selector: &str) {
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    element(host, selector)
        .dispatch_event(&web_sys::Event::new_with_event_init_dict("click", &init).unwrap())
        .unwrap();
}

fn style(host: &web_sys::Element, selector: &str) -> String {
    element(host, selector)
        .get_attribute("style")
        .unwrap_or_default()
}

/// Mounts the reference flow (or `view`) and enters edit mode once the page has settled in view
/// mode, as it does in the browser.
async fn editing(view: FlowView) -> (web_sys::Element, Box<dyn std::any::Any>, House) {
    let host = host();
    let (mounted, house) = mount_house(&host, vec![view], "/flows/living_led_motion");
    gloo_timers::future::TimeoutFuture::new(50).await;
    click(&host, ".flow-toolbar__edit");
    tick().await;
    assert_eq!(count(&host, ".flow-graph--editing"), 1);
    (host, mounted, house)
}

/// Clicks the card (a pointerdown and a pointerup without moving).
async fn select_card(host: &web_sys::Element, id: &str) {
    let card = element(host, &format!("[data-node={id}]"));
    let at = centre(&card);
    pointer_at(&card, "pointerdown", at);
    pointer_at(&card, "pointerup", at);
    tick().await;
}

#[wasm_bindgen_test]
async fn click_card_opens_settings() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    select_card(&host, "moving").await;
    let html = host.inner_html();
    assert_eq!(
        count(&host, "[data-node=moving].flow-node--selected"),
        1,
        "{html}"
    );
    assert_eq!(count(&host, ".flow-drawer--open"), 1, "{html}");
    assert!(count(&host, ".flow-drawer .field") > 0, "{html}");
    // The drawer shows the settings instead of the runs, with a delete button.
    assert_eq!(count(&host, ".flow-drawer .run-list"), 0, "{html}");
    assert_eq!(
        count(&host, ".flow-drawer .node-editor__delete"),
        1,
        "{html}"
    );
    // A click on the empty canvas clears the selection.
    let canvas = element(&host, ".flow-canvas");
    pointer_at(&canvas, "pointerdown", (1, 1));
    pointer_at(&canvas, "pointerup", (1, 1));
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-node--selected"), 0, "{html}");
    assert_eq!(count(&host, ".flow-drawer .node-editor"), 0, "{html}");
    assert_eq!(count(&host, ".flow-library"), 1, "{html}");
}

#[wasm_bindgen_test]
async fn view_mode_click_opens_settings() {
    let host = host();
    let _mounted = mount(
        &host,
        vec![reference_view(None, vec![done_run()])],
        "/flows/living_led_motion",
    );
    tick().await;
    // Keys do nothing outside edit mode.
    press(&body(), "Backspace", false, false);
    tick().await;
    assert_eq!(count(&host, "[data-node=moving]"), 1);
    select_card(&host, "moving").await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-graph--editing"), 1, "{html}");
    assert_eq!(
        count(&host, "[data-node=moving].flow-node--selected"),
        1,
        "{html}"
    );
    assert_eq!(count(&host, ".flow-drawer--open"), 1, "{html}");
    assert_eq!(
        element(&host, ".node-editor__id").text_content().as_deref(),
        Some("moving")
    );
}

#[wasm_bindgen_test]
async fn delete_key_removes_card_and_bridges_its_wires() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    assert_eq!(count(&host, ".flow-wire"), 4);
    select_card(&host, "wait").await;
    press(&body(), "Backspace", false, false);
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, "[data-node=wait]"), 0, "{html}");
    // `moving --no--> wait --> off` becomes `moving --no--> off`.
    assert_eq!(count(&host, ".flow-wire"), 3, "{html}");
    assert_eq!(count(&host, ".flow-wire--no"), 1, "{html}");
    assert_eq!(count(&host, ".flow-node--selected"), 0, "{html}");
}

#[wasm_bindgen_test]
async fn backspace_in_input_keeps_card() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    select_card(&host, "wait").await;
    let input = element(&host, ".flow-drawer input.field");
    input
        .clone()
        .unchecked_into::<web_sys::HtmlElement>()
        .focus()
        .unwrap();
    press(&input, "Backspace", false, false);
    press(&input, "Delete", false, false);
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, "[data-node=wait]"), 1, "{html}");
    assert_eq!(count(&host, ".flow-wire"), 4, "{html}");
}

#[wasm_bindgen_test]
async fn keys_in_an_open_picker_do_not_delete_the_card() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    select_card(&host, "on").await;
    click_any(&host, ".flow-drawer button.picker-field");
    tick().await;
    gloo_timers::future::TimeoutFuture::new(30).await;
    let chip = element(&host, ".picker__chips .chip");
    chip.clone()
        .unchecked_into::<web_sys::HtmlElement>()
        .focus()
        .unwrap();
    press(&chip, "Backspace", false, false);
    press(&chip, "z", true, false);
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, "[data-node=on]"), 1, "{html}");
    assert_eq!(count(&host, ".flow-wire"), 4, "{html}");
}

#[wasm_bindgen_test]
async fn wire_click_then_cut() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    assert_eq!(count(&host, ".flow-wire-hit"), 4);
    assert_eq!(count(&host, ".flow-wire-cut"), 0);
    click_any(&host, ".flow-wire-hit");
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-wire--selected"), 1, "{html}");
    assert_eq!(count(&host, ".flow-wire-cut"), 1, "{html}");
    click_any(&host, ".flow-wire-cut");
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-wire"), 3, "{html}");
    assert_eq!(count(&host, ".flow-wire-cut"), 0, "{html}");
}

/// Drags from `[data-node=from]`'s output pin and drops over `[data-node=to]`.
async fn drag_wire(host: &web_sys::Element, from: &str, to: &str) {
    let pin = element(host, &format!("[data-node={from}] .flow-node__out"));
    let start = centre(&pin);
    pointer_at(&pin, "pointerdown", start);
    let end = centre(&element(host, &format!("[data-node={to}]")));
    pointer_at(
        &pin,
        "pointermove",
        ((start.0 + end.0) / 2, (start.1 + end.1) / 2),
    );
    tick().await;
    pointer_at(&pin, "pointermove", end);
    pointer_at(&pin, "pointerup", end);
    tick().await;
}

#[wasm_bindgen_test]
async fn connect_by_drag() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    let pin = element(&host, "[data-node=on] .flow-node__out");
    let start = centre(&pin);
    pointer_at(&pin, "pointerdown", start);
    pointer_at(&pin, "pointermove", (start.0 + 30, start.1 + 30));
    tick().await;
    let html = host.inner_html();
    // A live preview, and only valid targets highlight.
    assert_eq!(count(&host, ".flow-wire-preview"), 1, "{html}");
    assert_eq!(
        count(&host, "[data-node=off].flow-node--target"),
        1,
        "{html}"
    );
    assert_eq!(
        count(&host, "[data-node=motion].flow-node--target"),
        0,
        "{html}"
    );
    assert_eq!(
        count(&host, "[data-node=on].flow-node--target"),
        0,
        "{html}"
    );
    // Released over no card (the test page has no stylesheet, so the pin's own spot on screen
    // is not where the graph thinks it is).
    pointer_at(&pin, "pointerup", (1, 1));
    tick().await;

    drag_wire(&host, "on", "off").await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-wire"), 5, "{html}");
    assert_eq!(count(&host, ".flow-wire-preview"), 0, "{html}");
    assert_eq!(count(&host, ".flow-node--target"), 0, "{html}");
}

#[wasm_bindgen_test]
async fn connect_into_trigger_refused() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    drag_wire(&host, "on", "motion").await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-wire"), 4, "{html}");
    // Nothing to undo either.
    assert!(
        element(&host, ".flow-toolbar button[aria-label=Undo]").has_attribute("disabled"),
        "{html}"
    );
}

#[wasm_bindgen_test]
async fn undo_restores() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    let undo = ".flow-toolbar button[aria-label=Undo]";
    assert!(element(&host, undo).has_attribute("disabled"));
    select_card(&host, "wait").await;
    press(&body(), "Delete", false, false);
    tick().await;
    assert_eq!(count(&host, "[data-node=wait]"), 0);
    assert!(!element(&host, undo).has_attribute("disabled"));
    press(&body(), "z", true, false);
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, "[data-node=wait]"), 1, "{html}");
    assert_eq!(count(&host, ".flow-wire"), 4, "{html}");
    assert!(element(&host, undo).has_attribute("disabled"), "{html}");
    // Shift-Mod-Z redoes.
    press(&body(), "Z", true, true);
    tick().await;
    assert_eq!(count(&host, "[data-node=wait]"), 0);
}

#[wasm_bindgen_test]
async fn duplicate_selects_the_copy() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    select_card(&host, "wait").await;
    press(&body(), "d", true, false);
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-node"), 6, "{html}");
    assert_eq!(
        count(&host, "[data-node=delay_1].flow-node--selected"),
        1,
        "{html}"
    );
    // Unwired.
    assert_eq!(count(&host, ".flow-wire"), 4, "{html}");
}

#[wasm_bindgen_test]
async fn drag_is_one_undo_step() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    let selector = "[data-node=on]";
    let before = style(&host, selector);
    let card = element(&host, selector);
    let (x, y) = centre(&card);
    pointer_at(&card, "pointerdown", (x, y));
    for dx in [10, 20, 40] {
        pointer_at(&card, "pointermove", (x + dx, y + dx));
    }
    pointer_at(&card, "pointerup", (x + 40, y + 40));
    tick().await;
    let moved = style(&host, selector);
    assert_ne!(moved, before);
    press(&body(), "z", true, false);
    tick().await;
    let html = host.inner_html();
    assert_eq!(style(&host, selector), before, "{html}");
    assert!(
        element(&host, ".flow-toolbar button[aria-label=Undo]").has_attribute("disabled"),
        "{html}"
    );
}

/// A bubbling `kind` drag event at client point `at` on `target`.
fn drag_at(target: &web_sys::Element, kind: &str, at: (i32, i32)) {
    let init = web_sys::DragEventInit::new();
    init.set_bubbles(true);
    init.set_cancelable(true);
    init.set_client_x(at.0);
    init.set_client_y(at.1);
    target
        .dispatch_event(&web_sys::DragEvent::new_with_event_init_dict(kind, &init).unwrap())
        .unwrap();
}

/// `(x, y, zoom)` of a `transform:translate(Xpx,Ypx) scale(Z)` style.
fn transform(style: &str) -> (f64, f64, f64) {
    let nums: Vec<f64> = style
        .split(|c: char| !(c.is_ascii_digit() || c == '.' || c == '-'))
        .filter_map(|t| t.parse().ok())
        .collect();
    (nums[0], nums[1], nums[2])
}

/// `(left, top)` of a `left:Xpx;top:Ypx` style.
fn corner(style: &str) -> (f64, f64) {
    let px = |key: &str| {
        let rest = &style[style.find(key).unwrap() + key.len()..];
        rest[..rest.find("px").unwrap()].parse().unwrap()
    };
    (px("left:"), px("top:"))
}

#[wasm_bindgen_test]
async fn drop_adds_under_the_pointer() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    click(&host, ".flow-controls__btn[aria-label=\"Zoom in\"]");
    tick().await;
    let row = (0..count(&host, ".flow-library__item"))
        .filter_map(|i| {
            host.query_selector_all(".flow-library__item")
                .unwrap()
                .item(i)
        })
        .filter_map(|n| n.dyn_into::<web_sys::Element>().ok())
        .find(|el| el.text_content().unwrap_or_default().contains("Wait"))
        .unwrap();
    let canvas = element(&host, ".flow-canvas");
    let rect = canvas.get_bounding_client_rect();
    let at = (rect.left() as i32 + 200, rect.top() as i32 + 200);
    drag_at(&row, "dragstart", (0, 0));
    drag_at(&canvas, "dragover", at);
    drag_at(&canvas, "drop", at);
    drag_at(&row, "dragend", (0, 0));
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, "[data-node=delay_1]"), 1, "{html}");
    // The card's stored corner, through the camera (`screen = world * zoom + pan`), puts its
    // centre on the drop point. The test page has no stylesheet, so rects can't tell.
    let (tx, ty, zoom) = transform(&style(&host, ".flow-world"));
    let (left, top) = corner(&style(&host, "[data-node=delay_1]"));
    let cx = left * zoom + tx + 232.0 * zoom / 2.0;
    let cy = top * zoom + ty + 52.0 * zoom / 2.0;
    // The drop point within the canvas (the canvas' corner is fractional, the event's not).
    let (dx, dy) = (f64::from(at.0) - rect.left(), f64::from(at.1) - rect.top());
    assert!(
        (cx - dx).abs() < 0.01 && (cy - dy).abs() < 0.01,
        "card centred at ({cx}, {cy}) in the canvas, dropped at ({dx}, {dy})"
    );
    assert_eq!(count(&host, ".flow-library"), 1, "{html}");
}

#[wasm_bindgen_test]
async fn ws_update_keeps_draft() {
    let (host, _mounted, house) = editing(reference_view(None, vec![done_run()])).await;
    select_card(&host, "on").await;
    press(&body(), "Backspace", false, false);
    tick().await;
    assert_eq!(count(&host, "[data-node=on]"), 0);
    click(&host, ".flow-toolbar__runs");
    tick().await;
    assert_eq!(count(&host, ".flow-drawer--open"), 1);
    assert_eq!(count(&host, ".run-list > li"), 1);
    let mut newer = done_run();
    newer.id = "r2".into();
    house
        .flows
        .set(vec![reference_view(None, vec![newer, done_run()])]);
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, "[data-node=on]"), 0, "{html}");
    assert_eq!(count(&host, ".flow-graph--editing"), 1, "{html}");
    assert_eq!(count(&host, ".run-list > li"), 2, "{html}");
}

#[wasm_bindgen_test]
async fn tidy_is_undoable() {
    // Hand-placed positions that the automatic layout would not pick.
    let mut definition = Flow::decode(REFERENCE.as_bytes()).unwrap();
    for (i, node) in definition.nodes.iter_mut().enumerate() {
        node.position = Some(Position {
            x: 0.0,
            y: 300.0 * i as f64,
        });
    }
    let view = FlowView {
        definition,
        invalid: None,
        active_run: None,
        runs: vec![],
    };
    let (host, _mounted, _) = editing(view).await;
    let selector = "[data-node=off]";
    let before = style(&host, selector);
    click(&host, ".flow-toolbar button[aria-label=Tidy]");
    tick().await;
    assert_ne!(style(&host, selector), before);
    click(&host, ".flow-toolbar button[aria-label=Undo]");
    tick().await;
    assert_eq!(style(&host, selector), before, "{}", host.inner_html());
}

/// Sets `input`'s value and fires a bubbling `kind` event on it (`input` or `change`).
fn type_into(input: &web_sys::HtmlInputElement, value: &str, kind: &str) {
    input.set_value(value);
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    input
        .dispatch_event(&web_sys::Event::new_with_event_init_dict(kind, &init).unwrap())
        .unwrap();
}

#[wasm_bindgen_test]
async fn field_edit_is_one_undo_step() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    select_card(&host, "wait").await;
    // The delay's fields: its name, then its duration.
    let duration: web_sys::HtmlInputElement = host
        .query_selector_all(".flow-drawer input.field")
        .unwrap()
        .item(1)
        .unwrap()
        .unchecked_into();
    duration.focus().unwrap();
    type_into(&duration, "1", "input");
    type_into(&duration, "10", "input");
    type_into(&duration, "10m", "input");
    type_into(&duration, "10m", "change");
    tick().await;
    assert_eq!(
        text(&host, "[data-node=wait] .flow-node__title"),
        "Wait 10\u{a0}min"
    );
    click(&host, ".flow-toolbar button[aria-label=Undo]");
    tick().await;
    let html = host.inner_html();
    assert_eq!(
        text(&host, "[data-node=wait] .flow-node__title"),
        "Wait 5\u{a0}min",
        "{html}"
    );
    assert!(
        element(&host, ".flow-toolbar button[aria-label=Undo]").has_attribute("disabled"),
        "{html}"
    );
    gloo_timers::future::TimeoutFuture::new(50).await;
    // The form shows the restored value.
    let duration: web_sys::HtmlInputElement = host
        .query_selector_all(".flow-drawer input.field")
        .unwrap()
        .item(1)
        .unwrap()
        .unchecked_into();
    assert_eq!(duration.value(), "5m");
}

// ---- Picker ---------------------------------------------------------------------------------

/// Opens the picker and types `query` into its search box.
async fn search(host: &web_sys::Element, query: &str) {
    let input: web_sys::HtmlInputElement = element(host, ".flow-library__search").unchecked_into();
    type_into(&input, query, "input");
    tick().await;
}

/// Clicks the library row titled `title`.
async fn add_row(host: &web_sys::Element, title: &str) {
    let rows = host.query_selector_all(".flow-library__item").unwrap();
    let row = (0..rows.length())
        .filter_map(|i| rows.item(i))
        .filter_map(|n| n.dyn_into::<web_sys::HtmlElement>().ok())
        .find(|el| {
            el.query_selector(".flow-library__title")
                .unwrap()
                .and_then(|t| t.text_content())
                .as_deref()
                == Some(title)
        })
        .unwrap_or_else(|| panic!("no library row {title}: {}", host.inner_html()));
    row.click();
    tick().await;
}

fn table_lamp() -> oikos_api::EntityView {
    use oikos_core::model::{
        Capabilities, EntityCategory, EntityState, Kind, StateValue, UiOverride,
    };
    oikos_api::EntityView {
        id: "light.table".into(),
        device_id: None,
        name: "Lumière table".into(),
        device_name: None,
        label: None,
        kind: Kind::Light,
        protocol_kind: Kind::Light,
        component: Kind::Light.component().to_string(),
        device_class: None,
        unit: None,
        icon: None,
        category: EntityCategory::Primary,
        enabled_by_default: true,
        area: "salon".into(),
        part_of: None,
        capabilities: Capabilities::default(),
        ui: UiOverride::default(),
        state: EntityState {
            value: StateValue::OnOff(false),
            ..Default::default()
        },
    }
}

#[wasm_bindgen_test]
async fn library_open_in_edit_mode() {
    let host = host();
    let _mounted = mount(
        &host,
        vec![reference_view(None, vec![])],
        "/flows/living_led_motion",
    );
    tick().await;
    assert_eq!(count(&host, ".flow-drawer--open"), 0);
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-drawer--open"), 1, "{html}");
    assert_eq!(count(&host, ".flow-library"), 1, "{html}");
    assert_eq!(count(&host, ".flow-library__item"), 7, "{html}");
    assert_eq!(count(&host, ".flow-picker-wrap"), 0, "{html}");
}

#[wasm_bindgen_test]
async fn library_click_adds_and_stays_open() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    add_row(&host, "Wait").await;
    add_row(&host, "Wait").await;
    let html = host.inner_html();
    assert_eq!(count(&host, "[data-node=delay_1]"), 1, "{html}");
    assert_eq!(count(&host, "[data-node=delay_2]"), 1, "{html}");
    assert_eq!(count(&host, ".flow-node--selected"), 0, "{html}");
    assert_eq!(count(&host, ".flow-library"), 1, "{html}");
    assert_ne!(
        style(&host, "[data-node=delay_1]"),
        style(&host, "[data-node=delay_2]"),
        "the second card does not sit on the first"
    );
    // Each add is one undo step.
    press(&body(), "z", true, false);
    tick().await;
    assert_eq!(count(&host, "[data-node=delay_2]"), 0);
    assert_eq!(count(&host, "[data-node=delay_1]"), 1);
}

#[wasm_bindgen_test]
async fn library_enter_adds_highlighted() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    search(&host, "Wait").await;
    press(
        &element(&host, ".flow-library__search"),
        "Enter",
        false,
        false,
    );
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, "[data-node=delay_1]"), 1, "{html}");
    assert_eq!(count(&host, ".flow-library"), 1, "{html}");
}

#[wasm_bindgen_test]
async fn undo_in_library_search_after_enter_undoes_the_add() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    search(&host, "Wait").await;
    let input = element(&host, ".flow-library__search");
    press(&input, "Enter", false, false);
    tick().await;
    assert_eq!(count(&host, "[data-node=delay_1]"), 1);
    // The search box keeps the focus, so Mod-Z lands there: it takes the card back out.
    press(&input, "z", true, false);
    tick().await;
    assert_eq!(
        count(&host, "[data-node=delay_1]"),
        0,
        "{}",
        host.inner_html()
    );
    press(&input, "z", true, true);
    tick().await;
    assert_eq!(
        count(&host, "[data-node=delay_1]"),
        1,
        "{}",
        host.inner_html()
    );
}

#[wasm_bindgen_test]
async fn undo_in_library_search_after_typing_leaves_the_flow() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    search(&host, "Wait").await;
    let input = element(&host, ".flow-library__search");
    press(&input, "Enter", false, false);
    tick().await;
    // Typing again hands Mod-Z back to the text.
    search(&host, "Wai").await;
    press(&input, "z", true, false);
    tick().await;
    assert_eq!(
        count(&host, "[data-node=delay_1]"),
        1,
        "{}",
        host.inner_html()
    );
}

#[wasm_bindgen_test]
async fn library_search_survives_a_trip_to_settings() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    search(&host, "Wait").await;
    let rows = count(&host, ".flow-library__item");
    select_card(&host, "moving").await;
    click(&host, ".node-editor__header .btn");
    // The new search box picks its value up from an effect.
    gloo_timers::future::TimeoutFuture::new(50).await;
    let input: web_sys::HtmlInputElement = element(&host, ".flow-library__search").unchecked_into();
    assert_eq!(input.value(), "Wait", "{}", host.inner_html());
    assert_eq!(count(&host, ".flow-library__item"), rows);
}

#[wasm_bindgen_test]
async fn leaving_edit_mode_clears_the_library_search() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    search(&host, "Wait").await;
    click(&host, ".flow-toolbar button[aria-label=\"Cancel editing\"]");
    tick().await;
    click(&host, ".flow-toolbar__edit");
    gloo_timers::future::TimeoutFuture::new(50).await;
    let input: web_sys::HtmlInputElement = element(&host, ".flow-library__search").unchecked_into();
    assert_eq!(input.value(), "", "{}", host.inner_html());
}

#[wasm_bindgen_test]
async fn leaving_edit_mode_slides_the_library_out() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    assert_eq!(count(&host, ".flow-library"), 1);
    click(&host, ".flow-toolbar button[aria-label=\"Cancel editing\"]");
    tick().await;
    // The closing drawer keeps the library; the runs do not flash in for the slide.
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-drawer--open"), 0, "{html}");
    assert_eq!(count(&host, ".flow-drawer .flow-library"), 1, "{html}");
    assert_eq!(count(&host, ".flow-drawer__close"), 0, "{html}");
}

#[wasm_bindgen_test]
async fn selecting_a_card_swaps_to_settings_and_back() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    select_card(&host, "moving").await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-drawer .node-editor"), 1, "{html}");
    assert_eq!(count(&host, ".flow-library"), 0, "{html}");
    click(&host, ".node-editor__header .btn");
    tick().await;
    assert_eq!(count(&host, ".flow-library"), 1, "{}", host.inner_html());
}

#[wasm_bindgen_test]
async fn edit_mode_refits_beside_the_library() {
    let host = host();
    let _mounted = mount(
        &host,
        vec![reference_view(None, vec![])],
        "/flows/living_led_motion",
    );
    gloo_timers::future::TimeoutFuture::new(50).await;
    let before = style(&host, ".flow-world");
    click(&host, ".flow-toolbar__edit");
    // The panel opens, then the camera refits: two effects.
    gloo_timers::future::TimeoutFuture::new(20).await;
    let after = style(&host, ".flow-world");
    assert_ne!(
        before, after,
        "the untouched view refits for the open panel"
    );
    // Fit frames the graph in what the panel leaves free.
    click(&host, ".flow-controls__btn[aria-label=\"Fit view\"]");
    tick().await;
    assert_eq!(style(&host, ".flow-world"), after);
}

#[wasm_bindgen_test]
async fn runs_button_shows_runs_over_settings() {
    let (host, _mounted, _) = editing(reference_view(None, vec![done_run()])).await;
    click(&host, ".flow-toolbar__runs");
    tick().await;
    select_card(&host, "moving").await;
    assert_eq!(count(&host, ".flow-drawer .node-editor"), 1);
    assert_eq!(
        element(&host, ".flow-toolbar__runs").get_attribute("aria-expanded"),
        Some("false".into()),
        "the button follows what the panel shows"
    );
    // One click brings the runs back.
    click(&host, ".flow-toolbar__runs");
    tick().await;
    let html = host.inner_html();
    assert_eq!(count(&host, ".flow-drawer .run-list"), 1, "{html}");
    assert_eq!(count(&host, ".flow-drawer .node-editor"), 0, "{html}");
}

#[wasm_bindgen_test]
async fn backspace_in_library_search_keeps_selection() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    // A selected wire leaves the library showing; ⌫ in its search edits the text only.
    click_any(&host, ".flow-wire-hit");
    tick().await;
    assert_eq!(count(&host, ".flow-wire--selected"), 1);
    let input: web_sys::HtmlElement = element(&host, ".flow-library__search").unchecked_into();
    input.focus().unwrap();
    press(&input, "Backspace", false, false);
    tick().await;
    assert_eq!(count(&host, ".flow-wire"), 4, "{}", host.inner_html());
}

#[wasm_bindgen_test]
async fn runs_from_edit_mode_returns_to_library() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    click(&host, ".flow-toolbar__runs");
    tick().await;
    let html = host.inner_html();
    assert!(
        count(
            &host,
            ".flow-drawer .run-list, .flow-drawer .flow-drawer__empty"
        ) >= 1,
        "{html}"
    );
    assert_eq!(count(&host, ".flow-library"), 0, "{html}");
    click(&host, ".flow-drawer__close");
    tick().await;
    assert_eq!(count(&host, ".flow-library"), 1, "{}", host.inner_html());
}

#[wasm_bindgen_test]
async fn picker_groups_entity_results() {
    let (host, _mounted, house) = editing(reference_view(None, vec![])).await;
    house
        .entities
        .update(|e| drop(e.insert("light.table".into(), table_lamp())));
    search(&host, "table").await;
    let headers: Vec<String> = (0..count(&host, ".flow-library__header"))
        .map(|i| {
            host.query_selector_all(".flow-library__header")
                .unwrap()
                .item(i)
                .unwrap()
                .text_content()
                .unwrap()
        })
        .collect();
    assert_eq!(
        headers,
        ["WHEN · TRIGGER", "AND · CHECK", "THEN · ACTION"],
        "{}",
        host.inner_html()
    );
}

// ---- New flow and renaming ------------------------------------------------------------------

#[wasm_bindgen_test]
async fn new_flow_starts_in_edit_mode() {
    let host = host();
    let _mounted = mount(&host, vec![], "/flows/new");
    gloo_timers::future::TimeoutFuture::new(50).await;
    let name: web_sys::HtmlInputElement =
        element(&host, ".flow-toolbar input[aria-label=\"Flow name\"]").unchecked_into();
    assert_eq!(name.value(), "New flow");
    assert_eq!(count(&host, ".flow-node"), 0);
    assert_eq!(count(&host, ".flow-switch"), 0);
    assert_eq!(
        count(&host, ".flow-graph--editing"),
        1,
        "{}",
        host.inner_html()
    );
}

#[wasm_bindgen_test]
async fn rename_updates_draft_name() {
    let (host, _mounted, _) = editing(reference_view(None, vec![])).await;
    gloo_timers::future::TimeoutFuture::new(50).await;
    let name: web_sys::HtmlInputElement =
        element(&host, ".flow-toolbar input[aria-label=\"Flow name\"]").unchecked_into();
    assert_eq!(name.value(), "Living LED on motion");
    name.focus().unwrap();
    type_into(&name, "Hall light", "input");
    tick().await;
    assert_eq!(name.value(), "Hall light");
    // Saving sends the new name under the original id.
    let fetch = FakeFetch::answer(&write_outcome(&reference_view(None, vec![])));
    click(&host, ".flow-toolbar button[aria-label=Save]");
    gloo_timers::future::TimeoutFuture::new(50).await;
    let requests = puts(&fetch);
    assert_eq!(requests.len(), 1, "{requests:?}");
    assert_eq!(requests[0].1, "/api/v1/flows/living_led_motion");
    let sent: serde_json::Value = serde_json::from_str(&requests[0].2).unwrap();
    assert_eq!(sent["id"], "living_led_motion");
    assert_eq!(sent["name"], "Hall light");
    drop(fetch);
    // Back in view mode the header shows what the hub holds.
    assert_eq!(
        text(&host, ".flow-toolbar h1.flow-toolbar__name"),
        "Living LED on motion"
    );
}

/// Saves a new flow called "Nouveau flow" with `existing` already in the house and the hub
/// answering with `answer_id`; returns the PUT it made and the path the app ended on.
async fn save_new_flow(
    existing: Vec<FlowView>,
    answer_id: &str,
) -> (Vec<(String, String, String)>, String, String) {
    let host = host();
    let _mounted = mount(&host, existing, "/flows/new");
    gloo_timers::future::TimeoutFuture::new(50).await;
    let saved = FlowView {
        definition: Flow {
            id: answer_id.into(),
            name: "Nouveau flow".into(),
            ..Default::default()
        },
        invalid: None,
        active_run: None,
        runs: vec![],
    };
    let name: web_sys::HtmlInputElement =
        element(&host, ".flow-toolbar input[aria-label=\"Flow name\"]").unchecked_into();
    type_into(&name, "Nouveau flow", "input");
    let fetch = FakeFetch::answer(&write_outcome(&saved));
    click(&host, ".flow-toolbar button[aria-label=Save]");
    gloo_timers::future::TimeoutFuture::new(100).await;
    let path = web_sys::window().unwrap().location().pathname().unwrap();
    assert_eq!(count(&host, ".flow-graph--editing"), 0);
    (puts(&fetch), path, host.inner_html())
}

/// The id a new flow's first Save sent, after checking it is a v4 UUID and the PUT's URL.
fn sent_uuid(requests: &[(String, String, String)]) -> String {
    assert_eq!(requests.len(), 1, "{requests:?}");
    let (method, url, body) = &requests[0];
    assert_eq!(method, "PUT");
    let sent: serde_json::Value = serde_json::from_str(body).unwrap();
    let id = sent["id"].as_str().unwrap().to_owned();
    assert_eq!(id.len(), 36, "{id}");
    assert_eq!(id.as_bytes()[14], b'4', "{id}");
    assert!(oikos_core::flow::is_valid_flow_id(&id), "{id}");
    assert!(!id.contains("nouveau"), "{id}");
    assert_eq!(url, &format!("/api/v1/flows/{id}"));
    assert_eq!(sent["enabled"], false);
    id
}

#[wasm_bindgen_test]
async fn saving_a_new_flow_gives_it_a_uuid_and_opens_it() {
    let answer = "0b6f3c1e-2d4a-4f8b-9c7d-5e1a2b3c4d5e";
    let (requests, path, html) = save_new_flow(vec![], answer).await;
    sent_uuid(&requests);
    assert_eq!(path, format!("/flows/{answer}"), "{html}");
}

#[wasm_bindgen_test]
async fn new_flows_with_the_same_name_get_different_ids() {
    let answer = "0b6f3c1e-2d4a-4f8b-9c7d-5e1a2b3c4d5e";
    let (first, _, _) = save_new_flow(vec![], answer).await;
    let (second, _, _) = save_new_flow(vec![], answer).await;
    assert_ne!(sent_uuid(&first), sent_uuid(&second));
}

#[wasm_bindgen_test]
async fn cancel_on_an_unsaved_flow_goes_to_the_list() {
    let host = host();
    let _mounted = mount(&host, vec![], "/flows/new");
    gloo_timers::future::TimeoutFuture::new(50).await;
    click(&host, ".flow-toolbar button[aria-label=\"Cancel editing\"]");
    gloo_timers::future::TimeoutFuture::new(50).await;
    let path = web_sys::window().unwrap().location().pathname().unwrap();
    assert_eq!(path, "/flows");
}

#[wasm_bindgen_test]
async fn signal_trigger_editor_lists_signals() {
    use oikos_core::model::{Capabilities, EntityCategory, EntityState, Kind, UiOverride};
    const SIGNAL_FLOW: &str = r#"{
      "id": "remote_flow", "name": "Remote", "enabled": true, "mode": "single",
      "nodes": [
        { "id": "press", "type": "trigger.signal",
          "params": { "entity": "remote_1", "signal": "on" } }
      ],
      "edges": []
    }"#;
    let view = FlowView {
        definition: Flow::decode(SIGNAL_FLOW.as_bytes()).unwrap(),
        invalid: None,
        active_run: None,
        runs: vec![],
    };
    let host = host();
    let (_mounted, house) = mount_house(&host, vec![view], "/flows/remote_flow");
    house.entities.update(|m| {
        m.insert(
            "remote_1".into(),
            oikos_api::EntityView {
                id: "remote_1".into(),
                device_id: None,
                name: "Remote".into(),
                device_name: None,
                label: None,
                kind: Kind::Generic,
                component: "device_automation".into(),
                protocol_kind: Kind::Generic,
                device_class: None,
                unit: None,
                icon: None,
                category: EntityCategory::Primary,
                enabled_by_default: true,
                area: "salon".into(),
                part_of: None,
                capabilities: Capabilities {
                    generic: Some(oikos_core::types::GenericCaps {
                        signals: vec!["on".into(), "off".into(), "brightness_move_up".into()],
                        ..Default::default()
                    }),
                    ..Default::default()
                },
                ui: UiOverride::default(),
                state: EntityState::default(),
            },
        );
    });
    gloo_timers::future::TimeoutFuture::new(50).await;
    click(&host, ".flow-toolbar__edit");
    tick().await;
    select_card(&host, "press").await;
    let chips = text(&host, ".signal-chips");
    for name in ["any", "on", "off", "brightness_move_up"] {
        assert!(chips.contains(name), "{chips}");
    }
    assert_eq!(count(&host, ".signal-chips .chip"), 4);
    assert_eq!(count(&host, ".signal-chips .chip--selected"), 1);
}

/// Opens the entity picker of the open card's settings.
async fn open_entity_picker(host: &web_sys::Element) {
    let button: web_sys::HtmlElement = host
        .query_selector(".flow-drawer button.picker-field")
        .unwrap()
        .unwrap_or_else(|| panic!("no picker field: {}", host.inner_html()))
        .unchecked_into();
    button.click();
    tick().await;
}

/// Closing the picker pops the history entry it pushed; let that land before the next test
/// mounts at its own path.
async fn settle_history() {
    gloo_timers::future::TimeoutFuture::new(100).await;
}

#[wasm_bindgen_test]
async fn command_entity_picked_from_picker() {
    let (host, _mounted, house) = editing(reference_view(None, vec![])).await;
    let lamp = table_lamp();
    house.entities.update(|m| {
        m.insert(lamp.id.clone(), lamp);
    });
    select_card(&host, "on").await;
    assert_eq!(
        count(&host, ".flow-drawer input[placeholder='entity.id']"),
        0
    );
    open_entity_picker(&host).await;
    let row: web_sys::HtmlElement = host
        .query_selector("[id$='-opt-light.table']")
        .unwrap()
        .unwrap_or_else(|| panic!("no light row: {}", host.inner_html()))
        .unchecked_into();
    row.click();
    tick().await;
    let field = text(&host, ".flow-drawer button.picker-field");
    assert!(field.contains("Lumière table"), "{field}");
    settle_history().await;
}

#[wasm_bindgen_test]
async fn signal_picker_lists_only_signals() {
    use oikos_core::model::Kind;
    const SIGNAL_FLOW: &str = r#"{
      "id": "remote_flow", "name": "Remote", "enabled": true, "mode": "single",
      "nodes": [ { "id": "press", "type": "trigger.signal", "params": { "entity": "remote_1" } } ],
      "edges": []
    }"#;
    let view = FlowView {
        definition: Flow::decode(SIGNAL_FLOW.as_bytes()).unwrap(),
        invalid: None,
        active_run: None,
        runs: vec![],
    };
    let host = host();
    let (_mounted, house) = mount_house(&host, vec![view], "/flows/remote_flow");
    let mut remote = table_lamp();
    remote.id = "remote_1".into();
    remote.name = "Remote".into();
    remote.kind = Kind::Generic;
    remote.protocol_kind = Kind::Generic;
    remote.component = "device_automation".into();
    let mut sensor = table_lamp();
    sensor.id = "sensor.temp".into();
    sensor.name = "Temperature".into();
    sensor.kind = Kind::Sensor;
    sensor.protocol_kind = Kind::Sensor;
    sensor.component = "sensor".into();
    house.entities.update(|m| {
        m.insert(remote.id.clone(), remote);
        m.insert(sensor.id.clone(), sensor);
    });
    gloo_timers::future::TimeoutFuture::new(50).await;
    click(&host, ".flow-toolbar__edit");
    tick().await;
    select_card(&host, "press").await;
    open_entity_picker(&host).await;
    let html = host.inner_html();
    assert_eq!(count(&host, "[id$='-opt-remote_1']"), 1, "{html}");
    assert_eq!(count(&host, "[id$='-opt-sensor.temp']"), 0, "{html}");
    click(&host, "[id$='-opt-remote_1']");
    tick().await;
    settle_history().await;
}

/// Stands in for the shell chrome the canvas sits beside.
fn fake_chrome(sidebar_position: &str) -> web_sys::Element {
    let document = web_sys::window().unwrap().document().unwrap();
    while let Some(stale) = document.query_selector(".sidebar").unwrap() {
        stale.parent_element().unwrap().remove();
    }
    let chrome = document.create_element("div").unwrap();
    chrome.set_inner_html(&format!(
        r#"<aside class="sidebar" style="position:{sidebar_position};left:0;top:0;width:210px;height:300px"></aside>
           <header class="content-header" style="position:fixed;left:210px;top:0;height:46px;width:300px"></header>"#
    ));
    document.body().unwrap().append_child(&chrome).unwrap();
    chrome
}

fn flow_style(host: &web_sys::Element) -> String {
    host.query_selector(".flow-canvas-screen")
        .unwrap()
        .unwrap()
        .get_attribute("style")
        .unwrap_or_default()
}

#[wasm_bindgen_test]
async fn canvas_inset_clears_the_sidebar() {
    let chrome = fake_chrome("sticky");
    let host = host();
    let _mounted = mount(
        &host,
        vec![reference_view(None, vec![])],
        "/flows/living_led_motion",
    );
    tick().await;
    let style = flow_style(&host);
    assert!(style.contains("--flow-top:46px"), "{style}");
    let right = chrome
        .query_selector(".sidebar")
        .unwrap()
        .unwrap()
        .get_bounding_client_rect()
        .right();
    assert!(right >= 210.0);
    assert!(style.contains(&format!("--flow-left:{right}px")), "{style}");
    chrome.remove();
}

#[wasm_bindgen_test]
async fn canvas_ignores_an_off_canvas_sidebar() {
    let chrome = fake_chrome("fixed");
    let host = host();
    let _mounted = mount(
        &host,
        vec![reference_view(None, vec![])],
        "/flows/living_led_motion",
    );
    tick().await;
    let style = flow_style(&host);
    assert!(style.contains("--flow-left:0px"), "{style}");
    chrome.remove();
}

#[wasm_bindgen_test]
async fn canvas_resets_the_scroll_before_measuring() {
    let chrome = fake_chrome("sticky");
    // The header scrolls with the page, as the real one does.
    chrome
        .query_selector(".content-header")
        .unwrap()
        .unwrap()
        .set_attribute(
            "style",
            "position:absolute;left:210px;top:0;height:46px;width:300px",
        )
        .unwrap();
    let document = web_sys::window().unwrap().document().unwrap();
    let spacer = document.create_element("div").unwrap();
    spacer.set_attribute("style", "height:5000px").unwrap();
    document.body().unwrap().append_child(&spacer).unwrap();
    let window = web_sys::window().unwrap();
    window.scroll_to_with_x_and_y(0.0, 1200.0);
    assert!(window.scroll_y().unwrap() > 0.0);
    let host = host();
    let _mounted = mount(
        &host,
        vec![reference_view(None, vec![])],
        "/flows/living_led_motion",
    );
    tick().await;
    assert_eq!(window.scroll_y().unwrap(), 0.0);
    let style = flow_style(&host);
    assert!(style.contains("--flow-top:46px"), "{style}");
    spacer.remove();
    chrome.remove();
}

// ---- French --------------------------------------------------------------------------------

fn headers(host: &web_sys::Element) -> Vec<String> {
    (0..count(host, ".flow-library__header"))
        .map(|i| {
            host.query_selector_all(".flow-library__header")
                .unwrap()
                .item(i)
                .unwrap()
                .text_content()
                .unwrap()
        })
        .collect()
}

/// The canvas, its toolbar and the card library read French, then English after the toggle,
/// on the same mounted screen.
#[wasm_bindgen_test]
async fn the_canvas_follows_the_language_without_a_remount() {
    let lang = RwSignal::new(Lang::Fr);
    let host = host();
    let (_mounted, house) = mount_house_lang(
        &host,
        vec![reference_view(None, vec![done_run()])],
        "/flows/living_led_motion",
        lang,
    );
    gloo_timers::future::TimeoutFuture::new(50).await;
    house
        .entities
        .update(|e| drop(e.insert("light.table".into(), table_lamp())));
    click(&host, ".flow-toolbar__edit");
    tick().await;
    search(&host, "table").await;
    let text = || host.text_content().unwrap_or_default();

    assert_eq!(
        headers(&host),
        ["Déclencheurs", "Conditions", "Actions"],
        "{}",
        host.inner_html()
    );
    assert!(text().contains("Allumer"), "{}", text());
    for english in ["When", "WHEN", "Turn on"] {
        assert!(!text().contains(english), "{english}: {}", text());
    }
    let back = element(&host, ".flow-toolbar__back");
    assert_eq!(back.get_attribute("aria-label").as_deref(), Some("Retour"));
    let html = host.inner_html();
    assert!(html.contains("aria-label=\"Ajouter une carte\""), "{html}");
    assert!(html.contains("aria-label=\"Zoom avant\""), "{html}");
    assert!(html.contains("aria-label=\"Enregistrer\""), "{html}");

    lang.set(Lang::En);
    tick().await;
    assert_eq!(
        headers(&host),
        ["WHEN · TRIGGER", "AND · CHECK", "THEN · ACTION"],
        "{}",
        host.inner_html()
    );
    assert!(text().contains("Turn on"), "{}", text());
    assert!(!text().contains("Allumer"), "{}", text());
    let back = element(&host, ".flow-toolbar__back");
    assert_eq!(back.get_attribute("aria-label").as_deref(), Some("Back"));
    let html = host.inner_html();
    assert!(html.contains("aria-label=\"Add a card\""), "{html}");
    assert!(html.contains("aria-label=\"Zoom in\""), "{html}");
}

/// Run times read "il y a …" with the exact time as the tooltip, and flip with the toggle.
#[wasm_bindgen_test]
async fn the_runs_drawer_follows_the_language_without_a_remount() {
    let lang = RwSignal::new(Lang::Fr);
    let host = host();
    let (_mounted, _house) = mount_house_lang(
        &host,
        vec![reference_view(None, vec![done_run()])],
        "/flows/living_led_motion",
        lang,
    );
    gloo_timers::future::TimeoutFuture::new(50).await;
    click(&host, ".flow-toolbar__runs");
    tick().await;
    let text = || text(&host, ".flow-drawer");
    assert!(text().contains("Lancements récents"), "{}", text());
    assert!(text().contains("Terminé"), "{}", text());
    assert!(text().contains("il y a"), "{}", text());
    assert!(!text().contains("ago"), "{}", text());
    let title = element(&host, ".run__time").get_attribute("title").unwrap();
    assert!(title.contains("nov."), "{title}");

    lang.set(Lang::En);
    tick().await;
    assert!(text().contains("Recent runs"), "{}", text());
    assert!(text().contains("Done"), "{}", text());
    assert!(text().contains("ago"), "{}", text());
    assert!(!text().contains("il y a"), "{}", text());
    let title = element(&host, ".run__time").get_attribute("title").unwrap();
    assert!(title.contains("Nov"), "{title}");
}

/// A flow created on the French UI gets a French default name, which the toggle leaves alone.
#[wasm_bindgen_test]
async fn a_new_flow_is_named_in_the_language_active_at_creation() {
    let lang = RwSignal::new(Lang::Fr);
    let host = host();
    let _mounted = mount_house_lang(&host, vec![], "/flows/new", lang);
    gloo_timers::future::TimeoutFuture::new(50).await;
    let name = || {
        element(&host, ".flow-toolbar input.flow-toolbar__name--input")
            .unchecked_into::<web_sys::HtmlInputElement>()
            .value()
    };
    assert_eq!(name(), "Nouvelle automatisation");
    lang.set(Lang::En);
    tick().await;
    assert_eq!(name(), "Nouvelle automatisation");
}

#[wasm_bindgen_test]
async fn the_mode_select_saves_the_other_mode() {
    let view = reference_view(None, vec![]);
    let (want, label) = match view.definition.mode {
        FlowMode::Single => ("restart", "single"),
        FlowMode::Restart => ("single", "restart"),
    };
    let (host, _mounted, _) = editing(view.clone()).await;
    let select: web_sys::HtmlSelectElement =
        element(&host, ".flow-toolbar select[aria-label=Mode]").unchecked_into();
    assert_eq!(select.value(), label);
    select.set_value(want);
    let init = web_sys::EventInit::new();
    init.set_bubbles(true);
    select
        .dispatch_event(&web_sys::Event::new_with_event_init_dict("change", &init).unwrap())
        .unwrap();
    tick().await;
    let fetch = FakeFetch::answer(&write_outcome(&view));
    click(&host, ".flow-toolbar button[aria-label=Save]");
    gloo_timers::future::TimeoutFuture::new(50).await;
    let requests = puts(&fetch);
    assert_eq!(requests.len(), 1, "{requests:?}");
    let sent: serde_json::Value = serde_json::from_str(&requests[0].2).unwrap();
    assert_eq!(sent["mode"], want);
}

/// An `if` with no value yet shows an empty entry, not the list's first value it never stored.
#[wasm_bindgen_test]
async fn if_value_without_a_value_shows_no_choice() {
    use oikos_core::model::Kind;
    const UNSET: &str = r#"{
      "id": "living_led_motion", "name": "Unset", "enabled": true, "mode": "single",
      "nodes": [
        { "id": "shutter", "type": "trigger.state", "params": { "entity": "cover.living" } },
        { "id": "check", "type": "if", "params": { "op": "eq" } }
      ],
      "edges": [ { "from": "shutter", "to": "check" } ]
    }"#;
    let view = FlowView {
        definition: Flow::decode(UNSET.as_bytes()).unwrap(),
        invalid: None,
        active_run: None,
        runs: vec![],
    };
    let (host, _mounted, house) = editing(view).await;
    let mut cover = table_lamp();
    cover.id = "cover.living".into();
    cover.kind = Kind::Cover;
    cover.protocol_kind = Kind::Cover;
    cover.component = "cover".into();
    house.entities.update(|m| {
        m.insert(cover.id.clone(), cover);
    });
    select_card(&host, "check").await;
    let select: web_sys::HtmlSelectElement = host
        .query_selector(".flow-drawer select:has(option[value='other'])")
        .unwrap()
        .unwrap_or_else(|| panic!("no value list: {}", host.inner_html()))
        .unchecked_into();
    assert_eq!(select.value(), "", "{}", host.inner_html());
    settle_history().await;
}
