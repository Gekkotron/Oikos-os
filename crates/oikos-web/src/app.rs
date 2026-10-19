//! The shell: top bar (name, connection pill, theme toggle, navigation) around the router.

use leptos::prelude::*;
use leptos_router::components::{A, Route, Router, Routes};
use leptos_router::path;

use crate::components::status_pill::StatusPill;
use crate::house::House;
use crate::screens::devices::DevicesScreen;
use crate::screens::entity_sheet::EntitySheetScreen;
use crate::screens::home::HomeScreen;
use crate::screens::settings::SettingsScreen;
use crate::theme::{Theme, install_theme};

/// The next theme in the Auto, Night, Warm cycle.
fn next_theme(theme: Theme) -> Theme {
    match theme {
        Theme::Auto => Theme::Night,
        Theme::Night => Theme::Warm,
        Theme::Warm => Theme::Auto,
    }
}

#[component]
pub fn App() -> impl IntoView {
    let house = House::new();
    provide_context(house);
    let theme = install_theme();
    #[cfg(target_arch = "wasm32")]
    crate::ws::connect("/ws", house);

    view! {
        <Router>
            <header class="topbar">
                <span class="topbar__name">"Oikos"</span>
                <StatusPill />
                <button
                    type="button"
                    class="theme-toggle"
                    title="Switch theme"
                    on:click=move |_| theme.update(|t| *t = next_theme(*t))
                >
                    {move || theme.get().as_str()}
                </button>
                <nav class="topbar__nav">
                    <A href="/" exact=true>"Home"</A>
                    <A href="/devices">"Devices"</A>
                    <A href="/settings">"Settings"</A>
                </nav>
            </header>
            <main class="screen">
                <Routes fallback=HomeScreen>
                    <Route path=path!("/") view=HomeScreen />
                    <Route path=path!("/entity/:id") view=EntitySheetScreen />
                    <Route path=path!("/devices") view=DevicesScreen />
                    <Route path=path!("/settings") view=SettingsScreen />
                </Routes>
            </main>
        </Router>
    }
}
