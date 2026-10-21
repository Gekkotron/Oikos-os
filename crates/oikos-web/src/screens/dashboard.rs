//! The dashboard screen: for id "house" render the existing rooms-overview `HomeScreen`
//! verbatim; for any other id render the generic widget grid.

use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};
use oikos_api::HOUSE_ID;

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

    view! {
        <Show
            when=move || id.get() == HOUSE_ID
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
