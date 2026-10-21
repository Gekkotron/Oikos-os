//! The shell every entity card shares: icon, name, status dots and a tap target.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::Availability;

use crate::house::House;
use crate::icons::entity_icon;

/// The prefix of the id of a hand-made group's entity.
pub const GROUP_PREFIX: &str = "group_";

/// The path of the sheet that opens for entity `id`: a group's own sheet for `group_<id>`.
pub fn sheet_path(id: &str) -> String {
    match id.strip_prefix(GROUP_PREFIX) {
        Some(group) => format!("/group/{}", crate::api::encode_id(group)),
        None => format!("/entity/{}", crate::api::encode_id(id)),
    }
}

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
    let path = sheet_path(id);
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
    // A group's entity spans like the derived group card and carries its `×N` badge.
    let group = entity.id.strip_prefix(GROUP_PREFIX).map(str::to_owned);
    let is_group = group.is_some();
    let count = Signal::derive(move || {
        let group = group.as_deref()?;
        use_context::<House>()?
            .groups
            .with(|g| g.get(group).map(|g| g.members.len()))
    });

    view! {
        <article
            class="card"
            class:card--wide=is_group
            class:card--on=is_on
            class:card--offline=offline
            class:card--pending=move || pending.get()
            class:card--alert=alert
        >
            <button type="button" class="card__header" on:click=move |_| on_click.run(())>
                <span class="card__icon" inner_html=icon></span>
                <span class="card__name" title=full_name>{short}</span>
                {move || {
                    count.get().map(|n| view! { <span class="group__count">{format!("×{n}")}</span> })
                }}
                <span class="card__dots">
                    <span class="dot dot--pending" class:dot--active=move || pending.get()></span>
                    <span class=availability_class(availability)></span>
                </span>
            </button>
            <div class="card__body">{children()}</div>
        </article>
    }
}
