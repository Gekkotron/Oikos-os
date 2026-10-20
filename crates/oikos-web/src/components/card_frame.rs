//! The shell every entity card shares: icon, name, status dots and a tap target.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::Availability;

use crate::icons::entity_icon;

fn availability_class(availability: Availability) -> &'static str {
    match availability {
        Availability::Online => "dot dot--online",
        Availability::Offline => "dot dot--offline",
        Availability::Unknown => "dot dot--unknown",
    }
}

/// A callback that opens the detail sheet of entity `id`. Call it while building a component
/// (it needs the router context).
pub fn open_entity(id: &str) -> Callback<()> {
    let navigate = leptos_router::hooks::use_navigate();
    let path = format!("/entity/{}", crate::api::encode_id(id));
    Callback::new(move |()| navigate(&path, Default::default()))
}

/// A card for one entity. `on_click` fires from the header tap target; `children` is the body
/// (value, controls). `pending` shows a pulsing dot while a command awaits its state echo.
#[component]
pub fn CardFrame(
    entity: EntityView,
    on_click: Callback<()>,
    #[prop(into, optional)] pending: Signal<bool>,
    /// Replaces the title computed from the entity; empty keeps the computed one.
    #[prop(into, optional)]
    title: String,
    /// Draws the card in the alarm colour (a ringing siren).
    #[prop(optional)]
    alert: bool,
    children: Children,
) -> impl IntoView {
    let icon = entity_icon(&entity);
    let availability = entity.state.availability;
    let is_on = crate::format::is_on(&entity.state);
    let title = if title.is_empty() {
        crate::format::entity_title(&entity)
    } else {
        title
    };
    let short = crate::format::wrap_id(&title);
    let full_name = entity.name.clone();
    let offline = availability == Availability::Offline;

    view! {
        <article
            class="card"
            class:card--on=is_on
            class:card--offline=offline
            class:card--pending=move || pending.get()
            class:card--alert=alert
        >
            <button type="button" class="card__header" on:click=move |_| on_click.run(())>
                <span class="card__icon" inner_html=icon></span>
                <span class="card__name" title=full_name>{short}</span>
                <span class="card__dots">
                    <span class="dot dot--pending" class:dot--active=move || pending.get()></span>
                    <span class=availability_class(availability)></span>
                </span>
            </button>
            <div class="card__body">{children()}</div>
        </article>
    }
}
