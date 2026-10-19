//! The sheet of a room group: controls for all its members, one row for each, and Customize.

use leptos::prelude::*;
use leptos_router::components::A;
use leptos_router::hooks::use_params_map;
use oikos_api::{AreaView, EntityView};
use oikos_core::model::{Command, GroupKey, GroupKind, Kind};

use crate::api::{set_area_group, set_override};
use crate::cards::group::{CoverButtons, GroupView};
use crate::cards::light::power_command;
use crate::cards::{PowerToggle, Sender};
use crate::components::card_frame::open_entity;
use crate::components::toast::Toasts;
use crate::format::{display_value, is_on};
use crate::groups::{
    cover_text, is_favorite, light_text, light_toggle, mean_brightness, mean_position, members,
    parse_group_path,
};
use crate::house::House;
use crate::icons::kind_icon;

const SAVE_FAILED: &str = "Couldn't save the group";

/// The route `/group/:area/:kind`: the group's sheet, or "This group is gone." when the route
/// names no group (a bad kind, or fewer than two members left).
#[component]
pub fn GroupSheetScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let params = use_params_map();
    let key = Memo::new(move |_| {
        let params = params.read();
        parse_group_path(
            &params.get("area").unwrap_or_default(),
            &params.get("kind").unwrap_or_default(),
        )
    });
    let exists = Memo::new(move |_| {
        key.with(|k| {
            k.as_ref()
                .is_some_and(|k| house.entities.with(|m| !members(m, k).is_empty()))
        })
    });
    let loaded = Memo::new(move |_| {
        house.entities.with(|m| !m.is_empty())
            || matches!(house.connection.get(), oikos_api::ConnectionStatus::Online)
    });

    view! {
        <div class="sheet">
            {move || match key.get() {
                Some(key) if exists.get() => view! { <GroupBody key=key /> }.into_any(),
                _ if loaded.get() => {
                    view! {
                        <header class="sheet__header">
                            <BackLink />
                            <div class="sheet__titles">
                                <h1 class="sheet__title">"This group is gone."</h1>
                            </div>
                        </header>
                    }
                        .into_any()
                }
                _ => view! { <div class="card card--skeleton" aria-busy="true"></div> }.into_any(),
            }}
        </div>
    }
}

/// The back link: Home, which keeps its room.
#[component]
fn BackLink() -> impl IntoView {
    view! {
        <A href="/" attr:class="sheet__back">
            "←"
            <span class="sheet__back-label">"Back"</span>
        </A>
    }
}

fn count_text(kind: GroupKind, n: usize) -> String {
    match kind {
        GroupKind::Cover => format!("{n} shutters"),
        GroupKind::Light => format!("{n} lights"),
    }
}

#[component]
fn GroupBody(key: GroupKey) -> impl IntoView {
    let sender = Sender::from_context();
    let view = GroupView::of_group(&key, sender);
    let GroupView {
        members,
        summary,
        name,
        area_name,
        ..
    } = view;
    let kind = key.kind;
    let icon = kind_icon(
        match kind {
            GroupKind::Cover => Kind::Cover,
            GroupKind::Light => Kind::Light,
        },
        None,
    );
    let heading = match kind {
        GroupKind::Cover => "Shutters",
        GroupKind::Light => "Lights",
    };

    let controls = match kind {
        GroupKind::Cover => {
            let on_change = move |ev: leptos::ev::Event| {
                let Ok(position) = event_target_value(&ev).parse::<u8>() else {
                    return;
                };
                view.send(sender, Command::SetPosition { position });
            };
            let start = members.with_untracked(|ms| mean_position(ms));
            view! {
                <div class="group__line">
                    <span class="card__value">{move || summary.with(cover_text)}</span>
                    <CoverButtons
                        suffix=" all shutters"
                        on_command=Callback::new(move |cmd| view.send(sender, cmd))
                        disabled=Signal::derive(move || summary.with(|s| !s.available))
                    />
                </div>
                <Show when=move || members.with(|ms| ms.iter().any(|m| m.capabilities.position))>
                    <label class="group__slider">
                        <span>"All to"</span>
                        <input
                            class="slider"
                            type="range"
                            min="0"
                            max="100"
                            step="1"
                            aria-label="All to"
                            disabled=move || summary.with(|s| !s.available)
                            prop:value=start.to_string()
                            on:change=on_change
                        />
                    </label>
                </Show>
            }
            .into_any()
        }
        GroupKind::Light => {
            let on_change = move |ev: leptos::ev::Event| {
                let Ok(value) = event_target_value(&ev).parse::<u8>() else {
                    return;
                };
                view.send(
                    sender,
                    Command::TurnOn {
                        brightness: Some(value),
                        color: None,
                        effect: None,
                    },
                );
            };
            let start = members.with_untracked(|ms| mean_brightness(ms));
            let toggle = Callback::new(move |()| {
                view.send(sender, summary.with_untracked(light_toggle));
            });
            view! {
                {move || {
                    let (on, text, available) = summary
                        .with(|s| (s.on > 0, light_text(s), s.available));
                    view! {
                        <PowerToggle
                            on=on
                            label=text
                            on_toggle=toggle
                            aria_label="All lights"
                            disabled=!available
                        />
                    }
                }}
                <Show when=move || members.with(|ms| ms.iter().any(|m| m.capabilities.brightness))>
                    <label class="group__slider">
                        <span>"Brightness"</span>
                        <input
                            class="slider"
                            type="range"
                            min="1"
                            max="255"
                            step="1"
                            aria-label="Brightness"
                            disabled=move || summary.with(|s| !s.available)
                            prop:value=start.to_string()
                            on:change=on_change
                        />
                    </label>
                </Show>
            }
            .into_any()
        }
    };

    let rows = move || {
        members.with(|ms| {
            ms.iter()
                .map(|m| member_row(sender, m.clone()))
                .collect_view()
        })
    };

    view! {
        <header class="sheet__header">
            <BackLink />
            <span class="sheet__icon" inner_html=icon></span>
            <div class="sheet__titles">
                <h1 class="sheet__title">{move || name.get()}</h1>
                <p class="sheet__subtitle">
                    {move || area_name.get()} " · "
                    {move || count_text(kind, members.with(Vec::len))}
                </p>
            </div>
        </header>
        <section class="sheet__panel">
            <h2 class="sheet__heading">"Controls"</h2>
            <div class="sheet__controls">{controls}</div>
        </section>
        <section class="sheet__panel">
            <h2 class="sheet__heading">{heading}</h2>
            <ul class="group__members">{rows}</ul>
        </section>
        <Customize key=key members=members />
    }
}

/// One member: its name (opens its sheet), its state and its own controls.
fn member_row(sender: Sender, member: EntityView) -> impl IntoView {
    let open = open_entity(&member.id);
    let state = display_value(&member.state, member.unit.as_deref());
    let controls = match member.kind {
        Kind::Cover => {
            let id = member.id.clone();
            let suffix = format!(" {}", member.name);
            view! {
                <CoverButtons
                    suffix=suffix
                    on_command=Callback::new(move |cmd| sender.send(&id, cmd, None))
                />
            }
            .into_any()
        }
        _ => {
            let id = member.id.clone();
            let on = is_on(&member.state);
            let toggle = Callback::new(move |()| sender.send(&id, power_command(!on), None));
            view! { <PowerToggle on=on label="" on_toggle=toggle aria_label=member.name.clone() /> }
                .into_any()
        }
    };
    view! {
        <li class="group__member">
            <button type="button" class="group__member-name" on:click=move |_| open.run(())>
                <span>{member.name.clone()}</span>
                <span class="group__member-state">{state}</span>
            </button>
            {controls}
        </li>
    }
}

/// The custom name of `key` in `areas`, if the room has one.
fn custom_name(areas: &[AreaView], key: &GroupKey) -> String {
    areas
        .iter()
        .find(|a| a.id == key.area)
        .and_then(|a| a.group_names.get(key.kind.key()))
        .cloned()
        .unwrap_or_default()
}

/// Rename, Favorites and membership of the group.
#[component]
fn Customize(key: GroupKey, members: Memo<Vec<EntityView>>) -> impl IntoView {
    let house = expect_context::<House>();
    let toasts = expect_context::<Toasts>();
    let draft = RwSignal::new(house.areas.with_untracked(|a| custom_name(a, &key)));
    let favorite = {
        let key = key.clone();
        Memo::new(move |_| house.areas.with(|a| is_favorite(a, &key)))
    };
    let default_name = key.kind.default_name();

    let save_group = {
        let key = key.clone();
        move |name: Option<Option<String>>, favorite: Option<bool>| {
            let key = key.clone();
            leptos::task::spawn_local(async move {
                let name = name.as_ref().map(Option::as_deref);
                match set_area_group(&key.area, key.kind.key(), name, favorite).await {
                    Ok(areas) => house.areas.set(areas),
                    Err(_) => toasts.push(SAVE_FAILED),
                }
            });
        }
    };

    let save_name = {
        let save_group = save_group.clone();
        move |_| {
            let name = draft.get_untracked();
            let name = name.trim();
            // Like the app: saving the name the group already has does nothing.
            if name == house.areas.with_untracked(|a| custom_name(a, &key)).trim() {
                return;
            }
            save_group(Some((!name.is_empty()).then(|| name.to_owned())), None);
        }
    };
    let toggle_favorite = {
        let save_group = save_group.clone();
        Callback::new(move |()| save_group(None, Some(!favorite.get_untracked())))
    };

    // Takes `ids` out of the group, one request each; the hub's echo folds the sheet.
    // `failed` runs when a request is refused, so a checkbox can tick itself back.
    let ungroup = move |ids: Vec<String>, failed: Callback<()>| {
        leptos::task::spawn_local(async move {
            for id in ids {
                let mut patch = oikos_api::OverridePatch::new();
                patch.insert("ungrouped".to_owned(), serde_json::json!(true));
                match set_override(&id, patch).await {
                    Ok(view) => house.entities.update(|m| {
                        m.insert(view.id.clone(), view);
                    }),
                    Err(_) => {
                        toasts.push(SAVE_FAILED);
                        failed.run(());
                        return;
                    }
                }
            }
        });
    };

    let checkboxes = move || {
        members.with(|ms| {
            ms.iter()
                .map(|m| {
                    let id = m.id.clone();
                    let ticked = RwSignal::new(true);
                    view! {
                        <li>
                            <label class="group__check">
                                <input
                                    type="checkbox"
                                    prop:checked=move || ticked.get()
                                    on:change=move |_| {
                                        ticked.set(false);
                                        ungroup(vec![id.clone()], Callback::new(move |()| ticked.set(true)));
                                    }
                                />
                                <span>{m.name.clone()}</span>
                            </label>
                        </li>
                    }
                })
                .collect_view()
        })
    };

    view! {
        <section class="sheet__panel">
            <h2 class="sheet__heading">"Customize"</h2>
            <div class="group__rename">
                <input
                    class="field"
                    type="text"
                    aria-label="Group name"
                    placeholder=default_name
                    prop:value=move || draft.get()
                    on:input=move |ev| draft.set(event_target_value(&ev))
                />
                <button type="button" class="btn" on:click=save_name>
                    "Save"
                </button>
            </div>
            {move || {
                view! {
                    <PowerToggle
                        on=favorite.get()
                        label="Show in Favorites"
                        on_toggle=toggle_favorite
                    />
                }
            }}
            <h3 class="sheet__subheading">"In this group"</h3>
            <ul class="group__members">{checkboxes}</ul>
            <button
                type="button"
                class="btn"
                on:click=move |_| ungroup(members.with_untracked(|ms| ms.iter().map(|m| m.id.clone()).collect()), Callback::new(|()| {}))
            >
                "Ungroup all"
            </button>
        </section>
    }
}
