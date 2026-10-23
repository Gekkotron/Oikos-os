#![cfg(target_arch = "wasm32")]

use leptos::prelude::*;
use leptos::task::tick;
use leptos_router::components::Router;
use oikos_api::DashboardView;
use oikos_core::dashboard::Dashboard;
use oikos_web::house::House;
use oikos_web::i18n::Lang;
use oikos_web::shell::Shell;
use oikos_web::theme::Theme;
use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

fn dashboard(id: &str, name: &str, order: u32) -> DashboardView {
    DashboardView {
        dashboard: Dashboard {
            id: id.into(),
            name: name.into(),
            order,
            widgets: Vec::new(),
        },
        synthesized: false,
    }
}

/// What a test drives from outside: the house, the engine and badge inputs, the theme.
struct Mounted {
    /// Unmounts on drop, so the next test's router is the only one handling link clicks.
    _app: Box<dyn std::any::Any>,
    host: web_sys::Element,
    house: House,
    climate_on: RwSignal<bool>,
    theme: RwSignal<Theme>,
}

async fn mount(lang: Lang, badge: Option<&str>) -> Mounted {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    let house = House::new();
    let climate_on = RwSignal::new(false);
    let theme = RwSignal::new(Theme::Auto);
    let badge = badge.map(str::to_owned);
    let app = leptos::mount::mount_to(host.clone().unchecked_into(), move || {
        provide_context(house);
        provide_context(theme);
        provide_context(RwSignal::new(lang));
        oikos_web::nav::install_nav();
        let badge: Signal<Option<String>> = Signal::stored(badge.clone());
        view! {
            <Router>
                <Shell
                    climate_on=climate_on
                    alarm_armed=Signal::from(false)
                    solar_active=Signal::from(false)
                    alerts_badge=badge
                >
                    <p class="shell-test-child">"child"</p>
                </Shell>
            </Router>
        }
    });
    tick().await;
    Mounted {
        _app: Box::new(app),
        host,
        house,
        climate_on,
        theme,
    }
}

fn text_of(host: &web_sys::Element, selector: &str) -> String {
    host.query_selector(selector)
        .unwrap()
        .unwrap_or_else(|| panic!("no {selector}"))
        .text_content()
        .unwrap_or_default()
}

#[wasm_bindgen_test]
async fn sidebar_lists_the_main_entries() {
    let m = mount(Lang::En, Some("3")).await;
    let sidebar = text_of(&m.host, "aside.sidebar#sidebar");
    for label in [
        "oikos.",
        "Overview",
        "Automations",
        "Groups",
        "My rooms",
        "Energy",
        "Security",
        "Alerts",
        "Settings",
    ] {
        assert!(sidebar.contains(label), "{label} missing from {sidebar}");
    }
    assert!(m.host.query_selector(".topbar").unwrap().is_none());
    assert!(
        m.host
            .query_selector(".sidebar__status .pill[role=\"status\"]")
            .unwrap()
            .is_some()
    );
    assert!(
        m.host
            .query_selector("ul.sidebar__rooms")
            .unwrap()
            .is_some()
    );
    assert!(
        m.host
            .query_selector(".nav__badge[aria-label=\"3 unseen alerts\"]")
            .unwrap()
            .is_some()
    );
    // The page itself sits in the content column, under the header.
    assert!(
        m.host
            .query_selector(".shell__content > header.content-header")
            .unwrap()
            .is_some()
    );
    assert!(
        m.host
            .query_selector(".shell__content > main.screen > .shell-test-child")
            .unwrap()
            .is_some()
    );
}

#[wasm_bindgen_test]
async fn climate_entry_follows_the_engine() {
    let m = mount(Lang::En, None).await;
    assert!(!text_of(&m.host, ".sidebar").contains("Climate"));
    m.climate_on.set(true);
    tick().await;
    let link = m
        .host
        .query_selector(".sidebar a[href=\"/climate\"]")
        .unwrap()
        .expect("a Climate link");
    let text = link.text_content().unwrap();
    assert!(text.contains("Climate"));
    assert!(text.contains("Beta"));
}

#[wasm_bindgen_test]
async fn dashboards_unfold_from_two() {
    let m = mount(Lang::En, None).await;
    m.house
        .dashboards
        .set(vec![dashboard(oikos_api::HOUSE_ID, "House", 0)]);
    tick().await;
    assert!(
        m.host
            .query_selector(".sidebar__dashboards")
            .unwrap()
            .is_none()
    );
    // Overview goes to the house when it is the only dashboard.
    assert!(
        m.host
            .query_selector(".sidebar a[href=\"/dashboards/house\"]")
            .unwrap()
            .is_some()
    );

    // Listed authored first by the hub; the sidebar still puts the house on top.
    m.house.dashboards.set(vec![
        dashboard("d1", "Wall tablet", 1),
        dashboard(oikos_api::HOUSE_ID, "House", 0),
    ]);
    tick().await;
    let links = m.host.query_selector_all(".sidebar__dashboards a").unwrap();
    assert_eq!(links.length(), 2);
    let first = links.item(0).unwrap().text_content().unwrap();
    let second = links.item(1).unwrap().text_content().unwrap();
    assert_eq!((first.as_str(), second.as_str()), ("House", "Wall tablet"));
    assert!(text_of(&m.host, ".sidebar__dashboards").contains("New dashboard"));
    // Overview now lands on the first authored dashboard, as `/` does.
    assert!(
        m.host
            .query_selector(".sidebar__link[href=\"/dashboards/d1\"]")
            .unwrap()
            .is_some()
    );
}

#[wasm_bindgen_test]
async fn french_shell() {
    let m = mount(Lang::Fr, Some("2")).await;
    let sidebar = text_of(&m.host, ".sidebar");
    for label in [
        "Vue d'ensemble",
        "Automatisations",
        "Mes pièces",
        "Réglages",
    ] {
        assert!(sidebar.contains(label), "{label} missing from {sidebar}");
    }
    assert!(
        text_of(&m.host, "nav.breadcrumb")
            .trim_start()
            .starts_with("Ma maison")
    );
    assert!(
        m.host
            .query_selector(".nav__badge[aria-label=\"2 alertes non lues\"]")
            .unwrap()
            .is_some()
    );
}

#[wasm_bindgen_test]
async fn theme_button_cycles() {
    let m = mount(Lang::En, None).await;
    let button: web_sys::HtmlElement = m
        .host
        .query_selector(".content-header__theme")
        .unwrap()
        .expect("a theme button")
        .unchecked_into();
    assert_eq!(
        button.get_attribute("aria-label").as_deref(),
        Some("Change theme")
    );
    let mut seen = Vec::new();
    for _ in 0..3 {
        button.click();
        tick().await;
        seen.push(m.theme.get_untracked());
        assert_eq!(
            button.get_attribute("title").as_deref(),
            Some(m.theme.get_untracked().label())
        );
    }
    assert_eq!(seen, [Theme::Night, Theme::Ivoire, Theme::Auto]);
}

#[wasm_bindgen_test]
async fn language_button_switches() {
    let m = mount(Lang::En, None).await;
    let button: web_sys::HtmlElement = m
        .host
        .query_selector(".content-header__lang")
        .unwrap()
        .expect("a language button")
        .unchecked_into();
    assert_eq!(
        button.get_attribute("aria-label").as_deref(),
        Some("Français")
    );
    button.click();
    tick().await;
    assert!(text_of(&m.host, ".sidebar").contains("Vue d'ensemble"));
    assert_eq!(
        button.get_attribute("aria-label").as_deref(),
        Some("English")
    );
}

/// Moves the page to `path` the way Back/Forward does; every mounted router follows popstate.
async fn go(path: &str) {
    let window = web_sys::window().unwrap();
    window
        .history()
        .unwrap()
        .push_state_with_url(&wasm_bindgen::JsValue::NULL, "", Some(path))
        .unwrap();
    window
        .dispatch_event(&web_sys::Event::new("popstate").unwrap())
        .unwrap();
    tick().await;
}

#[wasm_bindgen_test]
async fn active_entry_follows_the_route() {
    let m = mount(Lang::En, None).await;
    go("/flows/f1").await;
    let current = m
        .host
        .query_selector_all(".sidebar [aria-current]")
        .unwrap();
    assert_eq!(current.length(), 1);
    assert_eq!(
        current
            .item(0)
            .unwrap()
            .unchecked_into::<web_sys::Element>()
            .get_attribute("href")
            .as_deref(),
        Some("/flows")
    );
    let trail = text_of(&m.host, "nav.breadcrumb");
    assert!(
        trail.contains("Automations") && trail.contains("f1"),
        "{trail}"
    );

    go("/devices").await;
    assert!(
        m.host
            .query_selector(".sidebar a[href=\"/settings\"][aria-current=\"page\"]")
            .unwrap()
            .is_some()
    );
    go("/").await;
}
