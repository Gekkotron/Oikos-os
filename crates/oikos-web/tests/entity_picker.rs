#![cfg(target_arch = "wasm32")]

use std::cell::Cell;
use std::rc::Rc;

use gloo_timers::future::TimeoutFuture;
use leptos::prelude::*;
use leptos::task::tick;
use oikos_api::{AreaView, EntityView, WsEvent};
use oikos_core::model::{Capabilities, EntityCategory, EntityState, Kind, StateValue, UiOverride};
use oikos_web::components::now::Now;
use oikos_web::entity_picker::Suggest;
use oikos_web::entity_picker::needs::{is_thermometer, keep};
use oikos_web::entity_picker::view::{EntityPicker, Mode, Pinned};
use oikos_web::house::House;
use oikos_web::i18n::Lang;
use oikos_web::i18n::picker::PickerText;
use wasm_bindgen::JsCast;
use wasm_bindgen::prelude::*;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

fn entity(id: &str, name: &str, device: &str, kind: Kind, area: &str) -> EntityView {
    EntityView {
        id: id.into(),
        device_id: None,
        name: name.into(),
        device_name: (!device.is_empty()).then(|| device.to_owned()),
        label: None,
        kind,
        component: kind.component().to_string(),
        protocol_kind: kind,
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

fn probe(value: f64) -> EntityView {
    let mut e = entity("t", "Temp", "Probe", Kind::Sensor, "hall");
    e.device_class = Some("temperature".into());
    e.unit = Some("°C".into());
    e.state.value = StateValue::Number(value);
    e.state.last_update = Some(1_700_000_000);
    e
}

fn area(id: &str, name: &str) -> AreaView {
    AreaView {
        id: id.into(),
        name: name.into(),
        icon: None,
        order: None,
        entity_count: 1,
        primary_count: 1,
        group_names: Default::default(),
        group_favorites: Default::default(),
    }
}

struct Fx {
    host: web_sys::Element,
    open: RwSignal<bool>,
    picked: RwSignal<Vec<Option<String>>>,
    house: House,
}

fn mount(mode: Mode, current: Vec<&str>, suggest: bool, pinned: bool) -> Fx {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let house = House::new();
    for e in [
        entity("a", "Mode", "Plug A", Kind::Switch, "kitchen"),
        entity("b", "Mode", "Plug B", Kind::Switch, "kitchen"),
        probe(20.0),
    ] {
        house.apply(WsEvent::EntityUpserted(e));
    }
    house.apply(WsEvent::AreasChanged {
        areas: vec![area("kitchen", "Kitchen"), area("hall", "Hall")],
    });
    let open = RwSignal::new(true);
    let picked = RwSignal::new(Vec::new());
    let current: Vec<String> = current.into_iter().map(str::to_owned).collect();
    let mounted = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(house);
        provide_context(RwSignal::new(Lang::En));
        provide_context(Now(RwSignal::new(1_700_000_060)));
        let suggest = suggest.then(|| Suggest {
            text: PickerText::Thermometers,
            keep: keep(is_thermometer),
        });
        let pinned = pinned.then(|| Pinned {
            text: Signal::derive(|| "Nothing".to_string()),
        });
        view! {
            <EntityPicker
                open=open
                title="Pick"
                mode=mode
                current=Signal::derive(move || current.clone())
                suggest=suggest
                pinned=pinned
                on_pick=Callback::new(move |id| picked.update(|p| p.push(id)))
            />
        }
    });
    mounted.forget();
    Fx {
        host,
        open,
        picked,
        house,
    }
}

impl Fx {
    fn dialog(&self) -> web_sys::HtmlDialogElement {
        self.host
            .query_selector("dialog.picker")
            .unwrap()
            .unwrap()
            .unchecked_into()
    }

    fn options(&self) -> Vec<web_sys::Element> {
        let list = self.host.query_selector_all("[role=option]").unwrap();
        (0..list.length())
            .map(|i| list.item(i).unwrap().unchecked_into())
            .collect()
    }

    fn option(&self, id: &str) -> web_sys::HtmlElement {
        self.host
            .query_selector(&format!("[id='picker-opt-{id}']"))
            .unwrap()
            .unwrap_or_else(|| panic!("no row {id}"))
            .unchecked_into()
    }

    fn pinned(&self) -> web_sys::HtmlElement {
        self.host
            .query_selector("#picker-pin")
            .unwrap()
            .expect("pinned row")
            .unchecked_into()
    }

    fn search(&self) -> web_sys::HtmlInputElement {
        self.host
            .query_selector("input[type=search]")
            .unwrap()
            .unwrap()
            .unchecked_into()
    }

    async fn type_query(&self, q: &str) {
        let input = self.search();
        input.set_value(q);
        let ev = web_sys::Event::new_with_event_init_dict("input", &{
            let i = web_sys::EventInit::new();
            i.set_bubbles(true);
            i
        })
        .unwrap();
        input.dispatch_event(&ev).unwrap();
        tick().await;
    }

    async fn key(&self, key: &str) {
        let init = web_sys::KeyboardEventInit::new();
        init.set_key(key);
        init.set_bubbles(true);
        init.set_cancelable(true);
        let ev =
            web_sys::KeyboardEvent::new_with_keyboard_event_init_dict("keydown", &init).unwrap();
        self.search().dispatch_event(&ev).unwrap();
        tick().await;
    }

    async fn finish(self) {
        self.open.set(false);
        tick().await;
        TimeoutFuture::new(30).await;
    }
}

fn pathname() -> String {
    web_sys::window().unwrap().location().pathname().unwrap()
}

/// Counts every `popstate` from now on.
fn count_popstates() -> Rc<Cell<u32>> {
    let n = Rc::new(Cell::new(0));
    let seen = n.clone();
    let cb = Closure::<dyn FnMut()>::new(move || seen.set(seen.get() + 1));
    web_sys::window()
        .unwrap()
        .add_event_listener_with_callback("popstate", cb.as_ref().unchecked_ref())
        .unwrap();
    cb.forget();
    n
}

#[wasm_bindgen_test]
async fn opens_modal_with_search_focused() {
    let fx = mount(Mode::Single, vec![], false, false);
    tick().await;
    tick().await;
    assert!(fx.dialog().open());
    let active = web_sys::window()
        .unwrap()
        .document()
        .unwrap()
        .active_element();
    assert_eq!(
        active.unwrap(),
        fx.search().unchecked_into::<web_sys::Element>()
    );
    fx.finish().await;
}

#[wasm_bindgen_test]
async fn typing_filters_rows() {
    let fx = mount(Mode::Single, vec![], false, true);
    tick().await;
    assert_eq!(fx.options().len(), 4, "pinned + three rows");
    fx.type_query("plug b").await;
    let rows = fx.options();
    assert_eq!(rows.len(), 2, "pinned + one match");
    assert_eq!(rows[1].id(), "picker-opt-b");
    fx.finish().await;
}

#[wasm_bindgen_test]
async fn single_pick_reports_and_closes() {
    let fx = mount(Mode::Single, vec![], false, false);
    tick().await;
    fx.option("b").click();
    tick().await;
    assert_eq!(fx.picked.get_untracked(), vec![Some("b".to_string())]);
    assert!(!fx.open.get_untracked());
    assert!(!fx.dialog().open());
    fx.finish().await;
}

#[wasm_bindgen_test]
async fn pinned_row_reports_none() {
    let fx = mount(Mode::Single, vec![], false, true);
    tick().await;
    assert!(fx.host.text_content().unwrap().contains("Nothing"));
    fx.pinned().click();
    tick().await;
    assert_eq!(fx.picked.get_untracked(), vec![None]);
    fx.finish().await;
}

#[wasm_bindgen_test]
async fn multi_toggles_and_stays_open() {
    let fx = mount(Mode::Multi, vec!["a", "b"], false, false);
    tick().await;
    assert!(fx.host.text_content().unwrap().contains("Done (2)"));
    assert_eq!(
        fx.option("a").get_attribute("aria-selected").as_deref(),
        Some("true")
    );
    assert_eq!(
        fx.option("t").get_attribute("aria-selected").as_deref(),
        Some("false")
    );
    fx.option("a").click();
    fx.option("t").click();
    tick().await;
    assert_eq!(
        fx.picked.get_untracked(),
        vec![Some("a".to_string()), Some("t".to_string())]
    );
    assert!(fx.open.get_untracked());
    assert!(fx.dialog().open());
    fx.finish().await;
}

#[wasm_bindgen_test]
async fn escape_closes() {
    let fx = mount(Mode::Single, vec![], false, false);
    tick().await;
    let ev = web_sys::Event::new_with_event_init_dict("cancel", &{
        let i = web_sys::EventInit::new();
        i.set_cancelable(true);
        i
    })
    .unwrap();
    fx.dialog().dispatch_event(&ev).unwrap();
    tick().await;
    assert!(!fx.open.get_untracked());
    assert!(!fx.dialog().open());
    fx.finish().await;
}

#[wasm_bindgen_test]
async fn arrows_and_enter_pick() {
    let fx = mount(Mode::Single, vec![], false, false);
    tick().await;
    fx.key("ArrowDown").await;
    fx.key("ArrowDown").await;
    let id = fx.search().get_attribute("aria-activedescendant").unwrap();
    assert_eq!(id, "picker-opt-b");
    fx.key("Enter").await;
    assert_eq!(fx.picked.get_untracked(), vec![Some("b".to_string())]);
    fx.finish().await;
}

#[wasm_bindgen_test]
async fn echo_keeps_highlight() {
    let fx = mount(Mode::Single, vec![], false, false);
    tick().await;
    for _ in 0..3 {
        fx.key("ArrowDown").await;
    }
    let before = fx.search().get_attribute("aria-activedescendant").unwrap();
    assert_eq!(before, "picker-opt-t");
    assert!(fx.option("t").text_content().unwrap().contains("20"));
    fx.house.apply(WsEvent::EntityUpserted(probe(21.5)));
    tick().await;
    let after = fx.search().get_attribute("aria-activedescendant").unwrap();
    assert_eq!(before, after);
    assert!(fx.option("t").text_content().unwrap().contains("21.5"));
    fx.finish().await;
}

#[wasm_bindgen_test]
async fn suggested_chip_then_all() {
    let fx = mount(Mode::Single, vec![], true, false);
    tick().await;
    assert_eq!(fx.options().len(), 1);
    let chips = fx.host.query_selector_all(".chip").unwrap();
    let all: web_sys::HtmlElement = (0..chips.length())
        .map(|i| {
            chips
                .item(i)
                .unwrap()
                .unchecked_into::<web_sys::HtmlElement>()
        })
        .find(|c| c.text_content().unwrap() == "All")
        .expect("All chip");
    all.click();
    tick().await;
    assert_eq!(fx.options().len(), 3);
    fx.finish().await;
}

#[wasm_bindgen_test]
async fn history_back_closes_picker() {
    let path = pathname();
    let fx = mount(Mode::Single, vec![], false, false);
    tick().await;
    let pops = count_popstates();
    let before = pops.get();
    web_sys::window()
        .unwrap()
        .history()
        .unwrap()
        .back()
        .unwrap();
    for _ in 0..40 {
        if pops.get() > before {
            break;
        }
        TimeoutFuture::new(25).await;
    }
    tick().await;
    assert!(!fx.open.get_untracked());
    assert!(!fx.dialog().open());
    assert_eq!(pathname(), path);
    fx.finish().await;
}

#[wasm_bindgen_test]
async fn escape_pops_the_pushed_entry() {
    let path = pathname();
    let fx = mount(Mode::Single, vec![], false, false);
    tick().await;
    let pops = count_popstates();
    let before = pops.get();
    let ev = web_sys::Event::new_with_event_init_dict("cancel", &{
        let i = web_sys::EventInit::new();
        i.set_cancelable(true);
        i
    })
    .unwrap();
    fx.dialog().dispatch_event(&ev).unwrap();
    for _ in 0..40 {
        if pops.get() > before {
            break;
        }
        TimeoutFuture::new(25).await;
    }
    TimeoutFuture::new(100).await;
    assert_eq!(pops.get() - before, 1);
    assert!(!fx.open.get_untracked());
    assert_eq!(pathname(), path);
    fx.finish().await;
}
