#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::Router;
use oikos_web::house::House;
use oikos_web::i18n::Lang;
use oikos_web::shell::Shell;
use oikos_web::theme::Theme;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

/// Switching the language on a mounted screen rewrites its text, with no remount.
#[wasm_bindgen_test]
async fn the_sidebar_follows_the_language_without_a_remount() {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let lang = RwSignal::new(Lang::En);
    let _app = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(House::new());
        provide_context(RwSignal::new(Theme::Auto));
        provide_context(lang);
        oikos_web::nav::install_nav();
        view! {
            <Router>
                <Shell
                    climate_on=RwSignal::new(false)
                    alarm_armed=Signal::from(false)
                    solar_active=Signal::from(false)
                    alerts_badge=Signal::stored(None::<String>)
                >
                    <p>"child"</p>
                </Shell>
            </Router>
        }
    });
    tick().await;
    let sidebar = || {
        host.query_selector(".sidebar")
            .unwrap()
            .expect("a sidebar")
            .text_content()
            .unwrap_or_default()
    };
    assert!(sidebar().contains("Automations"), "{}", sidebar());

    lang.set(Lang::Fr);
    tick().await;
    let fr = sidebar();
    assert!(fr.contains("Automatisations"), "{fr}");
    assert!(!fr.contains("Automations"), "{fr}");
}

fn entity(
    id: &str,
    kind: oikos_core::model::Kind,
    value: oikos_core::model::StateValue,
) -> oikos_api::EntityView {
    use oikos_core::model::{Capabilities, EntityCategory, EntityState, UiOverride};
    oikos_api::EntityView {
        id: id.into(),
        device_id: None,
        name: "Thing".into(),
        device_name: None,
        label: None,
        kind,
        component: kind.component().to_string(),
        protocol_kind: kind,
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
            value,
            ..Default::default()
        },
    }
}

/// A card's value follows the toggle without the entity changing.
#[wasm_bindgen_test]
async fn card_values_follow_the_language_without_a_remount() {
    use oikos_core::model::{CoverState, Kind, StateValue};
    let mut sensor = entity("sensor.t", Kind::Sensor, StateValue::Number(21.46));
    sensor.unit = Some("W".into());
    let cover = entity(
        "cover.c",
        Kind::Cover,
        StateValue::Cover(CoverState {
            state: Some("open".into()),
            position: Some(40),
        }),
    );
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let lang = RwSignal::new(Lang::En);
    let _app = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(House::new());
        provide_context(oikos_web::components::toast::Toasts::new());
        provide_context(oikos_web::commands::Pending::new());
        provide_context(lang);
        view! {
            <Router>
                {oikos_web::screens::home::card_for(sensor.clone())}
                {oikos_web::screens::home::card_for(cover.clone())}
            </Router>
        }
    });
    tick().await;
    let text = || host.text_content().unwrap_or_default();
    assert!(text().contains("21.5\u{a0}W"), "{}", text());
    assert!(text().contains("Open · 40\u{a0}%"), "{}", text());

    lang.set(Lang::Fr);
    tick().await;
    assert!(text().contains("21,5\u{a0}W"), "{}", text());
    assert!(text().contains("Ouvert · 40\u{a0}%"), "{}", text());
}
