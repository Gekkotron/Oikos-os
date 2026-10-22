//! The dashboard screen: for id "house" render the existing rooms-overview `HomeScreen`
//! verbatim; for any other id render the generic widget grid.

use leptos::prelude::*;
use leptos_router::hooks::{use_navigate, use_params_map, use_query_map};
use oikos_api::HOUSE_ID;

use crate::dashboards::config_sheet::{DashboardSettingsSheet, WidgetConfigSheet};
use crate::dashboards::edit::{EditState, EditToolbar};
use crate::dashboards::grid::Grid;
use crate::house::House;
use crate::screens::home::HomeScreen;

#[component]
pub fn DashboardScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let params = use_params_map();
    // A Memo so a nav change updates the id without remounting the component.
    let query = use_query_map();
    let id = Memo::new(move |_| params.read().get("id").unwrap_or_default());

    // House hands off to the first authored dashboard once there is one; the rooms-overview
    // fallback below only shows while the user has no authored dashboard yet.
    let navigate = use_navigate();
    Effect::new(move |_| {
        if id.get() != HOUSE_ID {
            return;
        }
        let first = house.dashboards.with(|list| {
            list.iter()
                .find(|d| d.dashboard.id != HOUSE_ID)
                .map(|d| d.dashboard.id.clone())
        });
        if let Some(first) = first {
            navigate(
                &format!("/dashboards/{first}"),
                leptos_router::NavigateOptions {
                    replace: true,
                    ..Default::default()
                },
            );
        }
    });

    view! {
        <Show
            when=move || id.get() == HOUSE_ID
                && house.dashboards.with(|list| list.iter().all(|d| d.dashboard.id == HOUSE_ID))
            fallback=move || {
                // Re-reading `house.dashboards` here keeps the fallback reactive to the
                // retained-topic fetch landing after first paint.
                let current_id = id.get();
                house.dashboards.track();
                match house.dashboard(&current_id) {
                    Some(view) => {
                        // The picker's "New dashboard" lands here with ?edit=1.
                        let initial_edit = query.read_untracked().get("edit").as_deref() == Some("1");
                        let edit = EditState::new(view.dashboard.clone(), initial_edit);
                        let widgets = Signal::derive(move || edit.draft.get().widgets);
                        view! {
                            <EditToolbar state=edit />
                            <Grid widgets=widgets edit=edit />
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
