#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use oikos_climate::doc::{
    Block, Day, Decision, HhMm, Mode, ModeRules, Rules, Setpoint, Source, ZoneSettings, ZoneState,
};
use oikos_climate::view::{LearnStatus, ZoneView};
use oikos_web::climate::proposal::RulesPanel;
use oikos_web::climate::room_list::RoomList;
use oikos_web::climate::room_panel::RoomPanel;
use oikos_web::components::toast::Toasts;
use oikos_web::house::House;
use oikos_web::i18n::Lang;
use std::sync::{Arc, Mutex};
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Element, HtmlElement, HtmlInputElement};
wasm_bindgen_test_configure!(run_in_browser);

#[path = "support/fetch_stub.rs"]
mod fetch_stub;
use fetch_stub::FetchStub;

async fn settle() {
    tick().await;
    gloo_timers::future::TimeoutFuture::new(50).await;
    tick().await;
}

/// Tests load no stylesheet: the columns need a height for the vertical pointer mapping.
fn install_layout() {
    let document = web_sys::window().unwrap().document().unwrap();
    if document.get_element_by_id("rooms-test-layout").is_some() {
        return;
    }
    let style = document.create_element("style").unwrap();
    style.set_id("rooms-test-layout");
    style.set_text_content(Some(
        ".climate-timeline__col { position: relative; height: 480px; } \
         .climate-timeline__bar { position: absolute; left: 0; right: 0; }",
    ));
    document.head().unwrap().append_child(&style).unwrap();
}

fn find_button(host: &Element, label: &str) -> Option<HtmlElement> {
    let all = host.query_selector_all("button").unwrap();
    (0..all.length())
        .filter_map(|i| all.item(i))
        .map(|n| n.unchecked_into::<HtmlElement>())
        .find(|b| b.text_content().as_deref() == Some(label))
}

async fn mount_rules(zone: ZoneView) -> Element {
    install_layout();
    let current = RwSignal::new(zone);
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
    host
}

#[wasm_bindgen_test]
async fn a_room_without_rules_can_create_a_schedule() {
    let stub = FetchStub::install();
    stub.reply("PUT", "/api/v1/climate/zones/bedroom-2", 200, "{}");
    let host = mount_rules(zone("bedroom-2", "Bedroom 2", ZoneState::Pending, None)).await;
    assert!(
        host.text_content()
            .unwrap()
            .contains("No approved schedule yet")
    );

    find_button(&host, "Create schedule")
        .expect("Create schedule")
        .click();
    settle().await;
    assert_eq!(
        host.query_selector_all(".climate-timeline__col")
            .unwrap()
            .length(),
        7
    );
    let base: HtmlInputElement = host
        .query_selector(".climate-timeline__base-input")
        .unwrap()
        .expect("base field")
        .unchecked_into();
    assert_eq!(base.value(), "16");
    base.set_value("15.5");
    base.dispatch_event(&web_sys::Event::new("change").unwrap())
        .unwrap();
    tick().await;

    // Tap Monday just after 18:00 (blocks snap down to the quarter): two hours at base + 2.
    let monday = host
        .query_selector(".climate-timeline__col")
        .unwrap()
        .unwrap();
    let rect = monday.get_bounding_client_rect();
    let init = web_sys::PointerEventInit::new();
    init.set_bubbles(true);
    init.set_client_x((rect.left() + rect.width() / 2.0) as i32);
    init.set_client_y((rect.top() + rect.height() * 0.76) as i32);
    monday
        .dispatch_event(&web_sys::PointerEvent::new_with_event_init_dict("click", &init).unwrap())
        .unwrap();
    tick().await;

    find_button(&host, "Save").expect("Save").click();
    settle().await;
    let puts: Vec<_> = stub
        .requests()
        .into_iter()
        .filter(|r| r.method == "PUT")
        .collect();
    assert_eq!(puts.len(), 1, "{puts:?}");
    assert_eq!(puts[0].url, "/api/v1/climate/zones/bedroom-2");
    let body = &puts[0].body;
    assert!(body.contains("\"base\":15.5"), "{body}");
    assert!(body.contains("\"setpoint\":17.5"), "{body}");
    assert!(body.contains("18:00") && body.contains("20:00"), "{body}");
}

#[wasm_bindgen_test]
async fn cancelling_a_new_schedule_goes_back_to_none() {
    let host = mount_rules(zone("bedroom-3", "Bedroom 3", ZoneState::Pending, None)).await;
    find_button(&host, "Create schedule")
        .expect("Create schedule")
        .click();
    tick().await;
    find_button(&host, "Cancel").expect("Cancel").click();
    tick().await;
    assert!(
        host.text_content()
            .unwrap()
            .contains("No approved schedule yet")
    );
    assert!(
        host.query_selector(".climate-timeline__col")
            .unwrap()
            .is_none()
    );
}

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
            eco_restore: Default::default(),
        }),
        overrides: vec![],
        eco: None,
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
        provide_context(oikos_web::components::now::install_now());
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
    find("Edit schedule").expect("Edit schedule").click();
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
        .query_selector(".climate-timeline__editor input[inputmode=decimal]")
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

/// The room list and panel mounted with a language the test flips.
fn mount_room(zone: ZoneView, lang: RwSignal<Lang>) -> Element {
    let host = host();
    let zones = vec![zone.clone()];
    let current = RwSignal::new(zone);
    let mounted = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(Toasts::new());
        provide_context(House::new());
        provide_context(oikos_web::components::now::install_now());
        provide_context(lang);
        view! {
            <RoomList
                zones=Signal::from(zones.clone())
                selected=Signal::from(Some("bedroom-1".to_owned()))
                on_select=Callback::new(|_: String| {})
            />
            <RoomPanel
                zone=current.into()
                tz=Signal::from(chrono_tz::Tz::UTC)
                on_changed=Callback::new(|_| {})
            />
        }
    });
    std::mem::forget(mounted);
    host
}

#[wasm_bindgen_test]
async fn the_room_follows_the_language_without_a_remount() {
    let lang = RwSignal::new(Lang::Fr);
    let host = mount_room(
        zone("bedroom-1", "Bedroom 1", ZoneState::Active, Some(18.5)),
        lang,
    );
    tick().await;
    let text = || host.text_content().unwrap_or_default();
    for french in [
        "Consigne",
        "Maintenant",
        "Chauffage 18,5°",
        "Programmation",
        "Aucune programmation approuvée pour l'instant",
        "Cette semaine",
        "Apprendre maintenant",
        "Décisions récentes",
        "Active",
        "Arrêt",
        "En attente",
    ] {
        assert!(text().contains(french), "{french} in {}", text());
    }
    for english in [
        "Target",
        "Now",
        "Heat 18",
        "This week",
        "Learn now",
        "Recent decisions",
        "Pending",
    ] {
        assert!(!text().contains(english), "{english} in {}", text());
    }
    let hold: HtmlInputElement = host
        .query_selector(".climate-hold__setpoint")
        .unwrap()
        .expect("hold field")
        .unchecked_into();
    assert_eq!(
        hold.get_attribute("aria-label").as_deref(),
        Some("Température à maintenir")
    );

    lang.set(Lang::En);
    tick().await;
    for english in [
        "Target",
        "Now",
        "Heat 18.5",
        "This week",
        "Learn now",
        "Recent decisions",
        "Pending",
    ] {
        assert!(text().contains(english), "{english} in {}", text());
    }
    for french in ["Consigne", "Maintenant", "Chauffage", "Cette semaine"] {
        assert!(!text().contains(french), "{french} in {}", text());
    }
    assert_eq!(
        hold.get_attribute("aria-label").as_deref(),
        Some("Hold temperature")
    );
}
