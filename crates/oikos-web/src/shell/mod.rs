//! The frame every screen sits in: the sidebar beside a content column (header, then the screen).

pub mod header;
pub mod route;
pub mod sidebar;

use leptos::ev;
use leptos::prelude::*;
use leptos_router::hooks::use_location;
use wasm_bindgen::JsCast;

use header::ContentHeader;
use sidebar::Sidebar;

/// Render it inside the router, around the routes. The four signals are the sidebar's live marks:
/// the Climate entry, the armed and solar glyph colours, and the unseen-alerts badge.
#[component]
pub fn Shell(
    #[prop(into)] climate_on: Signal<bool>,
    #[prop(into)] alarm_armed: Signal<bool>,
    #[prop(into)] solar_active: Signal<bool>,
    #[prop(into)] alerts_badge: Signal<Option<String>>,
    children: Children,
) -> impl IntoView {
    // Below 900px the sidebar is a drawer; this says whether it is out.
    let drawer_open = RwSignal::new(false);
    let location = use_location();
    Effect::new(move |_| {
        location.pathname.track();
        drawer_open.set(false);
    });
    let _escape = window_event_listener(ev::keydown, move |event| {
        // `try_`: the listener can outlive an unmounted shell for a moment.
        if event.key() == "Escape" && drawer_open.try_get_untracked() == Some(true) {
            drawer_open.try_set(false);
        }
    });
    // Widening past the phone layout (rotating, resizing) leaves nothing to close by hand.
    let _widen = window_event_listener(ev::resize, move |_| {
        let wide = window()
            .inner_width()
            .ok()
            .and_then(|w| w.as_f64())
            .is_some_and(|w| w >= 900.0);
        if wide {
            drawer_open.try_set(false);
        }
    });
    // Focus follows the drawer: into it when it opens, back to the menu button when it closes.
    Effect::new(move |was_open: Option<bool>| {
        let open = drawer_open.get();
        // The content is inert while the drawer is out; free it before focus goes back to it.
        if !open && let Some(content) = document().query_selector(".shell__content").ok().flatten()
        {
            let _ = content.remove_attribute("inert");
        }
        let selector = match (was_open, open) {
            (_, true) => Some(".sidebar a"),
            (Some(true), false) => Some(".content-header__menu"),
            _ => None,
        };
        let target = selector
            .and_then(|s| document().query_selector(s).ok().flatten())
            .and_then(|el| el.dyn_into::<web_sys::HtmlElement>().ok());
        if let Some(el) = target {
            let _ = el.focus();
        }
        open
    });

    view! {
        <div class="shell">
            <Sidebar climate_on alarm_armed solar_active alerts_badge drawer_open />
            <Show when=move || drawer_open.get()>
                <div class="shell__scrim" aria-hidden="true" on:click=move |_| drawer_open.set(false)></div>
            </Show>
            <div class="shell__content" inert=move || drawer_open.get().then_some("")>
                <ContentHeader drawer_open />
                <main class="screen">{children()}</main>
            </div>
        </div>
    }
}
