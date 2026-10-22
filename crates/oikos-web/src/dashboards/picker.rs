//! The top-nav dashboards picker. Lists dashboards in `order` and ends with a `＋ New dashboard`
//! action that creates an empty dashboard and navigates to it.

use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use leptos_router::hooks::{use_location, use_navigate};
use oikos_api::DashboardView;
use oikos_core::dashboard::Dashboard;

use crate::api;
use crate::house::House;
use crate::icons::{CHEVRON_DOWN, HOUSE};

#[component]
pub fn DashboardsPicker() -> impl IntoView {
    let house = expect_context::<House>();
    let open = RwSignal::new(false);
    let navigate = use_navigate();
    let location = use_location();
    let trigger_ref = NodeRef::<leptos::html::Button>::new();
    // Fixed-position coords for the open menu, read from the trigger's rect each time it opens.
    // The menu uses position: fixed to escape .topbar__nav's overflow-x clipping, which would
    // otherwise hide the dropdown on narrower viewports where the nav scrolls horizontally.
    let menu_pos = RwSignal::new((0.0_f64, 0.0_f64));

    // The dashboard id in the current URL path, if the user is on a dashboard page.
    // Used both to light the picker trigger and to show the active dashboard's name on it.
    let active_id = Memo::new(move |_| {
        location
            .pathname
            .get()
            .strip_prefix("/dashboards/")
            .map(|rest| rest.split('/').next().unwrap_or("").to_owned())
    });

    // Any route change closes the menu, so clicking another top-nav tab (or any link that
    // navigates) does not leave the dropdown hovering over the new page.
    Effect::new(move |_| {
        location.pathname.track();
        open.set(false);
    });

    let label = move || match active_id.get() {
        Some(id) => house.dashboards.with(|list| {
            list.iter()
                .find(|d| d.dashboard.id == id)
                .map(|d| d.dashboard.name.clone())
                .unwrap_or_else(|| "Dashboards".to_owned())
        }),
        None => "Dashboards".to_owned(),
    };

    let is_active = move || active_id.get().is_some();

    let toggle = move || {
        if !open.get_untracked()
            && let Some(button) = trigger_ref.get_untracked()
        {
            let rect = button.get_bounding_client_rect();
            // The topbar has backdrop-filter, which makes it the containing block for
            // position: fixed descendants — so subtract the trigger's offsetParent origin to
            // turn viewport coords into containing-block coords.
            let (ox, oy) = button
                .offset_parent()
                .map(|p| {
                    let r = p.get_bounding_client_rect();
                    (r.left(), r.top())
                })
                .unwrap_or((0.0, 0.0));
            menu_pos.set((rect.left() - ox, rect.bottom() - oy + 6.0));
        }
        open.update(|o| *o = !*o);
    };

    view! {
        <div class="dashboards-picker">
            <button
                node_ref=trigger_ref
                type="button"
                class="dashboards-picker__trigger"
                class:dashboards-picker__trigger--active=is_active
                on:click=move |_| toggle()
            >
                <span class="nav__glyph" inner_html=HOUSE></span>
                <span class="nav__label">{label}</span>
                <span class="dashboards-picker__chevron" inner_html=CHEVRON_DOWN></span>
            </button>
            <Show when=move || open.get()>
                <ul
                    class="dashboards-picker__menu"
                    style=move || {
                        let (left, top) = menu_pos.get();
                        format!("left:{left}px;top:{top}px")
                    }
                    on:click=move |_| open.set(false)
                >
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
