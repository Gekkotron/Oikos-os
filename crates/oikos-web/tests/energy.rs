#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::Router;
use oikos_api::{EntityView, WsEvent};
use oikos_core::model::{Capabilities, EntityCategory, EntityState, Kind, StateValue, UiOverride};
use oikos_web::house::House;
use oikos_web::screens::energy::EnergyScreen;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Element, Event, HtmlElement};
wasm_bindgen_test_configure!(run_in_browser);

fn sensor(id: &str, name: &str, unit: &str, value: f64, ui: UiOverride) -> EntityView {
    EntityView {
        id: id.into(),
        device_id: None,
        name: name.into(),
        device_name: None,
        label: None,
        kind: Kind::Sensor,
        protocol_kind: Kind::Sensor,
        device_class: None,
        unit: Some(unit.into()),
        icon: None,
        category: EntityCategory::Primary,
        enabled_by_default: true,
        area: "salon".into(),
        part_of: None,
        capabilities: Capabilities::default(),
        ui,
        state: EntityState {
            value: StateValue::Number(value),
            ..Default::default()
        },
    }
}

fn role(role: &str) -> UiOverride {
    UiOverride {
        energy_role: Some(role.into()),
        ..Default::default()
    }
}

fn mount(entities: Vec<EntityView>) -> Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let house = House::new();
    for e in entities {
        house.apply(WsEvent::EntityUpserted(e));
    }
    std::mem::forget(leptos::mount::mount_to(
        host.clone().unchecked_into(),
        move || {
            provide_context(house);
            view! {
                <Router>
                    <EnergyScreen />
                </Router>
            }
        },
    ));
    host
}

fn text(host: &Element, selector: &str) -> String {
    host.query_selector(selector)
        .unwrap()
        .unwrap_or_else(|| panic!("no {selector}"))
        .text_content()
        .unwrap_or_default()
}

fn count(host: &Element, selector: &str) -> u32 {
    host.query_selector_all(selector).unwrap().length()
}

fn node_with(host: &Element, needle: &str) -> Element {
    let nodes = host.query_selector_all(".energy-node").unwrap();
    (0..nodes.length())
        .filter_map(|i| nodes.item(i))
        .map(|n| n.unchecked_into::<Element>())
        .find(|n| n.text_content().unwrap_or_default().contains(needle))
        .unwrap_or_else(|| panic!("no node with {needle}"))
}

#[wasm_bindgen_test]
async fn live_house_shows_nodes_and_cards() {
    let mut grid = sensor("grid", "Grid power", "W", 253.3, role("grid"));
    grid.ui.energy_meter = Some("grid_energy".into());
    let host = mount(vec![
        grid,
        sensor(
            "grid_energy",
            "Grid energy",
            "kWh",
            6584.04,
            UiOverride::default(),
        ),
        sensor("solar", "Solar power", "W", 3.0, role("solar")),
        sensor("heater", "Water heater", "W", 120.0, role("consumer")),
        sensor("ac", "AC", "W", 60.0, role("consumer")),
        sensor("tank", "Tank", "L", 42.0, role("water")),
    ]);
    tick().await;

    assert_eq!(text(&host, ".energy__subtitle"), "Live");
    // solar, grid, the derived house, two consumers and water
    assert_eq!(count(&host, ".energy-node"), 6);
    assert_eq!(count(&host, ".energy-node--water"), 1);
    assert_eq!(count(&host, ".energy-edge"), 4);
    let aria = host
        .query_selector(".energy-diagram__svg")
        .unwrap()
        .unwrap()
        .get_attribute("aria-label")
        .unwrap();
    assert!(
        aria.contains("Importing 253 watts from the grid."),
        "{aria}"
    );

    let cards = host.query_selector_all(".energy-card").unwrap();
    let grid_card = (0..cards.length())
        .filter_map(|i| cards.item(i))
        .map(|c| c.unchecked_into::<Element>())
        .find(|c| text(c, ".card__name") == "Grid")
        .expect("grid card");
    let body = grid_card.text_content().unwrap();
    assert!(body.contains("253 W"), "{body}");
    assert!(body.contains("6\u{2009}584 kWh"), "{body}");
}

#[wasm_bindgen_test]
async fn four_consumers_fold_into_more() {
    let mut entities = vec![sensor("solar", "Solar power", "W", 500.0, role("solar"))];
    for (i, name) in ["Oven", "Dryer", "Fridge", "Washer"].iter().enumerate() {
        entities.push(sensor(
            &format!("c{i}"),
            name,
            "W",
            400.0 - 100.0 * i as f64,
            role("consumer"),
        ));
    }
    let host = mount(entities);
    tick().await;

    assert_eq!(count(&host, ".energy-more li"), 0);
    let more = node_with(&host, "+2");
    more.dispatch_event(&Event::new("click").unwrap()).unwrap();
    tick().await;
    assert_eq!(count(&host, ".energy-more li"), 4);
    let list = text(&host, ".energy-more");
    for name in ["Oven", "Dryer", "Fridge", "Washer"] {
        assert!(list.contains(name), "{list}");
    }

    let toggle: HtmlElement = host
        .query_selector(".energy__more-toggle")
        .unwrap()
        .expect("toggle")
        .unchecked_into();
    toggle.click();
    tick().await;
    assert_eq!(count(&host, ".energy-more li"), 0);
}

#[wasm_bindgen_test]
async fn no_roles_shows_empty_state() {
    let host = mount(vec![sensor(
        "plain",
        "Plain",
        "W",
        5.0,
        UiOverride::default(),
    )]);
    tick().await;
    assert!(text(&host, ".energy__empty").contains("energy role"));
    assert_eq!(count(&host, ".energy-diagram"), 0);
}
