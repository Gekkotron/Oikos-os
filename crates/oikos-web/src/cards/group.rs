//! Group cards: a room's covers or lights as one wide card on Home.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::{Command, GroupKey, GroupKind, Kind, StateValue};

use serde_json::{Map, Value};
use std::collections::BTreeMap;

use super::climate::stepped;
use super::light::{power_command, read_on, with_on};
use super::{ModeSelect, PowerToggle, Sender, Stepper};
use crate::components::card_frame::{CardFrame, GROUP_PREFIX, open_entity};
use crate::format::{degrees, word};
use crate::groups::{
    GroupSummary, cover_text, group_name, group_path, light_text, light_toggle, members, summary,
    targets,
};
use crate::house::House;
use crate::icons::{CHEVRON_DOWN, CHEVRON_UP, STOP, kind_icon};

/// What a group card reads from the house, each part behind its own memo: `members` and
/// `summary` only notify when they change, so a state echo repaints only the card whose group
/// it touches.
#[derive(Clone, Copy)]
pub(crate) struct GroupView {
    pub(crate) members: Memo<Vec<EntityView>>,
    pub(crate) summary: Memo<GroupSummary>,
    pub(crate) name: Memo<String>,
    pub(crate) area_name: Memo<String>,
    pub(crate) pending: Signal<bool>,
}

impl GroupView {
    /// A group card's view: the `ids` it folds, which under Favorites can be a part of the room's
    /// group.
    pub(crate) fn of_ids(key: &GroupKey, ids: Vec<String>, sender: Sender) -> Self {
        let house = expect_context::<House>();
        let members = Memo::new(move |_| {
            house.entities.with(|m| {
                ids.iter()
                    .filter_map(|id| m.get(id).cloned())
                    .collect::<Vec<_>>()
            })
        });
        Self::new(key, members, sender)
    }

    /// The whole room group, as its sheet shows it.
    pub(crate) fn of_group(key: &GroupKey, sender: Sender) -> Self {
        let house = expect_context::<House>();
        let members = {
            let key = key.clone();
            Memo::new(move |_| house.entities.with(|m| members(m, &key)))
        };
        Self::new(key, members, sender)
    }

    fn new(key: &GroupKey, members: Memo<Vec<EntityView>>, sender: Sender) -> Self {
        let house = expect_context::<House>();
        let summary =
            Memo::new(move |_| members.with(|ms| house.entities.with(|m| summary(ms, m))));
        let name = {
            let key = key.clone();
            Memo::new(move |_| house.areas.with(|a| group_name(a, &key)))
        };
        let area_name = {
            let area = key.area.clone();
            Memo::new(move |_| {
                house.areas.with(|a| {
                    a.iter()
                        .find(|x| x.id == area)
                        .map_or_else(|| area.clone(), |x| x.name.clone())
                })
            })
        };
        let pending = Signal::derive(move || {
            members.with(|ms| sender.any_pending(ms.iter().map(|e| e.id.as_str())))
        });
        Self {
            members,
            summary,
            name,
            area_name,
            pending,
        }
    }

    /// Sends `cmd` to the members that can take it.
    pub(crate) fn send(&self, sender: Sender, cmd: Command) {
        let ids = self.members.with_untracked(|ms| targets(ms, &cmd));
        sender.send_group(&ids, cmd);
    }
}

/// The shell of a group card: icon, name, `×N` badge and status dot in a header that opens the
/// group sheet, the body below.
#[component]
fn GroupFrame(
    kind: Kind,
    key: GroupKey,
    view: GroupView,
    #[prop(into)] label: Signal<String>,
    children: Children,
) -> impl IntoView {
    let navigate = leptos_router::hooks::use_navigate();
    let path = group_path(&key);
    let icon = kind_icon(kind, None);
    let GroupView {
        members,
        summary,
        name,
        pending,
        ..
    } = view;
    view! {
        <article
            class="card card--wide"
            class:card--on=move || summary.with(|s| s.on > 0)
            class:card--offline=move || summary.with(|s| !s.available)
            class:card--pending=move || pending.get()
            aria-label=move || label.get()
        >
            <button
                type="button"
                class="card__header"
                on:click=move |_| navigate(&path, Default::default())
            >
                <span class="card__icon" inner_html=icon></span>
                <span class="card__name">{move || name.get()}</span>
                <span class="group__count">{move || format!("×{}", members.with(Vec::len))}</span>
                <span class="card__dots">
                    <span class="dot dot--pending" class:dot--active=move || pending.get()></span>
                </span>
            </button>
            <div class="card__body">{children()}</div>
        </article>
    }
}

/// The accessible name of a group card, "Shutters, Salon, 2 open, 1 closed".
fn card_label(view: GroupView, state: fn(&GroupSummary) -> String) -> Signal<String> {
    Signal::derive(move || {
        format!(
            "{}, {}, {}",
            view.name.get(),
            view.area_name.get(),
            view.summary.with(state)
        )
    })
}

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

/// A room's shutters as one card: how many are open, ▲ ■ ▼ for all of them, a bar for each.
#[component]
pub fn CoverGroupCard(key: GroupKey, members: Vec<String>) -> impl IntoView {
    debug_assert_eq!(key.kind, GroupKind::Cover);
    let sender = Sender::from_context();
    let view = GroupView::of_ids(&key, members, sender);
    let summary = view.summary;
    let label = card_label(view, |s| {
        format!("{} open, {} closed", s.on, s.total - s.on)
    });

    let bars = move || {
        summary.with(|s| {
            s.positions
                .iter()
                .map(|p| {
                    view! {
                        <span class="group__bar">
                            <span class="group__fill" style:height=format!("{p}%")></span>
                        </span>
                    }
                })
                .collect_view()
        })
    };
    view! {
        <GroupFrame kind=Kind::Cover key=key view=view label=label>
            <div class="group__row">
                <div class="group__status">
                    <span class="card__value">{move || summary.with(cover_text)}</span>
                    <div class="group__bars">{bars}</div>
                </div>
                <CoverButtons
                    suffix=" all shutters"
                    on_command=Callback::new(move |cmd| view.send(sender, cmd))
                    disabled=Signal::derive(move || summary.with(|s| !s.available))
                />
            </div>
        </GroupFrame>
    }
}

/// A room's lights as one card: how many are on and a switch for all of them.
#[component]
pub fn LightGroupCard(key: GroupKey, members: Vec<String>) -> impl IntoView {
    debug_assert_eq!(key.kind, GroupKind::Light);
    let sender = Sender::from_context();
    let view = GroupView::of_ids(&key, members, sender);
    let summary = view.summary;
    let label = card_label(view, |s| format!("{} of {} on", s.on, s.total));

    let toggle = Callback::new(move |()| {
        view.send(sender, summary.with_untracked(light_toggle));
    });
    view! {
        <GroupFrame kind=Kind::Light key=key view=view label=label>
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
        </GroupFrame>
    }
}

/// The accessible name and label of a group's press button.
pub const PRESS_ALL: &str = "Press all";

/// What a group of switches says: "All on" / "All off", or "N of M on" when `counts`
/// (`on`, `total`) shows the members disagree. Without counts the group's own state decides.
pub fn switch_text(any_on: bool, counts: Option<(usize, usize)>) -> String {
    match counts {
        Some((on, total)) if on > 0 && on < total => format!("{on} of {total} on"),
        Some((0, _)) => "All off".to_owned(),
        Some(_) => "All on".to_owned(),
        None if any_on => "All on".to_owned(),
        None => "All off".to_owned(),
    }
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
    let label = switch_text(on, counts);
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
            <PowerToggle on=on label=label on_toggle=toggle />
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
    let id = entity.id.clone();
    let text = {
        let id = id.clone();
        let fallback = crate::format::display_value(&entity.state, entity.unit.as_deref());
        move || house_summary(&id).map_or_else(|| fallback.clone(), |s| cover_text(&s))
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
                |s| (s.on > 0, light_text(&s), s.available),
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
