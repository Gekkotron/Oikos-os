//! The top-nav dashboards picker. Lists dashboards in `order` and ends with a `＋ New dashboard`
//! action that creates an empty dashboard and navigates to it.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::use_navigate;
use oikos_api::DashboardView;
use oikos_core::dashboard::Dashboard;

use crate::api;
use crate::house::House;

#[component]
pub fn DashboardsPicker() -> impl IntoView {
    let house = expect_context::<House>();
    let open = RwSignal::new(false);
    let navigate = use_navigate();

    view! {
        <div class="dashboards-picker">
            <button
                type="button"
                class="dashboards-picker__trigger"
                on:click=move |_| open.update(|o| *o = !*o)
            >
                "Dashboards ▾"
            </button>
            <Show when=move || open.get()>
                <ul class="dashboards-picker__menu" on:click=move |_| open.set(false)>
                    <For
                        each=move || house.dashboards.get()
                        key=|dv| (dv.dashboard.id.clone(), dv.dashboard.name.clone())
                        children=move |dv| {
                            let url = format!("/dashboards/{}", dv.dashboard.id);
                            view! {
                                <li>
                                    <A href=url>{dv.dashboard.name}</A>
                                </li>
                            }
                        }
                    />
                    <li class="dashboards-picker__new">
                        <button
                            type="button"
                            on:click={
                                let navigate = navigate.clone();
                                move |_| {
                                    let navigate = navigate.clone();
                                    spawn_local(async move {
                                        let order = house.dashboards.with_untracked(|list| {
                                            list.iter().map(|d| d.dashboard.order).max().unwrap_or(0) + 1
                                        });
                                        let d = Dashboard {
                                            id: format!("d_{}", gen_id()),
                                            name: "New dashboard".to_owned(),
                                            order,
                                            widgets: Vec::new(),
                                        };
                                        // A failed save leaves the menu as it was; the edit-mode task owns error display.
                                        if let Ok(saved) = api::put_dashboard(&d).await {
                                            let id = saved.id.clone();
                                            house.dashboards.update(|list| {
                                                list.push(DashboardView { dashboard: saved, synthesized: false });
                                            });
                                            navigate(&format!("/dashboards/{id}?edit=1"), Default::default());
                                        }
                                    });
                                }
                            }
                        >
                            "＋ New dashboard"
                        </button>
                    </li>
                </ul>
            </Show>
        </div>
    }
}

/// A short URL-safe id from the clock; avoids a uuid dependency for a single-user client.
fn gen_id() -> String {
    format!("{:x}", js_sys::Date::now() as u64)
}
