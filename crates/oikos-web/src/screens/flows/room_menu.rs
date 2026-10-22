//! The ⋯ menu of a flow row or card: open the flow, or move it to a room.

use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use oikos_core::model::UNASSIGNED_AREA;
use wasm_bindgen::JsCast;

use crate::api;
use crate::components::toast::Toasts;
use crate::house::House;

/// A ⋯ button over a small popover. `current` is the flow's room (a known area id), `invalid`
/// flows cannot be moved. Esc or a press outside the menu closes it.
#[component]
pub fn RoomMenu(id: String, href: String, current: Option<String>, invalid: bool) -> impl IntoView {
    let house = expect_context::<House>();
    let toasts = expect_context::<Toasts>();
    let open = RwSignal::new(false);

    let esc = window_event_listener(ev::keydown, move |ev| {
        if open.get_untracked() && ev.key() == "Escape" {
            open.set(false);
        }
    });
    let outside = window_event_listener(ev::pointerdown, move |ev| {
        if !open.get_untracked() {
            return;
        }
        let inside = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .is_some_and(|el| el.closest(".room-menu").ok().flatten().is_some());
        if !inside {
            open.set(false);
        }
    });
    on_cleanup(move || {
        esc.remove();
        outside.remove();
    });

    let id = StoredValue::new(id);
    // The hub's push refreshes the list, like the enable switch.
    let move_to = move |area: Option<String>| {
        open.set(false);
        let id = id.get_value();
        spawn_local(async move {
            if let Err(e) = api::set_flow_area(&id, area.as_deref()).await {
                toasts.push(e.error);
            }
        });
    };

    let item = move |label: String, area: Option<String>, checked: bool| {
        let target = area.clone();
        view! {
            <button
                type="button"
                role="menuitemradio"
                class="room-menu__item"
                aria-checked=checked.to_string()
                disabled=invalid
                title=invalid.then_some("Fix this flow first")
                on:click=move |_| move_to(target.clone())
            >
                <span class="room-menu__check" aria-hidden="true">
                    {checked.then_some("✓")}
                </span>
                {label}
            </button>
        }
    };
    let rooms = {
        let current = current.clone();
        move || {
            house.areas.with(|areas| {
                areas
                    .iter()
                    .filter(|a| a.id != UNASSIGNED_AREA)
                    .map(|a| {
                        let checked = current.as_deref() == Some(a.id.as_str());
                        item(a.name.clone(), Some(a.id.clone()), checked).into_any()
                    })
                    .collect_view()
            })
        }
    };
    let none_checked = current.is_none();

    view! {
        <span class="room-menu">
            <button
                type="button"
                class="btn btn--ghost btn--icon room-menu__button"
                aria-label="Flow actions"
                title="Flow actions"
                aria-haspopup="menu"
                aria-expanded=move || open.get().to_string()
                on:click=move |_| open.update(|o| *o = !*o)
            >
                "⋯"
            </button>
            <Show when=move || open.get()>
                <div class="room-menu__pop" role="menu">
                    <A href=href.clone() attr:class="room-menu__item" attr:role="menuitem">
                        <span class="room-menu__check" aria-hidden="true"></span>
                        "Open"
                    </A>
                    <p class="room-menu__head" role="presentation">"Move to room"</p>
                    {rooms.clone()}
                    {item("No room".to_owned(), None, none_checked)}
                </div>
            </Show>
        </span>
    }
}
