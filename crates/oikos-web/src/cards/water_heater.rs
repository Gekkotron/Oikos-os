//! The water heater card: current and target temperature, step buttons and a mode select.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::{Command, EntityState, StateValue, WaterHeaterState};

use super::climate::stepped;
use super::{ModeSelect, Sender, Stepper};
use crate::components::card_frame::{CardFrame, open_entity};
use crate::format::{degrees, display_value};

fn with_water_heater(
    state: &EntityState,
    f: impl FnOnce(&mut WaterHeaterState),
) -> Option<EntityState> {
    let StateValue::WaterHeater(heater) = &state.value else {
        return None;
    };
    let mut heater = heater.clone();
    f(&mut heater);
    Some(EntityState {
        value: StateValue::WaterHeater(heater),
        ..state.clone()
    })
}

#[component]
pub fn WaterHeaterCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let label = display_value(&entity.state, entity.unit.as_deref());
    let target_text = degrees(
        match &entity.state.value {
            StateValue::WaterHeater(w) => w.target,
            _ => None,
        },
        entity.unit.as_deref(),
    );
    let (target, mode) = match &entity.state.value {
        StateValue::WaterHeater(w) => (w.target, w.mode.clone()),
        _ => (None, None),
    };
    let caps = &entity.capabilities;

    let on_step = {
        let id = entity.id.clone();
        let state = entity.state.clone();
        let (step, min, max) = (caps.temp_step, caps.min_temp, caps.max_temp);
        Callback::new(move |direction: f64| {
            let Some(temperature) = stepped(target, step, direction, min, max) else {
                return;
            };
            let guess = with_water_heater(&state, |w| w.target = Some(temperature));
            sender.send(&id, Command::SetTemperature { temperature }, guess);
        })
    };

    let mode_select = (!caps.hvac_modes.is_empty()).then(|| {
        let id = entity.id.clone();
        let state = entity.state.clone();
        let on_select = Callback::new(move |picked: String| {
            let guess = with_water_heater(&state, |w| w.mode = Some(picked.clone()));
            sender.send(&id, Command::SetHvacMode { mode: picked }, guess);
        });
        view! { <ModeSelect label="Mode" options=caps.hvac_modes.clone() selected=mode on_select=on_select captioned=true /> }
    });

    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <div class="thermo">
                <span class="thermo__now">{label}</span>
                <Stepper label=target_text caption="Target" can_step=target.is_some() on_step=on_step />
                <div class="thermo__picks">{mode_select}</div>
            </div>
        </CardFrame>
    }
}
