//! The light card: a power toggle, a brightness slider and a colour entry.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::{Command, EntityState, StateValue};

use super::{PowerToggle, Sender};
use crate::components::card_frame::{CardFrame, open_entity};
use crate::format::display_value;

/// Whether the entity is on: from `OnOff` (a native switch, or a switch shown as a light) or
/// from `Light`; `None` while the state is unknown.
pub fn read_on(state: &EntityState) -> Option<bool> {
    match &state.value {
        StateValue::OnOff(on) => Some(*on),
        StateValue::Light(light) => Some(light.on),
        _ => None,
    }
}

/// `state` with its on flag set, the guess shown while a toggle is in flight. `None` for a
/// state that has no on flag.
pub(super) fn with_on(state: &EntityState, on: bool) -> Option<EntityState> {
    let value = match &state.value {
        StateValue::OnOff(_) => StateValue::OnOff(on),
        StateValue::Light(light) => StateValue::Light(oikos_core::model::LightState {
            on,
            ..light.clone()
        }),
        _ => return None,
    };
    Some(EntityState {
        value,
        ..state.clone()
    })
}

/// The turn-on or turn-off command for a toggle to `on`.
pub(super) fn power_command(on: bool) -> Command {
    if on {
        Command::TurnOn {
            brightness: None,
            color: None,
            effect: None,
        }
    } else {
        Command::TurnOff
    }
}

#[component]
pub fn LightCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let on = read_on(&entity.state).unwrap_or(false);
    let light = match &entity.state.value {
        StateValue::Light(light) => Some(light.clone()),
        _ => None,
    };
    let label = display_value(&entity.state, entity.unit.as_deref());

    let toggle = {
        let id = entity.id.clone();
        let state = entity.state.clone();
        Callback::new(move |()| {
            sender.send(&id, power_command(!on), with_on(&state, !on));
        })
    };

    let slider = entity.capabilities.brightness.then(|| {
        let id = entity.id.clone();
        let state = entity.state.clone();
        let level = light.as_ref().and_then(|l| l.brightness).unwrap_or(255);
        let on_change = move |ev: leptos::ev::Event| {
            let Ok(value) = event_target_value(&ev).parse::<u8>() else {
                return;
            };
            let guess = match &state.value {
                StateValue::Light(l) => Some(EntityState {
                    value: StateValue::Light(oikos_core::model::LightState {
                        on: true,
                        brightness: Some(value),
                        ..l.clone()
                    }),
                    ..state.clone()
                }),
                _ => None,
            };
            sender.send(
                &id,
                Command::TurnOn {
                    brightness: Some(value),
                    color: None,
                    effect: None,
                },
                guess,
            );
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

    let open = open_entity(&entity.id);
    let swatch = entity.capabilities.color_modes.contains("hs").then(|| {
        let color = light.as_ref().and_then(|l| l.hs).map_or_else(
            || "var(--accent-violet)".to_owned(),
            |hs| format!("hsl({} {}% 55%)", hs.h.round(), hs.s.round()),
        );
        view! {
            <button
                type="button"
                class="swatch"
                title="Colour"
                aria-label="Colour: open to change it"
                style:background=color
                on:click=move |_| open.run(())
            ></button>
        }
    });

    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <PowerToggle on=on label=label on_toggle=toggle />
            <div class="card__controls">{slider}{swatch}</div>
        </CardFrame>
    }
}
