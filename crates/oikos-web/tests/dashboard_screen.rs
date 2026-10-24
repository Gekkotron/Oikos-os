#![cfg(target_arch = "wasm32")]
//! Opening an authored dashboard's address directly: the screen paints before the dashboards
//! fetch lands and must swap "Unknown dashboard." for the dashboard once it does. Its own test
//! binary, so no router from another test reads the address.

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::{Route, Router, Routes};
use leptos_router::path;
use oikos_api::{DashboardView, EntityView};
use oikos_core::dashboard::{Dashboard, Span, Widget, WidgetConfig, WidgetKind};
use oikos_core::model::{Capabilities, EntityCategory, EntityState, Kind, UiOverride};
use oikos_web::house::House;
use oikos_web::i18n::Lang;
use oikos_web::screens::dashboard::DashboardScreen;
use oikos_web::theme::Theme;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

fn light(id: &str) -> EntityView {
    EntityView {
        id: id.into(),
        device_id: None,
        name: "Lamp".into(),
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
        area: "Kitchen".into(),
        part_of: None,
        capabilities: Capabilities::default(),
        ui: UiOverride::default(),
        state: EntityState::default(),
    }
}

async fn settle() {
    for _ in 0..3 {
        tick().await;
    }
    gloo_timers::future::TimeoutFuture::new(50).await;
}

#[wasm_bindgen_test]
async fn a_dashboard_fetched_after_first_paint_replaces_unknown() {
    let window = web_sys::window().unwrap();
    window
        .history()
        .unwrap()
        .push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some("/dashboards/d_late"))
        .unwrap();
    let document = window.document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();

    let house = House::new();
    let _app = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(house);
        provide_context(RwSignal::new(Theme::Auto));
        provide_context(RwSignal::new(Lang::En));
        provide_context(oikos_web::components::toast::Toasts::new());
        provide_context(oikos_web::commands::Pending::new());
        provide_context(oikos_web::components::confirm::Confirming::new());
        oikos_web::nav::install_nav();
        view! {
            <Router>
                <Routes fallback=|| ()>
                    <Route path=path!("/dashboards/:id") view=DashboardScreen />
                </Routes>
            </Router>
        }
    });
    settle().await;
    assert!(host.query_selector(".dashboard-unknown").unwrap().is_some());

    house.dashboards.set(vec![DashboardView {
        dashboard: Dashboard {
            id: "d_late".into(),
            name: "Late".into(),
            order: 1,
            widgets: vec![Widget {
                id: "w1".into(),
                kind: WidgetKind::Light,
                span: Span::X1x1,
                config: WidgetConfig::Entity {
                    entity_id: "light.k".into(),
                    title: None,
                },
            }],
            show_header: false,
        },
        synthesized: false,
    }]);
    settle().await;
    assert!(host.query_selector(".dashboard-unknown").unwrap().is_none());
    assert!(
        host.query_selector(".dashboard-edit-toolbar")
            .unwrap()
            .is_some()
    );
    // The state snapshot lands after the list: the tile swaps its placeholder for the card.
    assert!(
        host.query_selector(".widget-unavailable")
            .unwrap()
            .is_some()
    );
    house.entities.set(
        [light("light.k")]
            .into_iter()
            .map(|e| (e.id.clone(), e))
            .collect(),
    );
    settle().await;
    assert!(
        host.query_selector(".widget-unavailable")
            .unwrap()
            .is_none()
    );
}
