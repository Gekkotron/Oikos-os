//! The climate card: current and target temperature, step buttons, mode and fan selects.

use leptos::prelude::*;
use oikos_api::EntityView;
use oikos_core::model::{ClimateState, Command, EntityState, StateValue};

use super::{ModeSelect, Sender, Stepper};
use crate::components::card_frame::{CardFrame, open_entity};
use crate::format::display_value;

const DEFAULT_STEP: f64 = 0.5;

/// The target after one step up (`+1.0`) or down (`-1.0`) from `current`, kept within the known
/// bounds. `None` when there is no target to step from.
pub(super) fn stepped(
    current: Option<f64>,
    step: Option<f64>,
    direction: f64,
    min: Option<f64>,
    max: Option<f64>,
) -> Option<f64> {
    let step = step.filter(|s| *s > 0.0).unwrap_or(DEFAULT_STEP);
    let next = current? + direction * step;
    let next = min.map_or(next, |m| next.max(m));
    Some(max.map_or(next, |m| next.min(m)))
}

fn with_climate(state: &EntityState, f: impl FnOnce(&mut ClimateState)) -> Option<EntityState> {
    let StateValue::Climate(climate) = &state.value else {
        return None;
    };
    let mut climate = climate.clone();
    f(&mut climate);
    Some(EntityState {
        value: StateValue::Climate(climate),
        ..state.clone()
    })
}

#[component]
pub fn ClimateCard(entity: EntityView) -> impl IntoView {
    let sender = Sender::from_context();
    let label = display_value(&entity.state, entity.unit.as_deref());
    let (target, mode, fan) = match &entity.state.value {
        StateValue::Climate(c) => (c.target, c.mode.clone(), c.fan_mode.clone()),
        _ => (None, None, None),
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
            let guess = with_climate(&state, |c| c.target = Some(temperature));
            sender.send(&id, Command::SetTemperature { temperature }, guess);
        })
    };

    let mode_select = (!caps.hvac_modes.is_empty()).then(|| {
        let id = entity.id.clone();
        let state = entity.state.clone();
        let on_select = Callback::new(move |picked: String| {
            let guess = with_climate(&state, |c| c.mode = Some(picked.clone()));
            sender.send(&id, Command::SetHvacMode { mode: picked }, guess);
        });
        view! { <ModeSelect label="Mode" options=caps.hvac_modes.clone() selected=mode on_select=on_select /> }
    });
    let fan_select = (!caps.fan_modes.is_empty()).then(|| {
        let id = entity.id.clone();
        let state = entity.state.clone();
        let on_select = Callback::new(move |picked: String| {
            let guess = with_climate(&state, |c| c.fan_mode = Some(picked.clone()));
            sender.send(&id, Command::SetFanMode { mode: picked }, guess);
        });
        view! { <ModeSelect label="Fan" options=caps.fan_modes.clone() selected=fan on_select=on_select /> }
    });

    let pending = sender.pending(&entity.id);
    let on_click = open_entity(&entity.id);
    view! {
        <CardFrame entity=entity on_click=on_click pending=pending>
            <Stepper label=label can_step=target.is_some() on_step=on_step />
            <div class="card__controls">{mode_select}{fan_select}</div>
        </CardFrame>
    }
}

#[cfg(test)]
mod tests {
    use super::stepped;

    #[test]
    fn steps_by_the_given_step() {
        assert_eq!(stepped(Some(20.0), Some(1.0), 1.0, None, None), Some(21.0));
        assert_eq!(stepped(Some(20.0), Some(1.0), -1.0, None, None), Some(19.0));
    }

    #[test]
    fn defaults_to_half_a_degree() {
        assert_eq!(stepped(Some(20.0), None, 1.0, None, None), Some(20.5));
    }

    #[test]
    fn clamps_to_bounds() {
        assert_eq!(
            stepped(Some(30.0), Some(1.0), 1.0, Some(5.0), Some(30.0)),
            Some(30.0)
        );
        assert_eq!(
            stepped(Some(5.0), Some(1.0), -1.0, Some(5.0), Some(30.0)),
            Some(5.0)
        );
    }

    #[test]
    fn no_target_no_step() {
        assert_eq!(stepped(None, None, 1.0, None, None), None);
    }
}
