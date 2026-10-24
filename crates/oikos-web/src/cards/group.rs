//! Group cards: a room's covers or lights as one wide card on Home.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::{Command, StateValue};

use serde_json::{Map, Value};
use std::collections::BTreeMap;

use super::climate::stepped;
use super::light::{power_command, read_on, with_on};
use super::{ModeSelect, PowerToggle, Sender, Stepper};
use crate::components::card_frame::{CardFrame, GROUP_PREFIX, open_entity};
use crate::format::{degrees, word};
use crate::groups::{GroupSummary, cover_text, light_text, summary};
use crate::house::House;
use crate::i18n::{GroupState, Lang, group_state, use_lang};
use crate::icons::{CHEVRON_DOWN, CHEVRON_UP, STOP};

/// ▲ ■ ▼ as a row of buttons sending Open, Stop and Close to `on_command`. `suffix` ends their
/// accessible names ("Open" + " all shutters").
#[component]
pub fn CoverButtons(
    #[prop(into)] suffix: String,
    on_command: Callback<Command>,
    /// Greys the buttons out (a group with no member online).
    #[prop(optional, into)]
    disabled: Signal<bool>,
) -> impl IntoView {
    let button = move |text: &'static str, icon: &'static str, cmd: Command| {
        let label = format!("{text}{suffix}");
        view! {
            <button
                type="button"
                class="btn btn--icon"
                aria-label=label.clone()
                title=label
                disabled=move || disabled.get()
                inner_html=icon
                on:click=move |_| on_command.run(cmd.clone())
            ></button>
        }
    };
    view! {
        <div class="card__controls">
            {button("Open", CHEVRON_UP, Command::Open)}
            {button("Stop", STOP, Command::Stop)}
            {button("Close", CHEVRON_DOWN, Command::Close)}
        </div>
    }
}

/// The accessible name and label of a group's press button.
pub const PRESS_ALL: &str = "Press all";

/// What a group of switches says: all on / all off, or how many are on when `counts`
/// (`on`, `total`) shows the members disagree. Without counts the group's own state decides.
pub fn switch_text(lang: Lang, any_on: bool, counts: Option<(usize, usize)>) -> String {
    let state = match counts {
        Some((on, total)) if on > 0 && on < total => GroupState::SomeOn { on, total },
        Some((0, _)) => GroupState::AllOff,
        Some(_) => GroupState::AllOn,
        None if any_on => GroupState::AllOn,
        None => GroupState::AllOff,
    };
    group_state(lang, state)
}

fn plain(v: f64) -> String {
    if v.fract() == 0.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    }
}

/// The `range` the hub publishes next to a thermostat group's mean target when members'
/// targets differ.
pub fn range_of(attributes: &Map<String, Value>) -> Option<[f64; 2]> {
    let range = attributes.get("range")?.as_array()?;
    match range.as_slice() {
        [low, high] => Some([low.as_f64()?, high.as_f64()?]),
        _ => None,
    }
}

/// The current-value line of a thermostat group: the members' target range ("19–21 °C") when
/// they differ, else the shared mode, else "Mixed".
pub fn thermostat_text(range: Option<[f64; 2]>, mode: Option<&str>, unit: Option<&str>) -> String {
    if let Some([low, high]) = range {
        let unit = unit.filter(|u| !u.is_empty()).unwrap_or("°C");
        return format!("{}–{} {unit}", plain(low), plain(high));
    }
    mode.map_or_else(|| "Mixed".to_owned(), word)
}

/// How many of the group `entity_id`'s present members satisfy `pred`, with how many there
/// are. `None` while the group list has not loaded.
fn member_counts(entity_id: &str, pred: impl Fn(&EntityView) -> bool) -> Option<(usize, usize)> {
    let house = use_context::<House>()?;
    let group = entity_id.strip_prefix(GROUP_PREFIX)?;
    let ids: Vec<String> = house.groups.with_untracked(|g| {
        g.get(group)
            .map(|g| g.members.iter().map(|m| m.id.clone()).collect())
    })?;
    house.entities.with_untracked(|m| {
        let present: Vec<&EntityView> = ids.iter().filter_map(|id| m.get(id)).collect();
        Some((present.iter().filter(|e| pred(e)).count(), present.len()))
    })
}

/// A switch group: one toggle for every member, "All on", "All off" or "N of M on".
#[component]
pub fn SwitchGroupCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let on = read_on(&entity.state).unwrap_or(false);
    let counts = member_counts(&entity.id, |e| read_on(&e.state).unwrap_or(false));
    let lang = use_lang();
    let toggle = {
        let id = entity.id.clone();
        let state = entity.state.clone();
        Callback::new(move |()| {
            sender.send(&id, power_command(!on), with_on(&state, !on));
        })
    };
    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            {move || {
                view! { <PowerToggle on=on label=switch_text(lang(), on, counts) on_toggle=toggle /> }
            }}
        </CardFrame>
    }
}

/// A climate group: the members' target range or shared mode, a target stepper and a mode
/// select, all sent to the group's entity.
#[component]
pub fn ClimateGroupCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let (target, mode) = match &entity.state.value {
        StateValue::Climate(c) => (c.target, c.mode.clone()),
        _ => (None, None),
    };
    let unit = entity.unit.as_deref();
    let now_text = thermostat_text(range_of(&entity.state.attributes), mode.as_deref(), unit);
    let target_text = degrees(target, unit);
    let caps = &entity.capabilities;

    let on_step = {
        let id = entity.id.clone();
        let (step, min, max) = (caps.temp_step, caps.min_temp, caps.max_temp);
        Callback::new(move |direction: f64| {
            let Some(temperature) = stepped(target, step, direction, min, max) else {
                return;
            };
            sender.send(&id, Command::SetTemperature { temperature }, None);
        })
    };
    let mode_select = (!caps.hvac_modes.is_empty()).then(|| {
        let id = entity.id.clone();
        let on_select = Callback::new(move |picked: String| {
            sender.send(&id, Command::SetHvacMode { mode: picked }, None);
        });
        view! { <ModeSelect label="Mode" options=caps.hvac_modes.clone() selected=mode on_select=on_select captioned=true /> }
    });

    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <div class="thermo">
                <span class="thermo__now">{now_text}</span>
                <Stepper label=target_text caption="Target" can_step=target.is_some() on_step=on_step />
                <div class="thermo__picks">{mode_select}</div>
            </div>
        </CardFrame>
    }
}

/// A water heater group. The single-entity body already shows the aggregated target and a
/// blank mode when members disagree, so it is reused as is.
#[component]
pub fn WaterHeaterGroupCard(entity: EntityView) -> impl IntoView {
    view! { <super::water_heater::WaterHeaterCard entity=entity /> }
}

/// A number group: the shared or mean value on the single-entity slider.
#[component]
pub fn NumberGroupCard(entity: EntityView) -> impl IntoView {
    view! { <super::number::NumberCard entity=entity /> }
}

/// A vacuum group: the aggregated activity on the single-entity vacuum body.
#[component]
pub fn VacuumGroupCard(entity: EntityView) -> impl IntoView {
    view! { <super::vacuum::VacuumCard entity=entity /> }
}

/// A select group: the shared option picked, or "Mixed" when members differ.
#[component]
pub fn SelectGroupCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let options = entity.capabilities.options.clone();
    let current = match &entity.state.value {
        StateValue::Text(s) if options.contains(s) => Some(s.clone()),
        _ => None,
    };
    let mixed = current
        .is_none()
        .then(|| view! { <span class="card__value">"Mixed"</span> });
    let on_select = {
        let id = entity.id.clone();
        Callback::new(move |option: String| {
            sender.send(&id, Command::SelectOption { option }, None);
        })
    };
    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            {mixed}
            <div class="card__controls card__controls--stack">
                <ModeSelect label="Option" options=options selected=current on_select=on_select />
            </div>
        </CardFrame>
    }
}

/// A button group: one "Press all" button.
#[component]
pub fn ButtonGroupCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let on_press = {
        let id = entity.id.clone();
        move |_| sender.send(&id, Command::Press, None)
    };
    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <div class="card__controls">
                <button type="button" class="btn" on:click=on_press>
                    {PRESS_ALL}
                </button>
            </div>
        </CardFrame>
    }
}

/// The summary of hand-made group `group` (its id without the `group_` prefix) over its
/// members' own states; `None` while the group is not in `groups`.
pub fn group_summary(
    group: &str,
    groups: &BTreeMap<String, oikos_api::GroupView>,
    entities: &BTreeMap<String, EntityView>,
) -> Option<GroupSummary> {
    let group = groups.get(group)?;
    let members: Vec<EntityView> = group
        .members
        .iter()
        .filter_map(|m| entities.get(&m.id).cloned())
        .collect();
    Some(summary(&members, entities))
}

/// The group's summary read from the house, tracked.
fn house_summary(entity_id: &str) -> Option<GroupSummary> {
    let house = use_context::<House>()?;
    let group = entity_id.strip_prefix(GROUP_PREFIX)?;
    house
        .groups
        .with(|g| house.entities.with(|e| group_summary(group, g, e)))
}

/// A cover group: "All open", "All closed", "N open · M closed" or "Opening…" / "Closing…"
/// from its members, with ▲ ■ ▼ sent to the group's entity.
#[component]
pub fn CoverGroupEntityCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let lang = use_lang();
    let id = entity.id.clone();
    let text = {
        let id = id.clone();
        let fallback = crate::format::display_value(&entity.state, entity.unit.as_deref());
        move || house_summary(&id).map_or_else(|| fallback.clone(), |s| cover_text(lang(), &s))
    };
    let on_command = {
        let id = id.clone();
        Callback::new(move |cmd| sender.send(&id, cmd, None))
    };
    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <span class="card__value">{text}</span>
            <CoverButtons suffix="" on_command=on_command />
        </CardFrame>
    }
}

/// A light group: "All on", "All off" or "N of M on" from its members, one toggle and the
/// brightness slider when the members dim.
#[component]
pub fn LightGroupEntityCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let lang = use_lang();
    let id = entity.id.clone();
    let on_now = read_on(&entity.state).unwrap_or(false);
    let toggle = {
        let id = id.clone();
        Callback::new(move |()| {
            let on = house_summary(&id).map_or(on_now, |s| s.on > 0);
            sender.send(&id, power_command(!on), None);
        })
    };
    let toggle_view = {
        let id = id.clone();
        let fallback = crate::format::display_value(&entity.state, entity.unit.as_deref());
        move || {
            let (on, label, available) = house_summary(&id).map_or_else(
                || (on_now, fallback.clone(), true),
                |s| (s.on > 0, light_text(lang(), &s), s.available),
            );
            view! { <PowerToggle on=on label=label on_toggle=toggle disabled=!available /> }
        }
    };
    let slider = entity.capabilities.brightness.then(|| {
        let id = id.clone();
        let level = match &entity.state.value {
            StateValue::Light(l) => l.brightness.unwrap_or(255),
            _ => 255,
        };
        let on_change = move |ev: leptos::ev::Event| {
            let Ok(value) = event_target_value(&ev).parse::<u8>() else {
                return;
            };
            let cmd = Command::TurnOn {
                brightness: Some(value),
                color: None,
                effect: None,
            };
            sender.send(&id, cmd, None);
        };
        view! {
            <input
                class="slider"
                type="range"
                min="1"
                max="255"
                step="1"
                aria-label="Brightness"
                prop:value=level.to_string()
                on:change=on_change
            />
        }
    });
    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            {toggle_view}
            <div class="card__controls">{slider}</div>
        </CardFrame>
    }
}
