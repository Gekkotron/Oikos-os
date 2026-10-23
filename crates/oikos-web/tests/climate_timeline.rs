#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use oikos_climate::doc::{Block, Day, HhMm, ModeRules, Setpoint};
use oikos_web::climate::timeline::WeekTimeline;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
use web_sys::{Element, HtmlElement, PointerEvent, PointerEventInit};
wasm_bindgen_test_configure!(run_in_browser);

fn block(days: &[Day], from: &str, to: &str, t: f64) -> Block {
    Block {
        days: days.to_vec(),
        from: HhMm::parse(from).unwrap(),
        to: HhMm::parse(to).unwrap(),
        setpoint: Setpoint::Temp(t),
        evidence: None,
    }
}

async fn mount(rules: RwSignal<ModeRules>) -> Element {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    host.set_attribute("style", "width: 700px").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let mounted = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        view! {
            <WeekTimeline
                rules=rules.into()
                editable=true
                on_change=Callback::new(move |next| rules.set(next))
            />
        }
    });
    // Keep the view alive for the rest of the test.
    std::mem::forget(mounted);
    tick().await;
    host
}

/// The day track of `day` (rows go Mon to Sun).
fn track(host: &Element, day: Day) -> Element {
    let index = Day::ALL.iter().position(|d| *d == day).unwrap() as u32;
    host.query_selector_all(".climate-timeline__track")
        .unwrap()
        .item(index)
        .unwrap()
        .unchecked_into()
}

/// Clicks `target` at `fraction` of its width (a browser click is a `PointerEvent`).
fn click_at(target: &Element, fraction: f64) {
    let rect = target.get_bounding_client_rect();
    let init = PointerEventInit::new();
    init.set_bubbles(true);
    init.set_client_x((rect.left() + rect.width() * fraction) as i32);
    init.set_client_y((rect.top() + 1.0) as i32);
    let event = PointerEvent::new_with_event_init_dict("click", &init).unwrap();
    target.dispatch_event(&event).unwrap();
}

fn chip(host: &Element, label: &str) -> HtmlElement {
    let chips = host
        .query_selector_all(".climate-timeline__days .chip")
        .unwrap();
    (0..chips.length())
        .filter_map(|i| chips.item(i))
        .map(|n| n.unchecked_into::<HtmlElement>())
        .find(|c| c.text_content().as_deref() == Some(label))
        .expect("day chip")
}

#[wasm_bindgen_test]
async fn tapping_an_empty_spot_adds_a_block_on_that_day() {
    let rules = RwSignal::new(ModeRules {
        base: Setpoint::Temp(15.5),
        blocks: vec![block(&[Day::Sat], "00:00", "07:30", 18.0)],
    });
    let host = mount(rules).await;

    click_at(&track(&host, Day::Sun), 0.75);
    tick().await;
    let added = rules
        .with_untracked(|r| r.blocks.get(1).cloned())
        .expect("new block");
    assert_eq!(added.days, vec![Day::Sun]);
    assert_eq!((added.from, added.to), (HhMm(18 * 60), HhMm(20 * 60)));
    assert_eq!(added.setpoint, Setpoint::Temp(17.5));
    assert!(
        host.query_selector(".climate-timeline__editor")
            .unwrap()
            .is_some(),
        "the new block's popover opens"
    );

    // A click on a bar opens it instead of adding a block.
    let bar = track(&host, Day::Sat)
        .query_selector(".climate-timeline__bar")
        .unwrap()
        .expect("Saturday bar");
    click_at(&bar, 0.5);
    tick().await;
    assert_eq!(rules.with_untracked(|r| r.blocks.len()), 2);
}

#[wasm_bindgen_test]
async fn day_chips_extend_a_block_to_another_day() {
    let rules = RwSignal::new(ModeRules {
        base: Setpoint::Temp(15.5),
        blocks: vec![
            block(
                &[Day::Mon, Day::Tue, Day::Wed, Day::Thu, Day::Fri],
                "00:00",
                "07:30",
                18.0,
            ),
            block(&[Day::Sat], "00:00", "07:30", 18.0),
        ],
    });
    let host = mount(rules).await;

    let bar: HtmlElement = track(&host, Day::Sat)
        .query_selector(".climate-timeline__bar")
        .unwrap()
        .expect("Saturday bar")
        .unchecked_into();
    bar.click();
    tick().await;

    // Saturday is the block's only day; Monday holds the weekday block at the same time.
    assert!(chip(&host, "Sat").has_attribute("disabled"));
    assert!(chip(&host, "Mon").has_attribute("disabled"));
    let sunday = chip(&host, "Sun");
    assert_eq!(
        sunday.get_attribute("aria-pressed").as_deref(),
        Some("false")
    );
    sunday.click();
    tick().await;

    assert_eq!(
        rules.with_untracked(|r| r.blocks[1].days.clone()),
        vec![Day::Sat, Day::Sun]
    );
    assert_eq!(
        chip(&host, "Sun").get_attribute("aria-pressed").as_deref(),
        Some("true")
    );
    assert!(
        track(&host, Day::Sun)
            .query_selector(".climate-timeline__bar")
            .unwrap()
            .is_some(),
        "Sunday shows the block"
    );
}
