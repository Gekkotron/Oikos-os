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

#[path = "support/fetch_stub.rs"]
mod fetch_stub;
use fetch_stub::FetchStub;

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

fn text_of(host: &web_sys::Element, selector: &str) -> String {
    host.query_selector(selector)
        .unwrap()
        .map(|e| e.text_content().unwrap_or_default())
        .unwrap_or_default()
}

#[wasm_bindgen_test]
async fn the_dashboard_sheet_and_the_history_axis_follow_the_language() {
    let stub = FetchStub::install();
    let now = (js_sys::Date::now() / 1000.0) as i64;
    stub.reply(
        "GET",
        "/api/v1/history/series?entity=sensor.t&range=24h&",
        200,
        &format!(
            r#"{{"window":86400,"points":[[{},20.0,19.0,21.5]]}}"#,
            now - 3_600
        ),
    );
    let window = web_sys::window().unwrap();
    window
        .history()
        .unwrap()
        .push_state_with_url(
            &wasm_bindgen::JsValue::NULL,
            "",
            Some("/dashboards/d_fr?edit=1"),
        )
        .unwrap();
    let document = window.document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();

    let house = House::new();
    let lang = RwSignal::new(Lang::Fr);
    let mut sensor = light("sensor.t");
    sensor.kind = Kind::Sensor;
    sensor.component = Kind::Sensor.component().to_string();
    sensor.unit = Some("°C".into());
    house
        .entities
        .set([sensor].into_iter().map(|e| (e.id.clone(), e)).collect());
    house.dashboards.set(vec![DashboardView {
        dashboard: Dashboard {
            id: "d_fr".into(),
            name: "Mine".into(),
            order: 1,
            widgets: vec![Widget {
                id: "w1".into(),
                kind: WidgetKind::HistoryChart,
                span: Span::X2x1,
                config: WidgetConfig::History {
                    entity_id: "sensor.t".into(),
                    range: oikos_core::dashboard::DashboardHistoryRange::H24,
                },
            }],
            show_header: false,
        },
        synthesized: false,
    }]);
    let _app = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(house);
        provide_context(RwSignal::new(Theme::Auto));
        provide_context(lang);
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
    let gear: web_sys::HtmlElement = host
        .query_selector(".dashboard-edit-toolbar__gear")
        .unwrap()
        .unwrap()
        .unchecked_into();
    gear.click();
    settle().await;

    // French: the sheet and the chart's axis and legend.
    let sheet = text_of(&host, ".dashboard-settings-sheet");
    assert!(sheet.contains("Réglages du tableau de bord"), "{sheet}");
    assert!(sheet.contains("Afficher l'en-tête"), "{sheet}");
    assert!(!sheet.contains("Show header"), "{sheet}");
    let axis = text_of(&host, ".history-chart__axis-x");
    assert!(axis.contains("maintenant"), "{axis}");
    assert!(!axis.contains("now"), "{axis}");
    let legend = text_of(&host, ".history-chart__legend");
    assert!(legend.contains("24 dernières heures"), "{legend}");
    assert!(legend.contains("max 21,5\u{a0}°C"), "{legend}");

    // English, with no remount: the same elements read the other language.
    lang.set(Lang::En);
    settle().await;
    let sheet = text_of(&host, ".dashboard-settings-sheet");
    assert!(sheet.contains("Dashboard settings"), "{sheet}");
    assert!(!sheet.contains("Afficher"), "{sheet}");
    let axis = text_of(&host, ".history-chart__axis-x");
    assert!(axis.contains("now"), "{axis}");
    assert!(!axis.contains("maintenant"), "{axis}");
    let legend = text_of(&host, ".history-chart__legend");
    assert!(legend.contains("Last 24 hours"), "{legend}");
    assert!(legend.contains("max 21.5\u{a0}°C"), "{legend}");
    // The chart did not refetch for the language change.
    let fetches = stub
        .requests()
        .iter()
        .filter(|r| r.url.starts_with("/api/v1/history/series"))
        .count();
    assert_eq!(fetches, 1);
}
