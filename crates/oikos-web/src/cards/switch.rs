//! The switch card: a power toggle.

use leptos::prelude::*;
use oikos_api::EntityView;

pub use super::light::read_on;
use super::light::{power_command, with_on};
use super::{PowerToggle, Sender};
use crate::components::card_frame::{CardFrame, open_entity};
use crate::format::display_value;

#[component]
pub fn SwitchCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let on = read_on(&entity.state).unwrap_or(false);
    let label = display_value(&entity.state, entity.unit.as_deref());
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

#[cfg(test)]
mod tests {
    use super::read_on;
    use oikos_core::model::{EntityState, LightState, StateValue};

    #[test]
    fn as_light_switch_reads_on_off() {
        let s = EntityState {
            value: StateValue::OnOff(true),
            ..Default::default()
        };
        assert_eq!(read_on(&s), Some(true));
    }

    #[test]
    fn native_light_reads_light_state() {
        let s = EntityState {
            value: StateValue::Light(LightState {
                on: true,
                ..Default::default()
            }),
            ..Default::default()
        };
        assert_eq!(read_on(&s), Some(true));
    }

    #[test]
    fn unknown_reads_none() {
        let s = EntityState {
            value: StateValue::Unknown,
            ..Default::default()
        };
        assert_eq!(read_on(&s), None);
    }
}
