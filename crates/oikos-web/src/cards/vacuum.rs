//! The vacuum card: the activity with the battery level, one primary start / pause button, an
//! icon button per other supported action and a fan speed select.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::{Command, EntityState, StateValue, VacuumState};

use super::{ModeSelect, Sender};
use crate::components::card_frame::{CardFrame, open_entity};
use crate::format::{display_value, word};
use crate::icons::vacuum_action_icon;

/// The short button label for a vacuum action.
fn action_label(action: &str) -> String {
    match action {
        "return_to_base" => "Dock".into(),
        "clean_spot" => "Spot".into(),
        other => word(other),
    }
}

/// The primary action for the current activity and its label: pause while cleaning, resume
/// when paused, start otherwise. `None` when the vacuum supports neither.
fn primary_action(activity: Option<&str>, actions: &[String]) -> Option<(&'static str, String)> {
    let has = |a: &str| actions.iter().any(|x| x == a);
    match activity {
        Some("cleaning") if has("pause") => Some(("pause", "Pause".into())),
        Some("paused") if has("start") => Some(("start", "Resume".into())),
        _ if has("start") => Some(("start", "Start".into())),
        _ => None,
    }
}

/// The modifier colouring the activity: busy, heading home, failed or idle.
fn activity_class(activity: Option<&str>) -> &'static str {
    match activity {
        Some("cleaning") => "vacuum__state vacuum__state--busy",
        Some("returning") => "vacuum__state vacuum__state--returning",
        Some("error") => "vacuum__state vacuum__state--error",
        Some("paused") => "vacuum__state vacuum__state--paused",
        _ => "vacuum__state",
    }
}

#[component]
pub fn VacuumCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let label = display_value(&entity.state, entity.unit.as_deref());
    let (activity, battery, fan) = match &entity.state.value {
        StateValue::Vacuum(v) => (v.state.clone(), v.battery, v.fan_speed.clone()),
        _ => (None, None, None),
    };
    let actions = &entity.capabilities.vacuum_actions;
    let primary = primary_action(activity.as_deref(), actions);

    let battery_view = battery.map(|level| {
        let fill = format!("width: {}%", level.min(100));
        view! {
            <span class="vacuum__battery" class:vacuum__battery--low=level < 20 title="Battery">
                <span class="vacuum__battery-glyph" aria-hidden="true">
                    <span class="vacuum__battery-fill" style=fill></span>
                </span>
                {format!("{level}%")}
            </span>
        }
    });

    let primary_view = primary.map(|(action, text)| {
        let id = entity.id.clone();
        view! {
            <button
                type="button"
                class="btn vacuum__primary"
                class:vacuum__primary--pause=action == "pause"
                on:click=move |_| {
                    sender.send(&id, Command::VacuumAction { action: action.into() }, None);
                }
            >
                <span class="vacuum__primary-icon" inner_html=vacuum_action_icon(action)></span>
                {text}
            </button>
        }
    });

    // Start and pause live on the primary button; the rest are icon buttons.
    let secondary = actions
        .iter()
        .filter(|a| !matches!(a.as_str(), "start" | "pause"))
        .cloned()
        .map(|action| {
            let id = entity.id.clone();
            let text = action_label(&action);
            let icon = vacuum_action_icon(&action);
            view! {
                <button
                    type="button"
                    class="btn btn--icon"
                    aria-label=text.clone()
                    title=text
                    inner_html=icon
                    on:click=move |_| {
                        sender.send(&id, Command::VacuumAction { action: action.clone() }, None);
                    }
                ></button>
            }
        })
        .collect_view();

    let fan_select = (!entity.capabilities.fan_speeds.is_empty()).then(|| {
        let id = entity.id.clone();
        let state = entity.state.clone();
        let on_select = Callback::new(move |speed: String| {
            let guess = match &state.value {
                StateValue::Vacuum(v) => Some(EntityState {
                    value: StateValue::Vacuum(VacuumState {
                        fan_speed: Some(speed.clone()),
                        ..v.clone()
                    }),
                    ..state.clone()
                }),
                _ => None,
            };
            sender.send(&id, Command::SetFanSpeed { speed }, guess);
        });
        let options = entity.capabilities.fan_speeds.clone();
        view! {
            <ModeSelect label="Fan speed" options=options selected=fan on_select=on_select captioned=true />
        }
    });

    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    let state_class = activity_class(activity.as_deref());
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <div class="vacuum__status">
                <span class=state_class>{label}</span>
                {battery_view}
            </div>
            <div class="vacuum__controls">
                {primary_view}
                <div class="card__controls vacuum__actions">{secondary}</div>
                {fan_select}
            </div>
        </CardFrame>
    }
}

#[cfg(test)]
mod tests {
    use super::{action_label, primary_action};

    #[test]
    fn short_action_labels() {
        assert_eq!(action_label("return_to_base"), "Dock");
        assert_eq!(action_label("clean_spot"), "Spot");
        assert_eq!(action_label("locate"), "Locate");
        assert_eq!(action_label("start"), "Start");
        assert_eq!(action_label("pause"), "Pause");
        assert_eq!(action_label("stop"), "Stop");
        assert_eq!(action_label("custom_thing"), "Custom thing");
    }

    #[test]
    fn primary_follows_the_activity() {
        let both = vec!["start".to_owned(), "pause".to_owned()];
        assert_eq!(
            primary_action(Some("docked"), &both),
            Some(("start", "Start".into()))
        );
        assert_eq!(primary_action(None, &both), Some(("start", "Start".into())));
        assert_eq!(
            primary_action(Some("cleaning"), &both),
            Some(("pause", "Pause".into()))
        );
        assert_eq!(
            primary_action(Some("paused"), &both),
            Some(("start", "Resume".into()))
        );
        let start_only = vec!["start".to_owned()];
        assert_eq!(
            primary_action(Some("cleaning"), &start_only),
            Some(("start", "Start".into()))
        );
        assert_eq!(primary_action(Some("docked"), &["locate".to_owned()]), None);
    }
}
