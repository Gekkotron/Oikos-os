#![cfg(target_arch = "wasm32")]
//! Layout checks that need the real stylesheet: the climate rules and the screen width.

use leptos::prelude::*;
use leptos::task::tick;
use oikos_climate::doc::{Block, Day, HhMm, ModeRules, Setpoint};
use oikos_web::climate::timeline::WeekTimeline;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::Element;
wasm_bindgen_test_configure!(run_in_browser);

fn install_styles() {
    let document = web_sys::window().unwrap().document().unwrap();
    if document
        .get_element_by_id("climate-layout-styles")
        .is_some()
    {
        return;
    }
    let style = document.create_element("style").unwrap();
    style.set_id("climate-layout-styles");
    style.set_text_content(Some(concat!(
        include_str!("../styles/tokens.css"),
        include_str!("../styles/main.css")
    )));
    document.head().unwrap().append_child(&style).unwrap();
}

fn host(width: u32) -> Element {
    install_styles();
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    host.set_attribute("style", &format!("width: {width}px"))
        .unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    host
}

fn block(from: &str, to: &str, t: f64) -> Block {
    Block {
        days: vec![Day::Mon],
        from: HhMm::parse(from).unwrap(),
        to: HhMm::parse(to).unwrap(),
        setpoint: Setpoint::Temp(t),
        evidence: None,
    }
}

#[wasm_bindgen_test]
async fn a_short_block_is_drawn_to_scale() {
    let rules = RwSignal::new(ModeRules {
        base: Setpoint::Temp(15.0),
        blocks: vec![block("07:00", "07:15", 19.0), block("07:15", "09:00", 18.0)],
    });
    let host = host(900);
    let mounted = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        view! { <WeekTimeline rules=rules.into() editable=false on_change=Callback::new(|_| {}) /> }
    });
    std::mem::forget(mounted);
    tick().await;

    let col = host
        .query_selector(".climate-timeline__col")
        .unwrap()
        .unwrap();
    let short = col
        .query_selector(".climate-timeline__bar")
        .unwrap()
        .unwrap();
    let scale = col.get_bounding_client_rect().height() * 15.0 / 1440.0;
    let drawn = short.get_bounding_client_rect().height();
    // The 4 px floor keeps a tiny block visible; nothing beyond that.
    assert!(
        drawn <= scale.max(4.0) + 0.5,
        "15 min should be {scale:.1} px tall, drawn {drawn:.1} px"
    );
}

#[wasm_bindgen_test]
fn the_header_spans_the_climate_screen() {
    let host = host(1800);
    host.set_inner_html(
        "<div class=\"shell__content\">\
           <header class=\"content-header\"></header>\
           <main class=\"screen\"><div class=\"climate-screen\"></div></main>\
         </div>",
    );
    let width = |selector: &str| {
        host.query_selector(selector)
            .unwrap()
            .unwrap()
            .get_bounding_client_rect()
            .width()
    };
    assert!(width(".screen") > 1300.0, "the screen is not capped");
    assert_eq!(width(".content-header"), width(".screen"));
}
