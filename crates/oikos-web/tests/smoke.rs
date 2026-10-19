#![cfg(target_arch = "wasm32")]

use wasm_bindgen::JsCast;
use wasm_bindgen_test::*;
wasm_bindgen_test_configure!(run_in_browser);

#[wasm_bindgen_test]
fn app_renders() {
    let document = web_sys::window().unwrap().document().unwrap();
    let host = document.create_element("div").unwrap();
    document.body().unwrap().append_child(&host).unwrap();
    // The handle unmounts the app when dropped, so it must live until the assertions run.
    let _mounted = leptos::mount::mount_to(host.clone().unchecked_into(), oikos_web::app::App);
    let html = host.inner_html();
    assert!(html.contains("Oikos"));
    assert!(html.contains("pill"));
    assert!(html.contains("Alerts"));
    assert!(html.contains("Flows"));
}
