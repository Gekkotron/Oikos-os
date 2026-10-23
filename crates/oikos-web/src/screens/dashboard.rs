//! The dashboard screen: for id "house" render the existing rooms-overview `HomeScreen`
//! verbatim; for any other id render the generic widget grid.

use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};
use oikos_api::HOUSE_ID;

use crate::dashboards::config_sheet::{DashboardSettingsSheet, WidgetConfigSheet};
use crate::dashboards::edit::{EditState, EditToolbar, add_widget_handler};
use crate::dashboards::grid::Grid;
use crate::dashboards::library::WidgetLibrary;
use crate::house::House;
use crate::screens::home::HomeScreen;

#[component]
pub fn DashboardScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let params = use_params_map();
    // A Memo so a nav change updates the id without remounting the component.
    let query = use_query_map();
    let id = Memo::new(move |_| params.read().get("id").unwrap_or_default());
    // Whether the list holds this dashboard. The fallback tracks this instead of the whole
    // list so Save writing its result back does not rebuild the editor mid-edit.
    let known = Memo::new(move |_| {
        let current = id.get();
        house
            .dashboards
            .with(|v| v.iter().any(|d| d.dashboard.id == current))
    });

    view! {
        <Show
            when=move || id.get() == HOUSE_ID
            fallback=move || {
                // Tracking `known` keeps the fallback reactive to the fetch landing after
                // first paint.
                let current_id = id.get();
                known.track();
                match house.dashboard(&current_id) {
                    Some(view) => {
                        // The sidebar's "New dashboard" lands here with ?edit=1.
                        let initial_edit = query.read_untracked().get("edit").as_deref() == Some("1");
                        let edit = EditState::new(view.dashboard.clone(), initial_edit);
                        let widgets = Signal::derive(move || edit.draft.get().widgets);
                        view! {
                            <EditToolbar state=edit />
                            // The library sits beside the grid while editing; the layout wraps
                            // both only then so the plain grid keeps its full width.
                            <div class:dashboard-edit-layout=move || edit.active.get()>
                                <Grid widgets=widgets edit=edit />
                                <Show when=move || edit.active.get()>
                                    <aside class="dashboard-library-panel" aria-label="Widget library">
                                        <WidgetLibrary on_add=add_widget_handler(edit) />
                                    </aside>
                                </Show>
                            </div>
                            // Rebuilt per selected index so the sheet never shows a stale widget.
                            {move || {
                                edit.selected_widget.get().map(|index| view! {
                                    <WidgetConfigSheet
                                        state=edit
                                        index=index
                                        on_close=Callback::new(move |_| edit.selected_widget.set(None))
                                    />
                                })
                            }}
                            <Show when=move || edit.settings_open.get()>
                                <DashboardSettingsSheet
                                    state=edit
                                    on_close=Callback::new(move |_| edit.settings_open.set(false))
                                />
                            </Show>
                        }
                            .into_any()
                    }
                    None => {
                        view! {
                            <div class="dashboard-unknown">
                                <p>"Unknown dashboard."</p>
                                <a href="/">"Back to House"</a>
                            </div>
                        }
                            .into_any()
                    }
                }
            }
        >
            <HomeScreen />
        </Show>
    }
}
