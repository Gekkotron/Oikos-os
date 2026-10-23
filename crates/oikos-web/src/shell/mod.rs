//! The frame every screen sits in: the sidebar beside a content column (header, then the screen).

pub mod header;
pub mod route;
pub mod sidebar;

use leptos::prelude::*;

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
    view! {
        <div class="shell">
            <Sidebar climate_on alarm_armed solar_active alerts_badge />
            <div class="shell__content">
                <ContentHeader />
                <main class="screen">{children()}</main>
            </div>
        </div>
    }
}
