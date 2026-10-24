#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use oikos_climate::doc::{
    Block, Day, Decision, HhMm, Mode, ModeRules, Rules, Setpoint, Source, ZoneSettings, ZoneState,
};
use oikos_climate::view::{LearnStatus, ZoneView};
use oikos_web::climate::proposal::RulesPanel;
use oikos_web::climate::room_list::RoomList;
use oikos_web::components::toast::Toasts;
use std::sync::{Arc, Mutex};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Element, HtmlElement, HtmlInputElement};
wasm_bindgen_test_configure!(run_in_browser);

fn zone(id: &str, name: &str, state: ZoneState, setpoint: Option<f64>) -> ZoneView {
    ZoneView {
        zone: id.into(),
        name: name.into(),
        orphan: false,
        settings: ZoneSettings {
            state,
            ..ZoneSettings::default()
        },
        settings_invalid: None,
        entities: vec![],
        reading: Some(19.0),
        reading_age_s: Some(30),
        room_sensor: None,
        decision: setpoint.map(|t| Decision {
            zone: id.into(),
            mode: Some(Mode::Heat),
            setpoint: Some(Setpoint::Temp(t)),
            source: Source::Schedule,
            cause: None,
            reason: String::new(),
            reading: None,
            reading_age_s: None,
            outdoor_mean: None,
            home: true,
            applied: true,
            at: 0,
            entities: Default::default(),
        }),
        overrides: vec![],
        proposal: None,
        learn: LearnStatus::Off,
    }
}

fn host() -> Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    host.set_attribute("style", "width: 900px").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    host
}

fn button(host: &Element, name: &str) -> HtmlElement {
    let all = host.query_selector_all(".climate-room").unwrap();
    (0..all.length())
        .filter_map(|i| all.item(i))
        .map(|n| n.unchecked_into::<HtmlElement>())
        .find(|b| {
            b.query_selector(".climate-room__name")
                .unwrap()
                .and_then(|n| n.text_content())
                .as_deref()
                == Some(name)
        })
        .expect("room button")
}

#[wasm_bindgen_test]
async fn room_list_marks_the_selected_room_and_selects_on_click() {
    let zones = vec![
        zone("living-room", "Living room", ZoneState::Pending, None),
        zone("bedroom-1", "Bedroom 1", ZoneState::Active, Some(18.0)),
        zone("bedroom-2", "Bedroom 2", ZoneState::Active, Some(21.0)),
    ];
    let picked = Arc::new(Mutex::new(None::<String>));
    let sink = picked.clone();
    let host = host();
    let mounted = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        let sink = sink.clone();
        view! {
            <RoomList
                zones=Signal::from(zones.clone())
                selected=Signal::from(Some("bedroom-1".to_owned()))
                on_select=Callback::new(move |id: String| *sink.lock().unwrap() = Some(id))
            />
        }
    });
    std::mem::forget(mounted);
    tick().await;

    let bedroom = button(&host, "Bedroom 1");
    assert_eq!(
        bedroom.get_attribute("aria-current").as_deref(),
        Some("true")
    );
    assert!(
        button(&host, "Living room")
            .get_attribute("aria-current")
            .is_none()
    );
    let sub = bedroom
        .query_selector(".climate-room__sub")
        .unwrap()
        .and_then(|n| n.text_content());
    assert_eq!(sub.as_deref(), Some("Heat 18° · Active"));
    let dot = |name: &str| {
        button(&host, name)
            .query_selector(".climate-room__dot")
            .unwrap()
            .expect("dot")
            .class_list()
    };
    assert!(dot("Bedroom 1").contains("climate-temp--mid"));
    assert!(dot("Living room").contains("climate-temp--none"));

    button(&host, "Bedroom 2").click();
    tick().await;
    assert_eq!(picked.lock().unwrap().as_deref(), Some("bedroom-2"));
}

#[wasm_bindgen_test]
async fn refetch_keeps_the_draft_and_the_open_editor() {
    let mut start = zone("living-room", "Living room", ZoneState::Active, Some(16.0));
    start.settings.rules = Rules {
        heat: Some(ModeRules {
            base: Setpoint::Temp(15.0),
            blocks: vec![Block {
                days: vec![Day::Mon],
                from: HhMm::parse("07:00").unwrap(),
                to: HhMm::parse("09:00").unwrap(),
                setpoint: Setpoint::Temp(19.0),
                evidence: None,
            }],
        }),
        cool: None,
    };
    let current = RwSignal::new(start.clone());
    let host = host();
    let mounted = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(Toasts::new());
        view! {
            <RulesPanel
                zone=current.into()
                on_changed=Callback::new(|_| {})
                now=Signal::from((Day::Mon, 0))
            />
        }
    });
    std::mem::forget(mounted);
    tick().await;

    let find = |label: &str| {
        let all = host.query_selector_all("button").unwrap();
        (0..all.length())
            .filter_map(|i| all.item(i))
            .map(|n| n.unchecked_into::<HtmlElement>())
            .find(|b| b.text_content().as_deref() == Some(label))
    };
    find("Edit rules").expect("Edit rules").click();
    tick().await;
    let bar: HtmlElement = host
        .query_selector(".climate-timeline__bar")
        .unwrap()
        .expect("a bar")
        .unchecked_into();
    bar.click();
    tick().await;
    assert!(
        host.query_selector(".climate-timeline__editor")
            .unwrap()
            .is_some()
    );

    // Edit the draft: the block goes to 21 °C.
    let temp: HtmlInputElement = host
        .query_selector(".climate-timeline__editor input[type=number]")
        .unwrap()
        .expect("temperature field")
        .unchecked_into();
    temp.set_value("21");
    temp.dispatch_event(&web_sys::Event::new("change").unwrap())
        .unwrap();
    tick().await;

    // A refetch: a fresh reading, and approved rules changed elsewhere (the block at 17 °C).
    let mut fresh = start;
    fresh.reading = Some(19.4);
    if let Some(heat) = fresh.settings.rules.heat.as_mut() {
        heat.blocks[0].setpoint = Setpoint::Temp(17.0);
    }
    current.set(fresh);
    tick().await;

    let shown = host
        .query_selector(".climate-timeline__bar .climate-timeline__temp")
        .unwrap()
        .and_then(|n| n.text_content());
    assert_eq!(
        shown.as_deref(),
        Some("21°"),
        "the draft wins over the refetch"
    );

    assert!(
        host.query_selector(".climate-timeline__editor")
            .unwrap()
            .is_some(),
        "the block editor stays open"
    );
    assert!(find("Save").is_some(), "still editing");
}
