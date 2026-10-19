//! The vacuum card: one button per supported action and a fan speed select.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::{Command, EntityState, StateValue, VacuumState};

use super::{ModeSelect, Sender};
use crate::components::card_frame::{CardFrame, open_entity};
use crate::format::{display_value, word};

/// The short button label for a vacuum action.
fn action_label(action: &str) -> String {
    match action {
        "return_to_base" => "Dock".into(),
        "clean_spot" => "Spot".into(),
        other => word(other),
    }
}

#[component]
pub fn VacuumCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let label = display_value(&entity.state, entity.unit.as_deref());
    let fan = match &entity.state.value {
        StateValue::Vacuum(v) => v.fan_speed.clone(),
        _ => None,
    };

    let actions = entity
        .capabilities
        .vacuum_actions
        .iter()
        .cloned()
        .map(|action| {
            let id = entity.id.clone();
            let text = action_label(&action);
            view! {
                <button
                    type="button"
                    class="btn btn--compact"
                    on:click=move |_| {
                        sender.send(&id, Command::VacuumAction { action: action.clone() }, None);
                    }
                >
                    {text}
                </button>
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
        view! { <ModeSelect label="Fan speed" options=options selected=fan on_select=on_select /> }
    });

    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <span class="card__value">{label}</span>
            <div class="card__controls card__controls--grid">{actions}</div>
            <div class="card__controls card__controls--stack">{fan_select}</div>
        </CardFrame>
    }
}

#[cfg(test)]
mod tests {
    use super::action_label;

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
}
