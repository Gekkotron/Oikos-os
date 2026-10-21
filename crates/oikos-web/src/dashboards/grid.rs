use leptos::prelude::*;
use oikos_core::dashboard::Widget;

use super::widget::WidgetView;

/// Column grid that places widgets by their declared span. Column counts per breakpoint
/// live in `styles/main.css` (`.dashboard-grid`); a span wider than the column count is
/// clamped there so no widget overflows on a phone.
#[component]
pub fn Grid(widgets: Signal<Vec<Widget>>) -> impl IntoView {
    view! {
        <div class="dashboard-grid">
            <For
                each=move || widgets.get()
                key=|w| w.id.clone()
                children=move |w: Widget| {
                    let class = format!("widget widget-span-{}", w.span.as_str());
                    view! {
                        <div class=class>
                            <WidgetView widget=w />
                        </div>
                    }
                }
            />
        </div>
    }
}
