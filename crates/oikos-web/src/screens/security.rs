//! The Security screen: alarm panels, actions, the summary line and the sensor sections.

use leptos::prelude::*;
use leptos_router::components::A;
use oikos_core::model::Kind;

use crate::cards::Sender;
use crate::components::alarm_hero::AlarmHero;
use crate::components::alert_row::RecentAlerts;
use crate::house::House;
use crate::icons::{PLAY, PLUG, sensor_icon};
use crate::security::{ActionRow, SecurityUi, SensorRow};

fn entity_href(id: &str) -> String {
    format!("/entity/{}", crate::api::encode_id(id))
}

#[component]
fn SensorTile(row: SensorRow) -> impl IntoView {
    let class = format!(
        "tile{}{}{}",
        if row.danger_active() {
            " tile--danger"
        } else {
            ""
        },
        if row.intrusion_active() {
            " tile--intrusion"
        } else {
            ""
        },
        if row.available { "" } else { " tile--off" },
    );
    let label = row.aria_label();
    let icon = sensor_icon(row.device_class.as_deref());
    view! {
        <A href=entity_href(&row.id) attr:class=class attr:aria-label=label>
            <span class="tile__line">
                <span class="tile__icon" inner_html=icon></span>
                <span class="tile__name">{row.name.clone()}</span>
            </span>
            <span class="tile__line">
                <span class="tile__area">
                    {row.area_name.clone().unwrap_or_else(|| "Unassigned".to_owned())}
                </span>
                <span class="tile__state">{row.state_word()}</span>
            </span>
        </A>
    }
}

#[component]
fn ActionTile(row: ActionRow, confirming: RwSignal<Option<String>>) -> impl IntoView {
    let sender = Sender::from_context();
    let id = row.id.clone();
    let tap = row.tap();
    let is_button = row.kind == Kind::Button;
    let verb = row.verb();
    let question = row.question();
    let on = row.available && row.on == Some(true);
    let word = row.state_word();
    let tap_label = match word {
        Some(w) => format!("{}, {}", row.name, w.to_lowercase()),
        None => row.name.clone(),
    };
    let class = format!(
        "tile tile--action{}{}",
        if on { " tile--active" } else { "" },
        if row.available { "" } else { " tile--off" },
    );
    let command = tap.command.clone();
    let ask = tap.confirm;
    let send_id = id.clone();
    let on_tap = move |_| {
        if ask {
            confirming.set(Some(send_id.clone()));
        } else {
            sender.send(&send_id, command.clone(), None);
        }
    };
    let confirm_command = tap.command;
    let confirm_id = id.clone();
    let is_confirming = {
        let id = id.clone();
        move || confirming.with(|c| c.as_deref() == Some(id.as_str()))
    };
    let normal_class = class.clone();
    let normal = move || {
        let icon = if is_button { PLAY } else { PLUG };
        let name = row.name.clone();
        let href = entity_href(&row.id);
        let area = row
            .area_name
            .clone()
            .unwrap_or_else(|| "Unassigned".to_owned());
        view! {
            <div class=normal_class.clone()>
                <button
                    type="button"
                    class="tile__tap"
                    aria-label=tap_label.clone()
                    disabled=!row.available
                    on:click=on_tap.clone()
                ></button>
                <span class="tile__line">
                    <span class="tile__icon" inner_html=icon></span>
                    <A href=href attr:class="tile__name tile__link">
                        {name}
                    </A>
                </span>
                <span class="tile__line">
                    <span class="tile__area">{area}</span>
                    {word.map(|w| view! { <span class="tile__state">{w}</span> })}
                </span>
            </div>
        }
    };
    let confirm = move || {
        let command = confirm_command.clone();
        let id = confirm_id.clone();
        view! {
            <div class="tile tile--action tile--confirm" role="group" aria-label=question.clone()>
                <span class="tile__question">{question.clone()}</span>
                <span class="tile__buttons">
                    <button
                        type="button"
                        class="btn btn--compact"
                        on:click=move |_| {
                            confirming.set(None);
                            sender.send(&id, command.clone(), None);
                        }
                    >
                        {verb}
                    </button>
                    <button
                        type="button"
                        class="btn btn--compact"
                        on:click=move |_| confirming.set(None)
                    >
                        "Cancel"
                    </button>
                </span>
            </div>
        }
    };
    view! { <Show when=is_confirming fallback=normal.clone()>{confirm.clone()}</Show> }
}

#[component]
pub fn SecurityScreen() -> impl IntoView {
    let house = expect_context::<House>();
    let ui = Memo::new(move |_| {
        house
            .entities
            .with(|m| house.areas.with(|a| SecurityUi::build(m, a)))
    });
    let loaded = Memo::new(move |_| {
        house.areas.with(|a| !a.is_empty())
            || house.entities.with(|m| !m.is_empty())
            || matches!(house.connection.get(), oikos_api::ConnectionStatus::Online)
    });

    // The id of the action asking "Turn on X?"; it closes when the entity disappears or
    // becomes unavailable.
    let confirming = RwSignal::new(None::<String>);
    Effect::new(move |_| {
        if let Some(id) = confirming.get() {
            let usable = ui.with(|u| u.actions.iter().any(|a| a.id == id && a.available));
            if !usable {
                confirming.set(None);
            }
        }
    });

    let panels = move || ui.with(|u| u.panels.clone());
    let actions = move || ui.with(|u| u.actions.clone());
    let sections = move || ui.with(|u| u.sections.clone());
    let empty = move || ui.with(|u| u.is_empty());
    let has_sensors = move || ui.with(|u| !u.sensors.is_empty());
    let has_panels = move || ui.with(|u| !u.panels.is_empty());
    let has_actions = move || ui.with(|u| !u.actions.is_empty());

    view! {
        <Show
            when=move || loaded.get()
            fallback=|| {
                view! {
                    <div class="grid" aria-busy="true">
                        <div class="card card--skeleton"></div>
                        <div class="card card--skeleton"></div>
                    </div>
                }
            }
        >
            <section class="security">
                <h1 class="room__title">"Security"</h1>
                <Show
                    when=move || !empty()
                    fallback=|| view! { <p class="room__empty">"Nothing on Security yet."</p> }
                >
                    <Show
                        when=has_panels
                        fallback=|| {
                            view! {
                                <p class="card security__note">
                                    "No alarm panel. Publish an MQTT "
                                    <code>"alarm_control_panel"</code>
                                    " to see it here."
                                </p>
                            }
                        }
                    >
                        <For
                            each=panels
                            key=|p| format!("{p:?}")
                            children=|panel| view! { <AlarmHero panel=panel /> }
                        />
                    </Show>
                    <Show when=has_actions>
                        <h2 class="section-title">"Actions"</h2>
                        <div class="tiles">
                            <For
                                each=actions
                                key=|a| format!("{a:?}")
                                children=move |row| {
                                    view! { <ActionTile row=row confirming=confirming /> }
                                }
                            />
                        </div>
                    </Show>
                    <Show
                        when=has_sensors
                        fallback=|| view! { <p class="security__note">"No security sensors."</p> }
                    >
                        <p class="summary" aria-live="polite">
                            {move || ui.with(|u| u.summary_text())}
                        </p>
                        <For
                            each=sections
                            key=|s| format!("{s:?}")
                            children=|section| {
                                view! {
                                    <h2 class="section-title">{section.section.title()}</h2>
                                    <div class="tiles">
                                        {section
                                            .rows
                                            .into_iter()
                                            .map(|row| view! { <SensorTile row=row /> })
                                            .collect_view()}
                                    </div>
                                }
                            }
                        />
                    </Show>
                </Show>
                <RecentAlerts />
            </section>
        </Show>
    }
}
