#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::Router;
use oikos_api::{EntityView, WsEvent};
use oikos_core::model::{Capabilities, EntityCategory, EntityState, Kind, StateValue, UiOverride};
use oikos_web::commands::Pending;
use oikos_web::components::toast::Toasts;
use oikos_web::house::House;
use oikos_web::screens::entity_sheet::SheetBody;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Event, EventInit, HtmlInputElement};
wasm_bindgen_test_configure!(run_in_browser);

fn sensor(value: f64) -> EntityView {
    EntityView {
        id: "sensor.hall".into(),
        device_id: None,
        name: "Hall".into(),
        device_name: None,
        label: None,
        kind: Kind::Sensor,
        protocol_kind: Kind::Sensor,
        component: Kind::Sensor.component().to_string(),
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
            value: StateValue::Number(value),
            ..Default::default()
        },
    }
}

fn type_into(input: &HtmlInputElement, text: &str) {
    input.set_value(text);
    let init = EventInit::new();
    init.set_bubbles(true);
    let event = Event::new_with_event_init_dict("input", &init).unwrap();
    input.dispatch_event(&event).unwrap();
}

#[wasm_bindgen_test]
async fn state_echo_keeps_the_override_draft() {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();

    let house = House::new();
    house.apply(WsEvent::EntityUpserted(sensor(1.0)));
    let _mounted = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(house);
        provide_context(Toasts::new());
        provide_context(Pending::new());
        view! {
            <Router>
                <SheetBody id="sensor.hall".to_owned() />
            </Router>
        }
    });
    tick().await;

    let input: HtmlInputElement = host
        .query_selector(".override input[type=text]")
        .unwrap()
        .expect("name field")
        .unchecked_into();
    type_into(&input, "Hallway");
    tick().await;
    let save = host
        .query_selector(".override__save")
        .unwrap()
        .expect("save button");
    assert!(
        !save.has_attribute("disabled"),
        "the edit makes the form dirty"
    );

    // A save the test server cannot take fails and leaves an error on the form.
    save.clone()
        .unchecked_into::<web_sys::HtmlElement>()
        .click();
    gloo_timers::future::TimeoutFuture::new(500).await;
    let error = host
        .query_selector(".override__error")
        .unwrap()
        .expect("error line");
    let message = error.text_content().unwrap_or_default();
    assert!(!message.is_empty(), "the failed save shows an error");

    let echo = EntityState {
        value: StateValue::Number(2.0),
        ..Default::default()
    };
    house.apply(WsEvent::StateChanged {
        id: "sensor.hall".into(),
        state: echo,
    });
    tick().await;

    assert!(input.is_connected(), "the editor was not rebuilt");
    assert_eq!(input.value(), "Hallway");
    assert!(!save.has_attribute("disabled"), "the draft is still dirty");
    let rebuilt: HtmlInputElement = host
        .query_selector(".override input[type=text]")
        .unwrap()
        .unwrap()
        .unchecked_into();
    assert_eq!(error.text_content().unwrap_or_default(), message);
    assert!(input.is_same_node(Some(&rebuilt)));
}
