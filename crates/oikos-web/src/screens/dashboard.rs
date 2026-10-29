//! The dashboard screen: for id "house" render the existing rooms-overview `HomeScreen`
//! verbatim; for any other id render the generic widget grid.

use leptos::prelude::*;
use leptos_router::hooks::{use_params_map, use_query_map};
use oikos_api::HOUSE_ID;

use crate::dashboards::config_sheet::{DashboardSettingsSheet, WidgetConfigSheet};
use crate::dashboards::edit::{EditState, EditToolbar, add_widget_handler};
use crate::dashboards::grid::Grid;
use crate::dashboards::library::WidgetLibrary;
use crate::home_header::view::HomeHeader;
use crate::house::House;
use crate::i18n::{DashText, dash_text, use_lang};
use crate::screens::home::HomeScreen;

#[component]
pub fn DashboardScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let lang = use_lang();
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
    // A Memo too: the fallback reads it, so only the flag's first change may rebuild the editor.
    let loaded = Memo::new(move |_| house.dashboards_loaded.get());

    view! {
        <Show
            when=move || id.get() == HOUSE_ID
            fallback=move || {
                // Reading `known` keeps the fallback reactive to the fetch landing after first
                // paint. It must be read, not tracked: a memo runs (and subscribes to the list)
                // only when read.
                let current_id = id.get();
                let found = known.get().then(|| house.dashboard(&current_id)).flatten();
                match found {
                    Some(view) => {
                        // The sidebar's "New dashboard" lands here with ?edit=1.
                        let initial_edit = query.read_untracked().get("edit").as_deref() == Some("1");
                        let edit = EditState::new(view.dashboard.clone(), initial_edit);
                        // A refetch (on a reconnect) can bring a copy saved elsewhere: take it
                        // while nothing is being edited here.
                        Effect::new(move |_| {
                            let Some(fresh) = house
                                .dashboards
                                .with(|v| v.iter().find(|d| d.dashboard.id == current_id).cloned())
                            else {
                                return;
                            };
                            let original = edit.original.get_untracked();
                            let idle = !edit.active.get_untracked() && edit.draft.get_untracked() == original;
                            if idle && original != fresh.dashboard {
                                edit.original.set(fresh.dashboard.clone());
                                edit.draft.set(fresh.dashboard);
                            }
                        });
                        let widgets = Signal::derive(move || edit.draft.get().widgets);
                        let show_header = move || edit.draft.with(|d| d.show_header);
                        view! {
                            <Show when=show_header>
                                <HomeHeader />
                            </Show>
                            <EditToolbar state=edit />
                            // The library sits beside the grid while editing; the layout wraps
                            // both only then so the plain grid keeps its full width.
                            <div class:dashboard-edit-layout=move || edit.active.get()>
                                <Grid widgets=widgets edit=edit />
                                <Show when=move || edit.active.get()>
                                    <aside class="dashboard-library-panel" aria-label=move || dash_text(lang(), DashText::WidgetLibrary)>
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
                    None if !loaded.get() => {
                        view! {
                            <p class="dashboard-loading" aria-busy="true">
                                {move || dash_text(lang(), DashText::LoadingDashboard)}
                            </p>
                        }
                            .into_any()
                    }
                    None => {
                        view! {
                            <div class="dashboard-unknown">
                                <p>{move || dash_text(lang(), DashText::UnknownDashboard)}</p>
                                <a href="/">{move || dash_text(lang(), DashText::BackToHome)}</a>
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
