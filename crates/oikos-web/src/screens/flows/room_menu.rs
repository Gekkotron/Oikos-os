//! The ⋯ menu of a flow row or card: open the flow, move it to a room, or delete it.

use leptos::ev;
use leptos::prelude::*;
use leptos::task::spawn_local;
use leptos_router::components::A;
use oikos_core::model::UNASSIGNED_AREA;
use wasm_bindgen::JsCast;

use crate::api;
use crate::components::toast::Toasts;
use crate::house::House;

/// Which flow's ⋯ menu is open (its id), held by the page rather than by each menu: a row the
/// list rebuilds keeps its menu open, only one menu is ever open, and one pair of window
/// listeners serves them all.
#[derive(Clone, Copy)]
pub struct OpenMenu(RwSignal<Option<String>>);

/// Provides [`OpenMenu`] to the page and registers its one Escape / outside-press listener pair,
/// removed when the page goes away.
pub fn provide_open_menu() {
    let open = RwSignal::new(None::<String>);
    provide_context(OpenMenu(open));
    let esc = window_event_listener(ev::keydown, move |ev| {
        if ev.key() == "Escape" && open.with_untracked(Option::is_some) {
            open.set(None);
        }
    });
    let outside = window_event_listener(ev::pointerdown, move |ev| {
        if open.with_untracked(Option::is_none) {
            return;
        }
        let inside = ev
            .target()
            .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
            .is_some_and(|el| el.closest(".room-menu").ok().flatten().is_some());
        if !inside {
            open.set(None);
        }
    });
    on_cleanup(move || {
        esc.remove();
        outside.remove();
    });
}

/// A ⋯ button over a small popover. `current` is the flow's room (a known area id), `invalid`
/// flows cannot be moved (they can still be deleted). Delete asks first, inside the popover,
/// naming the flow by `name`. Esc or a press outside the menu closes it (see [`OpenMenu`]).
#[component]
pub fn RoomMenu(
    id: String,
    name: String,
    href: String,
    current: Option<String>,
    invalid: bool,
) -> impl IntoView {
    let house = expect_context::<House>();
    let toasts = expect_context::<Toasts>();
    let OpenMenu(open_menu) = expect_context::<OpenMenu>();
    let open = {
        let id = id.clone();
        Memo::new(move |_| open_menu.with(|o| o.as_deref() == Some(id.as_str())))
    };

    // Asking whether to delete; dropped whenever the menu closes.
    let confirming = RwSignal::new(false);
    Effect::new(move |_| {
        if !open.get() {
            confirming.set(false);
        }
    });
    // The swap removes the focused button, so focus follows it: the confirm opens on Cancel
    // and Cancel goes back to Delete flow…. Each returns whether it was confirming.
    let cancel_button = NodeRef::<leptos::html::Button>::new();
    let delete_item = NodeRef::<leptos::html::Button>::new();
    Effect::new(move |was: Option<bool>| {
        let now = confirming.get();
        if now {
            if let Some(button) = cancel_button.get() {
                let _ = button.focus();
            }
        } else if was == Some(true)
            && let Some(button) = delete_item.get()
        {
            let _ = button.focus();
        }
        now
    });

    let id = StoredValue::new(id);
    // The hub's push refreshes the list, like the enable switch.
    let move_to = move |area: Option<String>| {
        open_menu.set(None);
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
    // The hub's push drops the row; a refusal shows as a toast and the row stays.
    let delete = move |_| {
        open_menu.set(None);
        let id = id.get_value();
        spawn_local(async move {
            if let Err(e) = api::delete_flow(&id).await {
                toasts.push(e.error);
            }
        });
    };
    let prompt = StoredValue::new(format!("Delete “{name}”?"));

    view! {
        <span class="room-menu">
            <button
                type="button"
                class="btn btn--ghost btn--icon room-menu__button"
                aria-label="Flow actions"
                title="Flow actions"
                aria-haspopup="menu"
                aria-expanded=move || open.get().to_string()
                on:click=move |_| {
                    let id = id.get_value();
                    open_menu.update(|o| *o = (o.as_deref() != Some(id.as_str())).then_some(id));
                }
            >
                "⋯"
            </button>
            <Show when=move || open.get()>
                <div class="room-menu__pop" role="menu">
                    <Show
                        when=move || confirming.get()
                        fallback={
                            let href = href.clone();
                            let rooms = rooms.clone();
                            move || {
                                view! {
                                    <A
                                        href=href.clone()
                                        attr:class="room-menu__item"
                                        attr:role="menuitem"
                                    >
                                        <span class="room-menu__check" aria-hidden="true"></span>
                                        "Open"
                                    </A>
                                    <p class="room-menu__head" role="presentation">
                                        "Move to room"
                                    </p>
                                    {rooms.clone()}
                                    {item("No room".to_owned(), None, none_checked)}
                                    <button
                                        type="button"
                                        role="menuitem"
                                        class="room-menu__item room-menu__item--danger"
                                        node_ref=delete_item
                                        on:click=move |_| confirming.set(true)
                                    >
                                        <span class="room-menu__check" aria-hidden="true"></span>
                                        "Delete flow…"
                                    </button>
                                }
                            }
                        }
                    >
                        <div class="room-menu__confirm" role="alertdialog" aria-label="Delete flow">
                            <p class="room-menu__prompt">
                                <strong>{move || prompt.get_value()}</strong>
                                " This can't be undone."
                            </p>
                            <div class="room-menu__actions">
                                <button
                                    type="button"
                                    class="btn btn--ghost"
                                    node_ref=cancel_button
                                    on:click=move |_| confirming.set(false)
                                >
                                    "Cancel"
                                </button>
                                <button type="button" class="btn btn--danger" on:click=delete>
                                    "Delete"
                                </button>
                            </div>
                        </div>
                    </Show>
                </div>
            </Show>
        </span>
    }
}
