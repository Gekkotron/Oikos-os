#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;
use oikos_api::{
    Ambiance, AmbianceAction, AmbianceView, AreaView, Category, CategoryAction, DeviceAction,
    EntityView, Target, WsEvent,
};
use oikos_climate::doc::HouseSettings;
use oikos_climate::view::ClimateView;
use oikos_core::model::{Capabilities, Command, EntityCategory, EntityState, Kind, UiOverride};
use oikos_web::ambiances::editor::AmbianceEditorScreen;
use oikos_web::components::toast::Toasts;
use oikos_web::house::House;
use oikos_web::i18n::Lang;
use wasm_bindgen::{JsCast, JsValue};
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

#[path = "support/fetch_stub.rs"]
mod fetch_stub;
use fetch_stub::FetchStub;

fn mood(id: &str, order: u32, actions: Vec<AmbianceAction>) -> Ambiance {
    Ambiance {
        id: id.into(),
        name: format!("Mood {id}"),
        subtitle: None,
        icon: None,
        pinned: false,
        order,
        actions,
        extra: Default::default(),
    }
}

fn view(a: Ambiance) -> AmbianceView {
    AmbianceView {
        id: a.id.clone(),
        ambiance: Some(a),
        invalid: None,
    }
}

fn light(id: &str, area: &str) -> EntityView {
    EntityView {
        id: id.into(),
        device_id: None,
        name: format!("Lamp {id}"),
        device_name: None,
        label: None,
        kind: Kind::Light,
        component: Kind::Light.component().to_string(),
        protocol_kind: Kind::Light,
        device_class: None,
        unit: None,
        icon: None,
        category: EntityCategory::Primary,
        enabled_by_default: true,
        area: area.into(),
        part_of: None,
        capabilities: Capabilities::default(),
        ui: UiOverride::default(),
        state: EntityState::default(),
    }
}

fn area(id: &str, name: &str) -> AreaView {
    AreaView {
        id: id.into(),
        name: name.into(),
        icon: None,
        order: None,
        entity_count: 0,
        primary_count: 0,
        group_names: Default::default(),
        group_favorites: Default::default(),
    }
}

fn lights_off() -> AmbianceAction {
    AmbianceAction::Category(CategoryAction {
        what: Category::Lights,
        to: Target::Off,
        brightness: None,
        area: None,
        except: Vec::new(),
    })
}

fn seed(house: House, list: Vec<AmbianceView>) {
    house.apply(WsEvent::AmbiancesChanged { ambiances: list });
}

struct Mounted {
    _app: Box<dyn std::any::Any>,
    host: web_sys::Element,
    house: House,
}

/// The editor at `path_`, with a house the test seeds.
fn mount(path_: &str, seed_house: impl FnOnce(House)) -> Mounted {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    web_sys::window()
        .unwrap()
        .history()
        .unwrap()
        .replace_state_with_url(&JsValue::NULL, "", Some(path_))
        .unwrap();
    let house = House::new();
    seed_house(house);
    let app = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(house);
        provide_context(Toasts::new());
        provide_context(RwSignal::new(Lang::En));
        oikos_web::nav::install_nav();
        view! {
            <Router>
                <Routes fallback=|| "fallback">
                    <Route path=path!("/ambiances") view=|| "list" />
                    <Route path=path!("/ambiances/new") view=AmbianceEditorScreen />
                    <Route path=path!("/ambiances/:id") view=AmbianceEditorScreen />
                </Routes>
            </Router>
        }
    });
    Mounted {
        _app: Box::new(app),
        host,
        house,
    }
}

fn all(host: &web_sys::Element, selector: &str) -> Vec<web_sys::HtmlElement> {
    let nodes = host.query_selector_all(selector).unwrap();
    (0..nodes.length())
        .map(|i| nodes.item(i).unwrap().unchecked_into())
        .collect()
}

fn one(host: &web_sys::Element, selector: &str) -> web_sys::HtmlElement {
    all(host, selector)
        .into_iter()
        .next()
        .unwrap_or_else(|| panic!("no {selector}"))
}

fn text(host: &web_sys::Element, selector: &str) -> String {
    one(host, selector).text_content().unwrap_or_default()
}

/// Sets an input's or select's value and fires `event` on it.
fn set_value(el: &web_sys::HtmlElement, value: &str, event: &str) {
    js_sys::Reflect::set(el, &"value".into(), &value.into()).unwrap();
    el.dispatch_event(&web_sys::Event::new(event).unwrap())
        .unwrap();
}

fn disabled(el: &web_sys::HtmlElement) -> bool {
    el.has_attribute("disabled")
}

async fn settle() {
    tick().await;
    gloo_timers::future::TimeoutFuture::new(50).await;
    tick().await;
}

fn climate_json(enabled: bool) -> String {
    serde_json::to_string(&ClimateView {
        enabled,
        house: HouseSettings::default(),
        house_invalid: None,
        history: false,
        timezone: "UTC".into(),
        zones: Vec::new(),
    })
    .unwrap()
}

fn put_body(stub: &FetchStub) -> serde_json::Value {
    let put = stub
        .requests()
        .into_iter()
        .find(|r| r.method == "PUT")
        .expect("a PUT was sent");
    serde_json::from_str(&put.body).unwrap()
}

#[wasm_bindgen_test]
async fn new_editor_saves_slug_and_next_order() {
    let stub = FetchStub::install();
    let reply = serde_json::to_string(&view(mood("cosy_night", 4, vec![]))).unwrap();
    stub.reply("PUT", "/api/v1/ambiances/cosy_night", 200, &reply);
    let m = mount("/ambiances/new", |h| {
        seed(h, vec![view(mood("a", 3, vec![]))])
    });
    settle().await;
    assert!(disabled(&one(&m.host, ".ambiance-save")), "needs a name");
    set_value(&one(&m.host, ".ambiance-name"), "Cosy night", "input");
    settle().await;
    one(&m.host, ".ambiance-save").click();
    settle().await;
    let body = put_body(&stub);
    assert_eq!(body["id"], "cosy_night");
    assert_eq!(body["order"], 4);
    assert_eq!(body["name"], "Cosy night");
    assert_eq!(body["pinned"], true, "fewer than four are pinned");
    assert!(
        m.house
            .ambiances
            .get_untracked()
            .iter()
            .any(|v| v.id == "cosy_night")
    );
}

#[wasm_bindgen_test]
async fn a_mood_named_new_does_not_take_a_route_name() {
    let stub = FetchStub::install();
    let reply = serde_json::to_string(&view(mood("new_2", 1, vec![]))).unwrap();
    stub.reply("PUT", "/api/v1/ambiances/new_2", 200, &reply);
    let m = mount("/ambiances/new", |h| seed(h, vec![]));
    settle().await;
    set_value(&one(&m.host, ".ambiance-name"), "New", "input");
    settle().await;
    one(&m.host, ".ambiance-save").click();
    settle().await;
    assert_eq!(put_body(&stub)["id"], "new_2");
}

#[wasm_bindgen_test]
async fn a_refused_save_toasts_and_stays() {
    let stub = FetchStub::install();
    stub.reply(
        "PUT",
        "/api/v1/ambiances/x",
        422,
        r#"{"error":"action 0: nope"}"#,
    );
    let m = mount("/ambiances/new", |h| seed(h, vec![]));
    settle().await;
    set_value(&one(&m.host, ".ambiance-name"), "X", "input");
    settle().await;
    one(&m.host, ".ambiance-save").click();
    settle().await;
    assert!(m.host.query_selector(".ambiance-editor").unwrap().is_some());
    assert!(m.house.ambiances.get_untracked().is_empty());
    assert!(!disabled(&one(&m.host, ".ambiance-save")), "ready again");
}

#[wasm_bindgen_test]
async fn category_count_follows_area_and_except() {
    let _stub = FetchStub::install();
    let m = mount("/ambiances/mood_a", |h| {
        h.areas
            .set(vec![area("salon", "Salon"), area("cuisine", "Cuisine")]);
        h.entities.set(
            [("l1", "salon"), ("l2", "salon"), ("l3", "cuisine")]
                .into_iter()
                .map(|(id, a)| (id.to_owned(), light(id, a)))
                .collect(),
        );
        seed(h, vec![view(mood("mood_a", 1, vec![lights_off()]))]);
    });
    settle().await;
    assert_eq!(text(&m.host, ".ambiance-count__n"), "3 devices");
    set_value(&one(&m.host, ".ambiance-where"), "salon", "change");
    settle().await;
    assert_eq!(text(&m.host, ".ambiance-count__n"), "2 devices");
    one(&m.host, ".ambiance-count__toggle").click();
    settle().await;
    let boxes = all(&m.host, ".ambiance-member input");
    assert_eq!(boxes.len(), 2);
    boxes[0].click();
    settle().await;
    assert_eq!(text(&m.host, ".ambiance-count__n"), "1 device");
    // Ticking it again brings it back.
    all(&m.host, ".ambiance-member input")[0].click();
    settle().await;
    assert_eq!(text(&m.host, ".ambiance-count__n"), "2 devices");
}

#[wasm_bindgen_test]
async fn brightness_field_only_for_lights_on() {
    let _stub = FetchStub::install();
    let m = mount("/ambiances/mood_a", |h| {
        seed(h, vec![view(mood("mood_a", 1, vec![lights_off()]))])
    });
    settle().await;
    assert!(all(&m.host, ".ambiance-brightness").is_empty());
    set_value(&one(&m.host, ".ambiance-to"), "on", "change");
    settle().await;
    assert_eq!(all(&m.host, ".ambiance-brightness").len(), 1);
    set_value(&one(&m.host, ".ambiance-what"), "plugs", "change");
    settle().await;
    assert!(all(&m.host, ".ambiance-brightness").is_empty());
}

#[wasm_bindgen_test]
async fn heating_type_hidden_when_climate_off() {
    let stub = FetchStub::install();
    stub.reply(
        "GET",
        "/api/v1/climate",
        503,
        r#"{"error":"Climate is disabled"}"#,
    );
    let m = mount("/ambiances/new", |h| seed(h, vec![]));
    settle().await;
    one(&m.host, ".ambiance-add__toggle").click();
    settle().await;
    let types: Vec<_> = all(&m.host, ".ambiance-add__type")
        .iter()
        .map(|b| b.text_content().unwrap())
        .collect();
    assert_eq!(
        types,
        ["Every light, plug or shutter", "One device", "Automation"]
    );
}

#[wasm_bindgen_test]
async fn heating_type_listed_when_climate_on() {
    let stub = FetchStub::install();
    stub.reply("GET", "/api/v1/climate", 200, &climate_json(true));
    let m = mount("/ambiances/new", |h| seed(h, vec![]));
    settle().await;
    one(&m.host, ".ambiance-add__toggle").click();
    settle().await;
    let types: Vec<_> = all(&m.host, ".ambiance-add__type")
        .iter()
        .map(|b| b.text_content().unwrap())
        .collect();
    assert!(types.contains(&"Heating".to_owned()), "{types:?}");
    // Picking one adds a row.
    all(&m.host, ".ambiance-add__type")[0].click();
    settle().await;
    assert_eq!(all(&m.host, ".ambiance-action").len(), 1);
}

fn gone_device() -> AmbianceAction {
    AmbianceAction::Device(DeviceAction {
        entity: "gone".into(),
        command: Command::TurnOff,
    })
}

#[wasm_bindgen_test]
async fn missing_entity_row_is_marked_and_kept_on_save() {
    let stub = FetchStub::install();
    let saved = mood("mood_a", 1, vec![gone_device()]);
    stub.reply(
        "PUT",
        "/api/v1/ambiances/mood_a",
        200,
        &serde_json::to_string(&view(saved.clone())).unwrap(),
    );
    let m = mount("/ambiances/mood_a", |h| seed(h, vec![view(saved.clone())]));
    settle().await;
    let rows = all(&m.host, ".ambiance-action--missing");
    assert_eq!(rows.len(), 1);
    assert!(rows[0].text_content().unwrap().contains("(missing)"));
    one(&m.host, ".ambiance-save").click();
    settle().await;
    assert_eq!(put_body(&stub)["actions"][0]["entity"], "gone");
}

#[wasm_bindgen_test]
async fn try_it_disabled_while_dirty() {
    let stub = FetchStub::install();
    stub.hang("POST", "/api/v1/ambiances/mood_a/apply");
    let m = mount("/ambiances/mood_a", |h| {
        seed(h, vec![view(mood("mood_a", 1, vec![]))])
    });
    settle().await;
    let try_it = one(&m.host, ".ambiance-try");
    assert!(!disabled(&try_it), "a saved, untouched form can be tried");
    set_value(&one(&m.host, ".ambiance-name"), "Changed", "input");
    settle().await;
    assert!(disabled(&try_it));
    assert_eq!(try_it.get_attribute("title").as_deref(), Some("Save first"));
    set_value(&one(&m.host, ".ambiance-name"), "Mood mood_a", "input");
    settle().await;
    assert!(!disabled(&try_it), "back to the saved version");
    // While it runs it is busy and disabled.
    try_it.click();
    settle().await;
    assert_eq!(try_it.get_attribute("aria-busy").as_deref(), Some("true"));
    assert!(disabled(&try_it));
}

#[wasm_bindgen_test]
async fn try_it_disabled_on_a_new_mood() {
    let _stub = FetchStub::install();
    let m = mount("/ambiances/new", |h| seed(h, vec![]));
    settle().await;
    let try_it = one(&m.host, ".ambiance-try");
    assert!(disabled(&try_it));
    assert_eq!(try_it.get_attribute("title").as_deref(), Some("Save first"));
    assert!(
        all(&m.host, ".ambiance-delete").is_empty(),
        "nothing to delete yet"
    );
}

#[wasm_bindgen_test]
async fn unknown_fields_survive_edit() {
    let stub = FetchStub::install();
    let mut stored = mood("mood_a", 7, vec![lights_off()]);
    stored
        .extra
        .insert("tint".into(), serde_json::json!({"hue": 40}));
    stub.reply(
        "PUT",
        "/api/v1/ambiances/mood_a",
        200,
        &serde_json::to_string(&view(stored.clone())).unwrap(),
    );
    let m = mount("/ambiances/mood_a", |h| seed(h, vec![view(stored.clone())]));
    settle().await;
    set_value(&one(&m.host, ".ambiance-name"), "Renamed", "input");
    settle().await;
    one(&m.host, ".ambiance-save").click();
    settle().await;
    let body = put_body(&stub);
    assert_eq!(body["name"], "Renamed");
    assert_eq!(body["id"], "mood_a");
    assert_eq!(body["order"], 7);
    assert_eq!(body["tint"], serde_json::json!({"hue": 40}));
}
