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
