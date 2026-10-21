//! The dashboard screen: for id "house" render the existing rooms-overview `HomeScreen`
//! verbatim; for any other id render the generic widget grid.

use leptos::prelude::*;
use leptos_router::hooks::use_params_map;
use oikos_api::HOUSE_ID;

use crate::dashboards::grid::Grid;
use crate::house::House;
use crate::screens::home::HomeScreen;

#[component]
pub fn DashboardScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let params = use_params_map();
    // A Memo so a nav change updates the id without remounting the component.
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
                        let widgets = Signal::derive(move || view.dashboard.widgets.clone());
                        view! { <Grid widgets=widgets /> }.into_any()
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
